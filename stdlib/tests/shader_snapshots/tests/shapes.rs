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
