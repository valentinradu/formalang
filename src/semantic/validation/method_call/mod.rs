//! Method-call validation: receiver/argument convention checks plus method
//! existence lookup across local impls, trait impls, generic constraints,
//! cached modules, and qualified-type module paths.

mod lookup;

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};
use crate::error::CompilerError;
use crate::location::Span;
use crate::semantic::inference::substitute_all;

/// A method call: its receiver, its method name, and its arguments.
pub(super) type MethodCallParts<'a> = (
    &'a Expr,
    &'a crate::ast::Ident,
    &'a [(Option<crate::ast::Ident>, Expr)],
);

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a method call expression
    ///
    /// `key_reported` is true when the expected type of the call holds
    /// a float dictionary key that its annotation reports already.
    pub(super) fn validate_expr_method_call(
        &mut self,
        (receiver, method, args): MethodCallParts<'_>,
        span: Span,
        key_reported: bool,
        file: &File,
    ) {
        self.validate_expr(receiver, file);
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
        let receiver_type =
            (!receiver_sem.is_indeterminate()).then(|| Self::method_receiver_name(&receiver_sem));
        // A method overloads by the shape of the call. Check the call
        // against the overload it fits; only when none fits is
        // anything wrong, and then the first is what the message
        // names. The arguments take their expected types from the
        // same choice.
        let overloads = receiver_type.as_ref().map_or_else(Vec::new, |name| {
            Self::find_method_overloads(name, &method.name, file)
        });
        let chosen = Self::choose_method_overload(&overloads, args);
        let expected = self.method_argument_types(&receiver_sem, chosen, args, file);
        for ((_, arg), arg_expected) in args.iter().zip(expected) {
            self.validate_expr_expecting(arg, arg_expected, file);
        }
        let Some(receiver_type) = receiver_type else {
            return;
        };
        if let (Some(chosen), false) = (chosen, key_reported) {
            self.check_method_float_key(&receiver_sem, chosen, args, span, file);
        }
        if let Some((fn_def, impl_generics)) = chosen {
            let params = fn_def.params.clone();
            // A parameter type that names a type parameter of the impl
            // or of the method itself has no concrete type to compare.
            let generics: Vec<_> = impl_generics
                .iter()
                .chain(&fn_def.generics)
                .cloned()
                .collect();
            self.validate_fn_param_conventions_receiver(receiver, &params, span, file);
            let mut accesses = self.validate_fn_param_conventions_args(&params, args, span, file);
            // The receiver is an argument too, with the convention of
            // `self`. A method with no `self` takes no receiver.
            if let Some(self_param) = params.iter().find(|p| p.name.name == "self") {
                accesses.push((self_param.convention, receiver));
            }
            self.validate_exclusive_access(&accesses, span);
            // A method call checked its argument labels and conventions
            // but never their types, so `h.takes(p: "text")` against
            // `fn takes(self, p: I32)` compiled. Same rule as a free
            // function call.
            let views: Vec<_> = params
                .iter()
                .map(crate::semantic::validation::invocation::overloads::ParamView::of_fn_param)
                .collect();
            self.validate_arg_types(&views, &generics, args, file);
            if let Some((expected, actual)) = Self::method_arity_mismatch(&params, args) {
                self.errors.push(CompilerError::ArgumentCountMismatch {
                    callee: format!("Method '{}'", method.name),
                    expected,
                    actual,
                    span,
                });
            }
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

    /// The name of the type whose impl blocks hold the methods of a
    /// receiver of type `receiver_sem`.
    ///
    /// The four built-in compound shapes route to the prelude-defined
    /// generic structs and enum. A generic receiver is named by its
    /// base, because the impl blocks are declared against the bare
    /// name, not against `Seq<I32>`.
    pub(in crate::semantic::validation) fn method_receiver_name(
        receiver_sem: &crate::semantic::sem_type::SemType,
    ) -> String {
        match receiver_sem {
            crate::semantic::sem_type::SemType::Optional(_) => "Optional".to_string(),
            crate::semantic::sem_type::SemType::Array(_) => "Array".to_string(),
            crate::semantic::sem_type::SemType::Dictionary { .. } => "Dictionary".to_string(),
            // A generic receiver is named by its base. `display()`
            // renders it with its arguments — `Seq<I32>`, `Box<I32>` —
            // and the impl blocks are declared against the bare name,
            // so matching on the rendered form found nothing and every
            // check below was skipped. `s.collect(1, 2, 3)` and
            // `b.get(99, 100)` both compiled.
            crate::semantic::sem_type::SemType::Generic { base, .. } => base.clone(),
            crate::semantic::sem_type::SemType::Primitive(_)
            | crate::semantic::sem_type::SemType::Named(_)
            | crate::semantic::sem_type::SemType::Tuple(_)
            | crate::semantic::sem_type::SemType::Closure { .. }
            | crate::semantic::sem_type::SemType::Unknown
            | crate::semantic::sem_type::SemType::InferredEnum
            | crate::semantic::sem_type::SemType::Nil => receiver_sem.display(),
        }
    }

    /// The type arguments of a receiver, in the order its type declares
    /// its parameters: `[I32]` for `Seq<I32>` and for `[I32]`, and the
    /// key and value types for a dictionary.
    pub(in crate::semantic::validation) fn receiver_type_arguments(
        receiver_sem: &crate::semantic::sem_type::SemType,
    ) -> Vec<crate::semantic::sem_type::SemType> {
        use crate::semantic::sem_type::SemType;
        match receiver_sem {
            SemType::Generic { args, .. } => args.clone(),
            SemType::Array(inner) | SemType::Optional(inner) => vec![(**inner).clone()],
            SemType::Dictionary { key, value } => vec![(**key).clone(), (**value).clone()],
            SemType::Primitive(_)
            | SemType::Named(_)
            | SemType::Tuple(_)
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => Vec::new(),
        }
    }

    /// The expected type of each argument of a method call: the
    /// declared type of the parameter it fills, in `chosen`, the
    /// overload that the call fits. `Some(SemType::Unknown)` for each
    /// argument when the method is not known here.
    ///
    /// A type parameter of the method itself takes the type that the
    /// other arguments give it: `initial: 0` makes `A` in
    /// `fold<A>(initial: A, f: (A, T) -> A)` an `I32`, so the closure's
    /// parameters are `I32` too.
    fn method_argument_types(
        &self,
        receiver_sem: &crate::semantic::sem_type::SemType,
        chosen: Option<(&crate::ast::FnDef, &[crate::ast::GenericParam])>,
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> Vec<Option<crate::semantic::sem_type::SemType>> {
        let Some((fn_def, impl_generics)) = chosen else {
            return vec![Some(crate::semantic::sem_type::SemType::Unknown); args.len()];
        };
        let views: Vec<_> = fn_def
            .params
            .iter()
            .map(crate::semantic::validation::invocation::overloads::ParamView::of_fn_param)
            .collect();
        // The receiver's type arguments replace its type parameters,
        // so `(T, T) -> T` on a `Seq<I32>` is `(I32, I32) -> I32`. A bare
        // `impl Box { ... }` gets the names from the type's declaration.
        let receiver_args = Self::receiver_type_arguments(receiver_sem);
        let (view, fresh) = self.method_view(
            &Self::method_receiver_name(receiver_sem),
            impl_generics,
            &receiver_args,
            &fn_def.generics,
        );
        // A name that no argument binds stays: the closure check reports
        // an untyped parameter that it types.
        let outer = |ty| substitute_all(&ty, &view);
        let bindings = self.bind_call_generics(&fresh, &views, &outer, args, file);
        args.iter()
            .enumerate()
            .map(|(i, (label, _))| {
                let declared = Self::expected_argument(&views, i, label.as_ref());
                Some(substitute_all(&substitute_all(&declared, &view), &bindings))
            })
            .collect()
    }

    /// The overload of a method that a call means.
    ///
    /// The rule is `crate::ir::overload::choose`, the one IR lowering
    /// uses: of the overloads whose parameters take the call's labels
    /// and count, the one that leaves the fewest parameters to their
    /// defaults. When none fits, the first that takes the count, then
    /// the first, so a diagnostic names the likeliest one.
    pub(in crate::semantic) fn choose_method_overload<'f>(
        overloads: &[(&'f crate::ast::FnDef, &'f [crate::ast::GenericParam])],
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> Option<(&'f crate::ast::FnDef, &'f [crate::ast::GenericParam])> {
        let labels: Vec<Option<String>> = args
            .iter()
            .map(|(label, _)| label.as_ref().map(|l| l.name.clone()))
            .collect();
        crate::ir::overload::choose(
            overloads.iter().enumerate(),
            |(fn_def, _)| fn_def.params.as_slice(),
            &labels,
            args.len(),
        )
        .and_then(|index| overloads.get(index))
        .or_else(|| {
            overloads
                .iter()
                .find(|(fn_def, _)| Self::method_arity_mismatch(&fn_def.params, args).is_none())
        })
        .or_else(|| overloads.first())
        .copied()
    }

    /// Whether a method call gives the wrong number of arguments.
    ///
    /// Returns `Some((expected, actual))` when it does. `expected` is
    /// the number of parameters, less `self`; a parameter with a
    /// default value may be left out, so a call between the required
    /// count and the full count is correct.
    ///
    /// Nothing checked this before. A free function call gets its
    /// arity from overload resolution, which rejects a call that fits
    /// no overload. A method call had no equivalent, so
    /// `P(x: 1).add()` against `fn add(self, n: I32)` compiled and
    /// lowered to a call with no value for `n`.
    fn method_arity_mismatch(
        params: &[crate::ast::FnParam],
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> Option<(usize, usize)> {
        let non_self: Vec<_> = params.iter().filter(|p| p.name.name != "self").collect();
        let required = non_self.iter().filter(|p| p.default.is_none()).count();
        let total = non_self.len();

        if args.len() < required {
            // Name the number the call has to reach. Reporting the full
            // count instead named a number the call was never obliged
            // to give, because the parameters past `required` all carry
            // a default.
            Some((required, args.len()))
        } else if args.len() > total {
            Some((total, args.len()))
        } else {
            None
        }
    }

    /// Every method of that name on the type, in source order.
    ///
    /// A type may declare two methods of one name that differ in how
    /// many arguments they take, the way a free function may. Taking
    /// the first match and checking the call against it rejected a
    /// call that meant the second.
    pub(in crate::semantic) fn find_method_overloads<'f>(
        type_name: &str,
        method_name: &str,
        file: &'f File,
    ) -> Vec<(&'f crate::ast::FnDef, &'f [crate::ast::GenericParam])> {
        /// Push the methods called `method_name` of each impl for
        /// `type_name` in `defs`, and in each module nested inside. An
        /// impl in an inline `mod` names its type without the prefix.
        fn collect<'f>(
            defs: impl Iterator<Item = &'f crate::ast::Definition>,
            type_name: &str,
            method_name: &str,
            out: &mut Vec<(&'f crate::ast::FnDef, &'f [crate::ast::GenericParam])>,
        ) {
            for def in defs {
                match def {
                    crate::ast::Definition::Impl(impl_def) if impl_def.name.name == type_name => {
                        for func in &impl_def.functions {
                            if func.name.name == method_name {
                                out.push((func, impl_def.generics.as_slice()));
                            }
                        }
                    }
                    crate::ast::Definition::Module(m) => {
                        collect(m.definitions.iter(), type_name, method_name, out);
                    }
                    crate::ast::Definition::Impl(_)
                    | crate::ast::Definition::Trait(_)
                    | crate::ast::Definition::Struct(_)
                    | crate::ast::Definition::Enum(_)
                    | crate::ast::Definition::Function(_) => {}
                }
            }
        }
        let mut out = Vec::new();
        let top_level = file.statements.iter().filter_map(|stmt| match stmt {
            crate::ast::Statement::Definition(def) => Some(&**def),
            crate::ast::Statement::Use(_) | crate::ast::Statement::Let(_) => None,
        });
        collect(top_level, type_name, method_name, &mut out);
        out
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
    ///
    /// Returns each argument with the convention of the parameter it
    /// fills, for the exclusive-access check. An argument that fills no
    /// parameter goes in as `Let`.
    fn validate_fn_param_conventions_args<'a>(
        &mut self,
        params: &[crate::ast::FnParam],
        args: &'a [(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) -> Vec<(crate::ast::ParamConvention, &'a Expr)> {
        use crate::ast::ParamConvention;
        let non_self: Vec<_> = params.iter().filter(|p| p.name.name != "self").collect();
        let mut accesses = Vec::new();
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
        accesses
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
        let accesses: Vec<(ParamConvention, &Expr)> = args
            .iter()
            .enumerate()
            .map(|(i, (_, arg_expr))| (conventions.get(i).copied().unwrap_or_default(), arg_expr))
            .collect();
        self.validate_exclusive_access(&accesses, span);
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
