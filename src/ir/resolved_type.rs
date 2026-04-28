//! Fully-resolved type representation used by the IR. Unlike AST types,
//! these reference definitions by ID instead of by name.

use crate::ast::{ParamConvention, PrimitiveType};

use super::{EnumId, ImportedKind, IrModule, StructId, TraitId};

/// The target of a [`ResolvedType::Generic`] instantiation — a generic
/// struct, enum, or trait.
///
/// Traits appear here only inside generic constraints (`<T: Foo<X>>`)
/// and impl headers (`impl Foo<X> for Y`); `FormaLang` has no dynamic
/// dispatch, so a trait base never sits in a value-type position.
#[expect(
    clippy::exhaustive_enums,
    reason = "every generic target is a struct, enum, or trait; other kinds have their own ResolvedType variants"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GenericBase {
    /// A generic struct base, e.g. `Box` in `Box<T>`.
    Struct(StructId),
    /// A generic enum base, e.g. `Option` in `Option<T>`.
    Enum(EnumId),
    /// A generic trait base, e.g. `Container` in `Container<I32>`.
    Trait(TraitId),
}

/// A fully resolved type.
///
/// Unlike AST types which use string names, resolved types use IDs that
/// directly reference definitions. This eliminates the need for symbol
/// table lookups during code generation.
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ResolvedType {
    /// Primitive type (String, I32, I64, F32, F64, Boolean, Path, Regex, Never)
    Primitive(PrimitiveType),

    /// Reference to a struct definition
    Struct(StructId),

    /// Reference to a trait definition
    Trait(TraitId),

    /// Reference to an enum definition
    Enum(EnumId),

    /// Array type: `[T]`
    Array(Box<Self>),

    /// Range type: `T..T` — an iterable sequence over numeric `T`. Used
    /// as the type of `start..end` expressions and the iterator type
    /// consumed by `for x in start..end { ... }` loops. Backends choose
    /// between a native range type or a counted-loop desugaring.
    Range(Box<Self>),

    /// Optional type: `T?`
    Optional(Box<Self>),

    /// Named tuple type: `(name1: T1, name2: T2)`
    Tuple(Vec<(String, Self)>),

    /// Generic type instantiation: `Box<String>` or `Option<I32>`.
    Generic { base: GenericBase, args: Vec<Self> },

    /// Unresolved type parameter (e.g., `T` in a generic definition).
    /// Within a generic, the actual type is not yet known; codegen
    /// emits the parameter name.
    TypeParam(String),

    /// Reference to a type defined in another module — used for types
    /// imported via `use` statements. Code generators emit proper
    /// import statements based on this.
    ///
    /// # Example
    ///
    /// For `use utils::Helper`, a field of type `Helper` becomes:
    /// ```text
    /// External {
    ///     module_path: ["utils"],
    ///     name: "Helper",
    ///     kind: ImportedKind::Struct,
    ///     type_args: [],
    /// }
    /// ```
    External {
        /// Logical module path (e.g., `["utils", "helpers"]`)
        module_path: Vec<String>,
        /// Type name in that module
        name: String,
        /// Kind of type (struct, trait, or enum)
        kind: ImportedKind,
        /// Type arguments for generic types (empty for non-generic)
        type_args: Vec<Self>,
    },

    /// Dictionary type: `[K: V]` — maps keys of type K to values of type V.
    Dictionary {
        key_ty: Box<Self>,
        value_ty: Box<Self>,
    },

    /// Closure/function type: `(T1, T2) -> R`. Represents a general
    /// closure type with multiple parameters for arbitrary pure functions.
    Closure {
        param_tys: Vec<(ParamConvention, Self)>,
        return_ty: Box<Self>,
    },

    /// A typed-out-of-band error placeholder.
    ///
    /// Produced by IR lowering when an upstream `CompilerError` has
    /// already been pushed (e.g. `UndefinedType`, `InternalError`) but
    /// the surrounding lowering code still needs to materialise *some*
    /// `ResolvedType` to keep walking the AST. Replaces the previous
    /// stringly-typed `TypeParam("Unknown")` sentinel, which collided
    /// with any user-defined type literally named `Unknown` and made
    /// downstream "is this an error or a real type-param?" checks
    /// ambiguous.
    ///
    /// Backends should treat `Error` as unreachable: if it survives to
    /// code generation, the compile would already have returned the
    /// associated `CompilerError` to the caller.
    Error,
}

