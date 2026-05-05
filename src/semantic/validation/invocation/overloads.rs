//! Overload-resolution helpers shared by the function-call validator: per-
//! overload match testing, mutable-argument convention checks, and qualified-
//! path lookup through nested module symbol tables.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::sem_type::SemType;
use super::super::super::SemanticAnalyzer;
use crate::ast::File;
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// For each `mut`-convention parameter, verify the corresponding call argument is mutable.
    pub(super) fn validate_mut_param_args(
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
    pub(super) fn overload_matches(
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
            // Mode A: match by label set, accepting omitted parameters
            // when they have defaults. Required = labels without defaults.
            // The call's labels must be a subset of param_labels covering
            // every required label.
            let call_label_set: std::collections::HashSet<&str> =
                call_labels.iter().filter_map(|l| l.as_deref()).collect();
            let required_labels: std::collections::HashSet<&str> = params
                .iter()
                .filter(|p| p.name.name != "self" && p.default.is_none())
                .map(|p| {
                    p.external_label
                        .as_ref()
                        .map_or(p.name.name.as_str(), |l| l.name.as_str())
                })
                .collect();
            let param_label_set: std::collections::HashSet<&str> =
                param_labels.iter().map(String::as_str).collect();
            // Every call label must exist on the param; every required
            // label must be present in the call.
            call_label_set.iter().all(|l| param_label_set.contains(l))
                && required_labels.iter().all(|l| call_label_set.contains(l))
        } else if none_labeled && args.is_empty() {
            // Zero-arg call: matches a zero-required-arg overload. With
            // default values, an overload with all defaults (e.g.
            // `fn f(x: I32 = 0)`) also matches a zero-arg call.
            // Without context-type disambiguation (e.g., from a let
            // annotation), multiple zero-required-arg overloads will be
            // reported as AmbiguousCall by the caller.
            let required = params
                .iter()
                .filter(|p| p.name.name != "self" && p.default.is_none())
                .count();
            required == 0
        } else if none_labeled && !args.is_empty() {
            // Mode B: arity range check first, then match by first-argument type.
            // Defaults broaden the acceptable arity to [required, total].
            let non_self_count = params.iter().filter(|p| p.name.name != "self").count();
            let required = params
                .iter()
                .filter(|p| p.name.name != "self" && p.default.is_none())
                .count();
            if args.len() < required || args.len() > non_self_count {
                return false;
            }

            let first_arg_sem = args.first().map_or(SemType::Unknown, |(_, expr)| {
                self.infer_type_sem(expr, file)
            });

            let first_param_sem = params
                .iter()
                .find(|p| p.name.name != "self")
                .and_then(|p| p.ty.as_ref())
                .map_or(SemType::Unknown, SemType::from_ast);

            // Indeterminate either side means we can't tell — accept it
            // (conservative). `is_unknown` returns true only for the bare
            // `SemType::Unknown` variant; deeper compound types
            // containing `Unknown` are caught by `is_indeterminate`.
            first_arg_sem.is_indeterminate()
                || first_param_sem.is_indeterminate()
                || self
                    .type_strings_compatible(&first_param_sem.display(), &first_arg_sem.display())
        } else {
            // Mixed labeled/unlabeled args have no defined match — overload
            // resolution is all-labeled (mode A) or all-unlabeled (mode B).
            false
        }
    }

    /// Resolve a qualified function path like `math::compute` by traversing module symbol tables.
    #[expect(clippy::indexing_slicing, reason = "parts length checked above")]
    pub(super) fn resolve_qualified_function(&self, name: &str) -> bool {
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
