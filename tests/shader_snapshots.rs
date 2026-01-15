//! Shader Snapshot Tests
//!
//! Visual regression testing for FormaLang stdlib shader output.
//!
//! # Usage
//!
//! ```bash
//! # Run tests (compare mode)
//! cargo test --test shader_snapshots
//!
//! # Update golden files
//! SNAPSHOT_UPDATE=1 cargo test --test shader_snapshots
//! ```

// Include the module directory from stdlib/tests/
#[path = "../stdlib/tests/shader_snapshots/mod.rs"]
mod snapshots;

pub use snapshots::*;
