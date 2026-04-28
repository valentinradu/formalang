//! Function-level Pass 3 entry points: full per-function setup/teardown of
//! local-binding, closure-capture, and param-convention scopes plus
//! return-type validation.
//!
//! These are called from `type_resolution` (impl methods) and the Pass 3
//! orchestrator (standalone functions).

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{File, Type};
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
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_fn_scope_captures = self.fn_scope_closure_captures.clone();
        let saved_param_conventions = self.current_fn_param_conventions.clone();
        self.current_fn_param_conventions.clear();
        self.fn_scope_closure_captures.clear();

        // Register function parameters as local bindings
        for param in &func.params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty);
            }
            let ty_str = param.ty.as_ref().map_or_else(
                || {
                    if param.name.name == "self" {
                        self.current_impl_struct
                            .clone()
                            .unwrap_or_else(|| "Unknown".to_string())
                    } else {
                        "Unknown".to_string()
                    }
                },
                |ty| Self::type_to_string(ty),
            );
            let mutable = matches!(
                param.convention,
                crate::ast::ParamConvention::Mut | crate::ast::ParamConvention::Sink
            );
            self.local_let_bindings
                .insert(param.name.name.clone(), (ty_str, mutable));
            self.current_fn_param_conventions
                .insert(param.name.name.clone(), param.convention);
            // Register closure-typed parameters so they're callable inside the
            // body. Parameters have no captures of their own — no
            // closure_binding_captures entry.
            if let Some(Type::Closure {
                params: closure_params,
                ..
            }) = &param.ty
            {
                let conventions: Vec<_> = closure_params.iter().map(|(c, _)| *c).collect();
                self.closure_binding_conventions
                    .insert(param.name.name.clone(), conventions);
            }
        }

        // Validate the function body expression (only if body exists)
        if let Some(body) = &func.body {
            self.validate_expr(body, file);
            self.validate_function_return_escape(func.return_type.as_ref(), body);

            // If there's a declared return type, check it matches the body type
            if let Some(declared_return_type) = &func.return_type {
                let body_type = self.infer_type_sem(body, file).display();
                let expected_type = Self::type_to_string(declared_return_type);

                // Check if types are compatible
                if !self.type_strings_compatible(&expected_type, &body_type) {
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
        self.closure_binding_conventions = saved_closure_conventions;
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
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_fn_scope_captures = self.fn_scope_closure_captures.clone();
        let saved_param_conventions = self.current_fn_param_conventions.clone();
        self.current_fn_param_conventions.clear();
        self.fn_scope_closure_captures.clear();

        // Register function parameters as local bindings
        for param in &func.params {
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
            if let Some(Type::Closure {
                params: closure_params,
                ..
            }) = &param.ty
            {
                let conventions: Vec<_> = closure_params.iter().map(|(c, _)| *c).collect();
                self.closure_binding_conventions
                    .insert(param.name.name.clone(), conventions);
            }
        }

        // Validate return type if declared
        if let Some(return_type) = &func.return_type {
            self.validate_type(return_type);
        }

        // Validate the function body if present
        if let Some(body) = &func.body {
            self.validate_expr(body, file);
            self.validate_function_return_escape(func.return_type.as_ref(), body);

            // If there's a declared return type, check it matches the body type
            if let Some(declared_return_type) = &func.return_type {
                let body_type = self.infer_type_sem(body, file).display();
                let expected_type = Self::type_to_string(declared_return_type);

                // Check if types are compatible
                if !self.type_strings_compatible(&expected_type, &body_type) {
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
        self.closure_binding_conventions = saved_closure_conventions;
        self.closure_binding_captures = saved_closure_captures;
        self.fn_scope_closure_captures = saved_fn_scope_captures;
        self.current_fn_param_conventions = saved_param_conventions;
        self.pop_generic_scope();
    }
}
