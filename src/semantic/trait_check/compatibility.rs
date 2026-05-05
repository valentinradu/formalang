use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::Type;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Check if two type strings are compatible.
    ///
    /// Handles exact matches and `.variant(...)` inferred enum syntax.
    /// neither side gets a wildcard
    /// "Unknown" pass any more. Inference now resolves match-arm
    /// pattern bindings and impl-static / enum-constructor calls, so
    /// `Unknown` in inference output is genuinely an error signal.
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
    /// 1. It's a struct that implements the trait (via : Trait or impl Trait for Struct)
    /// 2. It's an enum that implements the trait
    /// 3. It's a type parameter that has the constraint in scope
    pub(in crate::semantic) fn type_satisfies_trait_constraint(
        &self,
        ty: &Type,
        trait_name: &str,
    ) -> bool {
        match ty {
            Type::Ident(ident) => {
                // Check trait impls (impl Trait for Struct)
                let all_traits = self.symbols.get_all_traits_for_struct(&ident.name);
                if all_traits.contains(&trait_name.to_string()) {
                    return true;
                }
                // Check if enum implements the trait
                let enum_traits = self.symbols.get_all_traits_for_enum(&ident.name);
                if enum_traits.contains(&trait_name.to_string()) {
                    return true;
                }
                false
            }
            Type::Generic { name, .. } => {
                // For generic types, check if the base type (struct or enum)
                // implements the trait. Generic arg bounds are validated at
                // their respective definition site.
                let trait_key = trait_name.to_string();
                let struct_traits = self.symbols.get_all_traits_for_struct(&name.name);
                if struct_traits.contains(&trait_key) {
                    return true;
                }
                let enum_traits = self.symbols.get_all_traits_for_enum(&name.name);
                enum_traits.contains(&trait_key)
            }
            // Primitives, arrays, optionals, tuples, etc. don't implement user-defined traits
            Type::Primitive(_)
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::Closure { .. } => false,
        }
    }
}

/// If `ty` is the shape `[T]`, return `T`. Rejects `[K: V]` (dictionary).
///
/// depth-tracks brackets so a nested array of dicts
/// `[[K: V]]` is recognised as an array and returns `[K: V]`.
fn strip_array_shape(ty: &str) -> Option<&str> {
    crate::semantic::strip_array_type(ty)
}
