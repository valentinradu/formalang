//! Fill type snapshot tests.
//!
//! Tests gradient and pattern fills on shapes.
//! - Linear: 0deg, 45deg, 90deg, 135deg, 180deg angles
//! - Radial: centered, offset center
//! - Angular: 0deg, 90deg start angles
//! - Pattern: repeat, repeatX, repeatY, nested gradient
//! - MultiLinear: 3+ stop gradients

use super::super::comparator::{load_golden, save_image, CompareResult, SnapshotComparator};
use super::super::harness::ShaderTestHarness;
use super::super::wrapper::{FillRenderSpec, FillSpec, PatternRepeat, ShapeFields};
use std::path::PathBuf;

// Standard colors for fill tests
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
#[allow(dead_code)]
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Check if running in update mode
fn is_update_mode() -> bool {
    std::env::var("SNAPSHOT_UPDATE").is_ok()
}

/// Directory for golden images.
fn golden_dir() -> PathBuf {
    PathBuf::from("stdlib/tests/shader_snapshots/golden/fills")
}

/// Directory for failed test outputs.
fn failed_dir() -> PathBuf {
    PathBuf::from("stdlib/tests/shader_snapshots/failed")
}

/// Run a fill snapshot test.
fn run_fill_snapshot_test(spec: &FillRenderSpec, test_name: &str) {
    pollster::block_on(async {
        let harness =
            match ShaderTestHarness::new(spec.size.0 as u32, spec.size.1 as u32).await {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("Skipping test (no GPU): {}", e);
                    return;
                }
            };

        let pixels = harness.render_fill(spec).await.expect("Render should succeed");

        let golden_path = golden_dir().join(format!("{}.png", test_name));

        // Update mode: save new golden
        if is_update_mode() {
            std::fs::create_dir_all(golden_dir()).ok();
            save_image(&pixels, harness.width(), harness.height(), &golden_path)
                .expect("Failed to save golden image");
            println!("Updated golden: {}", golden_path.display());
            return;
        }

        // Verify mode: compare against golden
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
            CompareResult::Match => {
                // Test passed
            }
            CompareResult::SizeMismatch {
                actual,
                expected,
            } => {
                panic!(
                    "Size mismatch for {}: actual={:?}, expected={:?}",
                    test_name, actual, expected
                );
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
                    "Snapshot mismatch for {}: {} pixels ({:.2}%) differ\nGolden: {}\nActual: {}\nDiff: {}",
                    test_name,
                    diff_count,
                    diff_percent,
                    golden_path.display(),
                    actual_path.display(),
                    diff_path.display()
                );
            }
        }
    });
}

// =============================================================================
// LINEAR GRADIENT TESTS
// =============================================================================

/// Linear gradient at 0 degrees (left to right).
///
/// Expected: Red on left edge, blue on right edge.
#[test]
fn linear_horizontal() {
    let spec = FillRenderSpec::rect_linear_gradient((100.0, 100.0), RED, BLUE, 0.0);
    run_fill_snapshot_test(&spec, "linear_horizontal");
}

/// Linear gradient at 90 degrees (bottom to top).
///
/// Expected: Red on bottom edge, blue on top edge.
#[test]
fn linear_vertical() {
    let spec = FillRenderSpec::rect_linear_gradient((100.0, 100.0), RED, BLUE, 90.0);
    run_fill_snapshot_test(&spec, "linear_vertical");
}

/// Linear gradient at 45 degrees (diagonal).
///
/// Expected: Red on bottom-left corner, blue on top-right corner.
#[test]
fn linear_diagonal() {
    let spec = FillRenderSpec::rect_linear_gradient((100.0, 100.0), RED, BLUE, 45.0);
    run_fill_snapshot_test(&spec, "linear_diagonal");
}

/// Linear gradient at 180 degrees (right to left).
///
/// Expected: Blue on left edge, red on right edge.
#[test]
fn linear_reverse() {
    let spec = FillRenderSpec::rect_linear_gradient((100.0, 100.0), RED, BLUE, 180.0);
    run_fill_snapshot_test(&spec, "linear_reverse");
}

// =============================================================================
// RADIAL GRADIENT TESTS
// =============================================================================

