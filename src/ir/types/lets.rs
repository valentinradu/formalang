//! Module-level let bindings.

use crate::ast::Visibility;
#[cfg(feature = "serde")]
use crate::ir::span::no_span;
use crate::ir::{IrExpr, IrSpan, ResolvedType};

/// A module-level let binding in the IR.
///
/// Represents a named constant or computed value defined at the module level.
/// These are used for theming, configuration values, and shared expressions.
///
/// # Example
///
/// ```formalang
/// pub enum Color { hex(value: String) }
/// pub struct Font { family: String, size: I32 }
///
/// let primaryColor: Color = .hex(value: "#2563EB")
/// let headingFont: Font = Font(family: "Inter", size: 24)
/// ```
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrLet {
    /// The binding name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Whether this binding is mutable
    pub mutable: bool,

    /// The resolved type of the binding
    pub ty: ResolvedType,

    /// The bound expression
    pub value: IrExpr,

    /// Joined `///` doc comments preceding this binding.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub doc: Option<String>,

    /// Source span for DWARF / source-map emission. Carries
    /// `IrSpan::default()` for synthetic / hand-built IR.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "no_span"))]
    pub span: IrSpan,
}
