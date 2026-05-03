//! Trait definition + the body-less function signature shape used for
//! trait-required methods.

use crate::ast::{FunctionAttribute, Visibility};
use crate::ir::{IrSpan, ResolvedType, TraitId};

use super::{IrField, IrFunctionParam, IrGenericParam};

/// A trait definition in the IR.
///
/// Traits define interfaces that structs can implement.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrTrait {
    /// The trait name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Traits composed into this trait (trait inheritance)
    pub composed_traits: Vec<TraitId>,

    /// Required fields
    pub fields: Vec<IrField>,

    /// Required method signatures
    pub methods: Vec<IrFunctionSig>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,

    /// Joined `///` doc comments preceding this trait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,

    /// Source span for DWARF / source-map emission.
    #[serde(default, skip_serializing_if = "IrSpan::is_default")]
    pub span: IrSpan,
}

/// A function signature in the IR (without a body).
///
/// Used for trait method declarations that define the interface
/// without providing an implementation.
///
/// # Example
///
/// ```formalang
/// trait Drawable {
///     fn draw(self) -> String
/// }
/// ```
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrFunctionSig {
    /// Function name
    pub name: String,

    /// Parameters (first is typically `self`)
    pub params: Vec<IrFunctionParam>,

    /// Return type (None = unit/void)
    pub return_type: Option<ResolvedType>,

    /// Codegen-hint attributes (`inline`, `no_inline`, `cold`)
    /// declared on the trait method signature. Empty when none are
    /// present. Round-trips serialised IR while remaining backwards-
    /// compatible with documents that predate this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<FunctionAttribute>,

    /// Source span for DWARF / source-map emission.
    #[serde(default, skip_serializing_if = "IrSpan::is_default")]
    pub span: IrSpan,
}