/// Radial gradient with center at (0.5, 0.5).
///
/// Expected: White in center, black at edges.
#[test]
fn radial_centered() {
    let spec = FillRenderSpec::rect_radial_gradient((100.0, 100.0), WHITE, BLACK, 0.5, 0.5);
    run_fill_snapshot_test(&spec, "radial_centered");
}

/// Radial gradient with center at (0.3, 0.3).
///
/// Expected: White near top-left, black at edges.
#[test]
fn radial_offset() {
    let spec = FillRenderSpec::rect_radial_gradient((100.0, 100.0), WHITE, BLACK, 0.3, 0.3);
    run_fill_snapshot_test(&spec, "radial_offset");
}

// =============================================================================
// ANGULAR GRADIENT TESTS
// =============================================================================

/// Angular gradient starting at 0 degrees.
///
/// Expected: Color sweep around center point.
#[test]
fn angular_basic() {
    let spec = FillRenderSpec::rect_angular_gradient((100.0, 100.0), RED, BLUE, 0.0);
    run_fill_snapshot_test(&spec, "angular_basic");
}

/// Angular gradient starting at 90 degrees.
///
/// Expected: Color sweep rotated 90 degrees.
#[test]
fn angular_rotated() {
    let spec = FillRenderSpec::rect_angular_gradient((100.0, 100.0), RED, BLUE, 90.0);
    run_fill_snapshot_test(&spec, "angular_rotated");
}

// =============================================================================
// PATTERN TESTS
// =============================================================================

/// Pattern with repeat in both directions.
///
/// Expected: Checkerboard-like tiled pattern.
/// BUG: Nested Fill source not serialized into FillData array (codegen/wgsl.rs flatten_expr_to_f32s)
#[test]
fn pattern_repeat() {
    let spec = FillRenderSpec::new(
        ShapeFields::Rect {
            fill_rgba: None,
            stroke_rgba: None,
            corner_radius: 0.0,
            stroke_width: 1.0,
        },
        FillSpec::Pattern {
            source: Box::new(FillSpec::Linear {
                from_rgba: RED,
                to_rgba: BLUE,
                angle: 45.0,
            }),
            width: 4.0,
            height: 4.0,
            repeat: PatternRepeat::Repeat,
        },
        (100.0, 100.0),
    );
    run_fill_snapshot_test(&spec, "pattern_repeat");
}

/// Pattern with horizontal repeat only.
///
/// Expected: Horizontal stripes.
/// BUG: Nested Fill source not serialized into FillData array (codegen/wgsl.rs flatten_expr_to_f32s)
#[test]
fn pattern_repeat_x() {
    let spec = FillRenderSpec::new(
        ShapeFields::Rect {
            fill_rgba: None,
            stroke_rgba: None,
            corner_radius: 0.0,
            stroke_width: 1.0,
        },
        FillSpec::Pattern {
            source: Box::new(FillSpec::Solid { rgba: RED }),
            width: 4.0,
            height: 1.0,
            repeat: PatternRepeat::RepeatX,
        },
        (100.0, 100.0),
    );
    run_fill_snapshot_test(&spec, "pattern_repeat_x");
}

/// Pattern with vertical repeat only.
///
/// Expected: Vertical stripes.
/// BUG: Nested Fill source not serialized into FillData array (codegen/wgsl.rs flatten_expr_to_f32s)
#[test]
fn pattern_repeat_y() {
    let spec = FillRenderSpec::new(
        ShapeFields::Rect {
            fill_rgba: None,
            stroke_rgba: None,
            corner_radius: 0.0,
            stroke_width: 1.0,
        },
        FillSpec::Pattern {
            source: Box::new(FillSpec::Solid { rgba: BLUE }),
            width: 1.0,
            height: 4.0,
            repeat: PatternRepeat::RepeatY,
        },
        (100.0, 100.0),
    );
    run_fill_snapshot_test(&spec, "pattern_repeat_y");
}

/// Pattern using linear gradient as source.
///
/// Expected: Tiled gradient pattern.
/// BUG: Nested Fill source not serialized into FillData array (codegen/wgsl.rs flatten_expr_to_f32s)
#[test]
fn pattern_nested_gradient() {
    let spec = FillRenderSpec::new(
        ShapeFields::Rect {
            fill_rgba: None,
            stroke_rgba: None,
            corner_radius: 0.0,
            stroke_width: 1.0,
        },
        FillSpec::Pattern {
            source: Box::new(FillSpec::Linear {
                from_rgba: WHITE,
                to_rgba: BLACK,
                angle: 0.0,
            }),
            width: 5.0,
            height: 5.0,
            repeat: PatternRepeat::Repeat,
        },
        (100.0, 100.0),
    );
    run_fill_snapshot_test(&spec, "pattern_nested_gradient");
}

