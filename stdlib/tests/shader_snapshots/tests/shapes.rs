//! Shape rendering snapshot tests.
//!
//! Tests core shapes (Rect, Circle, Ellipse) with solid fills.
//! Golden images stored in `stdlib/tests/shader_snapshots/golden/shapes/`.

use super::super::comparator::{load_golden, save_image, CompareResult, SnapshotComparator};
use super::super::harness::ShaderTestHarness;
use super::super::wrapper::ShapeRenderSpec;
use std::path::PathBuf;

/// Red color for test shapes.
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// Blue color for test shapes.
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// Green color for test shapes.
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/tests/shader_snapshots/golden/shapes")
}

fn failed_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/tests/shader_snapshots/failed")
}

fn is_update_mode() -> bool {
    std::env::var("SNAPSHOT_UPDATE").is_ok()
}

/// Run a snapshot test.
fn run_snapshot_test(spec: &ShapeRenderSpec, test_name: &str) {
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
            // Update mode: save new golden
            std::fs::create_dir_all(golden_dir()).ok();
            save_image(&pixels, harness.width(), harness.height(), &golden_path)
                .expect("Failed to save golden image");
            println!("Updated golden: {}", golden_path.display());
            return;
        }

        // Compare mode
        if !golden_path.exists() {
            // Save actual for reference
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
            CompareResult::Match => {
                // Test passed
            }
            CompareResult::Mismatch {
                diff_count,
                diff_percent,
            } => {
                // Save diff and actual for debugging
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

/// Test: 100x100 red rectangle with sharp corners.
#[test]
fn test_rect_solid_red() {
    let spec = ShapeRenderSpec::rect(100.0, 100.0, RED, 0.0);
    run_snapshot_test(&spec, "rect_solid_red");
}

/// Test: 100x100 blue rectangle with 8px corner radius.
#[test]
fn test_rect_rounded_blue() {
    let spec = ShapeRenderSpec::rect(100.0, 100.0, BLUE, 8.0);
    run_snapshot_test(&spec, "rect_rounded_blue");
}

/// Test: 100px diameter green circle.
#[test]
fn test_circle_solid_green() {
    let spec = ShapeRenderSpec::circle(100.0, GREEN);
    run_snapshot_test(&spec, "circle_solid_green");
}

/// Test: 120x80 red ellipse.
#[test]
fn test_ellipse_solid_red() {
    let spec = ShapeRenderSpec::ellipse(120.0, 80.0, RED);
    run_snapshot_test(&spec, "ellipse_solid_red");
}

// =============================================================================
// STROKE TESTS (STUBS)
// =============================================================================

/// Test: Rectangle with stroke only, no fill.
///
/// Expected: Outlined rectangle with 2px stroke.
#[test]
fn rect_stroke_only() {
    let spec = ShapeRenderSpec::rect_stroke(100.0, 100.0, BLUE, 0.0, 2.0);
    run_snapshot_test(&spec, "rect_stroke_only");
}

/// Test: Rectangle with both fill and stroke.
///
/// Expected: Filled rectangle with outline.
#[test]
fn rect_stroke_and_fill() {
    let spec = ShapeRenderSpec::rect_with_stroke(100.0, 100.0, RED, BLUE, 0.0, 2.0);
    run_snapshot_test(&spec, "rect_stroke_and_fill");
}

/// Test: Circle with stroke only, no fill.
///
/// Expected: Outlined circle with 2px stroke.
#[test]
fn circle_stroke_only() {
    let spec = ShapeRenderSpec::circle_stroke(100.0, GREEN, 2.0);
    run_snapshot_test(&spec, "circle_stroke_only");
}

/// Test: Ellipse with thick stroke.
///
/// Expected: Outlined ellipse with 4px stroke.
#[test]
fn ellipse_stroke_thick() {
    let spec = ShapeRenderSpec::ellipse_stroke(120.0, 80.0, RED, 4.0);
    run_snapshot_test(&spec, "ellipse_stroke_thick");
}

// =============================================================================
// POLYGON TESTS (STUBS)
// =============================================================================

/// Test: Triangle (3-sided polygon), no rotation.
///
/// Expected: Equilateral triangle pointing up.
#[test]
fn polygon_triangle_solid() {
    let spec = ShapeRenderSpec::polygon((100.0, 100.0), Some(RED), 3, 0.0);
    run_snapshot_test(&spec, "polygon_triangle_solid");
}

/// Test: Hexagon (6-sided polygon), no rotation.
///
/// Expected: Regular hexagon.
#[test]
fn polygon_hexagon_solid() {
    let spec = ShapeRenderSpec::polygon((100.0, 100.0), Some(BLUE), 6, 0.0);
    run_snapshot_test(&spec, "polygon_hexagon_solid");
}

/// Test: Pentagon with 36 degree rotation.
///
/// Expected: Pentagon rotated 36 degrees.
#[test]
fn polygon_pentagon_rotated() {
    let spec = ShapeRenderSpec::polygon((100.0, 100.0), Some(GREEN), 5, 36.0);
    run_snapshot_test(&spec, "polygon_pentagon_rotated");
}

/// Test: Hexagon with stroke, no fill.
///
/// Expected: Outlined hexagon.
#[test]
fn polygon_with_stroke() {
    let spec = ShapeRenderSpec::polygon_with_stroke(
        (100.0, 100.0),
        None,
        Some(BLUE),
        6,
        0.0,
        2.0,
    );
    run_snapshot_test(&spec, "polygon_with_stroke");
}

// =============================================================================
// LINE TESTS (STUBS)
// =============================================================================

/// Test: Horizontal line.
///
/// Expected: Line from left to right, 2px stroke.
#[test]
fn line_horizontal() {
    let spec = ShapeRenderSpec::line((10.0, 50.0), (90.0, 50.0), RED, 2.0);
    run_snapshot_test(&spec, "line_horizontal");
}

/// Test: Vertical line.
///
/// Expected: Line from top to bottom, 2px stroke.
#[test]
fn line_vertical() {
    let spec = ShapeRenderSpec::line((50.0, 10.0), (50.0, 90.0), GREEN, 2.0);
    run_snapshot_test(&spec, "line_vertical");
}

/// Test: Diagonal line.
///
/// Expected: Line from top-left to bottom-right, 2px stroke.
#[test]
fn line_diagonal() {
    let spec = ShapeRenderSpec::line((10.0, 10.0), (90.0, 90.0), BLUE, 2.0);
    run_snapshot_test(&spec, "line_diagonal");
}

/// Test: Thick line.
///
/// Expected: Line with 4px stroke width.
#[test]
fn line_thick() {
    let spec = ShapeRenderSpec::line((10.0, 50.0), (90.0, 50.0), RED, 4.0);
    run_snapshot_test(&spec, "line_thick");
}

// =============================================================================
// BOOLEAN OPERATION TESTS (STUBS)
// =============================================================================

/// Test: Union of two overlapping circles.
///
/// Expected: Combined shape where either circle exists.
#[test]
fn union_two_circles() {
    let spec = ShapeRenderSpec::shape_union_circles((100.0, 100.0), RED);
    run_snapshot_test(&spec, "union_two_circles");
}

/// Test: Intersection of rectangle and circle.
///
/// Expected: Only visible where both shapes overlap.
#[test]
fn intersection_rect_circle() {
    let spec = ShapeRenderSpec::shape_intersection_rect_circle((100.0, 100.0), GREEN);
    run_snapshot_test(&spec, "intersection_rect_circle");
}

/// Test: Rectangle with circle subtracted.
///
/// Expected: Rectangle with circular hole.
#[test]
fn subtraction_rect_minus_circle() {
    let spec = ShapeRenderSpec::shape_subtraction_rect_minus_circle((100.0, 100.0), BLUE);
    run_snapshot_test(&spec, "subtraction_rect_minus_circle");
}

// =============================================================================
// CONTOUR/PATH TESTS (STUBS)
// =============================================================================

/// Test: Triangle path using LineTo segments.
///
/// Expected: Closed triangular path.
#[test]
fn contour_triangle() {
    let spec = ShapeRenderSpec::contour_triangle((100.0, 100.0), RED);
    run_snapshot_test(&spec, "contour_triangle");
}

/// Test: Open path with stroke only.
///
/// Expected: Unclosed stroked path.
#[test]
fn contour_open_path() {
    let spec = ShapeRenderSpec::contour_open_stroke((100.0, 100.0), GREEN, 2.0);
    run_snapshot_test(&spec, "contour_open_path");
}

/// Test: Path with quadratic Bezier curve.
///
/// Expected: Smooth curved path.
#[test]
fn contour_quad_bezier() {
    let spec = ShapeRenderSpec::contour_quad_bezier((100.0, 100.0), BLUE);
    run_snapshot_test(&spec, "contour_quad_bezier");
}

/// Test: Path with cubic Bezier curve.
///
/// Expected: S-curve or similar complex curve.
#[test]
fn contour_cubic_bezier() {
    let spec = ShapeRenderSpec::contour_cubic_bezier((100.0, 100.0), RED);
    run_snapshot_test(&spec, "contour_cubic_bezier");
}

/// Test: Path with arc segment.
///
/// Expected: Path containing circular arc.
#[test]
fn contour_arc() {
    let spec = ShapeRenderSpec::contour_arc((100.0, 100.0), GREEN);
    run_snapshot_test(&spec, "contour_arc");
}
