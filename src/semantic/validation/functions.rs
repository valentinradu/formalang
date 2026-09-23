//! Function-level Pass 3 entry points: full per-function setup/teardown of
//! local-binding, closure-capture, and param-convention scopes plus
//! return-type validation.
//!
//! These are called from `type_resolution` (impl methods) and the Pass 3
//! orchestrator (standalone functions).

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::File;
use crate::error::CompilerError;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate function return type matches the body expression type
    pub(in crate::semantic) fn validate_function_return_type(
        &mut self,
        func: &crate::ast::FnDef,
        file: &File,
    ) {
        // Clear local let bindings and sink-consumed bindings for this function
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        // Snapshot closure-binding maps so entries introduced in this function
        // body don't leak into later functions.
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_fn_scope_captures = self.fn_scope_closure_captures.clone();
        let saved_param_conventions = self.current_fn_param_conventions.clone();
        self.current_fn_param_conventions.clear();
        self.fn_scope_closure_captures.clear();

        // Register function parameters as local bindings
        for param in &func.params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty, param.span);
            }
            let param_sem = param.ty.as_ref().map_or_else(
                || {
                    if param.name.name == "self" {
                        self.current_impl_struct
                            .as_ref()
                            .map_or(crate::semantic::sem_type::SemType::Unknown, |s| {
                                crate::semantic::sem_type::SemType::Named(s.clone())
                            })
                    } else {
                        crate::semantic::sem_type::SemType::Unknown
                    }
                },
                crate::semantic::sem_type::SemType::from_ast,
            );
            let mutable = matches!(
                param.convention,
                crate::ast::ParamConvention::Mut | crate::ast::ParamConvention::Sink
            );
            self.local_let_bindings
                .insert(param.name.name.clone(), (param_sem, mutable));
            self.current_fn_param_conventions
                .insert(param.name.name.clone(), param.convention);
        }

        // Check each default value against the type its parameter
        // declares. Only the ordering rule was enforced before, so
        // `fn takes(p: I32 = "text")` compiled and every call that let
        // the default fire passed a string where the body reads an
        // integer.
        for param in &func.params {
            let (Some(declared_ty), Some(default)) = (param.ty.as_ref(), param.default.as_ref())
            else {
                continue;
            };
            self.validate_expr_expecting(default, Some(SemType::from_ast(declared_ty)), file);
            let declared = Self::type_to_string(declared_ty);
            let inferred_sem = self.infer_type_sem(default, file);
            if !self.value_satisfies_declared(&declared, &inferred_sem) {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: declared,
                    found: inferred_sem.display(),
                    span: default.span(),
                });
            }
        }

        // Validate the function body expression (only if body exists)
        if let Some(body) = &func.body {
            // The declared return type is the expected type of the
            // body, so a closure in the result takes its types.
            let expected = func.return_type.as_ref().map(SemType::from_ast);
            self.validate_expr_expecting(body, expected, file);
            self.validate_function_return_escape(func.return_type.as_ref(), body);

            // If there's a declared return type, check it matches the body type
            if let Some(declared_return_type) = &func.return_type {
                let body_sem = self.infer_type_sem(body, file);
                let body_type = body_sem.display();
                let expected_type = Self::type_to_string(declared_return_type);

                // Check if types are compatible. Goes through the shared
                // rule so a return position accepts what a `let`
                // annotation does — `pub fn f() -> I32? { nil }` used to
                // be rejected here while `let v: I32? = nil` was fine.
                if !self.value_satisfies_declared(&expected_type, &body_sem) {
                    self.errors.push(CompilerError::FunctionReturnTypeMismatch {
                        function: func.name.name.clone(),
                        expected: expected_type,
                        actual: body_type,
                        span: func.name.span,
                    });
                }
            }
        }

        // Clear local let bindings after function
        self.local_let_bindings.clear();
        self.closure_binding_captures = saved_closure_captures;
        self.fn_scope_closure_captures = saved_fn_scope_captures;
        self.current_fn_param_conventions = saved_param_conventions;
    }

    /// Validate a standalone function definition (outside of impl blocks)
    pub(in crate::semantic) fn validate_standalone_function(
        &mut self,
        func: &crate::ast::FunctionDef,
        file: &File,
    ) {
        // Push the function's own generic parameters so its param/return
        // types and body can reference them without triggering
        // OutOfScopeTypeParameter.
        self.push_generic_scope(&func.generics);
        // Clear local let bindings and sink-consumed bindings for this function
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        // Snapshot closure-binding maps so entries introduced in this function
        // body don't leak into later functions.
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_fn_scope_captures = self.fn_scope_closure_captures.clone();
        let saved_param_conventions = self.current_fn_param_conventions.clone();
        self.current_fn_param_conventions.clear();
        self.fn_scope_closure_captures.clear();

        // Register function parameters as local bindings
        for param in &func.params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty, param.span);
            }
            let param_sem = param.ty.as_ref().map_or(
                crate::semantic::sem_type::SemType::Unknown,
                crate::semantic::sem_type::SemType::from_ast,
            );
            let mutable = matches!(
                param.convention,
                crate::ast::ParamConvention::Mut | crate::ast::ParamConvention::Sink
            );
            self.local_let_bindings
                .insert(param.name.name.clone(), (param_sem, mutable));
            self.current_fn_param_conventions
                .insert(param.name.name.clone(), param.convention);
        }

        // Check each default value against the type its parameter
        // declares. Only the ordering rule was enforced before, so
        // `fn takes(p: I32 = "text")` compiled and every call that let
        // the default fire passed a string where the body reads an
        // integer.
        for param in &func.params {
            let (Some(declared_ty), Some(default)) = (param.ty.as_ref(), param.default.as_ref())
            else {
                continue;
            };
            self.validate_expr_expecting(default, Some(SemType::from_ast(declared_ty)), file);
            let declared = Self::type_to_string(declared_ty);
            let inferred_sem = self.infer_type_sem(default, file);
            if !self.value_satisfies_declared(&declared, &inferred_sem) {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: declared,
                    found: inferred_sem.display(),
                    span: default.span(),
                });
            }
        }

        // Validate return type if declared
        if let Some(return_type) = &func.return_type {
            self.validate_type(return_type, func.span);
        }

        // Validate the function body if present
        if let Some(body) = &func.body {
            // The declared return type is the expected type of the
            // body, so a closure in the result takes its types.
            let expected = func.return_type.as_ref().map(SemType::from_ast);
            self.validate_expr_expecting(body, expected, file);
            self.validate_function_return_escape(func.return_type.as_ref(), body);

            // If there's a declared return type, check it matches the body type
            if let Some(declared_return_type) = &func.return_type {
                let body_sem = self.infer_type_sem(body, file);
                let body_type = body_sem.display();
                let expected_type = Self::type_to_string(declared_return_type);

                // Check if types are compatible. Goes through the shared
                // rule so a return position accepts what a `let`
                // annotation does — `pub fn f() -> I32? { nil }` used to
                // be rejected here while `let v: I32? = nil` was fine.
                if !self.value_satisfies_declared(&expected_type, &body_sem) {
                    self.errors.push(CompilerError::FunctionReturnTypeMismatch {
                        function: func.name.name.clone(),
                        expected: expected_type,
                        actual: body_type,
                        span: func.name.span,
                    });
                }
            }
        }

        // Clear local let bindings after function
        self.local_let_bindings.clear();
        self.closure_binding_captures = saved_closure_captures;
        self.fn_scope_closure_captures = saved_fn_scope_captures;
        self.current_fn_param_conventions = saved_param_conventions;
        self.pop_generic_scope();
    }
}
