//! Enum + enum-variant shapes.

use crate::ast::Visibility;
use crate::ir::IrSpan;

use super::{IrField, IrGenericParam};

/// An enum definition in the IR.
///
/// Enums are sum types with named variants, optionally carrying data.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrEnum {
    /// The enum name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Enum variants
    pub variants: Vec<IrEnumVariant>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,

    /// Joined `///` doc comments preceding this enum.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub doc: Option<String>,

    /// Source span for DWARF / source-map emission.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "no_span"))]
    pub span: IrSpan,
}
#[cfg(feature = "serde")]
use crate::ir::span::no_span;

/// An enum variant.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrEnumVariant {
    /// The variant name
    pub name: String,

    /// Associated data fields (empty for unit variants)
    pub fields: Vec<IrField>,

    /// Source span for DWARF / source-map emission.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "no_span"))]
    pub span: IrSpan,
}
