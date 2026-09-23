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
use crate::semantic::helpers::collect_bindings_from_pattern;
use std::collections::HashMap;

impl IrLowerer<'_> {
    /// Lower a module-level let binding
    pub(super) fn lower_let_binding(&mut self, let_binding: &LetBinding) {
        match &let_binding.pattern {
            BindingPattern::Simple(ident) => self.lower_simple_let(let_binding, &ident.name),
            BindingPattern::Array { .. }
            | BindingPattern::Struct { .. }
            | BindingPattern::Tuple { .. } => self.lower_destructuring_let(let_binding),
        }
    }

    /// Record the type of each name that a module-level `let` binds.
    ///
    /// A simple binding with an annotation takes the annotated type.
    /// Each other name takes the type that semantic analysis inferred.
    /// A name whose inferred type is indeterminate is not recorded.
    pub(super) fn record_module_let_types(&mut self, let_binding: &LetBinding) {
        if let (BindingPattern::Simple(ident), Some(annotation)) =
            (&let_binding.pattern, &let_binding.type_annotation)
        {
            let ty = self.lower_type(annotation);
            self.module_let_types.insert(ident.name.clone(), ty);
            return;
        }
        for binding in collect_bindings_from_pattern(&let_binding.pattern) {
            if !self.record_inferred_let_type(&binding.name, binding.span) {
                self.deferred_module_lets
                    .insert(binding.name, let_binding.clone());
            }
        }
    }

    /// Record the type that semantic analysis inferred for the
    /// module-level `let` `name`. False when the type is indeterminate.
    fn record_inferred_let_type(&mut self, name: &str, span: crate::location::Span) -> bool {
        let ast_type = self.symbols.get_let_type(name).and_then(|t| t.to_ast(span));
        let Some(ast_type) = ast_type else {
            return false;
        };
        let ty = self.lower_type(&ast_type);
        self.module_let_types.insert(name.to_string(), ty);
        true
    }

    /// The type of a deferred module-level `let`, from its value.
    ///
    /// The value lowers in the module context, with no local binding,
    /// generic scope or expected type of the definition that asked. The
    /// IR and the errors of this lowering are dropped: the third pass
    /// lowers the `let` again, in source order, and reports them there.
    pub(super) fn deferred_module_let_type(&mut self, name: &str) -> Option<ResolvedType> {
        let binding = self.deferred_module_lets.remove(name)?;
        let errors_before = self.errors.len();
        let saved_scopes = std::mem::take(&mut self.local_binding_scopes);
        let saved_generics = std::mem::take(&mut self.generic_scopes);
        let saved_return = self.current_function_return_type.take();
        let saved_impl = self.current_impl_struct.take();
        let saved_prefix = std::mem::take(&mut self.current_module_prefix);
        let saved_value = self.expected_value_type.take();
        let saved_closure = self.expected_closure_type.take();
        let saved_span = self.current_span;

        let annotation = binding.type_annotation.as_ref().map(|t| self.lower_type(t));
        let value = self.lower_with_expected_value(&binding.value, annotation.as_ref());
        let types: Vec<(String, ResolvedType)> = match &binding.pattern {
            BindingPattern::Simple(ident) => vec![(ident.name.clone(), value.ty().clone())],
            BindingPattern::Array { .. }
            | BindingPattern::Struct { .. }
            | BindingPattern::Tuple { .. } => self
                .destructure(&binding.pattern, value)
                .into_iter()
                .map(|(n, t, _)| (n, t))
                .collect(),
        };

        self.local_binding_scopes = saved_scopes;
        self.generic_scopes = saved_generics;
        self.current_function_return_type = saved_return;
        self.current_impl_struct = saved_impl;
        self.current_module_prefix = saved_prefix;
        self.expected_value_type = saved_value;
        self.expected_closure_type = saved_closure;
        self.current_span = saved_span;
        self.errors.truncate(errors_before);

        // One lowering of the value settles every name of the pattern.
        for (bound, ty) in types {
            self.deferred_module_lets.remove(&bound);
            self.module_let_types.entry(bound).or_insert(ty);
        }
        self.module_let_types.get(name).cloned()
    }

    /// Lower a simple `let name = value` binding.
    fn lower_simple_let(&mut self, let_binding: &LetBinding, ident_name: &str) {
        // The annotation is the expected type for the value, and
        // `lower_with_expected_value` routes it to the right slot: a
        // closure annotation supplies un-annotated closure params
        // (`let f: (I32) -> I32 = (n) -> n`), a container annotation
        // gets peeled one layer per level, and an enum annotation
        // resolves an inferred `.variant` (`let s: Status = .pending`).
        let lowered_annotation = let_binding
            .type_annotation
            .as_ref()
            .map(|t| self.lower_type(t));
        let mut value =
            self.lower_with_expected_value(&let_binding.value, lowered_annotation.as_ref());
        // The pre-pass recorded the type from the annotation or from
        // semantic inference. Without a record, inference did not
        // settle the type, and the value's own type is the answer.
        let ty = self
            .module_let_types
            .get(ident_name)
            .cloned()
            .unwrap_or_else(|| value.ty().clone());
        // An empty array literal lowers to `Array<Never>`. When the
        // binding is annotated `[T]`, retype the value's array generic
        // to `Array<T>` so backends and downstream IR passes see a
        // concrete element type instead of Never.
        if let IrExpr::Array {
            elements, ty: vty, ..
        } = &mut value
        {
            if elements.is_empty() {
                if let (Some(value_elem), Some(ann_elem)) =
                    (self.array_element_ty(vty), self.array_element_ty(&ty))
                {
                    if matches!(value_elem, ResolvedType::Primitive(PrimitiveType::Never)) {
                        if let Some(retyped) = self.array_of(ann_elem) {
                            *vty = retyped;
                        }
                    }
                }
            }
        }
        self.module.add_let(IrLet {
            name: ident_name.to_string(),
            visibility: let_binding.visibility,
            mutable: let_binding.mutable,
            ty,
            value,
            doc: let_binding.doc.clone(),
            span: self.current_ir_span(),
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
        // The declare pass records no module node: the full lowering
        // does that.
        if !self.signatures_only {
            self.module_node_stack.push(crate::ir::IrModuleNode {
                name: module_name.to_string(),
                ..Default::default()
            });
        }

        // Lower all definitions in the module. The declare pass lowers
        // only the signatures of the functions and the impls.
        for def in definitions {
            if self.signatures_only
                && matches!(
                    def,
                    Definition::Trait(_) | Definition::Struct(_) | Definition::Enum(_)
                )
            {
                continue;
            }
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
        if self.signatures_only {
            self.current_module_prefix = saved_prefix;
            return;
        }
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
                    span: self.current_ir_span(),
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
            span: self.current_ir_span(),
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
                    span: self.current_ir_span(),
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
            span: self.current_ir_span(),
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
                    span: self.current_ir_span(),
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
            span: self.current_ir_span(),
        }
    }
}
