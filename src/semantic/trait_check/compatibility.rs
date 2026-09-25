use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::Type;
use crate::semantic::SemType;

/// The inside of a `[...]` type, or `None` when the string is not one.
fn bracketed(ty: &str) -> Option<&str> {
    ty.strip_prefix('[')?.strip_suffix(']')
}

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Check if two type strings are compatible.
    ///
    /// Handles exact matches and `.variant(...)` inferred enum syntax.
    /// neither side gets a wildcard
    /// "Unknown" pass any more. Inference now resolves match-arm
    /// pattern bindings and impl-static / enum-constructor calls, so
    /// `Unknown` in inference output is genuinely an error signal.
    /// Whether a value of the inferred type satisfies a declaration of
    /// type `declared`.
    ///
    /// This is the one rule the analyser applies wherever a value
    /// meets a declared type — a `let` annotation, a call argument, a
    /// function or closure return, an assignment, an array element, a
    /// dictionary value. Those sites used to each decide for
    /// themselves, and they disagreed: `let v: I32? = nil` was
    /// accepted while `pub fn f() -> I32? { nil }` was not, and
    /// assignment applied none of the optional rules at all.
    ///
    /// Four things count as satisfying a declaration:
    ///
    /// - an inferred type that is indeterminate, because reporting a
    ///   mismatch there would be a guess;
    /// - `nil`, against any optional declaration;
    /// - `T` against `T?`, which is the implicit wrap;
    /// - a generic base whose arguments inference did not carry — see
    ///   [`Self::base_without_type_arguments`].
    ///
    /// Anything else falls through to
    /// [`Self::type_strings_compatible`].
    ///
    /// The caller still chooses the diagnostic: a `nil` against a
    /// non-optional reports
    /// [`CompilerError::NilAssignedToNonOptional`](crate::CompilerError::NilAssignedToNonOptional),
    /// not a bare type mismatch.
    pub(in crate::semantic) fn value_satisfies_declared(
        &self,
        declared: &str,
        inferred: &SemType,
    ) -> bool {
        if inferred.is_indeterminate() {
            return true;
        }

        if matches!(inferred, SemType::Nil) {
            return declared.ends_with('?');
        }

        let found = inferred.display();

        if declared.ends_with('?') && declared.trim_end_matches('?') == found.as_str() {
            return true;
        }

        if Self::generic_arguments_fit(declared, &found) {
            return true;
        }

        if Self::base_without_type_arguments(declared, &found) {
            return true;
        }

        // A literal holding nothing but `nil` — `[nil]`, `["k": nil]` —
        // joins to an element type of `Nil`, which names no concrete
        // type. It fits any container of an optional.
        if Self::nil_container_fits(declared, &found) {
            return true;
        }

        self.type_strings_compatible(declared, &found)
    }

    /// Whether a container of `T` or of `Nil` fits the same container
    /// declared over `T?`.
    ///
    /// The implicit wrap that lets `let a: I32? = 3` work, one level
    /// down. `[nil]` infers `[Nil]` and satisfies `[I32?]`, because
    /// `nil` is a value of every optional type; `[3]` infers `[I32]`
    /// and satisfies `[I32?]` the same way `3` satisfies `I32?`. A
    /// dictionary's values follow: `["k": nil]` and `["k": 1]` both
    /// satisfy `[String: I32?]`.
    ///
    /// Whether the optional was *worth* declaring is a separate
    /// question, and a separate report — see
    /// `check_optional_elements_are_used`.
    fn nil_container_fits(declared: &str, inferred: &str) -> bool {
        let Some(inferred_inner) = bracketed(inferred) else {
            return false;
        };
        let Some(declared_inner) = bracketed(declared) else {
            return false;
        };

        match (
            inferred_inner.split_once(": "),
            declared_inner.split_once(": "),
        ) {
            // Dictionaries: the key types must match and the declared
            // value must be an optional the inferred value fits.
            (Some((inferred_key, inferred_value)), Some((declared_key, declared_value))) => {
                inferred_key == declared_key && fits_optional(declared_value, inferred_value)
            }
            // Arrays: neither side is a dictionary.
            (None, None) => fits_optional(declared_inner, inferred_inner),
            _ => false,
        }
    }

    /// Whether `inferred` is `declared`'s generic base with its type
    /// arguments missing.
    ///
    /// Inference does not always carry a generic's arguments: an enum
    /// literal such as `Result.error(err: -1)` infers as `Result`, not
    /// as `Result<I32, I32>`. Treating that as a mismatch would reject
    /// correct programs, so the annotated-value and call-argument
    /// checks accept it and leave the arguments to the IR
    /// monomorphisation pass.
    ///
    /// This is deliberately one-directional and narrow. It accepts
    /// `Result` against `Result<I32, I32>`; it does not accept
    /// `Result<String, I32>` against `Result<I32, I32>`, nor a
    /// different base.
    /// Whether two instantiations of one generic differ only where a
    /// value widens into an optional.
    ///
    /// `base_without_type_arguments` covers the case where inference
    /// produced the bare name; this covers the case where it produced
    /// an instantiation of its own. Inferring an enum's type arguments
    /// from its payload made that the common case: `Maybe.some(v: 1)`
    /// used to be the bare `Maybe`, which fitted anything, and is now
    /// `Maybe<I32>`, which has to be related to `Maybe<I32?>` by the
    /// same rule that relates `I32` to `I32?` anywhere else.
    fn generic_arguments_fit(declared: &str, found: &str) -> bool {
        let Some((declared_base, declared_args)) = split_instantiation(declared) else {
            return false;
        };
        let Some((found_base, found_args)) = split_instantiation(found) else {
            return false;
        };
        if declared_base != found_base || declared_args.len() != found_args.len() {
            return false;
        }
        declared_args
            .iter()
            .zip(found_args.iter())
            .all(|(want, got)| argument_fits(want, got))
    }

    pub(in crate::semantic) fn base_without_type_arguments(declared: &str, inferred: &str) -> bool {
        let declared_base = declared.trim_end_matches('?');
        let inferred_base = inferred.trim_end_matches('?');
        declared_base
            .split_once('<')
            .is_some_and(|(base, _)| base == inferred_base)
    }

    pub(in crate::semantic) fn type_strings_compatible(
        &self,
        expected: &str,
        actual: &str,
    ) -> bool {
        if expected == actual {
            return true;
        }

        // Concrete-to-trait upcast: `expected` is a trait, `actual` is a
        // struct (or enum) implementing it. Lets a `let s: Shape = ...`
        // accept a `Square` value, and lets two if-branches of distinct
        // concrete types unify against a trait-typed surrounding context
        // (the if-branch checker calls this in both directions).
        if self.symbols.is_trait(expected) {
            let actual_base = actual.trim_end_matches('?');
            let actual_simple = actual_base.split_once('<').map_or(actual_base, |(n, _)| n);
            if self
                .symbols
                .get_all_traits_for_struct(actual_simple)
                .contains(&expected.to_string())
                || self
                    .symbols
                    .get_all_traits_for_enum(actual_simple)
                    .contains(&expected.to_string())
            {
                return true;
            }
        }

        // Two distinct concrete types that share at least one trait are
        // accepted as branch-compatible (used by the if-branch checker
        // when both arms construct different impl types of the same
        // trait, and the surrounding context expects the trait).
        {
            let exp_simple = expected
                .trim_end_matches('?')
                .split_once('<')
                .map_or_else(|| expected.trim_end_matches('?'), |(n, _)| n);
            let act_simple = actual
                .trim_end_matches('?')
                .split_once('<')
                .map_or_else(|| actual.trim_end_matches('?'), |(n, _)| n);
            if self.symbols.is_struct(exp_simple) && self.symbols.is_struct(act_simple) {
                let exp_traits = self.symbols.get_all_traits_for_struct(exp_simple);
                let act_traits = self.symbols.get_all_traits_for_struct(act_simple);
                if exp_traits.iter().any(|t| act_traits.contains(t)) {
                    return true;
                }
            }
        }

        // `.variant(...)` syntax: enum type is inferred from context
        // Strip optional suffix (e.g. "Event?" -> "Event") for the lookup
        if actual == "InferredEnum" {
            let base_expected = expected.trim_end_matches('?');
            if self.symbols.enums.contains_key(base_expected) {
                return true;
            }
        }

        // Array shape: `[T]` vs `[U]` decomposes to `T` vs `U`.
        if let (Some(exp_inner), Some(act_inner)) =
            (strip_array_shape(expected), strip_array_shape(actual))
        {
            return self.type_strings_compatible(exp_inner, act_inner);
        }

        // Optional shape: `T?` vs `U?` decomposes to `T` vs `U`.
        if let (Some(exp_inner), Some(act_inner)) =
            (expected.strip_suffix('?'), actual.strip_suffix('?'))
        {
            return self.type_strings_compatible(exp_inner, act_inner);
        }

        // Closure types: compare structurally, allowing InferredEnum in return position
        // e.g. "() -> InferredEnum" is compatible with "() -> Event?" when Event is an enum
        if let Some(exp_arrow) = expected.rfind(" -> ") {
            if let Some(act_arrow) = actual.rfind(" -> ") {
                let exp_params = &expected[..exp_arrow];
                let act_params = &actual[..act_arrow];
                let exp_ret = &expected[exp_arrow.saturating_add(4)..];
                let act_ret = &actual[act_arrow.saturating_add(4)..];
                if exp_params == act_params {
                    return self.type_strings_compatible(exp_ret, act_ret);
                }
            }
        }

        false
    }

    /// Check if a type satisfies a trait constraint
    ///
    /// A type satisfies a trait constraint if:
    /// 1. It's a struct, an enum or a primitive that implements the
    ///    trait, with the trait arguments that the constraint gives;
    /// 2. It's a type parameter that has the constraint in scope.
    pub(in crate::semantic) fn type_satisfies_trait_constraint(
        &self,
        ty: &Type,
        trait_name: &str,
        trait_args: &[Type],
    ) -> bool {
        let name = match ty {
            Type::Ident(ident) => ident.name.clone(),
            Type::Generic { name, .. } => name.name.clone(),
            Type::Primitive(p) => format!("{p:?}"),
            // Arrays, optionals, tuples, etc. don't implement user-defined traits
            Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::Closure { .. } => return false,
        };
        self.implements_trait(&name, trait_name, trait_args)
    }

    /// True when the type `type_name` implements `trait_name` with the
    /// trait arguments `trait_args`. Empty `trait_args` accept any
    /// arguments. A type parameter in scope implements the traits of
    /// its bounds.
    pub(in crate::semantic) fn implements_trait(
        &self,
        type_name: &str,
        trait_name: &str,
        trait_args: &[Type],
    ) -> bool {
        if self
            .generic_scopes
            .iter()
            .filter_map(|scope| scope.params.get(type_name))
            .any(|constraints| constraints.iter().any(|c| c == trait_name))
        {
            return true;
        }
        let by_impl = self
            .symbols
            .trait_impls
            .get(type_name)
            .is_some_and(|impls| {
                impls.iter().any(|i| {
                    i.trait_name == trait_name && self.trait_args_fit(&i.trait_args, trait_args)
                })
            });
        // `enum E: Trait` declares a trait with no arguments.
        by_impl
            || (trait_args.is_empty()
                && self
                    .symbols
                    .get_all_traits_for_enum(type_name)
                    .iter()
                    .any(|t| t == trait_name))
    }

    /// True when the trait arguments of an impl fit the arguments that a
    /// bound asks for. A bound argument that is a type parameter in
    /// scope fits any argument.
    fn trait_args_fit(&self, implemented: &[Type], wanted: &[Type]) -> bool {
        if wanted.is_empty() {
            return true;
        }
        implemented.len() == wanted.len()
            && implemented.iter().zip(wanted).all(|(have, want)| {
                matches!(want, Type::Ident(i) if self.is_type_parameter(&i.name))
                    || Self::types_match(have, want)
            })
    }
}

