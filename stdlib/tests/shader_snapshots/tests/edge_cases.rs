//! Edge case snapshot tests.
//!
//! Tests boundary conditions and unusual inputs.
//! - Shape edge cases: zero radius, max radius, tiny sizes
//! - Fill edge cases: same-color gradients, angle wrapping
//! - Layout edge cases: empty containers, single child, overflow
//! - Effect edge cases: zero/max opacity, no-op filters

use super::super::comparator::{load_golden, save_image, CompareResult, SnapshotComparator};
use super::super::harness::ShaderTestHarness;
use super::super::wrapper::{FillRenderSpec, ShapeRenderSpec};
use std::path::PathBuf;

/// Red color for test shapes.
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// Blue color for test shapes.
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// Green color for test shapes.
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("stdlib/tests/shader_snapshots/golden/edge_cases")
}

fn failed_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/tests/shader_snapshots/failed")
}

fn is_update_mode() -> bool {
    std::env::var("SNAPSHOT_UPDATE").is_ok()
}

/// Run a snapshot test for shapes.
fn run_shape_snapshot_test(spec: &ShapeRenderSpec, test_name: &str) {
    pollster::block_on(async {
        let harness = match ShaderTestHarness::new(spec.size.0 as u32, spec.size.1 as u32).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Skipping test (no GPU): {}", e);
                return;
            }
        };

        let pixels = harness.render(spec).await.expect("Render should succeed");

        let golden_path = golden_dir().join(format!("{}.png", test_name));

        if is_update_mode() {
            std::fs::create_dir_all(golden_dir()).ok();
            save_image(&pixels, harness.width(), harness.height(), &golden_path)
                .expect("Failed to save golden image");
            println!("Updated golden: {}", golden_path.display());
            return;
        }

        if !golden_path.exists() {
            let actual_path = failed_dir().join(format!("{}_actual.png", test_name));
            std::fs::create_dir_all(failed_dir()).ok();
            save_image(&pixels, harness.width(), harness.height(), &actual_path).ok();
            panic!(
                "Golden image not found: {}\nRun with SNAPSHOT_UPDATE=1 to create it.\nActual saved to: {}",
                golden_path.display(),
                actual_path.display()
            );
        }

        let (golden_pixels, golden_w, golden_h) =
            load_golden(&golden_path).expect("Failed to load golden image");

        if golden_w != harness.width() || golden_h != harness.height() {
            panic!(
                "Size mismatch: actual {}x{}, golden {}x{}",
                harness.width(),
                harness.height(),
                golden_w,
                golden_h
            );
        }

        let comparator = SnapshotComparator::default();
        let result = comparator.compare(&pixels, &golden_pixels, harness.width(), harness.height());

        match result {
            CompareResult::Match => {}
            CompareResult::Mismatch {
                diff_count,
                diff_percent,
            } => {
                std::fs::create_dir_all(failed_dir()).ok();

                let actual_path = failed_dir().join(format!("{}_actual.png", test_name));
                save_image(&pixels, harness.width(), harness.height(), &actual_path).ok();

                let diff_path = failed_dir().join(format!("{}_diff.png", test_name));
                comparator
                    .save_diff(
                        &pixels,
                        &golden_pixels,
                        harness.width(),
                        harness.height(),
                        &diff_path,
                    )
                    .ok();

                panic!(
                    "Snapshot mismatch: {} pixels differ ({:.2}%)\nActual: {}\nDiff: {}",
                    diff_count,
                    diff_percent,
                    actual_path.display(),
                    diff_path.display()
                );
            }
            CompareResult::SizeMismatch { actual, expected } => {
                panic!(
                    "Size mismatch: actual {:?}, expected {:?}",
                    actual, expected
                );
            }
        }
    });
}

