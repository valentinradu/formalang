//! Block-level IR statement types: `IrBlockStatement` and `IrMatchArm`.

use super::{expr::IrExpr, BindingId, ResolvedType, VariantIdx};

/// A statement within a block expression.
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum IrBlockStatement {
    /// Let binding: `let x = expr` or `let mut x = expr`
    Let {
        /// Per-function-unique identifier assigned by
        /// `ResolveReferencesPass`. Lowering emits `BindingId(0)` and the
        /// pass overwrites it.
        binding_id: BindingId,
        /// Binding name (preserved for diagnostics).
        name: String,
        /// Whether the binding is mutable
        mutable: bool,
        /// Optional type annotation
        ty: Option<ResolvedType>,
        /// Value expression
        value: IrExpr,
    },
    /// Assignment: `x = expr`
    Assign {
        /// Target expression (variable or field path)
        target: IrExpr,
        /// Value expression
        value: IrExpr,
    },
    /// Expression statement (evaluated for side effects)
    Expr(IrExpr),
}

impl IrBlockStatement {
    /// Transform every expression in this statement with `f`. Used by passes
    /// like constant folding and DCE.
    #[must_use]
    pub fn map_exprs<F>(self, mut f: F) -> Self
    where
        F: FnMut(IrExpr) -> IrExpr,
    {
        match self {
            Self::Let {
                binding_id,
                name,
                mutable,
                ty,
                value,
            } => Self::Let {
                binding_id,
                name,
                mutable,
                ty,
                value: f(value),
            },
            Self::Assign { target, value } => Self::Assign {
                target: f(target),
                value: f(value),
            },
            Self::Expr(expr) => Self::Expr(f(expr)),
        }
    }
}

/// A match arm: `Variant(bindings) => body` or `_ => body`
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrMatchArm {
    /// Variant name being matched (empty string for wildcard); preserved
    /// alongside [`Self::variant_idx`] for diagnostics.
    pub variant: String,

    /// Position of the matched variant in the scrutinee enum's `variants`
    /// vector. Lowering emits `VariantIdx(0)` and `ResolveReferencesPass`
    /// overwrites it.
    pub variant_idx: VariantIdx,

    /// Whether this is a wildcard pattern (`_`)
    pub is_wildcard: bool,

    /// Bindings for associated data: `(name, binding_id, type)`. The
    /// `binding_id` is a fresh per-function identifier introduced by
    /// the match arm; lowering emits `BindingId(0)` and
    /// `ResolveReferencesPass` overwrites it. Backends key on the
    /// `BindingId` to reach the slot the arm writes the payload into.
    pub bindings: Vec<(String, BindingId, ResolvedType)>,

    /// Body expression
    pub body: IrExpr,
}
