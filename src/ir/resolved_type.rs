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

    /// Named tuple type: `(name1: T1, name2: T2)`
    Tuple(Vec<(String, Self)>),

    /// Generic type instantiation: `Box<String>`, `Optional<I32>`, etc.
    ///
    /// The four built-in compound types are also represented through
    /// this variant: `[T]` is `Generic { base: Struct(prelude_array_id), args: [T] }`,
    /// `T?` is `Generic { base: Enum(prelude_optional_id), args: [T] }`,
    /// `[K: V]` is `Generic { base: Struct(prelude_dictionary_id), args: [K, V] }`,
    /// and `start..end` produces `Generic { base: Struct(prelude_range_id), args: [T] }`.
    /// The prelude declares those built-ins as ordinary generic
    /// definitions so dispatch and lookup are uniform with user types.
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
    /// `ResolvedType` to keep walking the AST. Backends should treat
    /// `Error` as unreachable: if it survives to code generation, the
    /// compile would already have returned the associated
    /// `CompilerError` to the caller.
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
            Self::Tuple(fields) => {
                let fields_str: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{}: {}", name, ty.display_name(module)))
                    .collect();
                format!("({})", fields_str.join(", "))
            }
            Self::Generic { base, args } => {
                // Surface-syntax sugar for the four built-in compound
                // types: render `[T]`, `T?`, `[K: V]`, `start..end`
                // instead of `Array<T>`/`Optional<T>`/etc.
                if let GenericBase::Enum(id) = base {
                    if Some(*id) == module.prelude_optional_id() && args.len() == 1 {
                        return format!("{}?", args[0].display_name(module));
                    }
                }
                if let GenericBase::Struct(id) = base {
                    if Some(*id) == module.prelude_array_id() && args.len() == 1 {
                        return format!("[{}]", args[0].display_name(module));
                    }
                    if Some(*id) == module.prelude_dictionary_id() && args.len() == 2 {
                        return format!(
                            "[{}: {}]",
                            args[0].display_name(module),
                            args[1].display_name(module)
                        );
                    }
                    if Some(*id) == module.prelude_range_id() && args.len() == 1 {
                        return format!(
                            "{}..{}",
                            args[0].display_name(module),
                            args[0].display_name(module)
                        );
                    }
                }
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