/// Run a snapshot test for fills.
fn run_fill_snapshot_test(spec: &FillRenderSpec, test_name: &str) {
    pollster::block_on(async {
        let harness = match ShaderTestHarness::new(spec.size.0 as u32, spec.size.1 as u32).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Skipping test (no GPU): {}", e);
                return;
            }
        };

        let pixels = harness
            .render_fill(spec)
            .await
            .expect("Render should succeed");

        let golden_path = golden_dir().join(format!("{}.png", test_name));

        if is_update_mode() {
            std::fs::create_dir_all(golden_dir()).ok();
            save_image(&pixels, harness.width(), harness.height(), &golden_path)
                .expect("Failed to save golden image");
            println!("Updated golden: {}", golden_path.display());
            return;
        }

        if !golden_path.exists() {
            let actual_path = failed_dir().join(format!("{}_actual.png", test_name));
            std::fs::create_dir_all(failed_dir()).ok();
            save_image(&pixels, harness.width(), harness.height(), &actual_path).ok();
            panic!(
                "Golden image not found: {}\nRun with SNAPSHOT_UPDATE=1 to create it.\nActual saved to: {}",
                golden_path.display(),
                actual_path.display()
            );
        }

        let (golden_pixels, golden_w, golden_h) =
            load_golden(&golden_path).expect("Failed to load golden image");

        if golden_w != harness.width() || golden_h != harness.height() {
            panic!(
                "Size mismatch: actual {}x{}, golden {}x{}",
                harness.width(),
                harness.height(),
                golden_w,
                golden_h
            );
        }

        let comparator = SnapshotComparator::default();
        let result = comparator.compare(&pixels, &golden_pixels, harness.width(), harness.height());

        match result {
            CompareResult::Match => {}
            CompareResult::Mismatch {
                diff_count,
                diff_percent,
            } => {
                std::fs::create_dir_all(failed_dir()).ok();

                let actual_path = failed_dir().join(format!("{}_actual.png", test_name));
                save_image(&pixels, harness.width(), harness.height(), &actual_path).ok();

                let diff_path = failed_dir().join(format!("{}_diff.png", test_name));
                comparator
                    .save_diff(
                        &pixels,
                        &golden_pixels,
                        harness.width(),
                        harness.height(),
                        &diff_path,
                    )
                    .ok();

                panic!(
                    "Snapshot mismatch: {} pixels differ ({:.2}%)\nActual: {}\nDiff: {}",
                    diff_count,
                    diff_percent,
                    actual_path.display(),
                    diff_path.display()
                );
            }
            CompareResult::SizeMismatch { actual, expected } => {
                panic!(
                    "Size mismatch: actual {:?}, expected {:?}",
                    actual, expected
                );
            }
        }
    });
}

// =============================================================================
// SHAPE EDGE CASES
// =============================================================================

/// Rectangle with cornerRadius=0 (sharp corners).
///
/// Expected: Rectangle with perfectly sharp corners.
#[test]
fn rect_zero_corner_radius() {
    let spec = ShapeRenderSpec::rect(100.0, 100.0, RED, 0.0);
    run_shape_snapshot_test(&spec, "rect_zero_corner_radius");
}

/// Rectangle with cornerRadius=min(w,h)/2 (pill shape).
///
/// Expected: Fully rounded ends (stadium shape).
#[test]
fn rect_max_corner_radius() {
    // For a 100x60 rect, max corner radius is 30 (min dimension / 2)
    let spec = ShapeRenderSpec::rect(100.0, 60.0, BLUE, 30.0);
    run_shape_snapshot_test(&spec, "rect_max_corner_radius");
}

/// Very small circle (4px diameter).
///
/// Expected: Circle visible with anti-aliasing at boundary.
#[test]
fn circle_tiny() {
    // Use a larger canvas to see the tiny circle
    let spec = ShapeRenderSpec::circle(4.0, GREEN);
    run_shape_snapshot_test(&spec, "circle_tiny");
}

/// Polygon with 12+ sides (near-circle).
///
/// Expected: Very smooth polygon approximating circle.
#[test]
fn polygon_many_sides() {
    let spec = ShapeRenderSpec::polygon((100.0, 100.0), Some(RED), 16, 0.0);
    run_shape_snapshot_test(&spec, "polygon_many_sides");
}

