//! Enum instantiations: the full form `E.v(...)` and the dot form
//! `.v(...)`.
//!
//! The expected type of the position gives the enum of the dot form,
//! and the type arguments of a generic enum: `Result.ok(value: "s")` in
//! a position that expects `Result<String, I32>` is a
//! `Result<String, I32>`. Each payload is checked against its field
//! type with those arguments put in, and the analyzer records the full
//! type of the instantiation, so an array or a field of it fits.

use super::super::super::literal_types::node_key;
use super::super::super::module_resolver::ModuleResolver;
use super::super::super::sem_type::SemType;
use super::super::super::SemanticAnalyzer;
use crate::ast::{Expr, File, Ident};
use crate::error::CompilerError;
use crate::location::Span;

/// The parts of `E.v(data)`: the enum, the variant and the payload.
pub(super) type EnumParts<'a> = (&'a Ident, &'a Ident, &'a [(Ident, Expr)]);

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Check the type arguments that a variant path writes, as in
    /// `Maybe<I32>.none`: each one must be a type, and their count must
    /// be the count of the enum's type parameters.
    ///
    /// Returns the type that the path writes, for the payload checks.
    /// A position that expects another instance of the enum is a
    /// mismatch.
    pub(super) fn validate_enum_path_type_args(
        &mut self,
        enum_name: &str,
        type_args: &[crate::ast::Type],
        span: crate::location::Span,
        expected: Option<&SemType>,
    ) -> Option<SemType> {
        if type_args.is_empty() {
            return None;
        }
        for ty in type_args {
            self.validate_type(ty, span);
        }
        let arity = self
            .symbols
            .get_generics(enum_name)
            .map_or(0, |generics| generics.len());
        if arity != type_args.len() {
            self.errors.push(CompilerError::GenericArityMismatch {
                name: enum_name.to_string(),
                expected: arity,
                actual: type_args.len(),
                span,
            });
            return None;
        }
        let written = SemType::Generic {
            base: enum_name.to_string(),
            args: type_args.iter().map(SemType::from_ast).collect(),
        };
        if let Some(want @ SemType::Generic { base, .. }) = expected {
            if base == enum_name && !want.is_indeterminate() && want != &written {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: want.display(),
                    found: written.display(),
                    span,
                });
            }
        }
        Some(written)
    }

    /// Validate `enum_name.variant(data)`.
    pub(super) fn validate_full_enum_expr(
        &mut self,
        expr: &Expr,
        (enum_name, variant, data): EnumParts<'_>,
        span: Span,
        expected: Option<&SemType>,
        file: &File,
    ) {
        let type_args = expected.and_then(|e| Self::enum_type_args(e, &enum_name.name));
        self.validate_payloads(&enum_name.name, variant, data, type_args.as_deref(), file);
        self.validate_enum_instantiation(
            enum_name,
            variant,
            data,
            span,
            type_args.as_deref(),
            file,
        );
        if let Some(args) = type_args {
            self.expr_types.insert(
                node_key(expr),
                SemType::Generic {
                    base: enum_name.name.clone(),
                    args,
                },
            );
        }
    }

    /// Validate `.variant(data)`.
    pub(super) fn validate_dot_enum_expr(
        &mut self,
        expr: &Expr,
        variant: &Ident,
        data: &[(Ident, Expr)],
        span: Span,
        expected: Option<&SemType>,
        file: &File,
    ) {
        let enum_name = expected.and_then(Self::expected_enum_name);
        let Some(name) = enum_name
            .clone()
            .filter(|n| self.symbols.get_enum_qualified(n).is_some())
        else {
            // No known enum: each payload still gets its checks.
            for (_, data_expr) in data {
                self.validate_expr_expecting(data_expr, Some(SemType::Unknown), file);
            }
            // A position that expects a type that is not an enum cannot
            // hold a variant: `.apply(x: 1)` where a `String` goes.
            if let Some(want) = expected.filter(|t| !t.is_indeterminate()) {
                let names_a_struct = enum_name
                    .as_deref()
                    .is_some_and(|n| self.symbols.get_struct_qualified(n).is_some());
                if enum_name.is_none() || names_a_struct {
                    self.errors.push(CompilerError::TypeMismatch {
                        expected: want.display(),
                        found: format!(".{}", variant.name),
                        span,
                    });
                }
            }
            return;
        };
        let type_args = expected.and_then(|e| Self::enum_type_args(e, &name));
        self.validate_payloads(&name, variant, data, type_args.as_deref(), file);
        // With a known enum, the dot form takes the checks of the full
        // form: the variant exists, and the payload fits it.
        let enum_ident = Ident::new(name.clone(), variant.span);
        self.validate_enum_instantiation(
            &enum_ident,
            variant,
            data,
            span,
            type_args.as_deref(),
            file,
        );
        let ty = match type_args {
            Some(args) => SemType::Generic { base: name, args },
            None => SemType::Named(name),
        };
        self.expr_types.insert(node_key(expr), ty);
    }

    /// Validate each payload value with the type of its field as the
    /// expected type.
    fn validate_payloads(
        &mut self,
        enum_name: &str,
        variant: &Ident,
        data: &[(Ident, Expr)],
        type_args: Option<&[SemType]>,
        file: &File,
    ) {
        // A payload field may be given once.
        let mut given = std::collections::HashSet::new();
        for (field, _) in data {
            if !given.insert(field.name.as_str()) {
                self.errors
                    .push(crate::error::CompilerError::DuplicateDefinition {
                        name: format!("payload field '{}'", field.name),
                        span: field.span,
                    });
            }
        }
        for (field, data_expr) in data {
            let declared = self.expected_payload(Some(enum_name), &variant.name, &field.name);
            let field_expected = self.with_enum_type_args(enum_name, declared, type_args);
            self.validate_expr_expecting(data_expr, Some(field_expected), file);
        }
    }

    /// `ty` with the type parameters of the enum `enum_name` read as
    /// `type_args` give them.
    pub(in crate::semantic::validation) fn with_enum_type_args(
        &self,
        enum_name: &str,
        ty: SemType,
        type_args: Option<&[SemType]>,
    ) -> SemType {
        let Some(args) = type_args else {
            return ty;
        };
        let generics = self.symbols.get_generics(enum_name).unwrap_or_default();
        generics
            .iter()
            .zip(args)
            .fold(ty, |acc, (g, arg)| acc.substitute_named(&g.name.name, arg))
    }

    /// The type arguments that `expected` gives the enum `enum_name`:
    /// `[String, I32]` for `Result<String, I32>`, also inside an
    /// optional.
    fn enum_type_args(expected: &SemType, enum_name: &str) -> Option<Vec<SemType>> {
        match expected {
            SemType::Optional(inner) => Self::enum_type_args(inner, enum_name),
            SemType::Generic { base, args } if base == enum_name => Some(args.clone()),
            SemType::Generic { .. }
            | SemType::Primitive(_)
            | SemType::Named(_)
            | SemType::Array(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => None,
        }
    }
}
