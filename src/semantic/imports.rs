//! Pass 0 — module resolution and `use`-statement handling.
//!
//! Drives loading and analysing imported modules through the generic
//! `R: ModuleResolver`. After resolving each module's source, this pass
//! parses it, builds its symbol table, runs all later passes against that
//! cached table, and lowers it to IR for downstream backends.

use super::helpers::{collect_bindings_from_pattern, is_primitive_name};
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
        for statement in &file.statements {
            if let Statement::Use(use_stmt) = statement {
                self.process_use_statement(use_stmt);
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

        self.import_use_items(
            &use_stmt.items,
            &module_symbols,
            &module_path,
            &path_segments,
            use_stmt.span,
        );
    }

    /// Dispatch symbol imports for all `UseItems` variants in `process_use_statement`
    fn import_use_items(
        &mut self,
        items: &UseItems,
        module_symbols: &SymbolTable,
        module_path: &std::path::Path,
        path_segments: &[String],
        span: Span,
    ) {
        match items {
            UseItems::Single(ident) => {
                self.import_symbol(
                    &ident.name,
                    module_symbols,
                    module_path,
                    path_segments.to_vec(),
                    span,
                );
            }
            UseItems::Multiple(idents) => {
                for ident in idents {
                    self.import_symbol(
                        &ident.name,
                        module_symbols,
                        module_path,
                        path_segments.to_vec(),
                        span,
                    );
                }
            }
            UseItems::Glob => {
                for name in module_symbols.all_public_symbols() {
                    self.import_symbol(
                        &name,
                        module_symbols,
                        module_path,
                        path_segments.to_vec(),
                        span,
                    );
                }
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

    /// Parse and analyze a module, returning its symbol table
    fn parse_and_analyze_module(
        &mut self,
        source: &str,
        module_path: &Path,
    ) -> Result<SymbolTable, Vec<CompilerError>> {
        // Parse the module
        use crate::lexer::Lexer;
        use crate::parser;

        let tokens = Lexer::tokenize_all(source);

        let file = match parser::parse_file_with_source(&tokens, source) {
            Ok(file) => file,
            Err(errors) => {
                // Convert parse errors to compiler errors
                let compiler_errors: Vec<CompilerError> = errors
                    .into_iter()
                    .map(|(message, span)| CompilerError::ParseError {
                        message: format!("In module {}: {}", module_path.display(), message),
                        span,
                    })
                    .collect();
                return Err(compiler_errors);
            }
        };

        // Create a new analyzer for the module with the same resolver
        // Note: We need to temporarily take ownership of the resolver
        // This is a design challenge - we may need to refactor to use &R or Rc<R>
        // Build the symbol table directly without a full recursive analysis
        let mut module_symbols = SymbolTable::new();
        let mut module_errors = Vec::new();

        // Pass 1: Build symbol table for the module's own definitions
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                Self::collect_definition_into(&mut module_symbols, &mut module_errors, def);
            } else if let Statement::Let(let_binding) = statement {
                // Register all bindings from the pattern (simple, array, struct, tuple)
                for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                    if is_primitive_name(&binding.name) {
                        module_errors.push(CompilerError::PrimitiveRedefinition {
                            name: binding.name.clone(),
                            span: binding.span,
                        });
                        continue;
                    }
                    if let Some((kind, _)) = module_symbols.define_let(
                        binding.name.clone(),
                        let_binding.visibility,
                        let_binding.span,
                        let_binding.doc.clone(),
                    ) {
                        module_errors.push(CompilerError::DuplicateDefinition {
                            name: format!(
                                "{} (already defined as {})",
                                binding.name,
                                kind.as_str()
                            ),
                            span: binding.span,
                        });
                    }
                }
            }
        }

        if !module_errors.is_empty() {
            return Err(module_errors);
        }

        // Cache the module first (with just definitions) to prevent infinite
        // recursion during use-statement processing if two modules pub-use
        // each other.
        self.module_cache.insert(
            module_path.to_path_buf(),
            (file.clone(), module_symbols.clone()),
        );

        // Run the remaining analysis passes on the module with its own
        // symbol table temporarily installed as `self.symbols`. This covers:
        //   Pass 0  — use-statement resolution (both pub and private)
        //   Pass 1.5 — validate_generic_parameters
        //   Pass 1.6 — infer_let_types
        //   Pass 2  — resolve_types
        //   Pass 3  — validate_expressions
        //   Pass 4  — validate_trait_implementations
        //   Pass 5  — detect_circular_dependencies
        let saved_current_file = self.current_file.take();
        self.current_file = Some(module_path.to_path_buf());
        let saved_symbols = std::mem::replace(&mut self.symbols, module_symbols);
        let saved_errors = std::mem::take(&mut self.errors);
        let saved_impl_struct = self.current_impl_struct.take();
        let saved_generic_scopes = std::mem::take(&mut self.generic_scopes);
        let saved_loop_var_scopes = std::mem::take(&mut self.loop_var_scopes);
        let saved_closure_param_scopes = std::mem::take(&mut self.closure_param_scopes);
        let saved_local_let_bindings = std::mem::take(&mut self.local_let_bindings);
        let saved_consumed_bindings = std::mem::take(&mut self.consumed_bindings);

        self.resolve_modules(&file);
        self.validate_generic_parameters(&file);
        self.infer_let_types(&file);
        self.resolve_types(&file);
        self.validate_expressions(&file);
        self.validate_trait_implementations(&file);
        self.detect_circular_dependencies(&file);

        module_symbols = std::mem::replace(&mut self.symbols, saved_symbols);
        let pass_errors = std::mem::replace(&mut self.errors, saved_errors);
        module_errors.extend(pass_errors);
        self.current_impl_struct = saved_impl_struct;
        self.generic_scopes = saved_generic_scopes;
        self.loop_var_scopes = saved_loop_var_scopes;
        self.closure_param_scopes = saved_closure_param_scopes;
        self.local_let_bindings = saved_local_let_bindings;
        self.consumed_bindings = saved_consumed_bindings;
        self.current_file = saved_current_file;

        // Update the cache with the final symbol table (post-passes).
        self.module_cache.insert(
            module_path.to_path_buf(),
            (file.clone(), module_symbols.clone()),
        );

        if !module_errors.is_empty() {
            return Err(module_errors);
        }

        // Lower the module to IR and cache it for codegen backends
        // This enables generating impl blocks from imported types
        if let Ok(ir_module) = crate::ir::lower_to_ir(&file, &module_symbols) {
            self.module_ir_cache
                .insert(module_path.to_path_buf(), ir_module);
        }
        // Note: If IR lowering fails, we still return the symbol table successfully
        // since semantic analysis passed. IR errors would be caught during main file lowering.

        Ok(module_symbols)
    }
}
