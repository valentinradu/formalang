//! Transform/modifier snapshot tests.
//!
//! Tests geometric transformations on shapes.
//! - Scale: uniform and non-uniform scaling
//! - Rotate: rotation in degrees
//! - Translate: position offset

use super::super::wrapper::{ShapeRenderSpec, TransformRenderSpec, TransformSpec};

// =============================================================================
// TRANSFORM TESTS
// =============================================================================

/// 2x uniform scale.
///
/// Expected: Shape scaled to twice its size.
/// NOTE: Transform modifiers require matrix transformation infrastructure.
#[test]
#[ignore = "Transform modifiers require matrix transformation infrastructure"]
fn transform_scale_2x() {
    todo!("transform_scale_2x test")
}

/// 45 degree rotation.
///
/// Expected: Shape rotated 45 degrees clockwise.
/// NOTE: Transform modifiers require matrix transformation infrastructure.
#[test]
#[ignore = "Transform modifiers require matrix transformation infrastructure"]
fn transform_rotate_45() {
    todo!("transform_rotate_45 test")
}

/// 10px translation offset.
///
/// Expected: Shape offset by 10px in both directions.
/// NOTE: Transform modifiers require matrix transformation infrastructure.
#[test]
#[ignore = "Transform modifiers require matrix transformation infrastructure"]
fn transform_translate() {
    todo!("transform_translate test")
}

/// Combined scale and rotation.
///
/// Expected: Shape scaled then rotated.
/// NOTE: Transform modifiers require matrix transformation infrastructure.
#[test]
#[ignore = "Transform modifiers require matrix transformation infrastructure"]
fn transform_combined() {
    todo!("transform_combined test")
}
