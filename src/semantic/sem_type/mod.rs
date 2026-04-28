//! Structural representation of a type used by semantic inference and
//! validation, replacing the legacy stringly-typed format.
//!
//! ## Why
//!
//! The legacy format encoded types as `String` (`"[T]"`, `"T?"`,
//! `"Base<T1, T2>"`, ...) with three magic sentinels: `"Unknown"`,
//! `"InferredEnum"`, `"Nil"`. Two real problems:
//!
//! 1. The sentinels collide with any user type literally named
//!    `Unknown`, `InferredEnum`, or `Nil`. The IR layer fixed its
//!    equivalent (`TypeParam("Unknown")`) by switching to
//!    `ResolvedType::Error`; this module is the semantic-layer
//!    counterpart.
//! 2. Compositional reasoning (`contains("Unknown")`, `rfind(" -> ")`,
//!    `strip_suffix('?')`) is fragile and quietly wrong on edge cases
//!    like nested closures inside optionals.
//!
//! ## Bridge
//!
//! [`SemType`] mirrors the legacy format exactly under
//! [`SemType::display`] and parses it back via
//! [`SemType::from_legacy_string`]. Round-trip on every shape the
//! existing codebase produces is guaranteed by the unit tests at the
//! bottom of this module. The bridge lets us migrate one call site at a
//! time without a flag day.
//!
//! ## Scope
//!
//! `SemType` is ID-free on purpose: semantic analysis runs before IR
//! lowering assigns IDs. Names are sufficient at this layer.
//!
//! ## Storage boundary
//!
//! Inference, validation, and the helpers they call are fully
//! `SemType`-native. A few storage sites still hold the legacy string
//! format — `local_let_bindings`, `inference_scope_stack`, and
//! `SymbolTable::LetInfo::inferred_type` — and lazy-parse via
//! [`Self::from_legacy_string`] at use. They were left string-typed
//! deliberately: the symbol table is the contract with IR lowering and
//! external consumers (LSP queries, downstream tooling), and the
//! sentinel-collision risk that motivated this refactor lives at the
//! *use* sites (now structural), not the storage sites. Migrating the
//! storage would cross into `pub(crate)` API territory without
//! removing any remaining bug surface.

mod legacy_parse;
#[cfg(test)]
mod tests;

use crate::ast::{ParamConvention, PrimitiveType, Type};

/// Structural type used during semantic analysis.
///
/// Variants mirror the legacy string format; see [`Self::display`] for
/// the textual rendering and [`Self::from_legacy_string`] for the
/// parser that accepts the same shapes.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SemType {
    Primitive(PrimitiveType),
    /// User-defined struct, enum, trait, or generic-parameter name.
    Named(String),
    Array(Box<Self>),
    Optional(Box<Self>),
    /// Named tuple fields, matching the AST's `Type::Tuple` shape.
    Tuple(Vec<(String, Self)>),
    Generic {
        base: String,
        args: Vec<Self>,
    },
    Dictionary {
        key: Box<Self>,
        value: Box<Self>,
    },
    Closure {
        params: Vec<Self>,
        return_ty: Box<Self>,
    },
    /// Type could not be determined; replaces the `"Unknown"` string
    /// sentinel. Propagates through composition (any operation
    /// involving `Unknown` yields `Unknown`).
    Unknown,
    /// `.variant(...)` syntax whose enum is inferred from context;
    /// replaces the `"InferredEnum"` sentinel.
    InferredEnum,
    /// `nil` literal; replaces the `"Nil"` sentinel.
    Nil,
}

impl SemType {
    /// Construct an array shape from an element type.
    pub(super) fn array_of(inner: Self) -> Self {
        Self::Array(Box::new(inner))
    }

    /// Construct an optional shape from a base type. Idempotent on
    /// already-optional types: `optional_of(T?) == T?`.
    pub(super) fn optional_of(inner: Self) -> Self {
        if matches!(inner, Self::Optional(_)) {
            inner
        } else {
            Self::Optional(Box::new(inner))
        }
    }

    /// Construct a closure shape from parameter and return types.
    pub(super) fn closure(params: Vec<Self>, return_ty: Self) -> Self {
        Self::Closure {
            params,
            return_ty: Box::new(return_ty),
        }
    }

    /// Construct a dictionary shape.
    pub(super) fn dictionary(key: Self, value: Self) -> Self {
        Self::Dictionary {
            key: Box::new(key),
            value: Box::new(value),
        }
    }

