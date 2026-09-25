//! Lowering for functions, methods and trait method signatures. Split
//! out of `let_and_module.rs` to keep each file under the line ceiling
//! that `scripts/check_file_sizes.sh` enforces.

use super::IrLowerer;
use crate::ast::{self, ExternAbi, FnDef, FunctionDef, ParamConvention};
use crate::ir::{IrFunction, IrFunctionParam, IrFunctionSig, ResolvedType};
use std::collections::HashMap;

impl IrLowerer<'_> {
    pub(super) fn lower_function(&mut self, f: &FunctionDef) {
        // Qualify function names declared inside `mod foo { … }` so
        // external callers (`foo::add`) can resolve via the joined-
        // name lookup that `ResolveReferencesPass` uses for
        // multi-segment paths. Top-level functions stay bare.
        let registered_name = if self.current_module_prefix.is_empty() {
            f.name.name.clone()
        } else {
            format!("{}::{}", self.current_module_prefix, f.name.name)
        };
        let generic_params = self.lower_generic_params(&f.generics);
        self.generic_scopes.push(generic_params.clone());
        // DP-4 support: scope each param-default lower against the
        // preceding params so `fn f(x, y = x)` resolves `x` inside
        // the default to the previous param.
        self.local_binding_scopes.push(HashMap::new());
        let params: Vec<IrFunctionParam> = f
            .params
            .iter()
            .map(|p| {
                let ty = p.ty.as_ref().map(|t| self.lower_type(t));
                let default = p
                    .default
                    .as_ref()
                    .map(|e| self.lower_with_expected_value(e, ty.as_ref()));
                if let Some(t) = &ty {
                    if let Some(scope) = self.local_binding_scopes.last_mut() {
                        scope.insert(p.name.name.clone(), (p.convention, t.clone()));
                    }
                }
                IrFunctionParam {
                    binding_id: crate::ir::BindingId(0),
                    name: p.name.name.clone(),
                    external_label: p.external_label.as_ref().map(|l| l.name.clone()),
                    ty,
                    default,
                    convention: p.convention,
                    span: self.ir_span(p.span),
                }
            })
            .collect();
        self.local_binding_scopes.pop();

        let return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Set return type context for inferred enum resolution
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Push a local scope so References inside the body resolve against
        // the parameters' declared types and so closure captures see the
        // parameter's convention.
        let mut frame: HashMap<String, (ParamConvention, ResolvedType)> = HashMap::new();
        for p in &params {
            if let Some(ty) = &p.ty {
                frame.insert(p.name.clone(), (p.convention, ty.clone()));
            }
        }
        self.local_binding_scopes.push(frame);

        // The declared return type is the expected type of the body,
        // so a closure in the result takes its parameter types. The
        // declare pass lowers no body.
        let body = f
            .body
            .as_ref()
            .filter(|_| !self.signatures_only)
            .map(|b| self.lower_with_expected_value(b, return_type.as_ref()));
        // trust the AST's explicit
        // `extern_abi` rather than re-deriving from `body.is_none()`.
        // Under parser error recovery the two can diverge; the
        // semantic layer surfaces the mismatch as `ExternFnWithBody` /
        // `RegularFnWithoutBody`.
        let extern_abi = f.extern_abi;

        self.local_binding_scopes.pop();

        // Restore previous return type context
        self.current_function_return_type = saved_return_type;

        self.generic_scopes.pop();

        let function = IrFunction {
            name: registered_name.clone(),
            visibility: f.visibility,
            generic_params,
            params,
            return_type,
            body,
            extern_abi,
            attributes: f.attributes.iter().map(|a| a.kind).collect(),
            doc: f.doc.clone(),
            span: self.ir_span(f.span),
        };
        if self.signatures_only {
            self.declared_functions.push(function);
            return;
        }
        if let Err(e) = self.module.add_function(registered_name.clone(), function) {
            self.errors.push(e);
        } else if let Some(node) = self.module_node_stack.last_mut() {
            // Tier-1 item G: associate the just-registered function
            // with the enclosing nested module. add_function only
            // returns Ok when a new id was allocated, so looking up by
            // name picks up that new id.
            if let Some(id) = self.module.function_id(&registered_name) {
                node.functions.push(id);
            }
        }
    }

    pub(super) fn lower_fn_def(
        &mut self,
        f: &FnDef,
        enclosing_extern: Option<ExternAbi>,
    ) -> IrFunction {
        let generic_params = self.lower_generic_params(&f.generics);
        self.generic_scopes.push(generic_params.clone());
        // DP-4 support: lower params in two stages so each param's
        // default expression sees its preceding params in the local
        // binding scope. Without the frame, `fn f(x: I32, y: I32 = x)`
        // emits `UndefinedReference` for the inner `x`.
        let mut frame: HashMap<String, (ParamConvention, ResolvedType)> = HashMap::new();
        self.local_binding_scopes.push(frame.clone());
        let params: Vec<IrFunctionParam> = f
            .params
            .iter()
            .map(|p| {
                let ty = p.ty.as_ref().map(|t| self.lower_type(t));
                let default = p
                    .default
                    .as_ref()
                    .map(|e| self.lower_with_expected_value(e, ty.as_ref()));
                if let Some(t) = &ty {
                    frame.insert(p.name.name.clone(), (p.convention, t.clone()));
                    if let Some(scope) = self.local_binding_scopes.last_mut() {
                        scope.insert(p.name.name.clone(), (p.convention, t.clone()));
                    }
                }
                IrFunctionParam {
                    binding_id: crate::ir::BindingId(0),
                    name: p.name.name.clone(),
                    external_label: p.external_label.as_ref().map(|l| l.name.clone()),
                    ty,
                    default,
                    convention: p.convention,
                    span: self.ir_span(p.span),
                }
            })
            .collect();
        // Pop the temporary frame; lower_fn_def re-pushes its own
        // below covering the body.
        self.local_binding_scopes.pop();

        let return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Set return type context for inferred enum resolution
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Push a local scope so the body's References to parameters resolve
        // to the declared param types rather than TypeParam(name) placeholders,
        // and so closures inherit the parameter convention when capturing
        //.
        let mut frame: HashMap<String, (ParamConvention, ResolvedType)> = HashMap::new();
        for p in &params {
            if let Some(ty) = &p.ty {
                frame.insert(p.name.clone(), (p.convention, ty.clone()));
            }
        }
        if let Some(impl_name) = self.current_impl_struct.clone() {
            if let Some(struct_id) = self.module.struct_id(&impl_name) {
                frame.insert(
                    "self".to_string(),
                    (ParamConvention::Let, ResolvedType::Struct(struct_id)),
                );
            } else if let Some(enum_id) = self.module.enum_id(&impl_name) {
                frame.insert(
                    "self".to_string(),
                    (ParamConvention::Let, ResolvedType::Enum(enum_id)),
                );
            }
        }
        self.local_binding_scopes.push(frame);

        // The declared return type is the expected type of the body,
        // so a closure in the result takes its parameter types. The
        // declare pass lowers no body.
        let body = f
            .body
            .as_ref()
            .filter(|_| !self.signatures_only)
            .map(|b| self.lower_with_expected_value(b, return_type.as_ref()));
        // source the extern ABI from the
        // enclosing `ImplDef` rather than re-deriving from
        // `body.is_none()`. The semantic layer enforces body/extern
        // consistency for valid programs, but under parser error
        // recovery a method may have `body: None` inside a regular
        // impl; we want the IR method's ABI to match the containing
        // impl definitionally.
        let extern_abi = enclosing_extern;

        self.local_binding_scopes.pop();

        // Restore previous return type context
        self.current_function_return_type = saved_return_type;
        self.generic_scopes.pop();

        IrFunction {
            name: f.name.name.clone(),
            // A method's reach is its impl's, and an impl follows the
            // type it is written for, so a method carries no `pub` of
            // its own. Only a top-level `fn` can be an export.
            visibility: crate::ast::Visibility::Private,
            // Its own type parameters; the impl's live on the IrImpl.
            generic_params,
            params,
            return_type,
            body,
            extern_abi,
            attributes: f.attributes.iter().map(|a| a.kind).collect(),
            doc: f.doc.clone(),
            span: self.ir_span(f.span),
        }
    }

    pub(super) fn lower_fn_sig(&mut self, sig: &ast::FnSig) -> IrFunctionSig {
        self.local_binding_scopes.push(HashMap::new());
        let params: Vec<IrFunctionParam> = sig
            .params
            .iter()
            .map(|p| {
                let ty = p.ty.as_ref().map(|t| self.lower_type(t));
                let default = p
                    .default
                    .as_ref()
                    .map(|e| self.lower_with_expected_value(e, ty.as_ref()));
                if let Some(t) = &ty {
                    if let Some(scope) = self.local_binding_scopes.last_mut() {
                        scope.insert(p.name.name.clone(), (p.convention, t.clone()));
                    }
                }
                IrFunctionParam {
                    binding_id: crate::ir::BindingId(0),
                    name: p.name.name.clone(),
                    external_label: p.external_label.as_ref().map(|l| l.name.clone()),
                    ty,
                    default,
                    convention: p.convention,
                    span: self.ir_span(p.span),
                }
            })
            .collect();
        self.local_binding_scopes.pop();

        let return_type = sig.return_type.as_ref().map(|t| self.lower_type(t));

        IrFunctionSig {
            name: sig.name.name.clone(),
            params,
            return_type,
            attributes: sig.attributes.iter().map(|a| a.kind).collect(),
            span: self.ir_span(sig.span),
        }
    }
}