/// If `ty` is the shape `[T]`, return `T`. Rejects `[K: V]` (dictionary).
///
/// depth-tracks brackets so a nested array of dicts
/// `[[K: V]]` is recognised as an array and returns `[K: V]`.
fn strip_array_shape(ty: &str) -> Option<&str> {
    crate::semantic::strip_array_type(ty)
}

/// Split `Base<A, B>` into its base and its arguments.
///
/// Splitting on the top-level commas only, so a nested instantiation
/// stays in one piece.
fn split_instantiation(ty: &str) -> Option<(&str, Vec<&str>)> {
    let rest = ty.strip_suffix('>')?;
    let (base, args) = rest.split_once('<')?;

    let mut out = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0_usize;
    for (index, c) in args.char_indices() {
        match c {
            '<' | '[' | '(' => depth = depth.saturating_add(1),
            '>' | ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(args.get(start..index)?.trim());
                start = index.saturating_add(1);
            }
            _ => {}
        }
    }
    out.push(args.get(start..)?.trim());
    Some((base, out))
}

/// Whether one type argument fits where another is declared.
///
/// The same widening the language allows anywhere: a value fits an
/// optional of its own type, and so does `nil`.
fn argument_fits(want: &str, got: &str) -> bool {
    if want == got {
        return true;
    }
    let Some(inner) = want.strip_suffix('?') else {
        return false;
    };
    inner == got || got == "Nil"
}

/// Whether `found` is a value of the optional type `declared`.
///
/// `nil` is a value of every optional; so is the wrapped type itself.
fn fits_optional(declared: &str, found: &str) -> bool {
    let Some(inner) = declared.strip_suffix('?') else {
        return false;
    };
    found == "Nil" || found == inner
}