    /// True iff this type is the `Unknown` sentinel.
    pub(super) const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// True if this type contains `Unknown` or `InferredEnum` anywhere
    /// in its structure. Replaces the legacy
    /// `t.contains("Unknown") || t.contains("InferredEnum")` substring
    /// check used by validation gating: both sentinels mean "cannot
    /// validate this type yet, more inference needed".
    pub(super) fn is_indeterminate(&self) -> bool {
        match self {
            Self::Unknown | Self::InferredEnum => true,
            Self::Array(inner) | Self::Optional(inner) => inner.is_indeterminate(),
            Self::Tuple(fields) => fields.iter().any(|(_, t)| t.is_indeterminate()),
            Self::Generic { args, .. } => args.iter().any(Self::is_indeterminate),
            Self::Dictionary { key, value } => key.is_indeterminate() || value.is_indeterminate(),
            Self::Closure { params, return_ty } => {
                params.iter().any(Self::is_indeterminate) || return_ty.is_indeterminate()
            }
            Self::Primitive(_) | Self::Named(_) | Self::Nil => false,
        }
    }

    /// If this is `Optional(T)`, return a clone of `T`; otherwise return self.
    pub(super) fn strip_optional(&self) -> Self {
        if let Self::Optional(inner) = self {
            (**inner).clone()
        } else {
            self.clone()
        }
    }

    /// True when the type is `Optional`.
    pub(super) const fn is_optional(&self) -> bool {
        matches!(self, Self::Optional(_))
    }

    /// Render to the legacy string format. Symmetric with
    /// [`Self::from_legacy_string`].
    pub(super) fn display(&self) -> String {
        match self {
            Self::Primitive(p) => primitive_name(*p).to_string(),
            Self::Named(n) => n.clone(),
            Self::Array(inner) => format!("[{}]", inner.display()),
            Self::Optional(inner) => format!("{}?", inner.display()),
            Self::Tuple(fields) => {
                let rendered: Vec<String> = fields
                    .iter()
                    .map(|(name, ty)| format!("{name}: {}", ty.display()))
                    .collect();
                format!("({})", rendered.join(", "))
            }
            Self::Generic { base, args } => {
                if args.is_empty() {
                    base.clone()
                } else {
                    let rendered: Vec<String> = args.iter().map(Self::display).collect();
                    format!("{base}<{}>", rendered.join(", "))
                }
            }
            Self::Dictionary { key, value } => {
                format!("[{}: {}]", key.display(), value.display())
            }
            Self::Closure { params, return_ty } => match params.split_first() {
                None => format!("() -> {}", return_ty.display()),
                Some((only, [])) => format!("{} -> {}", only.display(), return_ty.display()),
                Some(_) => {
                    let rendered: Vec<String> = params.iter().map(Self::display).collect();
                    format!("{} -> {}", rendered.join(", "), return_ty.display())
                }
            },
            Self::Unknown => "Unknown".to_string(),
            Self::InferredEnum => "InferredEnum".to_string(),
            Self::Nil => "Nil".to_string(),
        }
    }

