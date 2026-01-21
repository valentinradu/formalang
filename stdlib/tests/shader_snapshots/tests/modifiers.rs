//! Transform/modifier snapshot tests.
//!
//! Tests geometric transformations on shapes.
//! - Scale: uniform and non-uniform scaling
//! - Rotate: rotation in degrees
//! - Translate: position offset

use super::super::comparator::{load_golden, save_image, CompareResult, SnapshotComparator};
use super::super::harness::ShaderTestHarness;
use super::super::wrapper::{ShapeRenderSpec, Transform};
use std::path::PathBuf;

// Test colors
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/tests/shader_snapshots/golden/modifiers")
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
        match comparator.compare(&pixels, &golden_pixels, harness.width(), harness.height()) {
            CompareResult::Match => {}
            CompareResult::Mismatch { diff_count, diff_percent } => {
                // Save actual for debugging
                let actual_path = failed_dir().join(format!("{}_actual.png", test_name));
                std::fs::create_dir_all(failed_dir()).ok();
                save_image(&pixels, harness.width(), harness.height(), &actual_path).ok();

                panic!(
                    "Snapshot mismatch for '{}': {} pixels differ ({:.2}%)\nActual: {}",
                    test_name,
                    diff_count,
                    diff_percent,
                    actual_path.display()
                );
            }
            CompareResult::SizeMismatch { actual, expected } => {
                panic!(
                    "Size mismatch for '{}': actual {:?}, expected {:?}",
                    test_name, actual, expected
                );
            }
        }
    });
}

// =============================================================================
// TRANSFORM TESTS
// =============================================================================

/// 2x uniform scale.
///
/// Expected: Shape scaled to twice its size (will appear larger).
#[test]
fn transform_scale_2x() {
    let spec = ShapeRenderSpec::rect(100.0, 100.0, RED, 0.0)
        .with_transform(Transform::Scale { x: 2.0, y: 2.0 });
    run_snapshot_test(&spec, "transform_scale_2x");
}

/// 45 degree rotation.
///
/// Expected: Shape rotated 45 degrees clockwise.
#[test]
fn transform_rotate_45() {
    let spec = ShapeRenderSpec::rect(100.0, 100.0, GREEN, 0.0)
        .with_transform(Transform::Rotate { angle: 45.0 });
    run_snapshot_test(&spec, "transform_rotate_45");
}

/// 10px translation offset.
///
/// Expected: Shape offset by 10px in both directions.
#[test]
fn transform_translate() {
    let spec = ShapeRenderSpec::rect(100.0, 100.0, BLUE, 0.0)
        .with_transform(Transform::Translate { x: 10.0, y: 10.0 });
    run_snapshot_test(&spec, "transform_translate");
}

/// Combined scale and rotation.
///
/// Expected: Shape scaled then rotated.
#[test]
fn transform_combined() {
    let spec = ShapeRenderSpec::rect(100.0, 100.0, RED, 0.0)
        .with_transforms(vec![
            Transform::Scale { x: 1.5, y: 1.5 },
            Transform::Rotate { angle: 30.0 },
        ]);
    run_snapshot_test(&spec, "transform_combined");
}
