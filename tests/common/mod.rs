//! Shared test helpers. Not a public API — only consumed by integration
//! tests in this `tests/` directory.
//!
//! Rust's integration-test layout compiles each `tests/*.rs` as a
//! separate crate; helpers live here under `tests/common/` and are
//! included via `#[path = "common/mod.rs"] mod common;` at the top of
//! each test file that needs them.

#![allow(dead_code, unreachable_pub)]

use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
use std::collections::HashMap;
use std::path::PathBuf;

/// In-memory resolver for module-loading tests. Previously duplicated
/// across three test files — consolidated per audit finding #54.
pub struct MemResolver {
    modules: HashMap<Vec<String>, (String, PathBuf)>,
}

impl MemResolver {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    pub fn add(&mut self, path: Vec<String>, source: &str) {
        let file_path = PathBuf::from(format!("{}.forma", path.join("/")));
        self.modules.insert(path, (source.to_string(), file_path));
    }
}

impl ModuleResolver for MemResolver {
    fn resolve(
        &self,
        path: &[String],
        _current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        self.modules
            .get(path)
            .cloned()
            .ok_or_else(|| ModuleError::NotFound {
                path: path.to_vec(),
                searched_paths: vec![],
            })
    }
}
