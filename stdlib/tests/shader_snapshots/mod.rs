//! Shader Snapshot Testing
//!
//! Visual regression testing for FormaLang stdlib shader output.
//! Renders WGSL shaders to images and compares against golden files.
//!
//! # Usage
//!
//! ```bash
//! # Run tests (compare mode)
//! cargo test shader_snapshots
//!
//! # Update golden files
//! SNAPSHOT_UPDATE=1 cargo test shader_snapshots
//! ```

pub mod comparator;
pub mod harness;
pub mod wrapper;

mod tests;

pub use comparator::{CompareResult, SnapshotComparator};
pub use harness::{HarnessError, RenderError, ShaderTestHarness};
pub use wrapper::{ShapeFields, ShapeRenderSpec};
