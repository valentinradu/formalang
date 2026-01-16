//! Shader snapshot tests.
//!
//! Test modules organized by tier:
//! - shapes: Shape primitives (Tier 1)
//! - fills: Gradient and pattern fills (Tier 2)
//! - layout: Container layouts (Tier 3)
//! - effects: Visual effects (Tier 4)
//! - content: Text and image components (Tier 5)
//! - modifiers: Transform operations (Tier 6)
//! - animation: Easing/transition snapshots (Tier 7)
//! - composition: Complex nested structures (Tier 8)
//! - edge_cases: Boundary conditions

mod animation;
mod composition;
mod content;
mod edge_cases;
mod effects;
mod fills;
mod layout;
mod modifiers;
mod shapes;
