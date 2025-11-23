use crate::location::Span;
use std::path::PathBuf;

/// Error types for module resolution
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleError {
    /// Module file not found
    NotFound {
        path: Vec<String>,
        searched_paths: Vec<PathBuf>,
        span: Span,
    },
    /// Circular import detected
    CircularImport { cycle: Vec<String>, span: Span },
    /// Failed to read module file
    ReadError {
        path: PathBuf,
        error: String,
        span: Span,
    },
    /// Imported item is not public
    PrivateItem {
        item: String,
        module: String,
        span: Span,
    },
    /// Imported item not found in module
    ItemNotFound {
        item: String,
        module: String,
        available: Vec<String>,
        span: Span,
    },
}

/// Trait for resolving module paths to source code
///
/// This trait allows pluggable module resolution strategies.
/// The default implementation resolves modules from the filesystem,
/// but alternative implementations could resolve from memory, network, etc.
pub trait ModuleResolver {
    /// Resolve a module path to source code
    ///
    /// # Arguments
    /// * `path` - Module path segments (e.g., `["std", "View"]` for `use std::View`)
    /// * `current_file` - Path of the file making the import (for relative resolution)
    ///
    /// # Returns
    /// The source code of the module, or an error if resolution fails
    fn resolve(
        &self,
        path: &[String],
        current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError>;
}

/// Default filesystem-based module resolver
///
/// Resolves module paths by mapping them to .fv files on the filesystem.
///
/// # Examples
/// - `use std::View` → searches for `std/View.fv` or `std.fv`
/// - `use components::{Button, Text}` → searches for `components/Button.fv` and `components/Text.fv`
pub struct FileSystemResolver {
    /// Root directory for module resolution
    root_dir: PathBuf,
}

impl FileSystemResolver {
    /// Create a new filesystem resolver with the given root directory
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    /// Try to find a module file for the given path
    ///
    /// Tries the following locations in order:
    /// 1. `root_dir/path/to/module.fv`
    /// 2. `root_dir/path/to.fv` (if module is the last segment)
    fn find_module_file(&self, path: &[String]) -> Option<PathBuf> {
        // Try: root_dir/path/to/module.fv
        let mut file_path = self.root_dir.clone();
        for segment in path {
            file_path.push(segment);
        }
        file_path.set_extension("fv");

        if file_path.exists() {
            return Some(file_path);
        }

        // Try: root_dir/path/to.fv (treating last segment as module name)
        if path.len() > 1 {
            let mut dir_path = self.root_dir.clone();
            for segment in &path[..path.len() - 1] {
                dir_path.push(segment);
            }
            dir_path.push(&path[path.len() - 1]);
            dir_path.set_extension("fv");

            if dir_path.exists() {
                return Some(dir_path);
            }
        }

        None
    }
}

impl ModuleResolver for FileSystemResolver {
    fn resolve(
        &self,
        path: &[String],
        _current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        // Find the module file
        let module_file = self.find_module_file(path).ok_or_else(|| {
            // Collect searched paths for better error messages
            let mut searched = vec![];
            let mut file_path = self.root_dir.clone();
            for segment in path {
                file_path.push(segment);
            }
            file_path.set_extension("fv");
            searched.push(file_path);

            if path.len() > 1 {
                let mut dir_path = self.root_dir.clone();
                for segment in &path[..path.len() - 1] {
                    dir_path.push(segment);
                }
                dir_path.push(&path[path.len() - 1]);
                dir_path.set_extension("fv");
                searched.push(dir_path);
            }

            ModuleError::NotFound {
                path: path.to_vec(),
                searched_paths: searched,
                span: Span::default(),
            }
        })?;

        // Read the module file
        let source = std::fs::read_to_string(&module_file).map_err(|e| ModuleError::ReadError {
            path: module_file.clone(),
            error: e.to_string(),
            span: Span::default(),
        })?;

        Ok((source, module_file))
    }
}
