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

    /// Check each call argument against the type its parameter
    /// declares.
    ///
    /// Nothing did this before: a call site checked argument labels,
    /// arity, mutability and sink conventions, but never the types.
    /// `g(n: "text")` against `fn g(n: I32)` compiled, and the lowered
    /// `IrExpr::FunctionCall` carried a `String` argument into a slot
    /// the callee reads as `I32`.
    ///
    /// The comparison mirrors the one on struct fields in
    /// [`super::super::SemanticAnalyzer::validate_struct_fields`], and
    /// skips the same three shapes:
    ///
    /// - a declared type that names one of the function's own generic
    ///   parameters, because substitution happens in the IR
    ///   monomorphisation pass, not in this string comparison;
    /// - `nil` against an optional parameter, `T` against `T?`, and a
    ///   generic base whose arguments inference did not carry;
    /// - an argument whose inferred type is indeterminate, because
    ///   reporting one there would be a guess.
    pub(in crate::semantic) fn validate_arg_types(
        &mut self,
        params: &[ParamView<'_>],
        generics: &[crate::ast::GenericParam],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        file: &File,
    ) {
        let generic_names: Vec<String> = generics.iter().map(|g| g.name.name.clone()).collect();
        let non_self: Vec<_> = params.iter().filter(|p| p.name != "self").collect();

        for (i, (label_opt, arg_expr)) in args.iter().enumerate() {
            let param = label_opt.as_ref().map_or_else(
                || non_self.get(i).copied(),
                |label| {
                    non_self
                        .iter()
                        .find(|p| {
                            p.external_label.is_some_and(|l| l == label.name)
                                || p.name == label.name
                        })
                        .map(|v| &**v)
                },
            );
            let Some(param) = param else { continue };
            let Some(declared_ty) = param.ty else {
                continue;
            };

            let declared = Self::type_to_string(declared_ty);
            if super::super::type_names::type_mentions_any(declared_ty, &generic_names) {
                // The declared type names one of the callee's own
                // generic parameters, so there is no concrete type to
                // compare against. The trait bounds on that parameter
                // still apply: the argument chooses the type, and a
                // type that does not implement the bound reaches
                // monomorphisation with a method it does not have.
                if let Some(generic) = generics.iter().find(|g| g.name.name == declared) {
                    self.check_generic_bounds(generic, arg_expr, file);
                }
                // The parameter's own shape is still known even when
                // its type arguments are not: `(T) -> T` is a closure
                // whatever `T` turns out to be, so an argument that is
                // not a closure cannot fit it. Without this,
                // `s.map(f: 1)` against `fn map(f: (T) -> T)` compiled,
                // because naming a generic parameter skipped the check
                // entirely.
                let inferred_sem = self.infer_type_sem(arg_expr, file);
                if !Self::outer_shapes_agree(declared_ty, &inferred_sem) {
                    self.errors.push(CompilerError::TypeMismatch {
                        expected: declared,
                        found: inferred_sem.display(),
                        span: arg_expr.span(),
                    });
                }
                continue;
            }

            let inferred_sem = self.infer_type_sem(arg_expr, file);
            if !self.value_satisfies_declared(&declared, &inferred_sem) {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: declared,
                    found: inferred_sem.display(),
                    span: arg_expr.span(),
                });
            }
        }
    }

    /// Whether a declared type and an inferred one have the same outer
    /// shape.
    ///
    /// The comparison stops at the outermost constructor, so it says
    /// nothing about what a container holds. That is the point: it
    /// runs where the declared type names a generic parameter, so the
    /// contents are not yet known, but the shape around them is.
    ///
    /// An indeterminate argument agrees with everything — an unknown
    /// type is reported on its own, and guessing here would add noise.
    fn outer_shapes_agree(declared: &crate::ast::Type, inferred: &SemType) -> bool {
        use crate::ast::Type;

        if inferred.is_indeterminate() {
            return true;
        }
        match declared {
            Type::Closure { .. } => matches!(inferred, SemType::Closure { .. }),
            Type::Array(_) => matches!(inferred, SemType::Array(_)),
            Type::Dictionary { .. } => matches!(inferred, SemType::Dictionary { .. }),
            Type::Tuple(_) => matches!(inferred, SemType::Tuple(_)),
            // `T?` takes the value as well as the optional, and a bare
            // `T`, a named type and a primitive say nothing about the
            // shape once a parameter is involved.
            Type::Optional(_) | Type::Ident(_) | Type::Generic { .. } | Type::Primitive(_) => true,
        }
    }

    /// Check the argument that fixes a generic parameter against the
    /// trait bounds that parameter declares.
    ///
    /// A call site that writes the type argument (`total<Plain>(...)`)
    /// was already checked in
    /// [`super::functions`]. A call site that leaves the type argument
    /// to inference — the common shape — was not, so
    /// `total(item: Plain(n: 1))` against `fn total<T: Shape>(item: T)`
    /// compiled, and the lowered IR carried a method call with a
    /// vtable slot index for a trait `Plain` does not implement.
    fn check_generic_bounds(
        &mut self,
        generic: &crate::ast::GenericParam,
        arg_expr: &crate::ast::Expr,
        file: &File,
    ) {
        if generic.constraints.is_empty() {
            return;
        }

        let inferred = self.infer_type_sem(arg_expr, file);
        if inferred.is_indeterminate() {
            return;
        }

        for constraint in &generic.constraints {
            let crate::ast::GenericConstraint::Trait {
                name: trait_ref, ..
            } = constraint;
            if !self.sem_type_satisfies_trait(&inferred, &trait_ref.name) {
                self.errors.push(CompilerError::GenericConstraintViolation {
                    arg: inferred.display(),
                    constraint: trait_ref.name.clone(),
                    span: arg_expr.span(),
                });
            }
        }
    }

    /// Whether an inferred type implements the named trait.
    ///
    /// A type the analyser could not pin down satisfies every bound:
    /// the unknown type is reported on its own, and a second
    /// diagnostic for it would be a guess. A name that is a generic
    /// parameter in scope satisfies the bound when its own
    /// constraints list the trait, which is what lets one bounded
    /// function call another.
    fn sem_type_satisfies_trait(&self, ty: &SemType, trait_name: &str) -> bool {
        let name = match ty {
            SemType::Named(name) => name,
            SemType::Generic { base, .. } => base,
            SemType::Unknown | SemType::InferredEnum | SemType::Nil => return true,
            SemType::Primitive(_)
            | SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. } => return false,
        };

        // A generic parameter that is still in scope carries its own
        // bounds, so `fn outer<U: Shape>(x: U)` may pass `x` on to
        // `fn inner<T: Shape>(item: T)`.
        if self
            .generic_scopes
            .iter()
            .filter_map(|scope| scope.params.get(name.as_str()))
            .any(|constraints| constraints.iter().any(|c| c == trait_name))
        {
            return true;
        }

        let wanted = trait_name.to_string();
        self.symbols
            .get_all_traits_for_struct(name)
            .contains(&wanted)
            || self.symbols.get_all_traits_for_enum(name).contains(&wanted)
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

/// One parameter's call-site identity and declared type.
///
/// A free function's parameters arrive as `ParamInfo` from the symbol
/// table and a method's as `FnParam` from the AST. The type check is
/// the same either way, so both build this view rather than the check
/// being written twice.
#[derive(Clone, Copy, Debug)]
pub(in crate::semantic) struct ParamView<'a> {
    pub name: &'a str,
    pub external_label: Option<&'a str>,
    pub ty: Option<&'a crate::ast::Type>,
}

impl<'a> ParamView<'a> {
    /// The view of a symbol-table parameter.
    pub(in crate::semantic) fn of_param_info(
        p: &'a crate::semantic::symbol_table::ParamInfo,
    ) -> Self {
        Self {
            name: p.name.name.as_str(),
            external_label: p.external_label.as_ref().map(|l| l.name.as_str()),
            ty: p.ty.as_ref(),
        }
    }

    /// The view of an AST parameter.
    pub(in crate::semantic) fn of_fn_param(p: &'a crate::ast::FnParam) -> Self {
        Self {
            name: p.name.name.as_str(),
            external_label: p.external_label.as_ref().map(|l| l.name.as_str()),
            ty: p.ty.as_ref(),
        }
    }
}
