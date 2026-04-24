use std::path::PathBuf;

/// Error types for module resolution.
///
/// Each variant carries only data intrinsic to the resolution failure; the
/// source `Span` of the offending `use` statement is supplied by the caller
/// when converting a [`ModuleError`] into a user-facing [`crate::CompilerError`].
#[expect(
    clippy::exhaustive_enums,
    reason = "matched exhaustively by consumer code"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    /// Module file not found
    NotFound {
        path: Vec<String>,
        searched_paths: Vec<PathBuf>,
    },
    /// Circular import detected
    CircularImport { cycle: Vec<String> },
    /// Failed to read module file
    ReadError { path: PathBuf, error: String },
    /// Imported item is not public
    PrivateItem { item: String, module: String },
    /// Imported item not found in module
    ItemNotFound {
        item: String,
        module: String,
        available: Vec<String>,
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
    ///
    /// # Errors
    /// Returns a [`ModuleError`] if the module cannot be found or read.
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
#[derive(Debug)]
pub struct FileSystemResolver {
    /// Root directory for module resolution
    root_dir: PathBuf,
}

impl FileSystemResolver {
    /// Create a new filesystem resolver with the given root directory
    #[must_use]
    pub const fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    /// Try to find a module file for the given path, preferring a location
    /// relative to `current_file`'s directory before falling back to
    /// `root_dir`. See audit finding #18.
    fn find_module_file(&self, path: &[String], current_file: Option<&PathBuf>) -> Option<PathBuf> {
        // Search roots: current file's parent directory first, then the
        // configured project root. This lets `foo/bar.fv` do `use other`
        // and resolve `foo/other.fv` before `other.fv`.
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(cf) = current_file {
            if let Some(parent) = cf.parent() {
                if parent.as_os_str().is_empty() {
                    roots.push(PathBuf::from("."));
                } else {
                    roots.push(parent.to_path_buf());
                }
            }
        }
        if !roots.iter().any(|r| r == &self.root_dir) {
            roots.push(self.root_dir.clone());
        }

        for root in &roots {
            // Try: root/path/to/module.fv
            let mut file_path = root.clone();
            for segment in path {
                file_path.push(segment);
            }
            file_path.set_extension("fv");
            if file_path.exists() {
                return Some(file_path);
            }

            // Try: root/path/to.fv (init segments as path, last inside the file)
            if let Some((_last, init)) = path.split_last() {
                if !init.is_empty() {
                    let mut dir_path = root.clone();
                    for segment in init {
                        dir_path.push(segment);
                    }
                    dir_path.set_extension("fv");
                    if dir_path.exists() {
                        return Some(dir_path);
                    }
                }
            }
        }

        None
    }
}

impl ModuleResolver for FileSystemResolver {
    fn resolve(
        &self,
        path: &[String],
        current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        // Find the module file
        let module_file = self.find_module_file(path, current_file).ok_or_else(|| {
            // Collect searched paths (both current-file-relative and root-
            // relative) for a clearer not-found error.
            let mut searched = vec![];
            let mut search_roots: Vec<PathBuf> = Vec::new();
            if let Some(cf) = current_file {
                if let Some(parent) = cf.parent() {
                    search_roots.push(if parent.as_os_str().is_empty() {
                        PathBuf::from(".")
                    } else {
                        parent.to_path_buf()
                    });
                }
            }
            if !search_roots.iter().any(|r| r == &self.root_dir) {
                search_roots.push(self.root_dir.clone());
            }
            for root in &search_roots {
                let mut file_path = root.clone();
                for segment in path {
                    file_path.push(segment);
                }
                file_path.set_extension("fv");
                searched.push(file_path);
                if let Some((_last, init)) = path.split_last() {
                    if !init.is_empty() {
                        let mut dir_path = root.clone();
                        for segment in init {
                            dir_path.push(segment);
                        }
                        dir_path.set_extension("fv");
                        searched.push(dir_path);
                    }
                }
            }

            ModuleError::NotFound {
                path: path.to_vec(),
                searched_paths: searched,
            }
        })?;

        // Read the module file
        let source = std::fs::read_to_string(&module_file).map_err(|e| ModuleError::ReadError {
            path: module_file.clone(),
            error: e.to_string(),
        })?;

        Ok((source, module_file))
    }
}