// =============================================================================
// MULTI-LINEAR GRADIENT TESTS
// =============================================================================

/// Multi-stop gradient with 3 colors.
///
/// Expected: Red -> Green -> Blue gradient.
/// NOTE: fill::ColorStop path resolution issue - needs compiler fix.
#[test]
fn multilinear_3_stops() {
    let spec = FillRenderSpec::new(
        ShapeFields::Rect {
            fill_rgba: None,
            stroke_rgba: None,
            corner_radius: 0.0,
            stroke_width: 1.0,
        },
        FillSpec::MultiLinear {
            stops: vec![
                (RED, 0.0),
                ([0.0, 1.0, 0.0, 1.0], 0.5), // GREEN
                (BLUE, 1.0),
            ],
            angle: 0.0,
        },
        (100.0, 100.0),
    );
    run_fill_snapshot_test(&spec, "multilinear_3_stops");
}

// =============================================================================
// RELATIVE FILL TESTS
// =============================================================================
// BUG: Direct struct instantiation of Fill implementors doesn't wrap in FillData.
// The inferred enum syntax (.linear(), .solid(), etc.) handles FillData wrapping
// automatically, but direct paths like fill::relative::Linear don't.
// Fix: Add FillData wrapping for direct trait implementor struct instantiation.

/// Linear gradient using relative coordinates.
///
/// Expected: Same as linear_horizontal but using fill::relative::Linear.
#[test]
#[ignore = "Direct Fill struct instantiation needs FillData wrapping (codegen)"]
fn relative_linear() {
    let spec = FillRenderSpec::new(
        ShapeFields::Rect {
            fill_rgba: None,
            stroke_rgba: None,
            corner_radius: 0.0,
            stroke_width: 1.0,
        },
        FillSpec::RelativeLinear {
            from_rgba: RED,
            to_rgba: BLUE,
            angle: 0.0,
        },
        (100.0, 100.0),
    );
    run_fill_snapshot_test(&spec, "relative_linear");
}

/// Radial gradient using relative coordinates.
///
/// Expected: Same as radial_centered but using fill::relative::Radial.
#[test]
#[ignore = "Direct Fill struct instantiation needs FillData wrapping (codegen)"]
fn relative_radial() {
    let spec = FillRenderSpec::new(
        ShapeFields::Rect {
            fill_rgba: None,
            stroke_rgba: None,
            corner_radius: 0.0,
            stroke_width: 1.0,
        },
        FillSpec::RelativeRadial {
            from_rgba: WHITE,
            to_rgba: BLACK,
            center_x: 0.5,
            center_y: 0.5,
        },
        (100.0, 100.0),
    );
    run_fill_snapshot_test(&spec, "relative_radial");
}

/// Solid fill using relative coordinates.
///
/// Expected: Solid red fill.
#[test]
#[ignore = "Direct Fill struct instantiation needs FillData wrapping (codegen)"]
fn relative_solid() {
    let spec = FillRenderSpec::new(
        ShapeFields::Rect {
            fill_rgba: None,
            stroke_rgba: None,
            corner_radius: 0.0,
            stroke_width: 1.0,
        },
        FillSpec::RelativeSolid {
            rgba: RED,
        },
        (100.0, 100.0),
    );
    run_fill_snapshot_test(&spec, "relative_solid");
}

/// Angular gradient using relative coordinates.
///
/// Expected: Same as angular_basic but using fill::relative::Angular.
#[test]
#[ignore = "Direct Fill struct instantiation needs FillData wrapping (codegen)"]
fn relative_angular() {
    let spec = FillRenderSpec::new(
        ShapeFields::Rect {
            fill_rgba: None,
            stroke_rgba: None,
            corner_radius: 0.0,
            stroke_width: 1.0,
        },
        FillSpec::RelativeAngular {
            from_rgba: RED,
            to_rgba: BLUE,
            angle: 0.0,
        },
        (100.0, 100.0),
    );
    run_fill_snapshot_test(&spec, "relative_angular");
}
