//! Invocation validation: struct instantiation, function calls (single +
//! overload resolution), closure-binding calls, and the `mod::item` module
//! visibility check used at every qualified call/reference site.

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Check module visibility for a multi-segment path (`mod::item`,
    /// `outer::inner::item`, etc.).
    ///
    /// Walks the full module path, checking:
    /// 1. Each intermediate module segment must be `pub` to be accessible
    ///    across module boundaries.
    /// 2. The final item must be `pub` when accessed across any module boundary.
    ///
    /// Returns true if access is allowed, false if a `VisibilityViolation`
    /// was emitted.
    pub(super) fn check_module_visibility(
        &mut self,
        path: &[crate::ast::Ident],
        span: Span,
    ) -> bool {
        let Some((first, rest)) = path.split_first() else {
            return true;
        };
        if rest.is_empty() {
            return true;
        }
        let Some(root_module) = self.symbols.modules.get(first.name.as_str()) else {
            return true;
        };
        // Walk intermediate modules (all rest segments except the last).
        // Each intermediate module must itself be `pub`.
        let mut current = &root_module.symbols;
        let Some((item_ident, middle)) = rest.split_last() else {
            return true;
        };
        for seg in middle {
            let name = seg.name.as_str();
            let Some(next) = current.modules.get(name) else {
                // Unknown module: leave error reporting to the caller.
                return true;
            };
            if matches!(next.visibility, crate::ast::Visibility::Private) {
                self.errors.push(CompilerError::VisibilityViolation {
                    name: name.to_string(),
                    span,
                });
                return false;
            }
            current = &next.symbols;
        }
        // Final segment is the item name
        let item_name = item_ident.name.as_str();
        let item_visibility = current
            .structs
            .get(item_name)
            .map(|s| s.visibility)
            .or_else(|| {
                current
                    .functions
                    .get(item_name)
                    .and_then(|overloads| overloads.first().map(|f| f.visibility))
            })
            .or_else(|| current.enums.get(item_name).map(|e| e.visibility))
            .or_else(|| current.traits.get(item_name).map(|t| t.visibility))
            .or_else(|| current.lets.get(item_name).map(|l| l.visibility))
            .or_else(|| current.modules.get(item_name).map(|m| m.visibility));

        if matches!(item_visibility, Some(crate::ast::Visibility::Private)) {
            self.errors.push(CompilerError::VisibilityViolation {
                name: item_name.to_string(),
                span,
            });
            return false;
        }
        true
    }

    /// Validate an invocation expression (struct instantiation or function call)
    pub(super) fn validate_expr_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");

        for (_, arg_expr) in args {
            self.validate_expr(arg_expr, file);
        }
        for type_arg in type_args {
            self.validate_type(type_arg);
        }

        // Check module visibility for qualified paths (mod::item)
        if !self.check_module_visibility(path, span) {
            return;
        }

        let is_struct = self.symbols.get_struct_qualified(&name).is_some();
        if is_struct {
            self.validate_expr_invocation_struct(&name, type_args, args, span, file);
        } else {
            self.validate_expr_invocation_function(&name, type_args, args, span, file);
        }
    }

    /// Validate a struct instantiation invocation
    fn validate_expr_invocation_struct(
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

    /// Validate a function call invocation, performing overload resolution when multiple
    /// overloads exist for the same name.
    #[expect(
        clippy::too_many_lines,
        reason = "covers generic-arity checks, overload resolution, closure binding checks (conventions + captures) — splitting hurts readability"
    )]
    fn validate_expr_invocation_function(
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

                match matching.len() {
                    0 => {
                        self.errors.push(CompilerError::NoMatchingOverload {
                            function: name.rsplit("::").next().unwrap_or(name).to_string(),
                            span,
                        });
                    }
                    1 => {
                        // Resolved to a unique overload — check mut param mutability
                        if let Some(info) = matching.first() {
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

    /// For each `mut`-convention parameter, verify the corresponding call argument is mutable.
    fn validate_mut_param_args(
        &mut self,
        params: &[crate::semantic::symbol_table::ParamInfo],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
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
                        .map(|v| &**v)
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
                    // Escape analysis: a closure value passed to a sink param
                    // escapes with its captures — mark them consumed.
                    self.escape_closure_value(arg_expr);
                }
            }
        }
    }

    /// Check whether a single overload matches the given call arguments.
    ///
    /// Resolution order:
    /// 1. If all call arguments have labels, match by label set.
    /// 2. If no call arguments have labels, try to match by first-argument type.
    fn overload_matches(
        &self,
        overload: &crate::semantic::symbol_table::FunctionInfo,
        call_labels: &[Option<String>],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        file: &File,
    ) -> bool {
        let params = &overload.params;
        // Collect overload parameter labels (external_label if set, else param name)
        let param_labels: Vec<String> = params
            .iter()
            .filter(|p| p.name.name != "self")
            .map(|p| {
                p.external_label
                    .as_ref()
                    .map_or_else(|| p.name.name.clone(), |l| l.name.clone())
            })
            .collect();

        let all_labeled = call_labels.iter().all(Option::is_some);
        let none_labeled = call_labels.iter().all(Option::is_none);

        if all_labeled && !call_labels.is_empty() {
            // Mode A: match by label set
            let call_label_set: Vec<&str> =
                call_labels.iter().filter_map(|l| l.as_deref()).collect();
            let param_label_set: Vec<&str> = param_labels.iter().map(String::as_str).collect();
            call_label_set == param_label_set
        } else if none_labeled && args.is_empty() {
            // Zero-arg call: match only zero-arg overloads.
            // Without context-type disambiguation (e.g., from a let annotation),
            // multiple zero-arg overloads will be reported as AmbiguousCall by the
            // caller. This is the scope-limited behavior — see Fix 6 notes.
            params.iter().filter(|p| p.name.name != "self").count() == 0
        } else if none_labeled && !args.is_empty() {
            // Mode B: arity check first, then match by first-argument type
            let non_self_count = params.iter().filter(|p| p.name.name != "self").count();
            if args.len() != non_self_count {
                return false;
            }

            let first_arg_sem = args.first().map_or(SemType::Unknown, |(_, expr)| {
                self.infer_type_sem(expr, file)
            });

            let first_param_type = params
                .iter()
                .find(|p| p.name.name != "self")
                .and_then(|p| p.ty.as_ref())
                .map_or_else(|| "Unknown".to_string(), Self::type_to_string);

            // Unknown means we can't tell — accept it (conservative)
            first_arg_sem.is_unknown()
                || first_param_type == "Unknown"
                || self.type_strings_compatible(&first_param_type, &first_arg_sem.display())
        } else {
            // Mixed labeled/unlabeled args have no defined match — overload
            // resolution is all-labeled (mode A) or all-unlabeled (mode B).
            false
        }
    }

    /// Resolve a qualified function path like `math::compute` by traversing module symbol tables.
    #[expect(clippy::indexing_slicing, reason = "parts length checked above")]
    fn resolve_qualified_function(&self, name: &str) -> bool {
        let parts: Vec<&str> = name.splitn(2, "::").collect();
        if parts.len() != 2 {
            return false;
        }
        let (module_name, rest) = (parts[0], parts[1]);
        if let Some(module_info) = self.symbols.modules.get(module_name) {
            // Recurse into nested module paths
            if rest.contains("::") {
                let parts2: Vec<&str> = rest.splitn(2, "::").collect();
                if parts2.len() == 2 {
                    let (sub_module, fn_name) = (parts2[0], parts2[1]);
                    if let Some(sub_mod) = module_info.symbols.modules.get(sub_module) {
                        return sub_mod.symbols.get_function(fn_name).is_some();
                    }
                }
                false
            } else {
                module_info.symbols.get_function(rest).is_some()
            }
        } else {
            false
        }
    }
}
