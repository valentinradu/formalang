//! Conversion between an AST [`Type`] and a [`SemType`]. Split out of
//! `mod.rs` to keep each file under the line ceiling that
//! `scripts/check_file_sizes.sh` enforces.

use super::{primitive_from_name, SemType};
use crate::ast::{Ident, PrimitiveType, TupleField, Type};
use crate::location::Span;

impl SemType {
    /// Convert an AST [`Type`] node into a structural [`SemType`].
    /// Mirrors `trait_check::type_to_string` exactly so the two stay
    /// observationally interchangeable through the migration.
    pub(in crate::semantic) fn from_ast(ty: &Type) -> Self {
        match ty {
            Type::Primitive(p) => Self::Primitive(*p),
            Type::Ident(ident) => primitive_from_name(&ident.name)
                .map_or_else(|| Self::Named(ident.name.clone()), Self::Primitive),
            Type::Array(inner) => Self::Array(Box::new(Self::from_ast(inner))),
            Type::Optional(inner) => Self::Optional(Box::new(Self::from_ast(inner))),
            Type::Tuple(fields) => Self::Tuple(
                fields
                    .iter()
                    .map(|f| (f.name.name.clone(), Self::from_ast(&f.ty)))
                    .collect(),
            ),
            // The built-in carriers have two spellings each:
            // `Dictionary<K, V>` and `[K: V]`, `Array<T>` and `[T]`,
            // `Optional<T>` and `T?`. They name one type, so they
            // become one `SemType` here. Keeping them apart made the
            // two halves of the language disagree: `[F64: I32]` was
            // rejected as a float key while `Dictionary<F64, I32>` was
            // not, `d["k"]` on the generic spelling was "not
            // indexable", and `let d: Dictionary<String, I32> = [:]`
            // was a type mismatch against its own value.
            Type::Generic { name, args, .. } => {
                let lowered: Vec<Self> = args.iter().map(Self::from_ast).collect();
                match (name.name.as_str(), lowered.as_slice()) {
                    ("Array", [element]) => Self::array_of(element.clone()),
                    ("Optional", [inner]) => Self::optional_of(inner.clone()),
                    ("Dictionary", [key, value]) => Self::dictionary(key.clone(), value.clone()),
                    _ => Self::Generic {
                        base: name.name.clone(),
                        args: lowered,
                    },
                }
            }
            Type::Dictionary { key, value } => Self::Dictionary {
                key: Box::new(Self::from_ast(key)),
                value: Box::new(Self::from_ast(value)),
            },
            Type::Closure { params, ret } => Self::Closure {
                params: params
                    .iter()
                    .map(|(c, p)| (*c, Self::from_ast(p)))
                    .collect(),
                return_ty: Box::new(Self::from_ast(ret)),
            },
        }
    }

    /// Convert back to an AST [`Type`], so IR lowering can resolve it
    /// with `lower_type`. Every name gets `span`.
    ///
    /// Returns `None` for an indeterminate type.
    pub(crate) fn to_ast(&self, span: Span) -> Option<Type> {
        Some(match self {
            Self::Primitive(p) => Type::Primitive(*p),
            Self::Named(name) => primitive_from_name(name).map_or_else(
                || Type::Ident(Ident::new(name.clone(), span)),
                Type::Primitive,
            ),
            Self::Array(inner) => Type::Array(Box::new(inner.to_ast(span)?)),
            Self::Optional(inner) => Type::Optional(Box::new(inner.to_ast(span)?)),
            Self::Tuple(fields) => Type::Tuple(
                fields
                    .iter()
                    .map(|(name, ty)| {
                        Some(TupleField {
                            name: Ident::new(name.clone(), span),
                            ty: ty.to_ast(span)?,
                            span,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
            ),
            Self::Generic { base, args } if args.is_empty() => {
                Type::Ident(Ident::new(base.clone(), span))
            }
            Self::Generic { base, args } => Type::Generic {
                name: Ident::new(base.clone(), span),
                args: args
                    .iter()
                    .map(|a| a.to_ast(span))
                    .collect::<Option<Vec<_>>>()?,
                span,
            },
            Self::Dictionary { key, value } => Type::Dictionary {
                key: Box::new(key.to_ast(span)?),
                value: Box::new(value.to_ast(span)?),
            },
            Self::Closure { params, return_ty } => Type::Closure {
                params: params
                    .iter()
                    .map(|(c, p)| Some((*c, p.to_ast(span)?)))
                    .collect::<Option<Vec<_>>>()?,
                ret: Box::new(return_ty.to_ast(span)?),
            },
            // `nil` has the type `Optional<Never>`.
            Self::Nil => Type::Optional(Box::new(Type::Primitive(PrimitiveType::Never))),
            Self::Unknown | Self::InferredEnum => return None,
        })
    }
}
