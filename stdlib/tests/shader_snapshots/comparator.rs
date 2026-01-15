//! Snapshot Comparator
//!
//! Tolerance-based image comparison for GPU-rendered snapshots.
//! Handles cross-platform GPU precision differences.

use image::{ImageBuffer, RgbaImage};
use std::io;
use std::path::Path;

/// Result of comparing two images.
#[derive(Debug, Clone, PartialEq)]
pub enum CompareResult {
    /// Images match within tolerance.
    Match,

    /// Images differ beyond tolerance.
    Mismatch {
        /// Number of pixels that differ beyond tolerance.
        diff_count: u32,
        /// Percentage of pixels that differ (0.0-100.0).
        diff_percent: f32,
    },

    /// Images have different dimensions.
    SizeMismatch {
        /// Actual image dimensions (width, height).
        actual: (u32, u32),
        /// Expected image dimensions (width, height).
        expected: (u32, u32),
    },
}

/// Tolerance-based image comparator.
///
/// Compares RGBA pixel buffers with configurable tolerance to handle
/// GPU floating-point precision differences across platforms.
///
/// # Default Tolerances
///
/// - Per-channel tolerance: 2% (0.02)
/// - Max differing pixels: 0.1%
#[derive(Debug, Clone)]
pub struct SnapshotComparator {
    /// Per-channel color tolerance (0.0-1.0).
    /// Pixels differ if any channel differs by more than this.
    pub tolerance: f32,

    /// Maximum percentage of pixels allowed to differ (0.0-100.0).
    /// Test fails if more pixels differ than this threshold.
    pub max_diff_percent: f32,
}

impl Default for SnapshotComparator {
    fn default() -> Self {
        Self {
            tolerance: 0.02,       // 2% per channel
            max_diff_percent: 0.1, // 0.1% of pixels
        }
    }
}

impl SnapshotComparator {
    /// Create a comparator with custom tolerances.
    ///
    /// # Parameters
    /// - `tolerance`: Per-channel tolerance (0.0-1.0)
    /// - `max_diff_percent`: Max percentage of differing pixels (0.0-100.0)
    pub fn new(tolerance: f32, max_diff_percent: f32) -> Self {
        Self {
            tolerance,
            max_diff_percent,
        }
    }

    /// Compare two RGBA pixel buffers.
    ///
    /// # Parameters
    /// - `actual`: Rendered pixel data (RGBA, row-major, top-left origin)
    /// - `expected`: Golden pixel data (same format)
    /// - `width`, `height`: Image dimensions
    ///
    /// # Returns
    /// - `CompareResult::Match` if images match within tolerance
    /// - `CompareResult::Mismatch` if too many pixels differ
    /// - `CompareResult::SizeMismatch` if buffer sizes don't match dimensions
    pub fn compare(
        &self,
        actual: &[u8],
        expected: &[u8],
        width: u32,
        height: u32,
    ) -> CompareResult {
        let expected_len = (width * height * 4) as usize;

        // Check buffer sizes
        if actual.len() != expected_len {
            let actual_pixels = actual.len() / 4;
            let sqrt = (actual_pixels as f32).sqrt() as u32;
            return CompareResult::SizeMismatch {
                actual: (sqrt, sqrt),
                expected: (width, height),
            };
        }
        if expected.len() != expected_len {
            let expected_pixels = expected.len() / 4;
            let sqrt = (expected_pixels as f32).sqrt() as u32;
            return CompareResult::SizeMismatch {
                actual: (width, height),
                expected: (sqrt, sqrt),
            };
        }

        let tolerance_u8 = (self.tolerance * 255.0) as i32;
        let total_pixels = (width * height) as u32;
        let mut diff_count = 0u32;

        // Compare pixels
        for i in 0..total_pixels as usize {
            let base = i * 4;
            let mut pixel_differs = false;

            for c in 0..4 {
                let a = actual[base + c] as i32;
                let e = expected[base + c] as i32;
                if (a - e).abs() > tolerance_u8 {
                    pixel_differs = true;
                    break;
                }
            }

            if pixel_differs {
                diff_count += 1;
            }
        }

        let diff_percent = (diff_count as f32 / total_pixels as f32) * 100.0;

        if diff_percent <= self.max_diff_percent {
            CompareResult::Match
        } else {
            CompareResult::Mismatch {
                diff_count,
                diff_percent,
            }
        }
    }

