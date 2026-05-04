//! Module-level type / span / child walkers shared by every phase of the
//! monomorphisation pass.
//!
//! Read-only and mutable walks over `ResolvedType`, `IrSpan`, and direct
//! child expressions reachable from an `IrModule`. Expression-only walkers
//! (`walk_expr`, `iter_expr_children_mut`) live in [`super::expr_walk`].

mod children;
mod spans;
mod types_mut;
mod types_ro;

pub(super) use spans::{walk_expr_spans_mut, walk_function_spans_mut};
pub(super) use types_mut::{walk_expr_types_mut, walk_function_types_mut, walk_module_types_mut};
pub(super) use types_ro::{walk_expr_types, walk_function_types, walk_module_types};

pub(crate) use children::walk_expr_children_mut;
