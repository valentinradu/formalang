//! Pass 0 — module resolution and `use`-statement handling.
//!
//! Drives loading and analysing imported modules through the generic
//! `R: ModuleResolver`. After resolving each module's source, this pass
//! parses it, builds its symbol table, runs all later passes against that
//! cached table, and lowers it to IR for downstream backends.

use super::module_resolver::{ModuleError, ModuleResolver};
use super::symbol_table::SymbolTable;
use super::SemanticAnalyzer;
use crate::ast::{File, Statement, UseItems, UseStmt};
use crate::error::CompilerError;
use crate::location::Span;
use std::path::Path;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 0: Module resolution
    /// Resolve all use statements, load imported modules, and check for circular dependencies
    pub(super) fn resolve_modules(&mut self, file: &File) {
        let inline = inline_module_names(file);
        for statement in &file.statements {
            if let Statement::Use(use_stmt) = statement {
                match use_stmt.path.first() {
                    // A `use` of an inline module resolves after pass 1,
                    // when the module's symbols are known.
                    Some(first) if inline.contains(first.name.as_str()) => {
                        if self.names_a_module_file(use_stmt) {
                            self.errors.push(CompilerError::AmbiguousModulePath {
                                name: first.name.clone(),
                                span: use_stmt.span,
                            });
                        }
                    }
                    _ => self.process_use_statement(use_stmt),
                }
            }
        }
    }

    /// True when the resolver finds a module file for the path of
    /// `use_stmt`.
    fn names_a_module_file(&self, use_stmt: &UseStmt) -> bool {
        let path: Vec<String> = use_stmt.path.iter().map(|i| i.name.clone()).collect();
        self.resolver
            .resolve(&path, self.current_file.as_ref())
            .is_ok()
    }

    /// Resolve each `use` of an item of an inline module of `file`.
    /// Runs after pass 1, so the inline modules' symbols are known.
    ///
    /// Each name gets the item's information under the short name, and
    /// the symbol table records the qualified name for the lowering.
    pub(super) fn resolve_local_uses(&mut self, file: &File) {
        let inline = inline_module_names(file);
        for statement in &file.statements {
            let Statement::Use(use_stmt) = statement else {
                continue;
            };
            let Some(first) = use_stmt.path.first() else {
                continue;
            };
            if !inline.contains(first.name.as_str()) || self.names_a_module_file(use_stmt) {
                continue;
            }
            self.import_local_use(use_stmt);
        }
    }

    fn import_local_use(&mut self, use_stmt: &UseStmt) {
        let mut table = &self.symbols;
        for (depth, segment) in use_stmt.path.iter().enumerate() {
            let Some(info) = table.modules.get(&segment.name) else {
                self.errors.push(CompilerError::ModuleNotFound {
                    name: segment.name.clone(),
                    span: use_stmt.span,
                });
                return;
            };
            // The file's own inline module is open to the file; a module
            // inside it must be `pub`.
            if depth > 0 && info.visibility != crate::ast::Visibility::Public {
                self.errors.push(CompilerError::PrivateImport {
                    name: segment.name.clone(),
                    span: use_stmt.span,
                });
                return;
            }
            table = &info.symbols;
        }
        let table = table.clone();
        let prefix: Vec<&str> = use_stmt.path.iter().map(|i| i.name.as_str()).collect();
        let names: Vec<String> = match &use_stmt.items {
            UseItems::Single(ident) => vec![ident.name.clone()],
            UseItems::Multiple(idents) => idents.iter().map(|i| i.name.clone()).collect(),
            UseItems::Glob => table.all_public_symbols(),
        };
        for name in names {
            if let Some(kind) = self.symbols.get_symbol_kind(&name) {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!("{name} (already defined as {})", kind.as_str()),
                    span: use_stmt.span,
                });
                continue;
            }
            let qualified = format!("{}::{name}", prefix.join("::"));
            if let Err(error) = self.symbols.alias_local(&name, &table, qualified) {
                let error = match error {
                    super::symbol_table::ImportError::PrivateItem { name, .. } => {
                        ModuleError::PrivateItem {
                            item: name,
                            module: prefix.join("::"),
                        }
                    }
                    super::symbol_table::ImportError::ItemNotFound { name, available } => {
                        ModuleError::ItemNotFound {
                            item: name,
                            module: prefix.join("::"),
                            available,
                        }
                    }
                };
                self.errors.push(Self::module_error_to_compiler_error(
                    error,
                    use_stmt.span,
                    true,
                ));
            }
        }
    }

    /// Process a single use statement
    fn process_use_statement(&mut self, use_stmt: &UseStmt) {
        let path_segments: Vec<String> = use_stmt
            .path
            .iter()
            .map(|ident| ident.name.clone())
            .collect();

        let (source, module_path) = match self
            .resolver
            .resolve(&path_segments, self.current_file.as_ref())
        {
            Ok(result) => result,
            Err(err) => {
                let compiler_err = Self::module_error_to_compiler_error(err, use_stmt.span, false);
                self.errors.push(compiler_err);
                return;
            }
        };

        if let Err(error) = self
            .module_links
            .register(&module_path, &path_segments, use_stmt.span)
        {
            self.errors.push(error);
            return;
        }
        if !self.check_and_register_import(&module_path, use_stmt.span) {
            return;
        }

        let module_symbols = if let Some((_, symbols)) = self.module_cache.get(&module_path) {
            symbols.clone()
        } else {
            match self.parse_and_analyze_module(&source, &module_path) {
                Ok(symbols) => symbols,
                Err(errors) => {
                    self.errors.extend(errors);
                    return;
                }
            }
        };

        self.import_use_items(use_stmt, &module_symbols, &module_path, &path_segments);
    }

    /// Dispatch symbol imports for all `UseItems` variants in `process_use_statement`
    fn import_use_items(
        &mut self,
        use_stmt: &UseStmt,
        module_symbols: &SymbolTable,
        module_path: &std::path::Path,
        path_segments: &[String],
    ) {
        let names: Vec<(String, bool)> = match &use_stmt.items {
            UseItems::Single(ident) => vec![(ident.name.clone(), false)],
            UseItems::Multiple(idents) => idents.iter().map(|i| (i.name.clone(), false)).collect(),
            UseItems::Glob => module_symbols
                .all_public_symbols()
                .into_iter()
                .map(|name| (name, true))
                .collect(),
        };
        for (name, glob) in names {
            // A glob does not bring a name that the module has already.
            if glob && self.symbols.get_symbol_kind(&name).is_some() {
                continue;
            }
            self.import_symbol(
                &name,
                module_symbols,
                module_path,
                path_segments.to_vec(),
                use_stmt.span,
            );
            if use_stmt.visibility == crate::ast::Visibility::Public {
                self.symbols.mark_reexport(&name);
            }
        }
    }

    /// Convert a `ModuleError` into a `CompilerError` for the given span.
    /// `private_item_qualified` controls whether the `PrivateItem` format uses `module::item` (true)
    /// or `item from module` (false).
    pub(super) fn module_error_to_compiler_error(
        err: ModuleError,
        span: Span,
        private_item_qualified: bool,
    ) -> CompilerError {
        match err {
            ModuleError::NotFound {
                path,
                searched_paths,
                ..
            } => CompilerError::ModuleNotFound {
                name: format!(
                    "{} (searched: {})",
                    path.join("::"),
                    searched_paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                span,
            },
            ModuleError::ReadError { path, error, .. } => CompilerError::ModuleReadError {
                path: path.display().to_string(),
                error,
                span,
            },
            ModuleError::CircularImport { cycle, .. } => CompilerError::CircularImport {
                cycle: cycle.join(" -> "),
                span,
            },
            ModuleError::PrivateItem { item, module, .. } => CompilerError::PrivateImport {
                name: if private_item_qualified {
                    format!("{module}::{item}")
                } else {
                    format!("{item} from module {module}")
                },
                span,
            },
            ModuleError::ItemNotFound {
                item,
                module,
                available,
                ..
            } => CompilerError::ImportItemNotFound {
                item,
                module,
                available: available.join(", "),
                span,
            },
        }
    }

    /// Check for a potential circular import and register the import edge.
    /// Returns `true` if the import is valid (or there is no current file context).
    /// Returns `false` and pushes a `CircularImport` error if the import would create a cycle.
    fn check_and_register_import(&mut self, module_path: &std::path::Path, span: Span) -> bool {
        if let Some(current_path) = &self.current_file {
            let current_path = current_path.clone();
            let module_path_buf = module_path.to_path_buf();
            if let Some(cycle) = self
                .import_graph
                .would_create_cycle(&current_path, &module_path_buf)
            {
                let mut full_cycle = cycle;
                full_cycle.insert(0, current_path);
                self.errors.push(CompilerError::CircularImport {
                    cycle: full_cycle
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" -> "),
                    span,
                });
                return false;
            }
            self.import_graph
                .add_import(current_path, module_path.to_path_buf());
        }
        true
    }

    /// Parse and analyse the module in `module_path`, and lower it on
    /// top of the modules that it imports. Returns its symbol table.
    ///
    /// The module gets the prelude, as the entry module does, and a
    /// new analyzer of its own: no state of the importing module leaks
    /// into it. The two analyzers share the resolver and the caches of
    /// the modules.
    fn parse_and_analyze_module(
        &mut self,
        source: &str,
        module_path: &Path,
    ) -> Result<SymbolTable, Vec<CompilerError>> {
        let (tokens, lex_errors) = crate::lexer::Lexer::tokenize_all_with_errors(source);
        if !lex_errors.is_empty() {
            return Err(lex_errors);
        }
        let mut file =
            crate::parser::parse_file_with_source(&tokens, source).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|(message, span)| CompilerError::ParseError {
                        message: format!("In module {}: {}", module_path.display(), message),
                        span,
                    })
                    .collect::<Vec<_>>()
            })?;
        let prelude = crate::parse_prelude_file()?;
        let prelude_len = prelude.statements.len();
        let mut statements = prelude.statements;
        statements.append(&mut file.statements);
        file.statements = statements;

        let resolver: &dyn ModuleResolver = &self.resolver;
        let mut module =
            SemanticAnalyzer::<&dyn ModuleResolver>::new_with_file(resolver, module_path.into());
        module.module_cache = std::mem::take(&mut self.module_cache);
        module.module_ir_cache = std::mem::take(&mut self.module_ir_cache);
        module.import_graph = std::mem::take(&mut self.import_graph);
        module.module_links = std::mem::take(&mut self.module_links);
        module.module_links.depth = module.module_links.depth.saturating_add(1);

        module.run_passes(&mut file);

        module.module_links.depth = module.module_links.depth.saturating_sub(1);
        self.module_cache = std::mem::take(&mut module.module_cache);
        self.module_ir_cache = std::mem::take(&mut module.module_ir_cache);
        self.import_graph = std::mem::take(&mut module.import_graph);
        self.module_links = std::mem::take(&mut module.module_links);
        let errors = SemanticAnalyzer::<&dyn ModuleResolver>::deduplicated(&module.errors);
        let symbols = module.symbols;
        if !errors.is_empty() {
            return Err(errors);
        }

        let linked = self.module_links.lower(
            &file,
            prelude_len,
            &symbols,
            Some(module_path),
            Some(module_path.to_path_buf()),
        )?;
        self.module_links.store(module_path, linked);
        if let Some(view) = self.module_links.view(module_path) {
            self.module_ir_cache.insert(module_path.to_path_buf(), view);
        }
        self.module_cache
            .insert(module_path.to_path_buf(), (file, symbols.clone()));
        Ok(symbols)
    }

    /// Lower the entry module `ast` on top of every module that it
    /// imports, and give the imported items their final names.
    ///
    /// # Errors
    ///
    /// The errors of the lowering.
    pub(crate) fn lower_entry(
        &self,
        ast: &File,
        prelude_len: usize,
        path: Option<std::path::PathBuf>,
    ) -> Result<crate::ir::IrModule, Vec<CompilerError>> {
        let linked = self
            .module_links
            .lower(ast, prelude_len, &self.symbols, None, path)?;
        let mut module = linked.ir;
        crate::ir::link::finish_entry(&mut module, self.module_links.logical_paths());
        Ok(module)
    }
}

/// The names of the top-level inline modules of `file`.
fn inline_module_names(file: &File) -> std::collections::HashSet<&str> {
    file.statements
        .iter()
        .filter_map(|statement| {
            if let Statement::Definition(def) = statement {
                if let crate::ast::Definition::Module(m) = &**def {
                    return Some(m.name.name.as_str());
                }
            }
            None
        })
        .collect()
}
