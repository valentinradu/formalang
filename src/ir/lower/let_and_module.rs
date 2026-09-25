//! Let-binding, module, and function lowering for the IR lowering pass.
//!
//! Covers module-level `let` bindings (including destructuring patterns),
//! the recursive lowering of nested `mod` blocks, and free-standing
//! function definitions plus impl-method `FnDef`/`FnSig` lowering.

use super::IrLowerer;
use crate::ast::{BindingPattern, Definition, LetBinding, PrimitiveType};
use crate::ir::{IrExpr, IrLet, ResolvedType};
use crate::semantic::helpers::collect_bindings_from_pattern;

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
            span: self.ir_span(let_binding.span),
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
}
