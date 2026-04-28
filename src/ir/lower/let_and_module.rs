//! Let-binding, module, and function lowering for the IR lowering pass.
//!
//! Covers module-level `let` bindings (including destructuring patterns),
//! the recursive lowering of nested `mod` blocks, and free-standing
//! function definitions plus impl-method `FnDef`/`FnSig` lowering.

use super::IrLowerer;
use crate::ast::{
    self, BindingPattern, Definition, ExternAbi, FnDef, FunctionDef, LetBinding, ParamConvention,
    PrimitiveType,
};
use crate::ir::{IrExpr, IrFunction, IrFunctionParam, IrFunctionSig, IrLet, ResolvedType};
use std::collections::HashMap;

impl IrLowerer<'_> {
    /// Lower a module-level let binding
    pub(super) fn lower_let_binding(&mut self, let_binding: &LetBinding) {
        match &let_binding.pattern {
            BindingPattern::Simple(ident) => self.lower_simple_let(let_binding, &ident.name),
            BindingPattern::Array { elements, .. } => {
                self.lower_array_destructuring_let(let_binding, elements);
            }
            BindingPattern::Struct { fields, .. } => {
                self.lower_struct_destructuring_let(let_binding, fields);
            }
            BindingPattern::Tuple { elements, .. } => {
                self.lower_tuple_destructuring_let(let_binding, elements);
            }
        }
    }

    /// Lower a simple `let name = value` binding.
    fn lower_simple_let(&mut self, let_binding: &LetBinding, ident_name: &str) {
        // thread the let's annotation as the inferred-enum
        // target so `.variant` literals in the value resolve to the
        // declared enum (e.g. `let s: Status = .pending`) instead of
        // lowering to `TypeParam("InferredEnum")`.
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type =
            let_binding.type_annotation.as_ref().map(Self::type_name);
        // when the let's type annotation is a
        // closure type, thread it through `expected_closure_type` so
        // closure-literal values with un-annotated params (e.g.
        // `let f: I32 -> I32 = mut n -> n`) pick up the param types
        // from the annotation instead of falling back to
        // `ResolvedType::Error`. Mirrors the existing handling in
        // struct-field arg lowering and function-call arg lowering.
        let saved_closure = self.expected_closure_type.take();
        self.expected_closure_type = let_binding
            .type_annotation
            .as_ref()
            .map(|t| self.lower_type(t))
            .filter(|t| matches!(t, ResolvedType::Closure { .. }));
        let mut value = self.lower_expr(&let_binding.value);
        self.expected_closure_type = saved_closure;
        self.current_function_return_type = saved_return_type;
        let ty = if let Some(type_ann) = &let_binding.type_annotation {
            self.lower_type(type_ann)
        } else {
            self.symbols
                .get_let_type(ident_name)
                .map(str::to_string)
                .and_then(|s| self.string_to_resolved_type(&s))
                .unwrap_or_else(|| value.ty().clone())
        };
        // an empty array literal lowers to `Array(Never)`
        // because it has no elements to seed the element type from.
        // When the binding is annotated `[T]`, retype the value's
        // `Array(Never)` to `Array(T)` so backends and downstream IR
        // passes see a concrete element type instead of Never.
        if let (IrExpr::Array { elements, ty: vty }, ResolvedType::Array(annotated_elem)) =
            (&mut value, &ty)
        {
            if elements.is_empty()
                && matches!(
                    vty,
                    ResolvedType::Array(boxed)
                        if matches!(**boxed, ResolvedType::Primitive(PrimitiveType::Never))
                )
            {
                *vty = ResolvedType::Array(annotated_elem.clone());
            }
        }
        self.module.add_let(IrLet {
            name: ident_name.to_string(),
            visibility: let_binding.visibility,
            mutable: let_binding.mutable,
            ty,
            value,
            doc: let_binding.doc.clone(),
        });
    }

    /// Lower definitions within a module
    /// This processes nested definitions with their qualified names
    pub(super) fn lower_module(&mut self, module_name: &str, definitions: &[Definition]) {
        // Save current module prefix
        let saved_prefix = self.current_module_prefix.clone();

        // Update module prefix for nested definitions
        if self.current_module_prefix.is_empty() {
            self.current_module_prefix = module_name.to_string();
        } else {
            self.current_module_prefix = format!("{}::{}", self.current_module_prefix, module_name);
        }

        // Tier-1 item G: open a fresh module node for this scope.
        // Member IDs are appended by the lower_*_with_prefix helpers
        // and `lower_function` while the node sits on top of the
        // stack. On exit the node is attached to the parent node, or
        // to `module.modules` for top-level modules.
        self.module_node_stack.push(crate::ir::IrModuleNode {
            name: module_name.to_string(),
            ..Default::default()
        });

        // Lower all definitions in the module
        for def in definitions {
            match def {
                Definition::Trait(t) => {
                    // Traits in modules use qualified names
                    self.lower_trait_with_prefix(t, &self.current_module_prefix.clone());
                }
                Definition::Struct(s) => {
                    // Structs in modules use qualified names
                    self.lower_struct_with_prefix(s, &self.current_module_prefix.clone());
                }
                Definition::Enum(e) => {
                    // Enums in modules use qualified names
                    self.lower_enum_with_prefix(e, &self.current_module_prefix.clone());
                }
                Definition::Impl(i) => {
                    // Impls in modules
                    self.lower_impl(i);
                }
                Definition::Function(f) => {
                    // Functions in modules
                    self.lower_function(f.as_ref());
                }
                Definition::Module(m) => {
                    // Recursively process nested modules
                    self.lower_module(&m.name.name, &m.definitions);
                }
            }
        }

        // Pop the node we pushed at entry; attach to parent or to
        // module.modules if this was a top-level mod block.
        if let Some(node) = self.module_node_stack.pop() {
            if let Some(parent) = self.module_node_stack.last_mut() {
                parent.modules.push(node);
            } else {
                self.module.modules.push(node);
            }
        }

        // Restore module prefix
        self.current_module_prefix = saved_prefix;
    }

    pub(super) fn lower_function(&mut self, f: &FunctionDef) {
        let generic_params = self.lower_generic_params(&f.generics);
        self.generic_scopes.push(generic_params.clone());
        let params: Vec<IrFunctionParam> = f
            .params
            .iter()
            .map(|p| IrFunctionParam {
                binding_id: crate::ir::BindingId(0),
                name: p.name.name.clone(),
                external_label: p.external_label.as_ref().map(|l| l.name.clone()),
                ty: p.ty.as_ref().map(|t| self.lower_type(t)),
                default: p.default.as_ref().map(|e| self.lower_expr(e)),
                convention: p.convention,
            })
            .collect();

        let return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Set return type context for inferred enum resolution
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = f.return_type.as_ref().map(Self::type_name);

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

        let body = f.body.as_ref().map(|b| self.lower_expr(b));
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

        if let Err(e) = self.module.add_function(
            f.name.name.clone(),
            IrFunction {
                name: f.name.name.clone(),
                generic_params,
                params,
                return_type,
                body,
                extern_abi,
                attributes: f.attributes.iter().map(|a| a.kind).collect(),
                doc: f.doc.clone(),
            },
        ) {
            self.errors.push(e);
        } else if let Some(node) = self.module_node_stack.last_mut() {
            // Tier-1 item G: associate the just-registered function
            // with the enclosing nested module. add_function only
            // returns Ok when a new id was allocated, so looking up by
            // name picks up that new id.
            if let Some(id) = self.module.function_id(&f.name.name) {
                node.functions.push(id);
            }
        }
    }

    pub(super) fn lower_fn_def(
        &mut self,
        f: &FnDef,
        enclosing_extern: Option<ExternAbi>,
    ) -> IrFunction {
        let params: Vec<IrFunctionParam> = f
            .params
            .iter()
            .map(|p| IrFunctionParam {
                binding_id: crate::ir::BindingId(0),
                name: p.name.name.clone(),
                external_label: p.external_label.as_ref().map(|l| l.name.clone()),
                ty: p.ty.as_ref().map(|t| self.lower_type(t)),
                default: p.default.as_ref().map(|e| self.lower_expr(e)),
                convention: p.convention,
            })
            .collect();

        let return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Set return type context for inferred enum resolution
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = f.return_type.as_ref().map(Self::type_name);

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

        let body = f.body.as_ref().map(|b| self.lower_expr(b));
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

        IrFunction {
            name: f.name.name.clone(),
            // Method-level generics aren't yet supported; enclosing type
            // generics live on the containing IrImpl.
            generic_params: Vec::new(),
            params,
            return_type,
            body,
            extern_abi,
            attributes: f.attributes.iter().map(|a| a.kind).collect(),
            doc: f.doc.clone(),
        }
    }

    pub(super) fn lower_fn_sig(&mut self, sig: &ast::FnSig) -> IrFunctionSig {
        let params: Vec<IrFunctionParam> = sig
            .params
            .iter()
            .map(|p| IrFunctionParam {
                binding_id: crate::ir::BindingId(0),
                name: p.name.name.clone(),
                external_label: p.external_label.as_ref().map(|l| l.name.clone()),
                ty: p.ty.as_ref().map(|t| self.lower_type(t)),
                default: p.default.as_ref().map(|e| self.lower_expr(e)),
                convention: p.convention,
            })
            .collect();

        let return_type = sig.return_type.as_ref().map(|t| self.lower_type(t));

        IrFunctionSig {
            name: sig.name.name.clone(),
            params,
            return_type,
            attributes: sig.attributes.iter().map(|a| a.kind).collect(),
        }
    }
}