    /// Convert an AST [`Type`] node into a structural [`SemType`].
    /// Mirrors `trait_check::type_to_string` exactly so the two stay
    /// observationally interchangeable through the migration.
    pub(super) fn from_ast(ty: &Type) -> Self {
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
            Type::Generic { name, args, .. } => Self::Generic {
                base: name.name.clone(),
                args: args.iter().map(Self::from_ast).collect(),
            },
            Type::Dictionary { key, value } => Self::Dictionary {
                key: Box::new(Self::from_ast(key)),
                value: Box::new(Self::from_ast(value)),
            },
            Type::Closure { params, ret } => Self::Closure {
                params: params
                    .iter()
                    .map(|(_, p): &(ParamConvention, Type)| Self::from_ast(p))
                    .collect(),
                return_ty: Box::new(Self::from_ast(ret)),
            },
        }
    }

    /// Combine two branch types for if-expressions and match expressions.
    ///
    /// Widening rules:
    /// - `T` and `Nil` -> `T?`
    /// - `T` and `T?` -> `T?`
    /// - Identical types -> themselves
    /// - Otherwise, return [`SemType::Unknown`] so downstream validation
    ///   sees an indeterminate type rather than silently accepting a
    ///   wrong branch.
    pub(super) fn widen_branches(a: &Self, b: &Self) -> Self {
        if a == b {
            return a.clone();
        }
        if matches!(a, Self::Nil) && !matches!(b, Self::Nil) {
            return Self::optional_of(b.clone());
        }
        if matches!(b, Self::Nil) && !matches!(a, Self::Nil) {
            return Self::optional_of(a.clone());
        }
        if let Self::Optional(inner) = a {
            if **inner == *b {
                return a.clone();
            }
        }
        if let Self::Optional(inner) = b {
            if **inner == *a {
                return b.clone();
            }
        }
        Self::Unknown
    }

    /// True iff two branch types unify under optional widening (the
    /// validation-side variant of [`Self::widen_branches`] — returns
    /// `true` instead of producing the widened type).
    pub(super) fn unifies_with_optional_widening(a: &Self, b: &Self) -> bool {
        if matches!(a, Self::Nil) && matches!(b, Self::Optional(_)) {
            return true;
        }
        if matches!(b, Self::Nil) && matches!(a, Self::Optional(_)) {
            return true;
        }
        if matches!(a, Self::Nil) && !matches!(b, Self::Nil | Self::Unknown) {
            return true;
        }
        if matches!(b, Self::Nil) && !matches!(a, Self::Nil | Self::Unknown) {
            return true;
        }
        if let Self::Optional(inner) = a {
            if **inner == *b {
                return true;
            }
        }
        if let Self::Optional(inner) = b {
            if **inner == *a {
                return true;
            }
        }
        false
    }

    /// Substitute every standalone occurrence of [`Self::Named(param)`]
    /// inside `self` with `concrete`. Replaces the legacy
    /// `substitute_type_string` byte-walking implementation; this one
    /// is structural so `T` in `Box<T>` is substituted but a substring
    /// `T` inside a name like `TList` cannot match (different variant
    /// shape).
    pub(super) fn substitute_named(&self, param: &str, concrete: &Self) -> Self {
        match self {
            Self::Named(n) if n == param => concrete.clone(),
            Self::Named(_)
            | Self::Primitive(_)
            | Self::Unknown
            | Self::InferredEnum
            | Self::Nil => self.clone(),
            Self::Array(inner) => Self::Array(Box::new(inner.substitute_named(param, concrete))),
            Self::Optional(inner) => {
                Self::Optional(Box::new(inner.substitute_named(param, concrete)))
            }
            Self::Tuple(fields) => Self::Tuple(
                fields
                    .iter()
                    .map(|(n, t)| (n.clone(), t.substitute_named(param, concrete)))
                    .collect(),
            ),
            Self::Generic { base, args } => Self::Generic {
                base: base.clone(),
                args: args
                    .iter()
                    .map(|a| a.substitute_named(param, concrete))
                    .collect(),
            },
            Self::Dictionary { key, value } => Self::Dictionary {
                key: Box::new(key.substitute_named(param, concrete)),
                value: Box::new(value.substitute_named(param, concrete)),
            },
            Self::Closure { params, return_ty } => Self::Closure {
                params: params
                    .iter()
                    .map(|p| p.substitute_named(param, concrete))
                    .collect(),
                return_ty: Box::new(return_ty.substitute_named(param, concrete)),
            },
        }
    }
}

const fn primitive_name(p: PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::String => "String",
        PrimitiveType::I32 => "I32",
        PrimitiveType::I64 => "I64",
        PrimitiveType::F32 => "F32",
        PrimitiveType::F64 => "F64",
        PrimitiveType::Boolean => "Boolean",
        PrimitiveType::Path => "Path",
        PrimitiveType::Regex => "Regex",
        PrimitiveType::Never => "Never",
    }
}

pub(super) fn primitive_from_name(name: &str) -> Option<PrimitiveType> {
    match name {
        "String" => Some(PrimitiveType::String),
        "I32" => Some(PrimitiveType::I32),
        "I64" => Some(PrimitiveType::I64),
        "F32" => Some(PrimitiveType::F32),
        "F64" => Some(PrimitiveType::F64),
        "Boolean" => Some(PrimitiveType::Boolean),
        "Path" => Some(PrimitiveType::Path),
        "Regex" => Some(PrimitiveType::Regex),
        "Never" => Some(PrimitiveType::Never),
        _ => None,
    }
}
