//! Pass 3 — expression validation.
//!
//! Walks every statement / definition in the file and dispatches to per-shape
//! validators. The implementation is split across this module's siblings:
//!
//! - [`expr`]: the central `validate_expr` dispatcher plus reference,
//!   literal-homogeneity, and operator/destructuring helpers.
//! - [`let_and_block`]: top-level `let` statements, `let` expressions, and
//!   block-statement scoping (consumed-binding save/restore).
//! - [`invocation`]: struct instantiation, function-call overload resolution,
//!   closure-binding calls, and module-visibility checks for qualified paths.
//! - [`method_call`]: receiver / argument convention checks plus method
//!   existence lookup (local impls, trait impls, generics, qualified types).
//! - [`control_flow`]: match exhaustiveness, enum instantiation, and the
//!   optional-condition auto-binding for `if`.
//! - [`structs`]: struct field type/required checks and field mutability.
//! - [`closures`]: closure escape / capture validation; also exposes the
//!   shared free-variable walk used to populate capture lists.
//! - [`functions`]: per-function setup/teardown of binding scopes plus
//!   return-type validation. The `validate_function_return_type` and
//!   `validate_standalone_function` entry points are called from
//!   `type_resolution`.
//! - [`qualified_types`]: free helpers for traversing nested module paths
//!   like `m1::m2::Foo`.

mod closures;
mod control_flow;
mod expr;
mod functions;
mod invocation;
mod let_and_block;
mod method_call;
mod qualified_types;
mod structs;

use super::module_resolver::ModuleResolver;
use super::SemanticAnalyzer;
use crate::ast::{Definition, File, Statement, StructDef};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 3: Validate expressions
    /// Validate operators and control flow without evaluation
    pub(in crate::semantic) fn validate_expressions(&mut self, file: &File) {
        for statement in &file.statements {
            match statement {
                Statement::Let(let_binding) => self.validate_let_statement(let_binding, file),
                Statement::Definition(def) => self.validate_definition_expressions(def, file),
                Statement::Use(_) => {}
            }
        }
    }

    /// Dispatch expression validation for a single `Definition`, recursing
    /// through nested modules so that function bodies, struct field defaults,
    /// and impl blocks inside `module { ... }` all receive Pass 3 checks.
    fn validate_definition_expressions(&mut self, def: &Definition, file: &File) {
        match def {
            Definition::Struct(struct_def) => self.validate_struct_expressions(struct_def, file),
            Definition::Impl(impl_def) => self.validate_impl_expressions(impl_def, file),
            Definition::Function(func_def) => self.validate_function_body(func_def, file),
            Definition::Module(module_def) => {
                for nested_def in &module_def.definitions {
                    self.validate_definition_expressions(nested_def, file);
                }
            }
            Definition::Trait(_) | Definition::Enum(_) => {}
        }
    }

    fn validate_impl_expressions(&mut self, impl_def: &crate::ast::ImplDef, file: &File) {
        // Push the impl's generic scope (merging target struct/enum
        // generics) so method bodies see trait bounds on type
        // parameters during expression validation.
        self.push_impl_generic_scope(&impl_def.generics, &impl_def.name.name);
        self.current_impl_struct = Some(impl_def.name.name.clone());
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        for func in &impl_def.functions {
            self.validate_function_return_type(func, file);
        }
        self.current_impl_struct = None;
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        self.pop_generic_scope();
    }

    fn validate_function_body(&mut self, func_def: &crate::ast::FunctionDef, file: &File) {
        // Push the function's generic params so uses of `T` inside the
        // body and in param/return annotations don't trip the
        // OutOfScopeTypeParameter check.
        self.push_generic_scope(&func_def.generics);
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        // Snapshot closure-binding maps so entries introduced in
        // this function body don't leak into later functions.
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_fn_scope_captures = std::mem::take(&mut self.fn_scope_closure_captures);
        let saved_param_conventions = self.current_fn_param_conventions.clone();
        self.current_fn_param_conventions.clear();
        for param in &func_def.params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty);
            }
            let ty_str = param
                .ty
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |ty| Self::type_to_string(ty));
            let mutable = matches!(
                param.convention,
                crate::ast::ParamConvention::Mut | crate::ast::ParamConvention::Sink
            );
            self.local_let_bindings
                .insert(param.name.name.clone(), (ty_str, mutable));
            self.current_fn_param_conventions
                .insert(param.name.name.clone(), param.convention);
            // Register closure-typed parameters so they're callable inside the body.
            // Parameters have no captures of their own — no closure_binding_captures entry.
            if let Some(crate::ast::Type::Closure {
                params: closure_params,
                ..
            }) = &param.ty
            {
                let conventions: Vec<_> = closure_params.iter().map(|(c, _)| *c).collect();
                self.closure_binding_conventions
                    .insert(param.name.name.clone(), conventions);
            }
        }
        if let Some(body) = &func_def.body {
            self.validate_expr(body, file);
            self.validate_function_return_escape(func_def.return_type.as_ref(), body);
        }
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        self.closure_binding_conventions = saved_closure_conventions;
        self.closure_binding_captures = saved_closure_captures;
        self.fn_scope_closure_captures = saved_fn_scope_captures;
        self.current_fn_param_conventions = saved_param_conventions;
        self.pop_generic_scope();
    }

    /// Validate expressions in struct field defaults
    pub(in crate::semantic) fn validate_struct_expressions(
        &mut self,
        struct_def: &StructDef,
        file: &File,
    ) {
        // Validate field defaults
        for field in &struct_def.fields {
            if let Some(default_expr) = &field.default {
                self.validate_expr(default_expr, file);
                // Check that the default expression type matches the declared field type
                let inferred_sem = self.infer_type_sem(default_expr, file);
                let inferred = inferred_sem.display();
                let declared = Self::type_to_string(&field.ty);
                // nil is compatible with any optional type
                let nil_to_optional = matches!(inferred_sem, super::sem_type::SemType::Nil)
                    && declared.ends_with('?');
                // a value of type T is compatible with T? (implicit wrapping)
                let inner_to_optional =
                    declared.ends_with('?') && declared.trim_end_matches('?') == inferred.as_str();
                if !nil_to_optional
                    && !inner_to_optional
                    && !inferred_sem.is_indeterminate()
                    && declared != "Unknown"
                    && !self.type_strings_compatible(&declared, &inferred)
                {
                    self.errors.push(crate::error::CompilerError::TypeMismatch {
                        expected: declared,
                        found: inferred,
                        span: field.span,
                    });
                }
            }
        }
    }
}