    /// Generate a diff image highlighting pixel differences.
    ///
    /// Diff image uses:
    /// - Green: pixels that match
    /// - Red: pixels that differ
    /// - Intensity: magnitude of difference
    ///
    /// # Parameters
    /// - `actual`: Rendered pixel data
    /// - `expected`: Golden pixel data
    /// - `width`, `height`: Image dimensions
    ///
    /// # Returns
    /// RGBA pixel data for diff visualization.
    pub fn generate_diff(
        &self,
        actual: &[u8],
        expected: &[u8],
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let total_pixels = (width * height) as usize;
        let mut diff = vec![0u8; total_pixels * 4];
        let tolerance_u8 = (self.tolerance * 255.0) as i32;

        for i in 0..total_pixels {
            let base = i * 4;
            let mut max_diff = 0i32;

            for c in 0..4 {
                let a = actual.get(base + c).copied().unwrap_or(0) as i32;
                let e = expected.get(base + c).copied().unwrap_or(0) as i32;
                max_diff = max_diff.max((a - e).abs());
            }

            if max_diff <= tolerance_u8 {
                // Match: green
                diff[base] = 0;
                diff[base + 1] = 128;
                diff[base + 2] = 0;
                diff[base + 3] = 255;
            } else {
                // Differ: red with intensity based on difference
                let intensity = ((max_diff as f32 / 255.0) * 255.0).min(255.0) as u8;
                diff[base] = 255;
                diff[base + 1] = 0;
                diff[base + 2] = 0;
                diff[base + 3] = intensity.max(128);
            }
        }

        diff
    }

    /// Save diff image to file.
    ///
    /// # Parameters
    /// - `actual`: Rendered pixel data
    /// - `expected`: Golden pixel data
    /// - `width`, `height`: Image dimensions
    /// - `path`: Output PNG path
    ///
    /// # Errors
    /// Returns IO error if file write fails.
    pub fn save_diff(
        &self,
        actual: &[u8],
        expected: &[u8],
        width: u32,
        height: u32,
        path: &Path,
    ) -> io::Result<()> {
        let diff = self.generate_diff(actual, expected, width, height);
        save_image(&diff, width, height, path)
    }
}

/// Load a golden image from disk.
///
/// # Parameters
/// - `path`: Path to PNG file
///
/// # Returns
/// Tuple of (pixels, width, height) or IO error.
pub fn load_golden(path: &Path) -> io::Result<(Vec<u8>, u32, u32)> {
    let img =
        image::open(path).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();

    Ok((pixels, width, height))
}

/// Save pixels as a PNG image.
///
/// # Parameters
/// - `pixels`: RGBA pixel data
/// - `width`, `height`: Image dimensions
/// - `path`: Output path
///
/// # Errors
/// Returns IO error if write fails.
pub fn save_image(pixels: &[u8], width: u32, height: u32, path: &Path) -> io::Result<()> {
    let img: RgbaImage = ImageBuffer::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid pixel buffer size"))?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    img.save(path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_identical() {
        let pixels = vec![255, 0, 0, 255, 0, 255, 0, 255]; // 2 pixels
        let comparator = SnapshotComparator::default();
        let result = comparator.compare(&pixels, &pixels, 2, 1);
        assert_eq!(result, CompareResult::Match);
    }

    #[test]
    fn test_compare_within_tolerance() {
        let actual = vec![255, 0, 0, 255];
        let expected = vec![253, 2, 0, 255]; // 2 units off
        let comparator = SnapshotComparator::new(0.02, 100.0); // 2% = ~5 units
        let result = comparator.compare(&actual, &expected, 1, 1);
        assert_eq!(result, CompareResult::Match);
    }

    #[test]
    fn test_compare_beyond_tolerance() {
        let actual = vec![255, 0, 0, 255];
        let expected = vec![200, 0, 0, 255]; // 55 units off
        let comparator = SnapshotComparator::new(0.02, 0.0); // 0% allowed
        let result = comparator.compare(&actual, &expected, 1, 1);
        assert!(matches!(result, CompareResult::Mismatch { .. }));
    }

    #[test]
    fn test_generate_diff() {
        let actual = vec![255, 0, 0, 255];
        let expected = vec![0, 255, 0, 255];
        let comparator = SnapshotComparator::default();
        let diff = comparator.generate_diff(&actual, &expected, 1, 1);
        // Should be red (differs)
        assert_eq!(diff[0], 255); // R
        assert_eq!(diff[1], 0); // G
        assert_eq!(diff[2], 0); // B
    }
}
