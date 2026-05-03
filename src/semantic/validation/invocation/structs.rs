//! Struct-instantiation invocation: positional-arg rejection, generic-arity
//! and constraint checks, plus field-shape / mutability validation.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a struct instantiation invocation
    pub(super) fn validate_expr_invocation_struct(
        &mut self,
        name: &str,
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        let named_args: Vec<(crate::ast::Ident, Expr)> = args
            .iter()
            .filter_map(|(name_opt, expr)| name_opt.as_ref().map(|n| (n.clone(), expr.clone())))
            .collect();

        for (i, (name_opt, arg_expr)) in args.iter().enumerate() {
            if name_opt.is_none() {
                self.errors.push(CompilerError::PositionalArgInStruct {
                    struct_name: name.to_string(),
                    position: i.saturating_add(1),
                    span: arg_expr.span(),
                });
            }
        }

        if let Some(expected_params) = self.symbols.get_generics(name) {
            let expected = expected_params.len();
            let actual = type_args.len();
            if expected == actual {
                // Validate each type arg satisfies its constraints
                for (type_arg, generic_param) in type_args.iter().zip(expected_params.iter()) {
                    for constraint in &generic_param.constraints {
                        let crate::ast::GenericConstraint::Trait {
                            name: trait_ref, ..
                        } = constraint;
                        if !self.type_satisfies_trait_constraint(type_arg, &trait_ref.name) {
                            self.errors.push(CompilerError::GenericConstraintViolation {
                                arg: Self::type_to_string(type_arg),
                                constraint: trait_ref.name.clone(),
                                span,
                            });
                        }
                    }
                }
            } else if actual == 0 && expected > 0 {
                self.errors.push(CompilerError::MissingGenericArguments {
                    name: name.to_string(),
                    span,
                });
            } else {
                self.errors.push(CompilerError::GenericArityMismatch {
                    name: name.to_string(),
                    expected,
                    actual,
                    span,
                });
            }
        } else if !type_args.is_empty() {
            self.errors.push(CompilerError::GenericArityMismatch {
                name: name.to_string(),
                expected: 0,
                actual: type_args.len(),
                span,
            });
        }

        self.validate_struct_fields(name, &named_args, span, file);
        self.validate_struct_mutability(name, &named_args, file, span);
    }
}
