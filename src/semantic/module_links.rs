//! The modules that one compilation imports, and the linked IR of each.
//!
//! Each module file gets an id when a `use` first names it. The id
//! makes the hidden names of the module's items; see
//! [`crate::ir::link`]. Each module also keeps the path that a `use`
//! wrote for it, for the final names of its items.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::ast::File;
use crate::error::CompilerError;
use crate::ir::link::{Import, LinkedModule};
use crate::ir::{IrModule, LinkedLowering};
use crate::location::Span;

use super::symbol_table::SymbolTable;

/// The modules of one compilation.
#[derive(Debug, Default)]
pub(crate) struct ModuleLinks {
    /// The id of each module file.
    ids: HashMap<PathBuf, u32>,
    /// The module path of each module, by id.
    logical: Vec<Vec<String>>,
    /// The linked IR of each module that lowered, by file.
    linked: HashMap<PathBuf, LinkedModule>,
    /// How deep the analysis is: 0 while the entry module is under
    /// analysis, 1 in a module that it imports, and so on.
    pub(super) depth: usize,
}

impl ModuleLinks {
    /// The id of the module in `file`. The first `use` that names a
    /// file gives it its id and its path. A `use` in the entry module
    /// gives the path too, because the entry's paths are the ones that
    /// a reader of the final IR knows.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; the callers push it into a Vec"
    )]
    pub(super) fn register(
        &mut self,
        file: &Path,
        logical: &[String],
        span: Span,
    ) -> Result<u32, CompilerError> {
        if let Some(&id) = self.ids.get(file) {
            if self.depth == 0 {
                if let Some(slot) = usize::try_from(id)
                    .ok()
                    .and_then(|i| self.logical.get_mut(i))
                {
                    *slot = logical.to_vec();
                }
            }
            return Ok(id);
        }
        let id =
            u32::try_from(self.logical.len()).map_err(|_| CompilerError::TooManyDefinitions {
                kind: "module",
                span,
            })?;
        self.ids.insert(file.to_path_buf(), id);
        self.logical.push(logical.to_vec());
        Ok(id)
    }

    /// The module path of each module, by id.
    pub(crate) fn logical_paths(&self) -> &[Vec<String>] {
        &self.logical
    }

    /// Record the linked IR of the module in `file`.
    pub(super) fn store(&mut self, file: &Path, linked: LinkedModule) {
        self.linked.insert(file.to_path_buf(), linked);
    }

    /// The IR of the module in `file` as a module on its own, with the
    /// short names of its own items.
    pub(super) fn view(&self, file: &Path) -> Option<IrModule> {
        let id = *self.ids.get(file)?;
        let linked = self.linked.get(file)?;
        Some(crate::ir::link::module_view(linked, id, &self.logical))
    }

    /// Lower a module on top of the modules that it imports.
    ///
    /// `own` is the module's file when another module imports it, and
    /// `None` for the entry module. `path` is the source file for the
    /// spans.
    ///
    /// # Errors
    ///
    /// The errors of the lowering. An `InternalError` when a module
    /// that this one imports has no IR: its analysis failed, and that
    /// failure is already in the error list of the caller.
    pub(crate) fn lower(
        &self,
        ast: &File,
        prelude_len: usize,
        symbols: &SymbolTable,
        own: Option<&Path>,
        path: Option<PathBuf>,
    ) -> Result<LinkedModule, Vec<CompilerError>> {
        let mut dep_files: BTreeSet<(u32, &Path)> = BTreeSet::new();
        let mut imports = Vec::new();
        for (name, file) in symbols.imported_names() {
            let Some(&id) = self.ids.get(file) else {
                return Err(vec![missing(file)]);
            };
            dep_files.insert((id, file));
            imports.push(Import {
                name: name.to_string(),
                origin: id,
            });
        }
        let mut deps = Vec::with_capacity(dep_files.len());
        for (_, file) in dep_files {
            deps.push(self.linked.get(file).ok_or_else(|| vec![missing(file)])?);
        }
        let own = match own {
            Some(file) => Some(*self.ids.get(file).ok_or_else(|| vec![missing(file)])?),
            None => None,
        };
        crate::ir::lower_linked(LinkedLowering {
            ast,
            prelude_len,
            symbols,
            path,
            own,
            deps,
            imports,
        })
    }
}

fn missing(file: &Path) -> CompilerError {
    CompilerError::InternalError {
        detail: format!(
            "module linking: the imported module {} has no IR",
            file.display()
        ),
        span: Span::default(),
    }
}
