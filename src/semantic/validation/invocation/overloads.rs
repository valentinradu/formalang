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
        let mut accesses: Vec<(ParamConvention, &crate::ast::Expr)> = Vec::new();
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
            accesses.push((
                param.map_or(ParamConvention::Let, |p| p.convention),
                arg_expr,
            ));
            if let Some(param) = param {
                if param.convention == ParamConvention::Mut && !self.is_expr_mutable(arg_expr, file)
                {
                    self.errors.push(CompilerError::MutabilityMismatch {
                        param: param.name.name.clone(),
                        span,
                    });
                } else if param.convention == ParamConvention::Mut
                    && Self::root_binding(arg_expr)
                        .is_some_and(|root| self.is_closure_capture(&root))
                {
                    // A closure is pure: it does not change a binding
                    // that it captures.
                    self.errors.push(CompilerError::MutabilityMismatch {
                        param: param.name.name.clone(),
                        span,
                    });
                }
                if param.convention == ParamConvention::Sink {
                    if let Some(root) = Self::root_binding(arg_expr) {
                        self.check_sink_owner(&root, span);
                        self.consumed_bindings.insert(root);
                    }
                    // Escape analysis: a closure value passed to a sink param
                    // escapes with its captures — mark them consumed.
                    self.escape_closure_value(arg_expr);
                }
            }
        }
        self.validate_exclusive_access(&accesses, span);
    }

    /// Check that the code under check owns `root`, the binding that a
    /// `sink` argument gives away.
    ///
    /// A plain or `mut` parameter, and a field of a plain or `mut`
    /// `self`, belong to the caller. A module `let` belongs to every
    /// function. A binding from outside a loop body or a closure body
    /// would be given away once per pass.
    fn check_sink_owner(&mut self, root: &str, span: Span) {
        use crate::ast::ParamConvention;
        if let Some(convention) = self.current_fn_param_conventions.get(root) {
            if *convention != ParamConvention::Sink {
                self.errors.push(CompilerError::MutabilityMismatch {
                    param: root.to_string(),
                    span,
                });
                return;
            }
        } else if !self.in_module_let
            && !self.local_let_bindings.contains_key(root)
            && !self.names_a_local_binding(root)
            && self.symbols.is_let(root)
        {
            self.errors.push(CompilerError::UseAfterSink {
                name: root.to_string(),
                span,
            });
            return;
        }
        if self.sink_repeats(root) {
            self.errors.push(CompilerError::UseAfterSink {
                name: root.to_string(),
                span,
            });
        }
    }

    /// True when `name` is a loop variable, a closure parameter or a
    /// pattern binding in scope.
    fn names_a_local_binding(&self, name: &str) -> bool {
        self.loop_var_scopes.iter().any(|s| s.contains_key(name))
            || self.closure_param_scopes.iter().any(|s| s.contains(name))
            || self
                .inference_scope_stack
                .borrow()
                .iter()
                .any(|s| s.contains_key(name))
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
        substituted: &[Option<SemType>],
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
            // The type of the parameter with the type arguments of the
            // call put in: a written `<I32>`, or the type that another
            // argument gives. `id<I32>(x: "s")` and `same(a: 1, b: "x")`
            // compare against `I32`.
            let concrete = substituted.get(i).cloned().flatten().filter(|t| {
                !t.is_indeterminate() && !crate::semantic::inference::holds_an_unbound_type_param(t)
            });
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
                } else if let Some(concrete) = concrete {
                    if !inferred_sem.is_indeterminate()
                        && inferred_sem != concrete
                        && !self.value_satisfies_declared(&concrete.display(), &inferred_sem)
                    {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: concrete.display(),
                            found: inferred_sem.display(),
                            span: arg_expr.span(),
                        });
                    }
                }
                continue;
            }

            let inferred_sem = self.infer_type_sem(arg_expr, file);
            if !self.value_satisfies_declared(&declared, &inferred_sem) {
                let error = self.value_mismatch(declared, &inferred_sem, arg_expr.span());
                self.errors.push(error);
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
            // The number of parameters is part of the outer shape.
            Type::Closure { params, .. } => matches!(
                inferred,
                SemType::Closure { params: found, .. } if found.len() == params.len()
            ),
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
    pub(in crate::semantic::validation) fn check_generic_bounds(
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
                name: trait_ref,
                args: trait_args,
            } = constraint;
            if !self.sem_type_satisfies_trait(&inferred, &trait_ref.name, trait_args) {
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
    fn sem_type_satisfies_trait(
        &self,
        ty: &SemType,
        trait_name: &str,
        trait_args: &[crate::ast::Type],
    ) -> bool {
        let name = match ty {
            SemType::Named(name) => name.clone(),
            SemType::Generic { base, .. } => base.clone(),
            SemType::Primitive(p) => format!("{p:?}"),
            SemType::Unknown | SemType::InferredEnum | SemType::Nil => return true,
            SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. } => return false,
        };
        self.implements_trait(&name, trait_name, trait_args)
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
    /// True when the parameter declares a default value.
    pub has_default: bool,
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
            has_default: p.default.is_some(),
        }
    }

    /// The view of an AST parameter.
    pub(in crate::semantic) fn of_fn_param(p: &'a crate::ast::FnParam) -> Self {
        Self {
            name: p.name.name.as_str(),
            external_label: p.external_label.as_ref().map(|l| l.name.as_str()),
            ty: p.ty.as_ref(),
            has_default: p.default.is_some(),
        }
    }
}
