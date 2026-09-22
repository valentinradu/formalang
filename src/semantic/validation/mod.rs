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
//! - [`control_flow`]: match exhaustiveness and enum-instantiation
//!   field checks. Optional unwrap-and-bind is the `if let` form
//!   (parsed as a match) — there is no implicit auto-bind on `if`.
//! - [`structs`]: struct field type/required checks and field mutability.
//! - [`closures`]: closure escape / capture validation; also exposes the
//!   shared free-variable walk used to populate capture lists.
//! - [`functions`]: per-function setup/teardown of binding scopes plus
//!   return-type validation. The `validate_function_return_type` and
//!   `validate_standalone_function` entry points are called from
//!   `type_resolution`.
//! - [`exclusivity`]: two arguments of one call must not reach the same
//!   place when one of them is `mut` or `sink`.
//! - [`qualified_types`]: free helpers for traversing nested module paths
//!   like `m1::m2::Foo`.

mod closures;
mod control_flow;
mod duplicate_names;
mod exclusivity;
mod expr;
mod functions;
mod invocation;
mod let_and_block;
mod let_expr_and_block;
mod method_call;
mod private_in_public;
mod public_closure_field;
mod qualified_types;
mod sequence_linear;
mod sequence_placement;
mod structs;
mod type_names;

use super::module_resolver::ModuleResolver;
use super::SemanticAnalyzer;
use crate::ast::{Definition, File, Statement, StructDef};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 3: Validate expressions
    /// Validate operators and control flow without evaluation
    pub(in crate::semantic) fn validate_expressions(&mut self, file: &File) {
        self.validate_public_closure_fields(file);
        self.validate_duplicate_names(file);
        self.validate_private_in_public(file);
        self.validate_sequence_placement(file);
        self.validate_sequence_linearity(file);
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
            Definition::Module(module_def) => {
                for nested_def in &module_def.definitions {
                    self.validate_definition_expressions(nested_def, file);
                }
            }
            // A function's body is validated by
            // `validate_standalone_function`, which the type-resolution
            // pass runs over every function. This arm used to call a
            // second walk that did the same work, minus the parameter
            // defaults — mutation testing replaced that walk with
            // nothing and no test disagreed. A trait and an enum hold
            // no expressions to walk.
            Definition::Function(_) | Definition::Trait(_) | Definition::Enum(_) => {}
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
