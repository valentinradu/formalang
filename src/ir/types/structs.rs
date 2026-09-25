//! Struct shapes plus the field / generic-parameter / trait-reference
//! types they aggregate.

use crate::ast::{ParamConvention, Visibility};
#[cfg(feature = "serde")]
use crate::ir::span::no_span;
use crate::ir::{IrExpr, IrSpan, ResolvedType, TraitId};

/// A struct definition in the IR.
///
/// Structs are the primary data type in `FormaLang`, representing both
/// data models and UI components.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrStruct {
    /// The struct name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Traits implemented by this struct, with optional generic-trait
    /// args (`<T>`). Empty args means a non-generic trait. Each
    /// instantiation of a generic trait (`impl Eq<I32> for Foo`) is a
    /// separate entry.
    pub traits: Vec<IrTraitRef>,

    /// Regular fields
    pub fields: Vec<IrField>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,

    /// Joined `///` doc comments preceding this struct.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub doc: Option<String>,

    /// Source span for DWARF / source-map emission.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "no_span"))]
    pub span: IrSpan,
}

/// A field definition.
///
/// Used in structs, traits, and enum variants.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrField {
    /// Field name
    pub name: String,

    /// Resolved type
    pub ty: ResolvedType,

    /// Whether this field is mutable
    pub mutable: bool,

    /// Whether this field is optional (T?)
    pub optional: bool,

    /// Default value expression, if any
    pub default: Option<IrExpr>,

    /// Joined `///` doc comments preceding this field.
    pub doc: Option<String>,

    /// Capture / passing convention for this field.
    ///
    /// Always [`ParamConvention::Let`] for fields written in source
    /// (struct, trait, enum-variant fields). Set to a non-default
    /// value by [`ClosureConversionPass`](crate::ir::ClosureConversionPass)
    /// on synthesized env-struct fields so backends targeting linear-
    /// memory representations can choose between copy / move /
    /// reference semantics per capture without re-walking the
    /// original closure expression.
    ///
    /// `#[serde(default)]` keeps round-tripped IR documents
    /// produced before this field landed deserialisable as
    /// [`ParamConvention::Let`] (the existing implicit behaviour).
    #[cfg_attr(feature = "serde", serde(default))]
    pub convention: ParamConvention,

    /// Source span for DWARF / source-map emission.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "no_span"))]
    pub span: IrSpan,
}

/// A generic type parameter.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrGenericParam {
    /// Parameter name (e.g., "T")
    pub name: String,

    /// Trait constraints (e.g., `T: Container` or `T: Container<I32>`).
    /// Each entry carries the constrained trait id plus zero or more
    /// concrete arg types; empty when the trait isn't generic.
    pub constraints: Vec<IrTraitRef>,
}

/// A reference to a trait, optionally with concrete type arguments.
///
/// Used in two places: as the constraint shape on
/// [`IrGenericParam`] and as the trait-impl shape on
/// [`crate::ir::IrImpl`]. An empty `args` slot means the trait isn't
/// generic (`T: Container`, `impl Container for X`); a non-empty slot
/// carries the instantiation (`T: Container<I32>`,
/// `impl Container<I32> for X`) so monomorphisation can specialise
/// generic traits.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrTraitRef {
    pub trait_id: TraitId,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub args: Vec<ResolvedType>,
}

impl IrTraitRef {
    /// Construct a non-generic trait reference (no args).
    #[must_use]
    pub const fn simple(trait_id: TraitId) -> Self {
        Self {
            trait_id,
            args: Vec::new(),
        }
    }
}
