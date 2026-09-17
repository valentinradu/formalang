//! Match exhaustiveness and enum-instantiation field checks.

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{Definition, Expr, File, Statement};
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::HashSet;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate match expression exhaustiveness
    pub(super) fn validate_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[crate::ast::MatchArm],
        span: Span,
        file: &File,
    ) {
        // Infer scrutinee type - must be an enum, or an Optional<T>
        // (treated as a synthetic two-variant enum `.some(T)` / `.none`).
        let scrutinee_sem = self.infer_type_sem(scrutinee, file);

        // Skip when type is unknown (field access, method calls — IR lowering handles these)
        if matches!(scrutinee_sem, SemType::Unknown) {
            return;
        }

        // Optional acts as a built-in two-variant enum. The Rust-style
        // `if let pattern = optional { … } else { … }` form parses into
        // a match on `.some(pat)` / `.none`, so the validator treats
        // Optional uniformly with user-defined enums here.
        if matches!(scrutinee_sem, SemType::Optional(_)) {
            let mut variants = std::collections::HashMap::new();
            variants.insert("some".to_string(), (1usize, span));
            variants.insert("none".to_string(), (0usize, span));
            let scrutinee_type = "Optional".to_string();
            return self.validate_match_arms_against_variants(
                &scrutinee_type,
                arms,
                span,
                &variants,
            );
        }

        // The bare type name to look up; for a generic-instantiated enum like
        // `Result<I32, I32>` this peels back to `Result` so the enum lookup
        // and arm-validation succeed against the underlying definition.
        let lookup_name: String = match &scrutinee_sem {
            SemType::Generic { base, .. } => base.clone(),
            SemType::Named(n) => n.clone(),
            SemType::Primitive(_)
            | SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => scrutinee_sem.display(),
        };

        // Check if scrutinee is an enum (look it up in symbol table)
        if !self.symbols.is_enum(&lookup_name) {
            self.errors.push(CompilerError::MatchNotEnum {
                actual: scrutinee_sem.display(),
                span,
            });
            return;
        }

        // Get enum variants from symbol table
        let variants = match self.symbols.get_enum_variants(&lookup_name) {
            Some(v) => v.clone(),
            None => return, // Should not happen if is_enum returned true
        };
        let scrutinee_type = lookup_name;
        self.validate_match_arms_against_variants(&scrutinee_type, arms, span, &variants);
    }

    fn validate_match_arms_against_variants(
        &mut self,
        scrutinee_type: &str,
        arms: &[crate::ast::MatchArm],
        span: Span,
        variants: &std::collections::HashMap<String, (usize, Span)>,
    ) {
        // Collect all variant names from match arms
        let mut covered_variants = HashSet::new();
        let mut has_wildcard = false;
        for arm in arms {
            // A `_` arm takes every value the arms above it did not,
            // so nothing below it can run. Writing `_` first and a
            // variant after it compiled, and the variant's body was
            // silently dead: `match e { _: 0, .a: 1 }` answered 0 for
            // every value, including `.a`.
            if has_wildcard {
                self.errors
                    .push(CompilerError::UnreachableMatchArm { span: arm.span });
                continue;
            }
            match &arm.pattern {
                crate::ast::Pattern::Variant { name, bindings } => {
                    // Check for duplicate arms
                    if !covered_variants.insert(name.name.clone()) {
                        self.errors.push(CompilerError::DuplicateMatchArm {
                            variant: name.name.clone(),
                            span: arm.span,
                        });
                        continue;
                    }

                    // Validate variant exists and arity matches
                    self.validate_match_arm(
                        scrutinee_type,
                        &name.name,
                        bindings.len(),
                        arm.span,
                        variants,
                    );
                }
                crate::ast::Pattern::Wildcard => {
                    // Wildcard covers all remaining variants
                    has_wildcard = true;
                }
            }
        }

        // Check exhaustiveness - all variants must be covered (unless there's a wildcard)
        if !has_wildcard {
            let missing_variants: Vec<String> = variants
                .keys()
                .filter(|v| !covered_variants.contains(*v))
                .cloned()
                .collect();

            if !missing_variants.is_empty() {
                self.errors.push(CompilerError::NonExhaustiveMatch {
                    missing: missing_variants.join(", "),
                    span,
                });
            }
        }
    }

    /// Validate enum instantiation with named parameters
    pub(super) fn validate_enum_instantiation(
        &mut self,
        enum_name: &crate::ast::Ident,
        variant_name: &crate::ast::Ident,
        data: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        // Check if the enum exists
        if !self.symbols.is_enum(&enum_name.name) {
            self.errors.push(CompilerError::UndefinedType {
                name: enum_name.name.clone(),
                span: enum_name.span,
            });
            return;
        }

        // Get the enum definition to access variant field information
        let variant_fields =
            self.get_enum_variant_fields(&enum_name.name, &variant_name.name, file);

        match variant_fields {
            Some(fields) => {
                // Check if variant has no fields but data was provided
                if fields.is_empty() && !data.is_empty() {
                    self.errors.push(CompilerError::EnumVariantWithoutData {
                        variant: variant_name.name.clone(),
                        enum_name: enum_name.name.clone(),
                        span,
                    });
                    return;
                }

                // Check if variant has fields but no data was provided
                if !fields.is_empty() && data.is_empty() {
                    self.errors.push(CompilerError::EnumVariantRequiresData {
                        variant: variant_name.name.clone(),
                        enum_name: enum_name.name.clone(),
                        span,
                    });
                    return;
                }

                // Check that all required fields are provided
                let provided_fields: HashSet<&str> =
                    data.iter().map(|(name, _)| name.name.as_str()).collect();
                let required_fields: HashSet<&str> =
                    fields.iter().map(|f| f.name.name.as_str()).collect();

                // Check for missing fields
                for field in &required_fields {
                    if !provided_fields.contains(field) {
                        self.errors.push(CompilerError::MissingField {
                            field: field.to_string(),
                            type_name: format!("{}.{}", enum_name.name, variant_name.name),
                            span,
                        });
                    }
                }

                // Check for unknown fields
                for (provided_field, _) in data {
                    if !required_fields.contains(provided_field.name.as_str()) {
                        self.errors.push(CompilerError::UnknownField {
                            field: provided_field.name.clone(),
                            type_name: format!("{}.{}", enum_name.name, variant_name.name),
                            span: provided_field.span,
                        });
                    }
                }

                // Check each payload value against the type its field
                // declares. Only the field names were checked before, so
                // `Wrap.one(inner: "text")` against `one(inner: I32)`
                // compiled and the IR carried a string where the variant
                // says an integer.
                let declared_types: Vec<(String, Option<crate::ast::Type>)> = fields
                    .iter()
                    .map(|f| (f.name.name.clone(), Some(f.ty.clone())))
                    .collect();
                // A payload field declared as one of the enum's own
                // generic parameters — `error(err: E)` on
                // `Result<T, E>` — carries no concrete type here.
                // Substitution happens in the IR monomorphisation pass.
                let generic_names: Vec<String> = self
                    .symbols
                    .get_generics(&enum_name.name)
                    .map(|gs| gs.iter().map(|g| g.name.name.clone()).collect())
                    .unwrap_or_default();
                for (provided_field, value) in data {
                    let Some((_, Some(declared_ty))) = declared_types
                        .iter()
                        .find(|(name, _)| name == &provided_field.name)
                    else {
                        continue;
                    };
                    let declared = Self::type_to_string(declared_ty);
                    if super::type_names::type_mentions_any(declared_ty, &generic_names) {
                        continue;
                    }
                    let inferred_sem = self.infer_type_sem(value, file);
                    if !self.value_satisfies_declared(&declared, &inferred_sem) {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: declared,
                            found: inferred_sem.display(),
                            span: value.span(),
                        });
                    }
                }
            }
            None => {
                // Variant doesn't exist
                self.errors.push(CompilerError::UnknownEnumVariant {
                    variant: variant_name.name.clone(),
                    enum_name: enum_name.name.clone(),
                    span: variant_name.span,
                });
            }
        }
    }

    /// Get the field definitions for a specific enum variant
    /// Returns None if the enum or variant doesn't exist
    pub(super) fn get_enum_variant_fields(
        &self,
        enum_name: &str,
        variant_name: &str,
        current_file: &File,
    ) -> Option<Vec<crate::ast::FieldDef>> {
        // First, search in the current file
        for statement in &current_file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Enum(enum_def) = &**def {
                    if enum_def.name.name == enum_name {
                        // Find the variant
                        for variant in &enum_def.variants {
                            if variant.name.name == variant_name {
                                return Some(variant.fields.clone());
                            }
                        }
                        return None; // Variant not found
                    }
                }
            }
        }

        // If not found in current file, search through module cache
        for (file, _) in self.module_cache.values() {
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Enum(enum_def) = &**def {
                        if enum_def.name.name == enum_name {
                            // Find the variant
                            for variant in &enum_def.variants {
                                if variant.name.name == variant_name {
                                    return Some(variant.fields.clone());
                                }
                            }
                            return None; // Variant not found
                        }
                    }
                }
            }
        }
        None // Enum not found
    }

    /// Validate a single match arm
    pub(super) fn validate_match_arm(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        binding_count: usize,
        span: Span,
        variants: &std::collections::HashMap<String, (usize, Span)>,
    ) {
        // Check if variant exists
        match variants.get(variant_name) {
            Some((expected_arity, _)) => {
                // Check arity matches
                if *expected_arity != binding_count {
                    self.errors.push(CompilerError::VariantArityMismatch {
                        variant: variant_name.to_string(),
                        expected: *expected_arity,
                        actual: binding_count,
                        span,
                    });
                }
            }
            None => {
                // Variant doesn't exist in enum
                self.errors.push(CompilerError::UnknownEnumVariant {
                    variant: variant_name.to_string(),
                    enum_name: enum_name.to_string(),
                    span,
                });
            }
        }
    }
}