/// Rectangle with zero dimensions.
///
/// Expected: No crash, empty or invisible result.
#[test]
#[ignore = "Zero dimension shapes may cause GPU issues"]
fn rect_zero_dimension() {
    // This test verifies graceful handling of degenerate shapes
    let spec = ShapeRenderSpec::rect(0.0, 0.0, RED, 0.0);
    run_shape_snapshot_test(&spec, "rect_zero_dimension");
}

// =============================================================================
// FILL EDGE CASES
// =============================================================================

/// Gradient where from_color == to_color.
///
/// Expected: Solid color (no visible gradient).
#[test]
fn gradient_same_colors() {
    let spec = FillRenderSpec::rect_linear_gradient((100.0, 100.0), RED, RED, 0.0);
    run_fill_snapshot_test(&spec, "gradient_same_colors");
}

/// Gradient at 360 degree angle.
///
/// Expected: Same as 0 degrees (wraps around).
#[test]
fn gradient_angle_360() {
    let spec = FillRenderSpec::rect_linear_gradient((100.0, 100.0), RED, BLUE, 360.0);
    run_fill_snapshot_test(&spec, "gradient_angle_360");
}

/// Radial gradient with center at (0, 0).
///
/// Expected: Gradient emanating from corner.
#[test]
fn radial_center_edge() {
    let spec = FillRenderSpec::rect_radial_gradient((100.0, 100.0), RED, BLUE, 0.0, 0.0);
    run_fill_snapshot_test(&spec, "radial_center_edge");
}

// =============================================================================
// LAYOUT EDGE CASES
// =============================================================================

/// VStack with no children.
///
/// Expected: Empty container renders without crash.
#[test]
#[ignore = "Layout containers require runtime layout engine integration"]
fn vstack_empty() {
    todo!("vstack_empty test - requires layout engine")
}

/// VStack with single child.
///
/// Expected: Single child rendered, no spacing artifacts.
#[test]
#[ignore = "Layout containers require runtime layout engine integration"]
fn vstack_single() {
    todo!("vstack_single test - requires layout engine")
}

/// HStack where children exceed container width.
///
/// Expected: Children overflow or clip based on settings.
#[test]
#[ignore = "Layout containers require runtime layout engine integration"]
fn hstack_overflow() {
    todo!("hstack_overflow test - requires layout engine")
}

/// Frame where child is smaller than frame.
///
/// Expected: Child positioned per alignment, frame size unchanged.
#[test]
#[ignore = "Layout containers require runtime layout engine integration"]
fn frame_smaller_child() {
    todo!("frame_smaller_child test - requires layout engine")
}

/// Nested containers where child padding exceeds parent bounds.
///
/// Expected: Graceful handling, no overflow artifacts.
#[test]
#[ignore = "Layout containers require runtime layout engine integration"]
fn nested_padding_overflow() {
    todo!("nested_padding_overflow test - requires layout engine")
}

// =============================================================================
// EFFECT EDGE CASES
// =============================================================================

/// Opacity set to 0.0 (fully transparent).
///
/// Expected: Shape completely invisible.
#[test]
#[ignore = "Effect compositing requires multi-pass rendering infrastructure"]
fn opacity_zero() {
    todo!("opacity_zero test - requires effect infrastructure")
}

/// Opacity set to 1.0 (fully opaque).
///
/// Expected: Shape unchanged from original.
#[test]
#[ignore = "Effect compositing requires multi-pass rendering infrastructure"]
fn opacity_one() {
    todo!("opacity_one test - requires effect infrastructure")
}

/// Grayscale applied to already grayscale image.
///
/// Expected: No visible change (idempotent).
#[test]
#[ignore = "Effect compositing requires multi-pass rendering infrastructure"]
fn grayscale_on_grayscale() {
    todo!("grayscale_on_grayscale test - requires effect infrastructure")
}
