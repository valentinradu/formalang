//! Method-call validation: receiver/argument convention checks plus method
//! existence lookup across local impls, trait impls, generic constraints,
//! cached modules, and qualified-type module paths.

mod lookup;

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a method call expression
    pub(super) fn validate_expr_method_call(
        &mut self,
        receiver: &Expr,
        method: &crate::ast::Ident,
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        self.validate_expr(receiver, file);
        for (_, arg) in args {
            self.validate_expr(arg, file);
        }
        let receiver_sem = self.infer_type_sem(receiver, file);
        // The four built-in compound shapes route to the prelude-defined
        // generic structs/enum so `xs.len()`, `opt.is_some()`, `d.len()`,
        // `r.len()` resolve through the same machinery as user types.
        // See `src/prelude.fv`. Optional is checked first so a bare
        // `opt.is_some()` (without auto-strip) lands on Optional, not on
        // its inner T.
        // Indeterminate receivers (`SemType::Unknown` or types
        // containing `Unknown` anywhere) skip method validation;
        // there's nothing to check until inference resolves them.
        if receiver_sem.is_indeterminate() {
            return;
        }
        let receiver_type = match &receiver_sem {
            crate::semantic::sem_type::SemType::Optional(_) => "Optional".to_string(),
            crate::semantic::sem_type::SemType::Array(_) => "Array".to_string(),
            crate::semantic::sem_type::SemType::Dictionary { .. } => "Dictionary".to_string(),
            crate::semantic::sem_type::SemType::Primitive(_)
            | crate::semantic::sem_type::SemType::Named(_)
            | crate::semantic::sem_type::SemType::Tuple(_)
            | crate::semantic::sem_type::SemType::Generic { .. }
            | crate::semantic::sem_type::SemType::Closure { .. }
            | crate::semantic::sem_type::SemType::Unknown
            | crate::semantic::sem_type::SemType::InferredEnum
            | crate::semantic::sem_type::SemType::Nil => receiver_sem.display(),
        };
        if let Some(fn_def) = Self::find_method_fn_def(&receiver_type, &method.name, file) {
            let params = fn_def.params.clone();
            self.validate_fn_param_conventions_receiver(receiver, &params, span, file);
            self.validate_fn_param_conventions_args(&params, args, span, file);
        } else if self.method_exists_on_type(&receiver_type, &method.name, file) {
            // Method exists in a trait/impl block; convention checks on
            // those signatures still happen via `find_method_fn_def`
            // when the impl is in-file; cross-module impls are accepted
            // without further checks here.
        } else if self.struct_field_is_closure(&receiver_type, &method.name, file) {
            // Calling a closure-typed field of a struct: `f.onPress()`
            // where `onPress: () -> E`. The convention checks for the
            // closure's own params live in the closure-binding maps,
            // populated when the field was registered.
        } else {
            self.errors.push(CompilerError::UndefinedReference {
                name: format!("method '{}' on type '{}'", method.name, receiver_type),
                span,
            });
        }
    }

    /// Find the `FnDef` for `method_name` on the given type by scanning the file's impl blocks.
    fn find_method_fn_def<'f>(
        type_name: &str,
        method_name: &str,
        file: &'f File,
    ) -> Option<&'f crate::ast::FnDef> {
        for stmt in &file.statements {
            if let crate::ast::Statement::Definition(def) = stmt {
                if let crate::ast::Definition::Impl(impl_def) = &**def {
                    if impl_def.name.name == type_name {
                        for func in &impl_def.functions {
                            if func.name.name == method_name {
                                return Some(func);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Check `mut self` / `sink self` convention against the receiver expression.
    fn validate_fn_param_conventions_receiver(
        &mut self,
        receiver: &Expr,
        params: &[crate::ast::FnParam],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        let Some(self_param) = params.iter().find(|p| p.name.name == "self") else {
            return;
        };
        match self_param.convention {
            ParamConvention::Mut => {
                if !self.is_expr_mutable(receiver, file) {
                    self.errors.push(CompilerError::MutabilityMismatch {
                        param: "self".to_string(),
                        span,
                    });
                }
            }
            ParamConvention::Sink => {
                if let Some(root) = Self::root_binding(receiver) {
                    self.consumed_bindings.insert(root);
                }
            }
            ParamConvention::Let => {}
        }
    }

    /// Check `mut` / `sink` conventions on non-self parameters using AST `FnParam` directly.
    fn validate_fn_param_conventions_args(
        &mut self,
        params: &[crate::ast::FnParam],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        let non_self: Vec<_> = params.iter().filter(|p| p.name.name != "self").collect();
        for (i, (label_opt, arg_expr)) in args.iter().enumerate() {
            let param = label_opt.as_ref().map_or_else(
                || non_self.get(i).copied(),
                |label| {
                    non_self
                        .iter()
                        .find(|p| {
                            p.external_label
                                .as_ref()
                                .is_some_and(|l| l.name == label.name)
                                || p.name.name == label.name
                        })
                        .copied()
                },
            );
            if let Some(param) = param {
                if param.convention == ParamConvention::Mut && !self.is_expr_mutable(arg_expr, file)
                {
                    self.errors.push(CompilerError::MutabilityMismatch {
                        param: param.name.name.clone(),
                        span,
                    });
                }
                if param.convention == ParamConvention::Sink {
                    if let Some(root) = Self::root_binding(arg_expr) {
                        self.consumed_bindings.insert(root);
                    }
                    // Escape analysis: sink-passed closure carries its captures away.
                    self.escape_closure_value(arg_expr);
                }
            }
        }
    }

    /// Enforce closure param conventions at a call site where the callee is a closure binding.
    pub(super) fn validate_closure_call_conventions(
        &mut self,
        conventions: &[crate::ast::ParamConvention],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        for (i, (_, arg_expr)) in args.iter().enumerate() {
            let Some(&convention) = conventions.get(i) else {
                break;
            };
            match convention {
                ParamConvention::Mut => {
                    if !self.is_expr_mutable(arg_expr, file) {
                        self.errors.push(CompilerError::MutabilityMismatch {
                            param: format!("arg{i}"),
                            span,
                        });
                    }
                }
                ParamConvention::Sink => {
                    if let Some(root) = Self::root_binding(arg_expr) {
                        self.consumed_bindings.insert(root);
                    }
                    // Escape analysis: sink-passed closure carries its captures away.
                    self.escape_closure_value(arg_expr);
                }
                ParamConvention::Let => {}
            }
        }
    }
}
