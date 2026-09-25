//! Function-call invocation: generic-arity / constraint validation, then
//! single-overload or overload-resolution dispatch with closure-binding
//! fallback for callable values.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::sem_type::SemType;
use super::super::super::SemanticAnalyzer;
use super::overloads::ParamView;
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
        expected: &[Option<SemType>],
        span: Span,
        file: &File,
    ) {
        // Validate generic type arguments against the function's generic parameters
        if !type_args.is_empty() {
            let func_generics = self
                .function_overloads(name)
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
                            name: trait_ref,
                            args: trait_args,
                        } = constraint;
                        if !self.type_satisfies_trait_constraint(
                            type_arg,
                            &trait_ref.name,
                            trait_args,
                        ) {
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

        // A label may appear once, whatever the callee is.
        if !self.check_repeated_labels(args) {
            return;
        }
        let simple_name = name.rsplit("::").next().unwrap_or(name);
        let overloads = self.function_overloads(name);

        match overloads.len() {
            0 => {
                // A call through a binding that holds a closure. Its
                // type gives the parameter conventions and the shape.
                if let Some(SemType::Closure { params, .. }) = self.lookup_closure_type(simple_name)
                {
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
                    let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
                    self.validate_closure_call_conventions(&conventions, args, span, file);
                    self.validate_closure_call_shape(&params, args, span, file);
                } else if let Some(SemType::Optional(inner)) = self
                    .lookup_binding_type(simple_name)
                    .filter(|ty| matches!(ty, SemType::Optional(inner) if matches!(**inner, SemType::Closure { .. })))
                {
                    // A closure that may be absent must be unwrapped
                    // before the call.
                    self.errors.push(CompilerError::OptionalUsedAsNonOptional {
                        actual: SemType::Optional(inner.clone()).display(),
                        expected: inner.display(),
                        span,
                    });
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
                // Single overload — check the shape of the call, then
                // argument types and mut params.
                if let Some(info) = overloads.first() {
                    let params = info.params.clone();
                    let generics = info.generics.clone();
                    let views: Vec<_> = params.iter().map(ParamView::of_param_info).collect();
                    let callee = format!("Function '{simple_name}'");
                    self.check_type_params_given(
                        simple_name,
                        &params,
                        &generics,
                        type_args,
                        args,
                        span,
                    );
                    if self.validate_call_shape(&callee, simple_name, &views, args, span) {
                        self.validate_mut_param_args(&params, args, span, file);
                        self.validate_arg_types(&views, &generics, args, expected, file);
                    }
                }
            }
            _ => {
                // Multiple overloads: resolve by argument labels or first-arg type
                let most_specific = self.most_specific_overloads(&overloads, args, file);

                match most_specific.len() {
                    0 => {
                        self.errors.push(CompilerError::NoMatchingOverload {
                            function: name.rsplit("::").next().unwrap_or(name).to_string(),
                            span,
                        });
                    }
                    1 => {
                        // Resolved to a unique overload — check argument
                        // types and mut params.
                        if let Some(info) = most_specific.first() {
                            let index = overloads.iter().position(|o| std::ptr::eq(o, *info));
                            let params = info.params.clone();
                            let generics = info.generics.clone();
                            if let Some(index) = index {
                                self.symbols
                                    .record_overload_choice(span, simple_name, index);
                            }
                            self.check_type_params_given(
                                simple_name,
                                &params,
                                &generics,
                                type_args,
                                args,
                                span,
                            );
                            self.validate_mut_param_args(&params, args, span, file);
                            let views: Vec<_> =
                                params.iter().map(ParamView::of_param_info).collect();
                            self.validate_arg_types(&views, &generics, args, expected, file);
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

    /// Report a type parameter of a free function that the call gives
    /// no type.
    ///
    /// A call gives a type parameter its type through `<...>`, or
    /// through an argument whose parameter type mentions it. A
    /// parameter with a default that the call leaves out gives no type.
    fn check_type_params_given(
        &mut self,
        function: &str,
        params: &[crate::semantic::symbol_table::ParamInfo],
        generics: &[crate::ast::GenericParam],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        span: Span,
    ) {
        if !type_args.is_empty() {
            return;
        }
        let given = |param: &crate::semantic::symbol_table::ParamInfo| {
            param.default.is_none()
                || args.iter().any(|(label, _)| {
                    label.as_ref().is_some_and(|l| {
                        param.external_label.as_ref().unwrap_or(&param.name).name == l.name
                    })
                })
        };
        for generic in generics {
            let name = std::slice::from_ref(&generic.name.name);
            let mentioned = params.iter().any(|p| {
                given(p)
                    && p.ty
                        .as_ref()
                        .is_some_and(|ty| super::super::type_names::type_mentions_any(ty, name))
            });
            if !mentioned {
                self.errors
                    .push(CompilerError::UninferableMethodTypeParameter {
                        param: generic.name.name.clone(),
                        method: function.to_string(),
                        span,
                    });
            }
        }
    }

    /// Check a call through a closure-typed binding against the shape
    /// the closure declares: how many arguments it takes, and what
    /// type each one has.
    ///
    /// Nothing did this before. A direct call to a named function goes
    /// through overload resolution, which checks the arity, and then
    /// through
    /// [`super::overloads::SemanticAnalyzer::validate_arg_types`],
    /// which checks the types. A call through a binding took neither
    /// path: only the parameter conventions were checked. So
    /// `f(1, 2)` against `f: (I32) -> I32` compiled, and the lowered
    /// call carried a second argument into a closure with one
    /// parameter.
    pub(in crate::semantic::validation) fn validate_closure_call_shape(
        &mut self,
        params: &[(crate::ast::ParamConvention, SemType)],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        span: Span,
        file: &File,
    ) {
        // A closure type has no parameter names, so an argument label
        // names nothing.
        for label in args.iter().filter_map(|(label, _)| label.as_ref()) {
            self.errors.push(CompilerError::LabelledClosureArgument {
                label: label.name.clone(),
                span: label.span,
            });
        }
        if args.len() != params.len() {
            self.errors.push(CompilerError::ArgumentCountMismatch {
                callee: "This closure".to_string(),
                expected: params.len(),
                actual: args.len(),
                span,
            });
            return;
        }

        for ((_, arg_expr), (_, declared)) in args.iter().zip(params.iter()) {
            // A parameter the closure left untyped takes anything:
            // `(x) -> x + 1` states no type, so there is nothing to
            // check against.
            if declared.is_indeterminate() {
                continue;
            }
            let inferred = self.infer_type_sem(arg_expr, file);
            if !self.value_satisfies_declared(&declared.display(), &inferred) {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: declared.display(),
                    found: inferred.display(),
                    span: arg_expr.span(),
                });
            }
        }
    }
}