impl ResolvedType {
    /// Get a display name for this type.
    ///
    /// Useful for error messages and debugging. For code generation,
    /// prefer pattern matching on the variants directly.
    #[must_use]
    pub fn display_name(&self, module: &IrModule) -> String {
        match self {
            Self::Primitive(p) => match p {
                PrimitiveType::String => "String".to_string(),
                PrimitiveType::I32 => "I32".to_string(),
                PrimitiveType::I64 => "I64".to_string(),
                PrimitiveType::F32 => "F32".to_string(),
                PrimitiveType::F64 => "F64".to_string(),
                PrimitiveType::Boolean => "Boolean".to_string(),
                PrimitiveType::Path => "Path".to_string(),
                PrimitiveType::Regex => "Regex".to_string(),
                PrimitiveType::Never => "Never".to_string(),
            },
            Self::Struct(id) => module
                .get_struct(*id)
                .map_or_else(|| format!("<invalid-struct-{}>", id.0), |s| s.name.clone()),
            Self::Trait(id) => module
                .get_trait(*id)
                .map_or_else(|| format!("<invalid-trait-{}>", id.0), |t| t.name.clone()),
            Self::Enum(id) => module
                .get_enum(*id)
                .map_or_else(|| format!("<invalid-enum-{}>", id.0), |e| e.name.clone()),
            Self::Array(inner) => format!("[{}]", inner.display_name(module)),
            Self::Range(inner) => format!(
                "{}..{}",
                inner.display_name(module),
                inner.display_name(module)
            ),
            Self::Optional(inner) => format!("{}?", inner.display_name(module)),
            Self::Tuple(fields) => {
                let fields_str: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{}: {}", name, ty.display_name(module)))
                    .collect();
                format!("({})", fields_str.join(", "))
            }
            Self::Generic { base, args } => {
                let base_name = match base {
                    GenericBase::Struct(id) => module
                        .get_struct(*id)
                        .map_or_else(|| format!("<invalid-struct-{}>", id.0), |s| s.name.clone()),
                    GenericBase::Enum(id) => module
                        .get_enum(*id)
                        .map_or_else(|| format!("<invalid-enum-{}>", id.0), |e| e.name.clone()),
                    GenericBase::Trait(id) => module
                        .get_trait(*id)
                        .map_or_else(|| format!("<invalid-trait-{}>", id.0), |t| t.name.clone()),
                };
                let args_str: Vec<_> = args.iter().map(|a| a.display_name(module)).collect();
                format!("{}<{}>", base_name, args_str.join(", "))
            }
            Self::TypeParam(name) => name.clone(),
            Self::External {
                name, type_args, ..
            } => {
                if type_args.is_empty() {
                    name.clone()
                } else {
                    let args_str: Vec<_> =
                        type_args.iter().map(|a| a.display_name(module)).collect();
                    format!("{}<{}>", name, args_str.join(", "))
                }
            }
            Self::Dictionary { key_ty, value_ty } => {
                format!(
                    "[{}: {}]",
                    key_ty.display_name(module),
                    value_ty.display_name(module)
                )
            }
            Self::Closure {
                param_tys,
                return_ty,
            } => {
                let params_str: Vec<_> = param_tys
                    .iter()
                    .map(|(_, t)| t.display_name(module))
                    .collect();
                format!(
                    "({}) -> {}",
                    params_str.join(", "),
                    return_ty.display_name(module)
                )
            }
            Self::Error => "<error>".to_string(),
        }
    }
}
