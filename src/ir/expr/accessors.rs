//! Inherent accessors on [`IrExpr`]: type lookup and structural
//! predicates.
//!
//! Split out of [`super`] to keep `expr/mod.rs` under the 500-LOC
//! ceiling. The variant definitions live in `expr/mod.rs`; the
//! per-variant operations live here.

use super::IrExpr;
use crate::ir::ResolvedType;

impl IrExpr {
    /// Get the resolved type of this expression.
    #[must_use]
    pub const fn ty(&self) -> &ResolvedType {
        match self {
            Self::Literal { ty, .. }
            | Self::StructInst { ty, .. }
            | Self::EnumInst { ty, .. }
            | Self::Array { ty, .. }
            | Self::Tuple { ty, .. }
            | Self::Reference { ty, .. }
            | Self::SelfFieldRef { ty, .. }
            | Self::FieldAccess { ty, .. }
            | Self::LetRef { ty, .. }
            | Self::BinaryOp { ty, .. }
            | Self::UnaryOp { ty, .. }
            | Self::If { ty, .. }
            | Self::For { ty, .. }
            | Self::Match { ty, .. }
            | Self::FunctionCall { ty, .. }
            | Self::MethodCall { ty, .. }
            | Self::Closure { ty, .. }
            | Self::ClosureRef { ty, .. }
            | Self::DictLiteral { ty, .. }
            | Self::DictAccess { ty, .. }
            | Self::Block { ty, .. } => ty,
        }
    }

    /// Get a mutable reference to the resolved type of this expression.
    pub const fn ty_mut(&mut self) -> &mut ResolvedType {
        match self {
            Self::Literal { ty, .. }
            | Self::StructInst { ty, .. }
            | Self::EnumInst { ty, .. }
            | Self::Array { ty, .. }
            | Self::Tuple { ty, .. }
            | Self::Reference { ty, .. }
            | Self::SelfFieldRef { ty, .. }
            | Self::FieldAccess { ty, .. }
            | Self::LetRef { ty, .. }
            | Self::BinaryOp { ty, .. }
            | Self::UnaryOp { ty, .. }
            | Self::If { ty, .. }
            | Self::For { ty, .. }
            | Self::Match { ty, .. }
            | Self::FunctionCall { ty, .. }
            | Self::MethodCall { ty, .. }
            | Self::Closure { ty, .. }
            | Self::ClosureRef { ty, .. }
            | Self::DictLiteral { ty, .. }
            | Self::DictAccess { ty, .. }
            | Self::Block { ty, .. } => ty,
        }
    }

    /// Whether this expression is a constant aggregate — a literal, or an
    /// aggregate (array / tuple / struct / enum / dict) whose every leaf is a
    /// literal. After [`fold_constants`](crate::ir::fold_constants) this
    /// predicate is the load-bearing marker for "static initializer": backends
    /// emitting read-only data segments can short-circuit on it instead of
    /// re-walking children themselves.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        match self {
            Self::Literal { .. } => true,
            Self::Array { elements, .. } => elements.iter().all(Self::is_constant),
            Self::Tuple { fields, .. } => fields.iter().all(|(_, e)| e.is_constant()),
            Self::StructInst { fields, .. } | Self::EnumInst { fields, .. } => {
                fields.iter().all(|(_, _, e)| e.is_constant())
            }
            Self::DictLiteral { entries, .. } => entries
                .iter()
                .all(|(k, v)| k.is_constant() && v.is_constant()),
            Self::Reference { .. }
            | Self::SelfFieldRef { .. }
            | Self::FieldAccess { .. }
            | Self::LetRef { .. }
            | Self::BinaryOp { .. }
            | Self::UnaryOp { .. }
            | Self::If { .. }
            | Self::For { .. }
            | Self::Match { .. }
            | Self::FunctionCall { .. }
            | Self::MethodCall { .. }
            | Self::Closure { .. }
            | Self::ClosureRef { .. }
            | Self::DictAccess { .. }
            | Self::Block { .. } => false,
        }
    }
}
