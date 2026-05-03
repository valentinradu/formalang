//! Function-call invocation: generic-arity / constraint validation, then
//! single-overload or overload-resolution dispatch with closure-binding
//! fallback for callable values.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use crate::ast::File;
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a function call invocation, performing overload resolution when multiple
    /// overloads exist for the same name.
    #[expect(
        clippy::too_many_lines,
        reason = "covers generic-arity checks, overload resolution, closure binding checks (conventions + captures) — splitting hurts readability"
    )]
    pub(super) fn validate_expr_invocation_function(
        &mut self,
        name: &str,
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        span: Span,
        file: &File,
    ) {
        // Validate generic type arguments against the function's generic parameters
        if !type_args.is_empty() {
            let simple_name_for_lookup = name.rsplit("::").next().unwrap_or(name);
            let overloads_for_generics = {
                let direct = self.symbols.get_function_overloads(name);
                if direct.is_empty() {
                    self.symbols.get_function_overloads(simple_name_for_lookup)
                } else {
                    direct
                }
            };
            let func_generics = overloads_for_generics
                .first()
                .map(|f| f.generics.clone())
                .unwrap_or_default();

            if func_generics.is_empty() {
                self.errors.push(CompilerError::GenericArityMismatch {
                    name: name.to_string(),
                    expected: 0,
                    actual: type_args.len(),
                    span,
                });
            } else if type_args.len() != func_generics.len() {
                self.errors.push(CompilerError::GenericArityMismatch {
                    name: name.to_string(),
                    expected: func_generics.len(),
                    actual: type_args.len(),
                    span,
                });
            } else {
                // Validate each type arg satisfies constraints
                for (type_arg, generic_param) in type_args.iter().zip(func_generics.iter()) {
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
            }
        }

        let simple_name = name.rsplit("::").next().unwrap_or(name);
        let overloads: &[_] = {
            let direct = self.symbols.get_function_overloads(name);
            if direct.is_empty() {
                self.symbols.get_function_overloads(simple_name)
            } else {
                direct
            }
        };

        match overloads.len() {
            0 => {
                // Check if this is a closure binding call — enforce closure param conventions
                let closure_conventions =
                    self.closure_binding_conventions.get(simple_name).cloned();
                if let Some(conventions) = closure_conventions {
                    // Before applying param conventions (which may mark new bindings
                    // as consumed), check if any captured binding has already been
                    // consumed — that's an after-the-fact use-after-sink via the
                    // closure.
                    if let Some(captures) = self.closure_binding_captures.get(simple_name).cloned()
                    {
                        for captured in &captures {
                            if self.consumed_bindings.contains(captured) {
                                self.errors.push(CompilerError::UseAfterSink {
                                    name: captured.clone(),
                                    span,
                                });
                            }
                        }
                    }
                    self.validate_closure_call_conventions(&conventions, args, span, file);
                } else if !self.resolve_qualified_function(name) {
                    // a missing function is an undefined
                    // reference, not an undefined type — use the correct
                    // error variant so downstream tooling can distinguish
                    // the two cases.
                    self.errors.push(CompilerError::UndefinedReference {
                        name: name.to_string(),
                        span,
                    });
                }
            }
            1 => {
                // Single overload — check mut param mutability
                if let Some(info) = overloads.first() {
                    let params = info.params.clone();
                    self.validate_mut_param_args(&params, args, span, file);
                }
            }
            _ => {
                // Multiple overloads: resolve by argument labels or first-arg type
                let call_labels: Vec<Option<String>> = args
                    .iter()
                    .map(|(label, _)| label.as_ref().map(|l| l.name.clone()))
                    .collect();

                let matching: Vec<_> = overloads
                    .iter()
                    .filter(|overload| self.overload_matches(overload, &call_labels, args, file))
                    .collect();

                // DP-3: most-specific wins under defaults. When several
                // overloads pass the broadened arity check, prefer the
                // one whose `non_self_count - args.len()` is smallest
                // (i.e., fewest default values fired). Ties at the same
                // gap fall through to the existing ambiguous-call path.
                let min_gap: Option<usize> = matching
                    .iter()
                    .map(|overload| {
                        let non_self = overload
                            .params
                            .iter()
                            .filter(|p| p.name.name != "self")
                            .count();
                        non_self.saturating_sub(args.len())
                    })
                    .min();
                let most_specific: Vec<_> = min_gap.map_or_else(
                    || matching.clone(),
                    |g| {
                        matching
                            .iter()
                            .copied()
                            .filter(|overload| {
                                let non_self = overload
                                    .params
                                    .iter()
                                    .filter(|p| p.name.name != "self")
                                    .count();
                                non_self.saturating_sub(args.len()) == g
                            })
                            .collect()
                    },
                );

                match most_specific.len() {
                    0 => {
                        self.errors.push(CompilerError::NoMatchingOverload {
                            function: name.rsplit("::").next().unwrap_or(name).to_string(),
                            span,
                        });
                    }
                    1 => {
                        // Resolved to a unique overload — check mut param mutability
                        if let Some(info) = most_specific.first() {
                            let params = info.params.clone();
                            self.validate_mut_param_args(&params, args, span, file);
                        }
                    }
                    _ => {
                        self.errors.push(CompilerError::AmbiguousCall {
                            function: name.rsplit("::").next().unwrap_or(name).to_string(),
                            span,
                        });
                    }
                }
            }
        }
    }
}
