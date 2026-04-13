// Semantic analysis (validation only - no evaluation or expansion)
// Pass 1: Build symbol table
// Pass 2: Resolve type references
// Pass 3: Validate expressions (operators, for/if/match)
// Pass 4: Validate trait composition (model trait field requirements only; view traits are categories)
// Pass 5: Detect circular dependencies

pub(crate) mod import_graph;
pub mod module_resolver;
pub mod node_finder;
pub mod position;
pub mod queries;
pub mod symbol_table;
pub(crate) mod type_graph;

use crate::ast::{
    ArrayPatternElement, BinaryOperator, BindingPattern, BlockStatement, Definition, Expr, File,
    Literal, Pattern, PrimitiveType, Statement, StructDef, TraitDef, Type, UnaryOperator, UseItems,
    UseStmt, Visibility,
};
use crate::error::CompilerError;
use crate::location::Span;
use import_graph::ImportGraph;
use module_resolver::{ModuleError, ModuleResolver};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use symbol_table::SymbolTable;
use type_graph::TypeGraph;

/// Tracks generic parameters in scope for a definition
#[derive(Debug, Clone)]
struct GenericScope {
    /// Generic parameter names and their constraints
    params: HashMap<String, Vec<String>>, // name -> list of trait constraints
}

/// Represents a binding extracted from a pattern
#[derive(Debug, Clone)]
struct PatternBinding {
    name: String,
    span: Span,
}

/// Collect all binding names from a pattern recursively
fn collect_bindings_from_pattern(pattern: &BindingPattern) -> Vec<PatternBinding> {
    let mut bindings = Vec::new();
    collect_bindings_recursive(pattern, &mut bindings);
    bindings
}

fn collect_bindings_recursive(pattern: &BindingPattern, bindings: &mut Vec<PatternBinding>) {
    match pattern {
        BindingPattern::Simple(ident) => {
            bindings.push(PatternBinding {
                name: ident.name.clone(),
                span: ident.span,
            });
        }
        BindingPattern::Array { elements, .. } => {
            for element in elements {
                match element {
                    ArrayPatternElement::Binding(inner) => {
                        collect_bindings_recursive(inner, bindings);
                    }
                    ArrayPatternElement::Rest(Some(ident)) => {
                        bindings.push(PatternBinding {
                            name: ident.name.clone(),
                            span: ident.span,
                        });
                    }
                    ArrayPatternElement::Rest(None) | ArrayPatternElement::Wildcard => {
                        // No binding for anonymous rest or wildcard
                    }
                }
            }
        }
        BindingPattern::Struct { fields, .. } => {
            for field in fields {
                // Use alias if present, otherwise use field name
                let binding_ident = field.alias.as_ref().unwrap_or(&field.name);
                bindings.push(PatternBinding {
                    name: binding_ident.name.clone(),
                    span: binding_ident.span,
                });
            }
        }
        BindingPattern::Tuple { elements, .. } => {
            for element in elements {
                collect_bindings_recursive(element, bindings);
            }
        }
    }
}

/// Semantic analyzer validates the AST without evaluation or expansion
pub struct SemanticAnalyzer<R: ModuleResolver> {
    symbols: SymbolTable,
    errors: Vec<CompilerError>,
    resolver: R,
    import_graph: ImportGraph,
    /// Cache of parsed modules (path -> (AST, SymbolTable))
    module_cache: HashMap<PathBuf, (File, SymbolTable)>,
    /// Cache of IR modules for imported modules (keyed by file path)
    ///
    /// Populated during `parse_and_analyze_module()` to enable WGSL codegen
    /// to generate impl blocks from imported types.
    module_ir_cache: HashMap<PathBuf, crate::ir::IrModule>,
    /// Current file path being analyzed
    current_file: Option<PathBuf>,
    /// Stack of generic scopes (for tracking type parameters)
    generic_scopes: Vec<GenericScope>,
    /// Current struct name when inside an impl block (for field type resolution)
    current_impl_struct: Option<String>,
    /// Stack of loop variable scopes (for tracking for loop bindings)
    loop_var_scopes: Vec<HashSet<String>>,
    /// Stack of closure parameter scopes (for tracking closure/event mapping params)
    closure_param_scopes: Vec<HashSet<String>>,
    /// Local let bindings in current expression context: (type, mutable)
    local_let_bindings: HashMap<String, (String, bool)>,
    /// Recursion depth counter for validate_expr (to prevent stack overflow)
    validate_expr_depth: usize,
}

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    pub fn new(resolver: R) -> Self {
        Self {
            symbols: SymbolTable::new(),
            errors: Vec::new(),
            resolver,
            import_graph: ImportGraph::new(),
            module_cache: HashMap::new(),
            module_ir_cache: HashMap::new(),
            current_file: None,
            generic_scopes: Vec::new(),
            current_impl_struct: None,
            loop_var_scopes: Vec::new(),
            closure_param_scopes: Vec::new(),
            local_let_bindings: HashMap::new(),
            validate_expr_depth: 0,
        }
    }

    /// Create a new analyzer with a specific file path
    pub fn new_with_file(resolver: R, file_path: PathBuf) -> Self {
        Self {
            symbols: SymbolTable::new(),
            errors: Vec::new(),
            resolver,
            import_graph: ImportGraph::new(),
            module_cache: HashMap::new(),
            module_ir_cache: HashMap::new(),
            current_file: Some(file_path),
            generic_scopes: Vec::new(),
            current_impl_struct: None,
            loop_var_scopes: Vec::new(),
            closure_param_scopes: Vec::new(),
            local_let_bindings: HashMap::new(),
            validate_expr_depth: 0,
        }
    }

    /// Pass 0: Module resolution
    /// Resolve all use statements, load imported modules, and check for circular dependencies
    fn resolve_modules(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Use(use_stmt) = statement {
                self.process_use_statement(use_stmt);
            }
        }
    }

    /// Process a single use statement
    fn process_use_statement(&mut self, use_stmt: &UseStmt) {
        // Convert path to string segments
        let path_segments: Vec<String> = use_stmt
            .path
            .iter()
            .map(|ident| ident.name.clone())
            .collect();

        // Resolve the module path
        let (source, module_path) = match self
            .resolver
            .resolve(&path_segments, self.current_file.as_ref())
        {
            Ok(result) => result,
            Err(ModuleError::NotFound {
                path,
                searched_paths,
                ..
            }) => {
                self.errors.push(CompilerError::ModuleNotFound {
                    name: format!(
                        "{} (searched: {})",
                        path.join("::"),
                        searched_paths
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    span: use_stmt.span,
                });
                return;
            }
            Err(ModuleError::ReadError { path, error, .. }) => {
                self.errors.push(CompilerError::ModuleReadError {
                    path: path.display().to_string(),
                    error,
                    span: use_stmt.span,
                });
                return;
            }
            Err(ModuleError::CircularImport { cycle, .. }) => {
                self.errors.push(CompilerError::CircularImport {
                    cycle: cycle.join(" -> "),
                    span: use_stmt.span,
                });
                return;
            }
            Err(ModuleError::PrivateItem { item, module, .. }) => {
                self.errors.push(CompilerError::PrivateImport {
                    name: format!("{} from module {}", item, module),
                    span: use_stmt.span,
                });
                return;
            }
            Err(ModuleError::ItemNotFound {
                item,
                module,
                available,
                ..
            }) => {
                self.errors.push(CompilerError::ImportItemNotFound {
                    item,
                    module,
                    available: available.join(", "),
                    span: use_stmt.span,
                });
                return;
            }
        };

        // Check for circular imports
        if let Some(current_path) = &self.current_file {
            if let Some(cycle) = self
                .import_graph
                .would_create_cycle(current_path, &module_path)
            {
                let mut full_cycle = cycle;
                full_cycle.push(current_path.clone());
                self.errors.push(CompilerError::CircularImport {
                    cycle: full_cycle
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" -> "),
                    span: use_stmt.span,
                });
                return;
            }

            // Add the import edge
            self.import_graph
                .add_import(current_path.clone(), module_path.clone());
        }

        // Parse and analyze the module if not already cached
        let module_symbols = if let Some((_, symbols)) = self.module_cache.get(&module_path) {
            symbols.clone()
        } else {
            match self.parse_and_analyze_module(&source, &module_path) {
                Ok(symbols) => symbols,
                Err(errors) => {
                    // Add all errors from the imported module
                    self.errors.extend(errors);
                    return;
                }
            }
        };

        // Import the requested symbols
        match &use_stmt.items {
            UseItems::Single(ident) => {
                self.import_symbol(
                    &ident.name,
                    &module_symbols,
                    module_path.clone(),
                    path_segments.clone(),
                    use_stmt.span,
                );
            }
            UseItems::Multiple(idents) => {
                for ident in idents {
                    self.import_symbol(
                        &ident.name,
                        &module_symbols,
                        module_path.clone(),
                        path_segments.clone(),
                        use_stmt.span,
                    );
                }
            }
            UseItems::Glob => {
                // Import all public symbols from the module
                for name in module_symbols.all_public_symbols() {
                    self.import_symbol(
                        &name,
                        &module_symbols,
                        module_path.clone(),
                        path_segments.clone(),
                        use_stmt.span,
                    );
                }
            }
        }
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
                    if let Some((kind, _)) = module_symbols.define_let(
                        binding.name.clone(),
                        let_binding.visibility,
                        let_binding.span,
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

        // Pass 2: Process pub use statements for re-exports
        // We need to cache the module first (with just definitions) to prevent infinite recursion
        // in case of circular pub use statements
        self.module_cache.insert(
            module_path.to_path_buf(),
            (file.clone(), module_symbols.clone()),
        );

        // Now process pub use statements
        for statement in &file.statements {
            if let Statement::Use(use_stmt) = statement {
                // Only process public use statements for re-export
                if use_stmt.visibility == Visibility::Public {
                    self.process_pub_use_for_module(
                        use_stmt,
                        &mut module_symbols,
                        &mut module_errors,
                    );
                }
            }
        }

        // Update the cache with the final symbol table (including re-exports)
        self.module_cache.insert(
            module_path.to_path_buf(),
            (file.clone(), module_symbols.clone()),
        );

        if !module_errors.is_empty() {
            return Err(module_errors);
        }

        // Lower the module to IR and cache it for WGSL codegen
        // This enables generating impl blocks from imported types
        if let Ok(ir_module) = crate::ir::lower_to_ir(&file, &module_symbols) {
            self.module_ir_cache
                .insert(module_path.to_path_buf(), ir_module);
        }
        // Note: If IR lowering fails, we still return the symbol table successfully
        // since semantic analysis passed. IR errors would be caught during main file lowering.

        Ok(module_symbols)
    }

    /// Process a pub use statement for a module being loaded
    /// This adds the re-exported symbols to the module's symbol table
    fn process_pub_use_for_module(
        &mut self,
        use_stmt: &UseStmt,
        module_symbols: &mut SymbolTable,
        module_errors: &mut Vec<CompilerError>,
    ) {
        // Convert path to string segments
        let path_segments: Vec<String> = use_stmt
            .path
            .iter()
            .map(|ident| ident.name.clone())
            .collect();

        // Resolve the module path
        let (source, imported_module_path) = match self.resolver.resolve(&path_segments, None) {
            Ok(result) => result,
            Err(ModuleError::NotFound {
                path,
                searched_paths,
                ..
            }) => {
                module_errors.push(CompilerError::ModuleNotFound {
                    name: format!(
                        "{} (searched: {})",
                        path.join("::"),
                        searched_paths
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    span: use_stmt.span,
                });
                return;
            }
            Err(ModuleError::ReadError { path, error, .. }) => {
                module_errors.push(CompilerError::ModuleReadError {
                    path: path.display().to_string(),
                    error,
                    span: use_stmt.span,
                });
                return;
            }
            Err(ModuleError::CircularImport { cycle, .. }) => {
                module_errors.push(CompilerError::CircularImport {
                    cycle: cycle.join(" -> "),
                    span: use_stmt.span,
                });
                return;
            }
            Err(ModuleError::PrivateItem { item, module, .. }) => {
                module_errors.push(CompilerError::PrivateImport {
                    name: format!("{}::{}", module, item),
                    span: use_stmt.span,
                });
                return;
            }
            Err(ModuleError::ItemNotFound {
                item,
                module,
                available,
                ..
            }) => {
                module_errors.push(CompilerError::ImportItemNotFound {
                    item,
                    module,
                    available: available.join(", "),
                    span: use_stmt.span,
                });
                return;
            }
        };

        // Load and parse the imported module (recursively handles its own pub use statements)
        let imported_symbols =
            if let Some((_, symbols)) = self.module_cache.get(&imported_module_path) {
                symbols.clone()
            } else {
                match self.parse_and_analyze_module(&source, &imported_module_path) {
                    Ok(symbols) => symbols,
                    Err(errors) => {
                        module_errors.extend(errors);
                        return;
                    }
                }
            };

        // Re-export the symbols with public visibility
        match &use_stmt.items {
            UseItems::Single(ident) => {
                Self::reexport_symbol(
                    &ident.name,
                    &imported_symbols,
                    module_symbols,
                    imported_module_path.clone(),
                    path_segments.clone(),
                );
            }
            UseItems::Multiple(idents) => {
                for ident in idents {
                    Self::reexport_symbol(
                        &ident.name,
                        &imported_symbols,
                        module_symbols,
                        imported_module_path.clone(),
                        path_segments.clone(),
                    );
                }
            }
            UseItems::Glob => {
                // Re-export all public symbols from the imported module
                for name in imported_symbols.all_public_symbols() {
                    Self::reexport_symbol(
                        &name,
                        &imported_symbols,
                        module_symbols,
                        imported_module_path.clone(),
                        path_segments.clone(),
                    );
                }
            }
        }
    }

    /// Re-export a symbol from one module into another module's symbol table
    fn reexport_symbol(
        name: &str,
        source_symbols: &SymbolTable,
        target_symbols: &mut SymbolTable,
        module_path: PathBuf,
        logical_path: Vec<String>,
    ) {
        // Import the symbol into the target symbol table with public visibility
        // This makes it available for further re-export
        let _ = target_symbols.import_symbol(name, source_symbols, module_path, logical_path);
    }

    /// Helper to collect a definition into a symbol table (static version for module parsing)
    fn collect_definition_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        def: &Definition,
    ) {
        match def {
            Definition::Trait(trait_def) => {
                // Collect field requirements
                let fields: HashMap<String, Type> = trait_def
                    .fields
                    .iter()
                    .map(|f| (f.name.name.clone(), f.ty.clone()))
                    .collect();

                // Collect mounting point requirements
                let mount_fields: HashMap<String, Type> = trait_def
                    .mount_fields
                    .iter()
                    .map(|f| (f.name.name.clone(), f.ty.clone()))
                    .collect();

                // Collect composed trait names
                let composed_traits: Vec<String> =
                    trait_def.traits.iter().map(|t| t.name.clone()).collect();

                let result = symbols.define_trait(
                    trait_def.name.name.clone(),
                    trait_def.visibility,
                    trait_def.span,
                    trait_def.generics.clone(),
                    fields,
                    mount_fields,
                    composed_traits,
                );

                if let Some((kind, _)) = result {
                    errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            trait_def.name.name,
                            kind.as_str()
                        ),
                        span: trait_def.name.span,
                    });
                }
            }
            Definition::Struct(struct_def) => {
                use symbol_table::FieldInfo;

                let traits: Vec<String> =
                    struct_def.traits.iter().map(|t| t.name.clone()).collect();
                let fields: Vec<FieldInfo> = struct_def
                    .fields
                    .iter()
                    .map(|f| FieldInfo {
                        name: f.name.name.clone(),
                        ty: f.ty.clone(),
                    })
                    .collect();
                let mount_fields: Vec<FieldInfo> = struct_def
                    .mount_fields
                    .iter()
                    .map(|f| FieldInfo {
                        name: f.name.name.clone(),
                        ty: f.ty.clone(),
                    })
                    .collect();

                if let Some((kind, _)) = symbols.define_struct(
                    struct_def.name.name.clone(),
                    struct_def.visibility,
                    struct_def.span,
                    struct_def.generics.clone(),
                    traits,
                    fields,
                    mount_fields,
                ) {
                    errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            struct_def.name.name,
                            kind.as_str()
                        ),
                        span: struct_def.name.span,
                    });
                }
            }
            Definition::Impl(impl_def) => {
                use symbol_table::ImplInfo;

                if let Some(trait_ident) = &impl_def.trait_name {
                    // Trait implementation: impl Trait for Struct
                    // Note: We can't validate trait/struct existence here because this is
                    // a static method called during module loading. Full validation happens
                    // later in collect_definition.
                    symbols
                        .trait_impls
                        .entry(impl_def.name.name.clone())
                        .or_default()
                        .push(symbol_table::TraitImplInfo {
                            trait_name: trait_ident.name.clone(),
                            struct_name: impl_def.name.name.clone(),
                            generics: impl_def.generics.clone(),
                            span: impl_def.span,
                        });
                } else {
                    // Inherent implementation: impl Struct
                    let info = ImplInfo {
                        struct_name: impl_def.name.name.clone(),
                        generics: impl_def.generics.clone(),
                        span: impl_def.span,
                    };

                    if let Some((kind, _)) = symbols.define_impl(impl_def.name.name.clone(), info) {
                        errors.push(CompilerError::DuplicateDefinition {
                            name: format!(
                                "{} (already defined as {})",
                                impl_def.name.name,
                                kind.as_str()
                            ),
                            span: impl_def.name.span,
                        });
                    }
                }
            }
            Definition::Enum(enum_def) => {
                let variants = enum_def
                    .variants
                    .iter()
                    .map(|v| (v.name.name.clone(), (v.fields.len(), v.span)))
                    .collect();

                if let Some((kind, _)) = symbols.define_enum(
                    enum_def.name.name.clone(),
                    enum_def.visibility,
                    enum_def.span,
                    enum_def.generics.clone(),
                    variants,
                    Vec::new(), // Enums don't support inline trait syntax yet
                ) {
                    errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            enum_def.name.name,
                            kind.as_str()
                        ),
                        span: enum_def.name.span,
                    });
                }
            }
            Definition::Module(module_def) => {
                // Create a nested symbol table for the module
                let mut module_symbols = SymbolTable::new();

                // Collect all definitions within the module
                for nested_def in &module_def.definitions {
                    Self::collect_definition_into(&mut module_symbols, errors, nested_def);
                }

                // Register the module in the parent symbol table
                if let Some((kind, _)) = symbols.define_module(
                    module_def.name.name.clone(),
                    module_def.visibility,
                    module_def.span,
                    module_symbols,
                ) {
                    errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            module_def.name.name,
                            kind.as_str()
                        ),
                        span: module_def.name.span,
                    });
                }
            }
            Definition::Function(func_def) => {
                // Register standalone function in symbol table
                let params: Vec<(String, Option<Type>)> = func_def
                    .params
                    .iter()
                    .map(|p| (p.name.name.clone(), p.ty.clone()))
                    .collect();

                if let Some((kind, _)) = symbols.define_function(
                    func_def.name.name.clone(),
                    func_def.visibility,
                    func_def.span,
                    params,
                    func_def.return_type.clone(),
                ) {
                    errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            func_def.name.name,
                            kind.as_str()
                        ),
                        span: func_def.name.span,
                    });
                }
            }
        }
    }

    /// Import a single symbol from a module
    fn import_symbol(
        &mut self,
        name: &str,
        module_symbols: &SymbolTable,
        module_path: PathBuf,
        logical_path: Vec<String>,
        span: Span,
    ) {
        use symbol_table::ImportError;

        match self
            .symbols
            .import_symbol(name, module_symbols, module_path.clone(), logical_path)
        {
            Ok(()) => {
                // Success
            }
            Err(ImportError::PrivateItem { name, kind: _ }) => {
                self.errors.push(CompilerError::PrivateImport {
                    name: format!("{} from module {}", name, module_path.display()),
                    span,
                });
            }
            Err(ImportError::ItemNotFound { name, available }) => {
                self.errors.push(CompilerError::ImportItemNotFound {
                    item: name,
                    module: module_path.display().to_string(),
                    available: available.join(", "),
                    span,
                });
            }
        }
    }

    /// Analyze a file and validate all semantic rules
    pub fn analyze(&mut self, file: &File) -> Result<(), Vec<CompilerError>> {
        // Pass 0: Module resolution (process use statements)
        self.resolve_modules(file);

        // Pass 1: Build symbol table (collect all definitions)
        self.build_symbol_table(file);

        // Pass 1.5: Validate generic parameters
        self.validate_generic_parameters(file);

        // Pass 1.6: Infer let binding types
        self.infer_let_types(file);

        // Pass 2: Resolve type references
        self.resolve_types(file);

        // Pass 3: Validate expressions
        self.validate_expressions(file);

        // Pass 4: Validate trait implementations (field requirements)
        self.validate_trait_implementations(file);

        // Pass 5: Detect circular dependencies
        self.detect_circular_dependencies(file);

        // Return errors if any
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    /// Analyze a file, validate all semantic rules, and classify fields
    pub fn analyze_and_classify(&mut self, file: &mut File) -> Result<(), Vec<CompilerError>> {
        // Pass 0: Module resolution (process use statements)
        self.resolve_modules(file);

        // Pass 1: Build symbol table (collect all definitions)
        self.build_symbol_table(file);

        // Pass 1.5: Validate generic parameters
        self.validate_generic_parameters(file);

        // Pass 1.6: Infer let binding types
        self.infer_let_types(file);

        // Pass 2: Resolve type references
        self.resolve_types(file);

        // Pass 3: Validate expressions
        self.validate_expressions(file);

        // Pass 4: Validate trait implementations (field requirements)
        self.validate_trait_implementations(file);

        // Pass 5: Detect circular dependencies
        self.detect_circular_dependencies(file);

        // Return errors if any
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    /// Pass 1.5: Validate generic parameters
    /// Check for duplicate parameters and validate constraints
    fn validate_generic_parameters(&mut self, file: &File) {
        use crate::ast::GenericConstraint;

        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                let generics = match &**def {
                    Definition::Trait(trait_def) => &trait_def.generics,
                    Definition::Struct(struct_def) => &struct_def.generics,
                    Definition::Impl(impl_def) => &impl_def.generics,
                    Definition::Enum(enum_def) => &enum_def.generics,
                    Definition::Module(_) | Definition::Function(_) => continue, // No generics, skip
                };

                // Check for duplicate generic parameters
                let mut seen_params = HashSet::new();
                for param in generics {
                    if !seen_params.insert(&param.name.name) {
                        self.errors.push(CompilerError::DuplicateGenericParam {
                            param: param.name.name.clone(),
                            span: param.span,
                        });
                    }

                    // Validate constraints reference valid traits
                    for constraint in &param.constraints {
                        match constraint {
                            GenericConstraint::Trait(trait_ref) => {
                                if !self.symbols.is_trait(&trait_ref.name) {
                                    self.errors.push(CompilerError::UndefinedTrait {
                                        name: trait_ref.name.clone(),
                                        span: trait_ref.span,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Pass 1.6: Infer let binding types
    /// Infer the type of each let binding from its value expression
    fn infer_let_types(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                let inferred_type = self.infer_type(&let_binding.value, file);
                // Store type for all bindings in the pattern
                // For destructuring, each binding gets the inferred type of its position
                // (simplified: using source type for now, proper element types would need more work)
                for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                    self.symbols
                        .set_let_type(&binding.name, inferred_type.clone());
                }
            }
        }
    }

    /// Pass 1: Build symbol table
    /// Collect all definitions and detect duplicates
    fn build_symbol_table(&mut self, file: &File) {
        for statement in &file.statements {
            match statement {
                Statement::Use(_) => {
                    // Module resolution handled in Pass 0
                }
                Statement::Let(let_binding) => {
                    // Register all bindings from the pattern (simple, array, struct, tuple)
                    for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                        if let Some((kind, _)) = self.symbols.define_let(
                            binding.name.clone(),
                            let_binding.visibility,
                            let_binding.span,
                        ) {
                            self.errors.push(CompilerError::DuplicateDefinition {
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
                Statement::Definition(def) => {
                    self.collect_definition(def);
                }
            }
        }
    }

    /// Collect a single definition
    fn collect_definition(&mut self, def: &Definition) {
        match def {
            Definition::Trait(trait_def) => {
                // Collect field requirements
                let fields: HashMap<String, Type> = trait_def
                    .fields
                    .iter()
                    .map(|f| (f.name.name.clone(), f.ty.clone()))
                    .collect();

                // Collect mounting point requirements
                let mount_fields: HashMap<String, Type> = trait_def
                    .mount_fields
                    .iter()
                    .map(|f| (f.name.name.clone(), f.ty.clone()))
                    .collect();

                // Collect composed trait names
                let composed_traits: Vec<String> =
                    trait_def.traits.iter().map(|t| t.name.clone()).collect();

                // Define unified trait
                let result = self.symbols.define_trait(
                    trait_def.name.name.clone(),
                    trait_def.visibility,
                    trait_def.span,
                    trait_def.generics.clone(),
                    fields,
                    mount_fields,
                    composed_traits,
                );

                if let Some((kind, _)) = result {
                    self.errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            trait_def.name.name,
                            kind.as_str()
                        ),
                        span: trait_def.name.span,
                    });
                }
            }
            Definition::Struct(struct_def) => {
                use symbol_table::FieldInfo;

                let traits: Vec<String> =
                    struct_def.traits.iter().map(|t| t.name.clone()).collect();
                let fields: Vec<FieldInfo> = struct_def
                    .fields
                    .iter()
                    .map(|f| FieldInfo {
                        name: f.name.name.clone(),
                        ty: f.ty.clone(),
                    })
                    .collect();
                let mount_fields: Vec<FieldInfo> = struct_def
                    .mount_fields
                    .iter()
                    .map(|f| FieldInfo {
                        name: f.name.name.clone(),
                        ty: f.ty.clone(),
                    })
                    .collect();

                if let Some((kind, _)) = self.symbols.define_struct(
                    struct_def.name.name.clone(),
                    struct_def.visibility,
                    struct_def.span,
                    struct_def.generics.clone(),
                    traits,
                    fields,
                    mount_fields,
                ) {
                    self.errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            struct_def.name.name,
                            kind.as_str()
                        ),
                        span: struct_def.name.span,
                    });
                }
            }
            Definition::Impl(impl_def) => {
                use symbol_table::ImplInfo;

                // Check that struct or enum exists
                let type_exists = self.symbols.get_struct(&impl_def.name.name).is_some()
                    || self
                        .symbols
                        .get_enum_variants(&impl_def.name.name)
                        .is_some();

                if !type_exists {
                    self.errors.push(CompilerError::UndefinedType {
                        name: impl_def.name.name.clone(),
                        span: impl_def.span,
                    });
                    return;
                }

                if let Some(trait_ident) = &impl_def.trait_name {
                    // Trait implementation: impl Trait for Struct
                    // Check that trait exists
                    if !self.symbols.traits.contains_key(&trait_ident.name) {
                        self.errors.push(CompilerError::UndefinedType {
                            name: trait_ident.name.clone(),
                            span: trait_ident.span,
                        });
                        return;
                    }

                    // Register using the validated method
                    if let Err((kind, _)) = self.symbols.define_trait_impl(
                        trait_ident.name.clone(),
                        impl_def.name.name.clone(),
                        impl_def.generics.clone(),
                        impl_def.span,
                    ) {
                        self.errors.push(CompilerError::DuplicateDefinition {
                            name: format!(
                                "impl {} for {} (already defined as {})",
                                trait_ident.name,
                                impl_def.name.name,
                                kind.as_str()
                            ),
                            span: impl_def.span,
                        });
                    }
                } else {
                    // Inherent implementation: impl Struct
                    let info = ImplInfo {
                        struct_name: impl_def.name.name.clone(),
                        generics: impl_def.generics.clone(),
                        span: impl_def.span,
                    };

                    if let Some((kind, _)) =
                        self.symbols.define_impl(impl_def.name.name.clone(), info)
                    {
                        self.errors.push(CompilerError::DuplicateDefinition {
                            name: format!(
                                "{} (already defined as {})",
                                impl_def.name.name,
                                kind.as_str()
                            ),
                            span: impl_def.name.span,
                        });
                    }
                }
            }
            Definition::Enum(enum_def) => {
                // Check for duplicate variants
                let mut seen_variants = std::collections::HashSet::new();
                for variant in &enum_def.variants {
                    if !seen_variants.insert(&variant.name.name) {
                        self.errors.push(CompilerError::DuplicateDefinition {
                            name: format!(
                                "enum variant '{}' in enum '{}'",
                                variant.name.name, enum_def.name.name
                            ),
                            span: variant.name.span,
                        });
                    }
                }

                // Collect variant information
                let variants = enum_def
                    .variants
                    .iter()
                    .map(|v| (v.name.name.clone(), (v.fields.len(), v.span)))
                    .collect();

                if let Some((kind, _)) = self.symbols.define_enum(
                    enum_def.name.name.clone(),
                    enum_def.visibility,
                    enum_def.span,
                    enum_def.generics.clone(),
                    variants,
                    Vec::new(), // Enums don't support inline trait syntax yet
                ) {
                    self.errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            enum_def.name.name,
                            kind.as_str()
                        ),
                        span: enum_def.name.span,
                    });
                }
            }
            Definition::Module(module_def) => {
                // Create a nested symbol table for the module
                let mut module_symbols = SymbolTable::new();

                // Collect all definitions within the module (using static helper)
                for nested_def in &module_def.definitions {
                    Self::collect_definition_into(
                        &mut module_symbols,
                        &mut self.errors,
                        nested_def,
                    );
                }

                // Register the module in the parent symbol table
                if let Some((kind, _)) = self.symbols.define_module(
                    module_def.name.name.clone(),
                    module_def.visibility,
                    module_def.span,
                    module_symbols,
                ) {
                    self.errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            module_def.name.name,
                            kind.as_str()
                        ),
                        span: module_def.name.span,
                    });
                }
            }
            Definition::Function(func_def) => {
                // Register standalone function in symbol table
                let params: Vec<(String, Option<Type>)> = func_def
                    .params
                    .iter()
                    .map(|p| (p.name.name.clone(), p.ty.clone()))
                    .collect();

                if let Some((kind, _)) = self.symbols.define_function(
                    func_def.name.name.clone(),
                    func_def.visibility,
                    func_def.span,
                    params,
                    func_def.return_type.clone(),
                ) {
                    self.errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "{} (already defined as {})",
                            func_def.name.name,
                            kind.as_str()
                        ),
                        span: func_def.name.span,
                    });
                }
            }
        }
    }

    /// Pass 2: Resolve type references
    /// Ensure all type references point to defined types
    fn resolve_types(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                match &**def {
                    Definition::Trait(trait_def) => {
                        self.resolve_trait_types(trait_def);
                    }
                    Definition::Struct(struct_def) => {
                        self.resolve_struct_types(struct_def);
                    }
                    Definition::Impl(impl_def) => {
                        // Push generic scope for this definition
                        self.push_generic_scope(&impl_def.generics);

                        // Set current impl struct for field type resolution
                        self.current_impl_struct = Some(impl_def.name.name.clone());

                        // Clear local let bindings for this impl block
                        self.local_let_bindings.clear();

                        // Validate functions in impl block
                        for func in &impl_def.functions {
                            self.validate_function_return_type(func, file);
                        }

                        // Clear impl struct context and local bindings
                        self.current_impl_struct = None;
                        self.local_let_bindings.clear();

                        // Pop generic scope
                        self.pop_generic_scope();
                    }
                    Definition::Enum(enum_def) => {
                        // Push generic scope for this definition
                        self.push_generic_scope(&enum_def.generics);

                        // Resolve associated data types (named fields)
                        for variant in &enum_def.variants {
                            for field in &variant.fields {
                                self.validate_type(&field.ty);
                            }
                        }

                        // Pop generic scope
                        self.pop_generic_scope();
                    }
                    Definition::Module(module_def) => {
                        // Temporarily import module symbols into parent for type resolution
                        // This allows module-internal references to resolve correctly
                        let module_symbols = self.collect_module_symbols(module_def);

                        // Import module symbols temporarily
                        for (name, trait_info) in &module_symbols.traits {
                            self.symbols.traits.insert(name.clone(), trait_info.clone());
                        }
                        for (name, struct_info) in &module_symbols.structs {
                            self.symbols
                                .structs
                                .insert(name.clone(), struct_info.clone());
                        }
                        for (name, enum_info) in &module_symbols.enums {
                            self.symbols.enums.insert(name.clone(), enum_info.clone());
                        }

                        // Recursively validate types in nested definitions
                        for nested_def in &module_def.definitions {
                            match nested_def {
                                Definition::Trait(trait_def) => {
                                    self.resolve_trait_types(trait_def);
                                }
                                Definition::Struct(struct_def) => {
                                    self.resolve_struct_types(struct_def);
                                }
                                Definition::Impl(impl_def) => {
                                    self.push_generic_scope(&impl_def.generics);
                                    self.current_impl_struct = Some(impl_def.name.name.clone());
                                    self.local_let_bindings.clear();
                                    for func in &impl_def.functions {
                                        self.validate_function_return_type(func, file);
                                    }
                                    self.current_impl_struct = None;
                                    self.local_let_bindings.clear();
                                    self.pop_generic_scope();
                                }
                                Definition::Enum(enum_def) => {
                                    self.push_generic_scope(&enum_def.generics);
                                    for variant in &enum_def.variants {
                                        for field in &variant.fields {
                                            self.validate_type(&field.ty);
                                        }
                                    }
                                    self.pop_generic_scope();
                                }
                                Definition::Module(nested_module) => {
                                    // Recursively process nested modules
                                    self.resolve_module_types(nested_module, file);
                                }
                                Definition::Function(func_def) => {
                                    // Validate function parameter and return types
                                    self.validate_standalone_function(func_def.as_ref(), file);
                                }
                            }
                        }

                        // Remove temporarily imported symbols (restore parent scope)
                        for name in module_symbols.traits.keys() {
                            self.symbols.traits.remove(name);
                        }
                        for name in module_symbols.structs.keys() {
                            self.symbols.structs.remove(name);
                        }
                        for name in module_symbols.enums.keys() {
                            self.symbols.enums.remove(name);
                        }
                    }
                    Definition::Function(func_def) => {
                        // Validate standalone function parameter and return types
                        self.validate_standalone_function(func_def.as_ref(), file);
                    }
                }
            }
        }
    }

    /// Collect symbols from a module definition for temporary import
    fn collect_module_symbols(&self, module_def: &crate::ast::ModuleDef) -> SymbolTable {
        let mut symbols = SymbolTable::new();
        for def in &module_def.definitions {
            match def {
                Definition::Trait(trait_def) => {
                    let fields: HashMap<String, Type> = trait_def
                        .fields
                        .iter()
                        .map(|f| (f.name.name.clone(), f.ty.clone()))
                        .collect();
                    let mount_fields: HashMap<String, Type> = trait_def
                        .mount_fields
                        .iter()
                        .map(|f| (f.name.name.clone(), f.ty.clone()))
                        .collect();
                    let composed_traits: Vec<String> =
                        trait_def.traits.iter().map(|t| t.name.clone()).collect();
                    symbols.define_trait(
                        trait_def.name.name.clone(),
                        trait_def.visibility,
                        trait_def.span,
                        trait_def.generics.clone(),
                        fields,
                        mount_fields,
                        composed_traits,
                    );
                }
                Definition::Struct(struct_def) => {
                    let traits: Vec<_> = struct_def.traits.iter().map(|t| t.name.clone()).collect();
                    let fields: Vec<_> = struct_def
                        .fields
                        .iter()
                        .map(|f| symbol_table::FieldInfo {
                            name: f.name.name.clone(),
                            ty: f.ty.clone(),
                        })
                        .collect();
                    let mount_fields: Vec<_> = struct_def
                        .mount_fields
                        .iter()
                        .map(|f| symbol_table::FieldInfo {
                            name: f.name.name.clone(),
                            ty: f.ty.clone(),
                        })
                        .collect();
                    symbols.define_struct(
                        struct_def.name.name.clone(),
                        struct_def.visibility,
                        struct_def.span,
                        struct_def.generics.clone(),
                        traits,
                        fields,
                        mount_fields,
                    );
                }
                Definition::Enum(enum_def) => {
                    let variants: HashMap<String, (usize, Span)> = enum_def
                        .variants
                        .iter()
                        .map(|v| (v.name.name.clone(), (v.fields.len(), v.span)))
                        .collect();
                    symbols.define_enum(
                        enum_def.name.name.clone(),
                        enum_def.visibility,
                        enum_def.span,
                        enum_def.generics.clone(),
                        variants,
                        Vec::new(), // Enums don't support inline trait syntax yet
                    );
                }
                Definition::Impl(_) | Definition::Module(_) | Definition::Function(_) => {}
            }
        }
        symbols
    }

    /// Resolve types in a module definition (recursive)
    fn resolve_module_types(&mut self, module_def: &crate::ast::ModuleDef, file: &File) {
        // Temporarily import module symbols
        let module_symbols = self.collect_module_symbols(module_def);
        for (name, trait_info) in &module_symbols.traits {
            self.symbols.traits.insert(name.clone(), trait_info.clone());
        }
        for (name, struct_info) in &module_symbols.structs {
            self.symbols
                .structs
                .insert(name.clone(), struct_info.clone());
        }
        for (name, enum_info) in &module_symbols.enums {
            self.symbols.enums.insert(name.clone(), enum_info.clone());
        }

        for nested_def in &module_def.definitions {
            match nested_def {
                Definition::Trait(trait_def) => {
                    self.resolve_trait_types(trait_def);
                }
                Definition::Struct(struct_def) => {
                    self.resolve_struct_types(struct_def);
                }
                Definition::Impl(impl_def) => {
                    self.push_generic_scope(&impl_def.generics);
                    self.current_impl_struct = Some(impl_def.name.name.clone());
                    self.local_let_bindings.clear();
                    self.current_impl_struct = None;
                    self.local_let_bindings.clear();
                    self.pop_generic_scope();
                }
                Definition::Enum(enum_def) => {
                    self.push_generic_scope(&enum_def.generics);
                    for variant in &enum_def.variants {
                        for field in &variant.fields {
                            self.validate_type(&field.ty);
                        }
                    }
                    self.pop_generic_scope();
                }
                Definition::Module(nested_module) => {
                    // Recursively process nested modules
                    self.resolve_module_types(nested_module, file);
                }
                Definition::Function(func_def) => {
                    // Validate function parameter and return types
                    self.validate_standalone_function(func_def.as_ref(), file);
                }
            }
        }

        // Remove temporarily imported symbols
        for name in module_symbols.traits.keys() {
            self.symbols.traits.remove(name);
        }
        for name in module_symbols.structs.keys() {
            self.symbols.structs.remove(name);
        }
        for name in module_symbols.enums.keys() {
            self.symbols.enums.remove(name);
        }
    }

    /// Resolve types in a trait definition
    fn resolve_trait_types(&mut self, trait_def: &TraitDef) {
        // Push generic scope for this definition
        self.push_generic_scope(&trait_def.generics);

        // Validate trait composition (traits list)
        for trait_ref in &trait_def.traits {
            if self.symbols.get_trait(&trait_ref.name).is_some() {
                // OK: trait exists
            } else {
                // Check if it's defined as something else
                if self.symbols.is_struct(&trait_ref.name) {
                    self.errors.push(CompilerError::NotATrait {
                        name: trait_ref.name.clone(),
                        actual_kind: "struct".to_string(),
                        span: trait_ref.span,
                    });
                } else if self.symbols.is_enum(&trait_ref.name) {
                    self.errors.push(CompilerError::NotATrait {
                        name: trait_ref.name.clone(),
                        actual_kind: "enum".to_string(),
                        span: trait_ref.span,
                    });
                } else {
                    self.errors.push(CompilerError::UndefinedTrait {
                        name: trait_ref.name.clone(),
                        span: trait_ref.span,
                    });
                }
            }
        }

        // Validate field types
        for field in &trait_def.fields {
            self.validate_type(&field.ty);
        }

        // Validate mounting point types
        for field in &trait_def.mount_fields {
            self.validate_type(&field.ty);
        }

        // Pop generic scope
        self.pop_generic_scope();
    }

    /// Resolve types in a struct definition
    fn resolve_struct_types(&mut self, struct_def: &StructDef) {
        // Push generic scope for this definition
        self.push_generic_scope(&struct_def.generics);

        // Validate trait implementations
        for trait_ref in &struct_def.traits {
            if self.symbols.get_trait(&trait_ref.name).is_some() {
                // OK: trait exists
            } else {
                // Check if it's defined as something else
                if self.symbols.is_struct(&trait_ref.name) || self.symbols.is_enum(&trait_ref.name)
                {
                    self.errors.push(CompilerError::NotATrait {
                        name: trait_ref.name.clone(),
                        actual_kind: "not a trait".to_string(),
                        span: trait_ref.span,
                    });
                } else {
                    self.errors.push(CompilerError::UndefinedTrait {
                        name: trait_ref.name.clone(),
                        span: trait_ref.span,
                    });
                }
            }
        }

        // Validate field types
        for field in &struct_def.fields {
            self.validate_type(&field.ty);
        }

        // Validate mount field types
        for field in &struct_def.mount_fields {
            self.validate_type(&field.ty);
        }

        // Pop generic scope
        self.pop_generic_scope();
    }

    /// Validate a type reference
    fn validate_type(&mut self, ty: &Type) {
        match ty {
            Type::Primitive(_) => {
                // Primitive types are always valid
            }
            Type::Ident(ident) => {
                // Check for module path (e.g., alignment::Horizontal or outer::inner::Type)
                if ident.name.contains("::") {
                    let parts: Vec<&str> = ident.name.split("::").collect();
                    if parts.len() >= 2 {
                        // Resolve nested module path
                        if let Some(error_msg) = self.resolve_nested_module_type(&parts, ident.span)
                        {
                            self.errors.push(CompilerError::UndefinedType {
                                name: error_msg,
                                span: ident.span,
                            });
                        }
                    } else {
                        // Invalid path format
                        self.errors.push(CompilerError::UndefinedType {
                            name: format!("invalid module path: {}", ident.name),
                            span: ident.span,
                        });
                    }
                } else {
                    // Regular identifier without module path
                    if self.symbols.is_type(&ident.name) {
                        // It's a valid type (model, view, or enum)
                    } else if self.symbols.is_trait(&ident.name) {
                        // It's a valid trait type (will be replaced with concrete type at instantiation)
                    } else if self.is_type_parameter(&ident.name) {
                        // It's a valid type parameter in scope
                    } else {
                        // Check if it looks like a type parameter (single uppercase letter)
                        // This is a heuristic to provide better error messages
                        if ident.name.len() == 1
                            && ident.name.chars().next().is_some_and(char::is_uppercase)
                        {
                            // Likely meant to be a type parameter
                            self.errors.push(CompilerError::OutOfScopeTypeParameter {
                                param: ident.name.clone(),
                                span: ident.span,
                            });
                        } else {
                            // Regular undefined type
                            self.errors.push(CompilerError::UndefinedType {
                                name: ident.name.clone(),
                                span: ident.span,
                            });
                        }
                    }
                }
            }
            Type::Array(element_ty) => {
                // Recursively validate element type
                self.validate_type(element_ty);
            }
            Type::Optional(inner_ty) => {
                // Recursively validate inner type
                self.validate_type(inner_ty);
            }
            Type::Tuple(fields) => {
                // Recursively validate all field types in tuple
                for field in fields {
                    self.validate_type(&field.ty);
                }
            }
            Type::Generic { name, args, span } => {
                // Validate the base type exists
                if !self.symbols.is_type(&name.name) {
                    self.errors.push(CompilerError::UndefinedType {
                        name: name.name.clone(),
                        span: name.span,
                    });
                    return;
                }

                // Get the expected number of generic parameters
                if let Some(expected_params) = self.symbols.get_generics(&name.name) {
                    let expected = expected_params.len();
                    let actual = args.len();

                    if expected != actual {
                        self.errors.push(CompilerError::GenericArityMismatch {
                            name: name.name.clone(),
                            expected,
                            actual,
                            span: *span,
                        });
                    }
                }

                // Validate that type arguments satisfy constraints
                if let Some(expected_params) = self.symbols.get_generics(&name.name) {
                    for (i, arg) in args.iter().enumerate() {
                        if let Some(param) = expected_params.get(i) {
                            for constraint in &param.constraints {
                                let crate::ast::GenericConstraint::Trait(trait_ref) = constraint;
                                if !self.type_satisfies_trait_constraint(arg, &trait_ref.name) {
                                    self.errors.push(CompilerError::GenericConstraintViolation {
                                        arg: self.type_to_string(arg),
                                        constraint: trait_ref.name.clone(),
                                        span: *span,
                                    });
                                }
                            }
                        }
                    }
                }

                // Recursively validate type arguments
                for arg in args {
                    self.validate_type(arg);
                }
            }
            Type::TypeParameter(param) => {
                // Check if type parameter is in scope
                if !self.is_type_parameter(&param.name) {
                    self.errors.push(CompilerError::OutOfScopeTypeParameter {
                        param: param.name.clone(),
                        span: param.span,
                    });
                }
            }
            Type::Dictionary { key, value } => {
                // Recursively validate key and value types
                self.validate_type(key);
                self.validate_type(value);
            }
            Type::Closure { params, ret } => {
                // Recursively validate parameter and return types
                for param in params {
                    self.validate_type(param);
                }
                self.validate_type(ret);
            }
        }
    }

    /// Pass 3: Validate expressions
    /// Validate operators and control flow without evaluation
    fn validate_expressions(&mut self, file: &File) {
        for statement in &file.statements {
            match statement {
                Statement::Let(let_binding) => {
                    self.validate_expr(&let_binding.value, file);
                    // Validate destructuring pattern type compatibility
                    self.validate_destructuring_pattern(
                        &let_binding.pattern,
                        &let_binding.value,
                        let_binding.span,
                        file,
                    );
                }
                Statement::Definition(def) => match &**def {
                    Definition::Struct(struct_def) => {
                        self.validate_struct_expressions(struct_def, file);
                    }
                    Definition::Impl(impl_def) => {
                        // Set current impl struct for field type resolution
                        self.current_impl_struct = Some(impl_def.name.name.clone());
                        // Clear local let bindings for this impl block
                        self.local_let_bindings.clear();
                        // Clear impl struct context and local bindings
                        self.current_impl_struct = None;
                        self.local_let_bindings.clear();
                    }
                    Definition::Trait(_)
                    | Definition::Enum(_)
                    | Definition::Module(_)
                    | Definition::Function(_) => {}
                },
                Statement::Use(_) => {}
            }
        }
    }

    /// Validate expressions in struct field defaults
    fn validate_struct_expressions(&mut self, struct_def: &StructDef, file: &File) {
        // Validate field defaults
        for field in &struct_def.fields {
            if let Some(default_expr) = &field.default {
                self.validate_expr(default_expr, file);
            }
        }
        // Validate mount field defaults
        for field in &struct_def.mount_fields {
            if let Some(default_expr) = &field.default {
                self.validate_expr(default_expr, file);
            }
        }
    }

    /// Validate a single expression (recursively)
    fn validate_expr(&mut self, expr: &Expr, file: &File) {
        // Check recursion depth to prevent stack overflow
        const MAX_EXPR_DEPTH: usize = 500;
        self.validate_expr_depth = self.validate_expr_depth.saturating_add(1);
        if self.validate_expr_depth > MAX_EXPR_DEPTH {
            self.validate_expr_depth = self.validate_expr_depth.saturating_sub(1);
            self.errors
                .push(CompilerError::ExpressionDepthExceeded { span: expr.span() });
            return;
        }

        match expr {
            Expr::Literal(_) => {
                // Literals are always valid
            }
            Expr::Array { elements, .. } => {
                for elem in elements {
                    self.validate_expr(elem, file);
                }
            }
            Expr::Tuple { fields, .. } => {
                // Validate all field expressions in tuple
                for (_, field_expr) in fields {
                    self.validate_expr(field_expr, file);
                }
            }
            Expr::Reference { path, span } => {
                // Handle self keyword
                if !path.is_empty() && path[0].name == "self" {
                    // Validate that we're inside an impl block
                    if self.current_impl_struct.is_none() {
                        self.errors.push(CompilerError::UndefinedReference {
                            name: "self".to_string(),
                            span: *span,
                        });
                        return;
                    }

                    // If it's just "self", it's valid
                    if path.len() == 1 {
                        return;
                    }

                    // If it's "self.field", validate the field exists
                    if path.len() == 2 {
                        let field_name = &path[1].name;
                        if let Some(ref struct_name) = self.current_impl_struct {
                            if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                                // Check regular fields
                                for field in &struct_info.fields {
                                    if field.name == *field_name {
                                        return;
                                    }
                                }
                                // Check mount fields
                                for field in &struct_info.mount_fields {
                                    if field.name == *field_name {
                                        return;
                                    }
                                }
                                // Field not found
                                self.errors.push(CompilerError::UndefinedReference {
                                    name: format!("self.{}", field_name),
                                    span: *span,
                                });
                                return;
                            }
                        }
                    }

                    // self.field.subfield or longer paths not supported yet
                    return;
                }

                // Validate references in impl blocks
                if path.len() == 1 {
                    let name = &path[0].name;

                    // Check if it's a top-level let binding
                    if self.symbols.is_let(name) {
                        return;
                    }

                    // Check if it's a local let binding (from let expressions)
                    if self.local_let_bindings.contains_key(name) {
                        return;
                    }

                    // Check if it's a loop variable
                    for scope in &self.loop_var_scopes {
                        if scope.contains(name) {
                            return;
                        }
                    }

                    // Check if it's a closure parameter
                    for scope in &self.closure_param_scopes {
                        if scope.contains(name) {
                            return;
                        }
                    }

                    // Check if it's a known type (struct, enum, trait)
                    if self.symbols.is_struct(name)
                        || self.symbols.is_enum(name)
                        || self.symbols.is_trait(name)
                    {
                        return;
                    }

                    // Check if we're inside an impl block and this is a field reference
                    if let Some(ref struct_name) = self.current_impl_struct {
                        if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                            // Check regular fields
                            for field in &struct_info.fields {
                                if field.name == *name {
                                    return;
                                }
                            }
                            // Check mount fields
                            for field in &struct_info.mount_fields {
                                if field.name == *name {
                                    return;
                                }
                            }
                        }
                        // Inside impl block but reference is not a field - error
                        self.errors.push(CompilerError::UndefinedReference {
                            name: name.clone(),
                            span: *span,
                        });
                    }
                    // Outside impl block, simple references might be valid (generic params, etc.)
                }
                // Multi-segment paths (like Foo.bar or Foo::bar) are validated elsewhere
            }
            Expr::Invocation {
                path,
                type_args,
                args,
                mounts,
                span,
            } => {
                // Join path to get the name for lookup
                let name = path
                    .iter()
                    .map(|id| id.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");

                // First validate all argument expressions
                for (_, arg_expr) in args {
                    self.validate_expr(arg_expr, file);
                }
                for (_, mount_expr) in mounts {
                    self.validate_expr(mount_expr, file);
                }

                // Validate each type argument
                for type_arg in type_args {
                    self.validate_type(type_arg);
                }

                // Determine if this is a struct instantiation or function call
                // Use get_struct_qualified to handle nested module paths like "fill::relative::Linear"
                let is_struct = self.symbols.get_struct_qualified(&name).is_some();

                if is_struct {
                    // Validate as struct instantiation
                    // Struct instantiation requires named arguments
                    let named_args: Vec<(crate::ast::Ident, Expr)> = args
                        .iter()
                        .filter_map(|(name_opt, expr)| {
                            name_opt.as_ref().map(|name| (name.clone(), expr.clone()))
                        })
                        .collect();

                    // Check that all args are named - report all positional args
                    for (i, (name_opt, arg_expr)) in args.iter().enumerate() {
                        if name_opt.is_none() {
                            self.errors.push(CompilerError::PositionalArgInStruct {
                                struct_name: name.clone(),
                                position: i.saturating_add(1), // 1-indexed for user-friendly message
                                span: arg_expr.span(),
                            });
                        }
                    }

                    // Check if the struct has generic parameters
                    if let Some(expected_params) = self.symbols.get_generics(&name) {
                        let expected = expected_params.len();
                        let actual = type_args.len();

                        // Check for arity mismatch
                        if expected != actual {
                            if actual == 0 && expected > 0 {
                                // Missing generic arguments
                                self.errors.push(CompilerError::MissingGenericArguments {
                                    name: name.clone(),
                                    span: *span,
                                });
                            } else {
                                // Wrong number of generic arguments
                                self.errors.push(CompilerError::GenericArityMismatch {
                                    name: name.clone(),
                                    expected,
                                    actual,
                                    span: *span,
                                });
                            }
                        }
                    } else if !type_args.is_empty() {
                        // Struct is not generic but type arguments were provided
                        self.errors.push(CompilerError::GenericArityMismatch {
                            name: name.clone(),
                            expected: 0,
                            actual: type_args.len(),
                            span: *span,
                        });
                    }

                    // Validate struct field requirements
                    self.validate_struct_fields(&name, &named_args, mounts, *span, file);

                    // Validate mutability: mut parameters must receive mut values
                    self.validate_struct_mutability(&name, &named_args, mounts, file, *span);
                } else {
                    // Validate as function call
                    // Function calls use positional arguments (named args are allowed too)
                    // Function calls should not have type_args or mounts
                    if !type_args.is_empty() {
                        self.errors.push(CompilerError::GenericArityMismatch {
                            name: name.clone(),
                            expected: 0,
                            actual: type_args.len(),
                            span: *span,
                        });
                    }
                    if !mounts.is_empty() {
                        self.errors.push(CompilerError::UnknownMount {
                            mount: mounts[0].0.name.clone(),
                            struct_name: name.clone(),
                            span: mounts[0].0.span,
                        });
                    }

                    // Validate function exists
                    // For qualified names like "math::sin", check the last component against builtins
                    let simple_name = name.rsplit("::").next().unwrap_or(&name);
                    let is_builtin =
                        crate::builtins::BuiltinRegistry::global().is_builtin(simple_name);
                    let is_user_function = self.symbols.get_function(&name).is_some()
                        || self.symbols.get_function(simple_name).is_some();

                    if !is_builtin && !is_user_function {
                        self.errors.push(CompilerError::UndefinedType {
                            name: format!("function '{}'", name),
                            span: *span,
                        });
                    }
                }
            }
            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                span,
            } => {
                // Validate each field expression
                for (_, data_expr) in data {
                    self.validate_expr(data_expr, file);
                }

                // Validate that the enum and variant exist
                self.validate_enum_instantiation(enum_name, variant, data, *span, file);
            }
            Expr::InferredEnumInstantiation {
                variant: _,
                data,
                span: _,
            } => {
                // Validate each field expression
                for (_, data_expr) in data {
                    self.validate_expr(data_expr, file);
                }

                // Type inference will be done in a separate pass
                // For now, just validate the data expressions
            }
            Expr::BinaryOp {
                left,
                op,
                right,
                span,
            } => {
                // Recursively validate operands first
                self.validate_expr(left, file);
                self.validate_expr(right, file);

                // Validate operator type compatibility
                self.validate_binary_op(left, *op, right, *span, file);
            }
            Expr::UnaryOp { operand, .. } => {
                // Recursively validate operand
                self.validate_expr(operand, file);
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                span,
            } => {
                // Recursively validate collection
                self.validate_expr(collection, file);

                // Push loop variable scope before validating body
                let mut scope = HashSet::new();
                scope.insert(var.name.clone());
                self.loop_var_scopes.push(scope);

                // Validate body with loop variable in scope
                self.validate_expr(body, file);

                // Pop loop variable scope
                self.loop_var_scopes.pop();

                // Validate for loop over array type
                self.validate_for_loop(collection, *span, file);
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                // Recursively validate branches
                self.validate_expr(condition, file);
                self.validate_expr(then_branch, file);
                if let Some(else_expr) = else_branch {
                    self.validate_expr(else_expr, file);
                }

                // Validate if condition is boolean or optional
                self.validate_if_condition(condition, *span, file);
            }
            Expr::MatchExpr {
                scrutinee,
                arms,
                span,
            } => {
                // Recursively validate scrutinee and arm bodies
                self.validate_expr(scrutinee, file);
                for arm in arms {
                    self.validate_expr(&arm.body, file);
                }

                // Validate match exhaustiveness
                self.validate_match(scrutinee, arms, *span, file);
            }
            Expr::Group { expr, .. } => {
                self.validate_expr(expr, file);
            }
            Expr::DictLiteral { entries, .. } => {
                // Validate all key-value expressions
                for (key, value) in entries {
                    self.validate_expr(key, file);
                    self.validate_expr(value, file);
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                // Validate dictionary and key expressions
                self.validate_expr(dict, file);
                self.validate_expr(key, file);
            }
            Expr::FieldAccess { object, .. } => {
                // Validate the object expression
                self.validate_expr(object, file);
            }
            Expr::ClosureExpr { params, body, .. } => {
                // Validate parameter type annotations if present
                for param in params {
                    if let Some(ty) = &param.ty {
                        self.validate_type(ty);
                    }
                }

                // Push closure parameters to scope before validating body
                let mut param_scope = HashSet::new();
                for param in params {
                    param_scope.insert(param.name.name.clone());
                }
                self.closure_param_scopes.push(param_scope);

                // Validate body expression with closure params in scope
                self.validate_expr(body, file);

                // Pop closure parameter scope
                self.closure_param_scopes.pop();
            }
            Expr::LetExpr {
                mutable,
                pattern,
                ty,
                value,
                body,
                span,
            } => {
                // Validate type annotation if present
                if let Some(type_ann) = ty {
                    self.validate_type(type_ann);
                }
                // Validate value expression
                self.validate_expr(value, file);

                // Validate destructuring pattern type compatibility
                self.validate_destructuring_pattern(pattern, value, *span, file);

                // Add all bindings from the pattern to local scope before validating body
                for binding in collect_bindings_from_pattern(pattern) {
                    // Infer the type of the binding from the value
                    let ty = self.infer_type(value, file);
                    self.local_let_bindings.insert(binding.name, (ty, *mutable));
                }

                // Validate body expression with the bindings in scope
                self.validate_expr(body, file);
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                span,
            } => {
                // Validate receiver and all argument expressions
                self.validate_expr(receiver, file);
                for arg in args {
                    self.validate_expr(arg, file);
                }

                // Validate method exists on receiver type
                let receiver_type = self.infer_type(receiver, file);
                if !self.method_exists_on_type(&receiver_type, &method.name, file) {
                    self.errors.push(CompilerError::UndefinedReference {
                        name: format!("method '{}' on type '{}'", method.name, receiver_type),
                        span: *span,
                    });
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                // Validate all statements in the block
                for stmt in statements {
                    match stmt {
                        BlockStatement::Let {
                            mutable,
                            pattern,
                            value,
                            ty,
                            ..
                        } => {
                            self.validate_expr(value, file);
                            // Register bindings with their types and mutability
                            let ty_str = if let Some(t) = ty {
                                self.type_to_string(t)
                            } else {
                                self.infer_type(value, file)
                            };
                            for binding in collect_bindings_from_pattern(pattern) {
                                self.local_let_bindings
                                    .insert(binding.name, (ty_str.clone(), *mutable));
                            }
                        }
                        BlockStatement::Assign {
                            target,
                            value,
                            span,
                        } => {
                            // Validate both target and value expressions
                            self.validate_expr(target, file);
                            self.validate_expr(value, file);
                            // Check that target is mutable
                            if !self.is_expr_mutable(target, file) {
                                self.errors
                                    .push(CompilerError::AssignmentToImmutable { span: *span });
                            }
                        }
                        BlockStatement::Expr(expr) => {
                            self.validate_expr(expr, file);
                        }
                    }
                }
                // Validate the result expression
                self.validate_expr(result, file);
            }
        }

        // Decrement depth counter
        self.validate_expr_depth = self.validate_expr_depth.saturating_sub(1);
    }

    /// Validate that struct instantiation respects mutability requirements
    /// Validate struct field requirements: all required fields must be provided, no unknown fields
    fn validate_struct_fields(
        &mut self,
        struct_name: &str,
        args: &[(crate::ast::Ident, Expr)],
        mounts: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        // Find the struct definition in current file or module cache
        // Clone necessary data to avoid borrow checker issues
        let (field_names, mount_field_names, required_fields, required_mounts) = {
            if let Some(def) = self.find_struct_def_in_files(struct_name, file) {
                let field_names: Vec<String> =
                    def.fields.iter().map(|f| f.name.name.clone()).collect();
                let mount_field_names: Vec<String> = def
                    .mount_fields
                    .iter()
                    .map(|f| f.name.name.clone())
                    .collect();

                let required_fields: Vec<String> = def
                    .fields
                    .iter()
                    .filter(|f| {
                        // Field is required if it has no inline default and is not optional
                        f.default.is_none() && !f.optional
                    })
                    .map(|f| f.name.name.clone())
                    .collect();

                let required_mounts: Vec<String> = def
                    .mount_fields
                    .iter()
                    .filter(|f| {
                        // Mount fields with inline defaults are optional
                        if f.default.is_some() {
                            return false;
                        }
                        // Mount fields of type `Never` are always optional since
                        // they can never have a value (used by terminal types like Empty)
                        if matches!(&f.ty, Type::Primitive(PrimitiveType::Never)) {
                            return false;
                        }
                        true
                    })
                    .map(|f| f.name.name.clone())
                    .collect();

                (
                    field_names,
                    mount_field_names,
                    required_fields,
                    required_mounts,
                )
            } else {
                return; // Struct not found, skip validation
            }
        };

        // Now we can safely borrow self.errors mutably
        // Check all provided regular fields exist
        for (arg_name, _) in args {
            if !field_names.contains(&arg_name.name) {
                self.errors.push(CompilerError::UnknownField {
                    field: arg_name.name.clone(),
                    type_name: struct_name.to_string(),
                    span: arg_name.span,
                });
            }
        }

        // Check all provided mount fields exist
        for (mount_name, _) in mounts {
            if !mount_field_names.contains(&mount_name.name) {
                self.errors.push(CompilerError::UnknownMount {
                    mount: mount_name.name.clone(),
                    struct_name: struct_name.to_string(),
                    span: mount_name.span,
                });
            }
        }

        // Check all required regular fields are provided
        for field_name in required_fields {
            if !args.iter().any(|(name, _)| name.name == field_name) {
                self.errors.push(CompilerError::MissingField {
                    field: field_name,
                    type_name: struct_name.to_string(),
                    span,
                });
            }
        }

        // Check all required mount fields are provided
        for mount_name in required_mounts {
            if !mounts.iter().any(|(name, _)| name.name == mount_name) {
                self.errors.push(CompilerError::MissingField {
                    field: mount_name,
                    type_name: struct_name.to_string(),
                    span,
                });
            }
        }
    }

    /// Find a struct definition in the current file and module cache
    fn find_struct_def_in_files<'a>(
        &'a self,
        struct_name: &str,
        current_file: &'a File,
    ) -> Option<&'a StructDef> {
        // Search in current file
        for statement in &current_file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == struct_name {
                        return Some(struct_def);
                    }
                }
            }
        }

        // Search in module cache
        for (file, _) in self.module_cache.values() {
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Struct(struct_def) = &**def {
                        if struct_def.name.name == struct_name {
                            return Some(struct_def);
                        }
                    }
                }
            }
        }

        None
    }

    fn validate_struct_mutability(
        &mut self,
        struct_name: &str,
        args: &[(crate::ast::Ident, Expr)],
        mounts: &[(crate::ast::Ident, Expr)],
        file: &File,
        span: Span,
    ) {
        // Find the struct definition
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == struct_name {
                        // Check each regular field argument
                        for (arg_name, arg_expr) in args {
                            // Find the corresponding field in the struct
                            if let Some(field) = struct_def
                                .fields
                                .iter()
                                .find(|f| f.name.name == arg_name.name)
                            {
                                // If field is mutable, check that the arg expression is mutable
                                if field.mutable && !self.is_expr_mutable(arg_expr, file) {
                                    self.errors.push(CompilerError::MutabilityMismatch {
                                        param: arg_name.name.clone(),
                                        span,
                                    });
                                }
                            }
                        }
                        // Check each mount field argument
                        for (mount_name, mount_expr) in mounts {
                            // Find the corresponding mount field in the struct
                            if let Some(field) = struct_def
                                .mount_fields
                                .iter()
                                .find(|f| f.name.name == mount_name.name)
                            {
                                // If mount field is mutable, check that the mount expression is mutable
                                if field.mutable && !self.is_expr_mutable(mount_expr, file) {
                                    self.errors.push(CompilerError::MutabilityMismatch {
                                        param: mount_name.name.clone(),
                                        span,
                                    });
                                }
                            }
                        }
                        return;
                    }
                }
            }
        }
    }

    /// Validate binary operator type compatibility
    fn validate_binary_op(
        &mut self,
        left: &Expr,
        op: BinaryOperator,
        right: &Expr,
        span: Span,
        file: &File,
    ) {
        let left_type = self.infer_type(left, file);
        let right_type = self.infer_type(right, file);

        // Check type compatibility based on operator
        let valid = match op {
            // Add: Number + Number or String + String (concatenation) or GPU numeric types
            BinaryOperator::Add => {
                matches!(
                    (&left_type[..], &right_type[..]),
                    ("Number", "Number") | ("String", "String")
                ) || Self::are_gpu_numeric_compatible(&left_type, &right_type)
            }
            // Other arithmetic operators: Number + Number or GPU numeric types
            BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod => {
                matches!((&left_type[..], &right_type[..]), ("Number", "Number"))
                    || Self::are_gpu_numeric_compatible(&left_type, &right_type)
            }
            // Comparison operators: Number + Number or GPU numeric types → Boolean
            BinaryOperator::Lt | BinaryOperator::Gt | BinaryOperator::Le | BinaryOperator::Ge => {
                matches!((&left_type[..], &right_type[..]), ("Number", "Number"))
                    || Self::are_gpu_numeric_compatible(&left_type, &right_type)
            }
            // Equality operators: same types or compatible GPU types
            BinaryOperator::Eq | BinaryOperator::Ne => {
                left_type == right_type || Self::are_gpu_numeric_compatible(&left_type, &right_type)
            }
            // Logical operators: Boolean + Boolean or bool + bool
            BinaryOperator::And | BinaryOperator::Or => {
                (left_type == "Boolean" && right_type == "Boolean")
                    || (left_type == "bool" && right_type == "bool")
            }
            // Range operator: Number + Number or compatible GPU integer types
            BinaryOperator::Range => {
                matches!((&left_type[..], &right_type[..]), ("Number", "Number"))
                    || Self::are_gpu_numeric_compatible(&left_type, &right_type)
            }
        };

        if !valid {
            self.errors.push(CompilerError::InvalidBinaryOp {
                op: format!("{:?}", op),
                left_type,
                right_type,
                span,
            });
        }
    }

    /// Check if two types are compatible GPU numeric types
    fn are_gpu_numeric_compatible(left: &str, right: &str) -> bool {
        // GPU scalar types
        const GPU_SCALARS: &[&str] = &["f32", "i32", "u32"];
        // GPU vector types (same component type can do arithmetic)
        const GPU_FLOAT_VECTORS: &[&str] = &["vec2", "vec3", "vec4"];
        const GPU_INT_VECTORS: &[&str] = &["ivec2", "ivec3", "ivec4"];
        const GPU_UINT_VECTORS: &[&str] = &["uvec2", "uvec3", "uvec4"];

        // Same scalar type
        if left == right && GPU_SCALARS.contains(&left) {
            return true;
        }

        // Same vector type
        if left == right
            && (GPU_FLOAT_VECTORS.contains(&left)
                || GPU_INT_VECTORS.contains(&left)
                || GPU_UINT_VECTORS.contains(&left))
        {
            return true;
        }

        // Scalar with matching vector (for scalar*vector operations)
        if left == "f32" && GPU_FLOAT_VECTORS.contains(&right) {
            return true;
        }
        if right == "f32" && GPU_FLOAT_VECTORS.contains(&left) {
            return true;
        }

        false
    }

    /// Validate for loop collection is an array
    fn validate_for_loop(&mut self, collection: &Expr, span: Span, file: &File) {
        let collection_type = self.infer_type(collection, file);

        // Check if it's an array type (starts with '[')
        if !collection_type.starts_with('[') {
            self.errors.push(CompilerError::ForLoopNotArray {
                actual: collection_type,
                span,
            });
        }
    }

    /// Validate destructuring pattern matches the value type
    fn validate_destructuring_pattern(
        &mut self,
        pattern: &BindingPattern,
        value: &Expr,
        span: Span,
        file: &File,
    ) {
        let value_type = self.infer_type(value, file);

        match pattern {
            BindingPattern::Array { .. } => {
                // Array destructuring requires an array type
                if !value_type.starts_with('[') {
                    self.errors.push(CompilerError::ArrayDestructuringNotArray {
                        actual: value_type,
                        span,
                    });
                }
            }
            BindingPattern::Struct { fields, .. } => {
                // Struct destructuring requires a struct type
                // Check if the type is a known struct
                if let Some(struct_info) = self.symbols.get_struct(&value_type) {
                    // Validate that all destructured fields exist on the struct
                    let field_names: Vec<&str> =
                        struct_info.fields.iter().map(|f| f.name.as_str()).collect();
                    for field in fields {
                        if !field_names.contains(&field.name.name.as_str()) {
                            self.errors.push(CompilerError::UnknownField {
                                field: field.name.name.clone(),
                                type_name: value_type.clone(),
                                span: field.name.span,
                            });
                        }
                    }
                } else {
                    // Not a known struct - report error (includes primitives)
                    self.errors
                        .push(CompilerError::StructDestructuringNotStruct {
                            actual: value_type,
                            span,
                        });
                }
            }
            BindingPattern::Tuple { .. } | BindingPattern::Simple(_) => {
                // Tuple and simple patterns don't require type validation here
            }
        }
    }

    /// Validate if condition is boolean or optional
    fn validate_if_condition(&mut self, condition: &Expr, span: Span, file: &File) {
        let condition_type = self.infer_type(condition, file);

        // Condition must be Boolean or optional (ends with '?')
        if condition_type != "Boolean" && !condition_type.ends_with('?') {
            self.errors.push(CompilerError::InvalidIfCondition {
                actual: condition_type,
                span,
            });
        }
    }

    /// Validate match expression exhaustiveness
    fn validate_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[crate::ast::MatchArm],
        span: Span,
        file: &File,
    ) {
        // Infer scrutinee type - must be an enum
        let scrutinee_type = self.infer_type(scrutinee, file);

        // Check if scrutinee is an enum (look it up in symbol table)
        if !self.symbols.is_enum(&scrutinee_type) {
            self.errors.push(CompilerError::MatchNotEnum {
                actual: scrutinee_type,
                span,
            });
            return;
        }

        // Get enum variants from symbol table
        let variants = match self.symbols.get_enum_variants(&scrutinee_type) {
            Some(v) => v.clone(),
            None => return, // Should not happen if is_enum returned true
        };

        // Collect all variant names from match arms
        let mut covered_variants = HashSet::new();
        let mut has_wildcard = false;
        for arm in arms {
            match &arm.pattern {
                Pattern::Variant { name, bindings } => {
                    // Check for duplicate arms
                    if !covered_variants.insert(name.name.clone()) {
                        self.errors.push(CompilerError::DuplicateMatchArm {
                            variant: name.name.clone(),
                            span: arm.span,
                        });
                        continue;
                    }

                    // Validate variant exists and arity matches
                    self.validate_match_arm(
                        &scrutinee_type,
                        &name.name,
                        bindings.len(),
                        arm.span,
                        &variants,
                    );
                }
                Pattern::Wildcard => {
                    // Wildcard covers all remaining variants
                    has_wildcard = true;
                }
            }
        }

        // Check exhaustiveness - all variants must be covered (unless there's a wildcard)
        if !has_wildcard {
            let missing_variants: Vec<String> = variants
                .keys()
                .filter(|v| !covered_variants.contains(*v))
                .cloned()
                .collect();

            if !missing_variants.is_empty() {
                self.errors.push(CompilerError::NonExhaustiveMatch {
                    missing: missing_variants.join(", "),
                    span,
                });
            }
        }
    }

    /// Validate enum instantiation with named parameters
    fn validate_enum_instantiation(
        &mut self,
        enum_name: &crate::ast::Ident,
        variant_name: &crate::ast::Ident,
        data: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        // Check if the enum exists
        if !self.symbols.is_enum(&enum_name.name) {
            self.errors.push(CompilerError::UndefinedType {
                name: enum_name.name.clone(),
                span: enum_name.span,
            });
            return;
        }

        // Get the enum definition to access variant field information
        let variant_fields =
            self.get_enum_variant_fields(&enum_name.name, &variant_name.name, file);

        match variant_fields {
            Some(fields) => {
                // Check if variant has no fields but data was provided
                if fields.is_empty() && !data.is_empty() {
                    self.errors.push(CompilerError::EnumVariantWithoutData {
                        variant: variant_name.name.clone(),
                        enum_name: enum_name.name.clone(),
                        span,
                    });
                    return;
                }

                // Check if variant has fields but no data was provided
                if !fields.is_empty() && data.is_empty() {
                    self.errors.push(CompilerError::EnumVariantRequiresData {
                        variant: variant_name.name.clone(),
                        enum_name: enum_name.name.clone(),
                        span,
                    });
                    return;
                }

                // Check that all required fields are provided
                let provided_fields: HashSet<&str> =
                    data.iter().map(|(name, _)| name.name.as_str()).collect();
                let required_fields: HashSet<&str> =
                    fields.iter().map(|f| f.name.name.as_str()).collect();

                // Check for missing fields
                for field in &required_fields {
                    if !provided_fields.contains(field) {
                        self.errors.push(CompilerError::MissingField {
                            field: field.to_string(),
                            type_name: format!("{}.{}", enum_name.name, variant_name.name),
                            span,
                        });
                    }
                }

                // Check for unknown fields
                for (provided_field, _) in data {
                    if !required_fields.contains(provided_field.name.as_str()) {
                        self.errors.push(CompilerError::UnknownField {
                            field: provided_field.name.clone(),
                            type_name: format!("{}.{}", enum_name.name, variant_name.name),
                            span: provided_field.span,
                        });
                    }
                }
            }
            None => {
                // Variant doesn't exist
                self.errors.push(CompilerError::UnknownEnumVariant {
                    variant: variant_name.name.clone(),
                    enum_name: enum_name.name.clone(),
                    span: variant_name.span,
                });
            }
        }
    }

    /// Get the field definitions for a specific enum variant
    /// Returns None if the enum or variant doesn't exist
    fn get_enum_variant_fields(
        &self,
        enum_name: &str,
        variant_name: &str,
        current_file: &File,
    ) -> Option<Vec<crate::ast::FieldDef>> {
        // First, search in the current file
        for statement in &current_file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Enum(enum_def) = &**def {
                    if enum_def.name.name == enum_name {
                        // Find the variant
                        for variant in &enum_def.variants {
                            if variant.name.name == variant_name {
                                return Some(variant.fields.clone());
                            }
                        }
                        return None; // Variant not found
                    }
                }
            }
        }

        // If not found in current file, search through module cache
        for (file, _) in self.module_cache.values() {
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Enum(enum_def) = &**def {
                        if enum_def.name.name == enum_name {
                            // Find the variant
                            for variant in &enum_def.variants {
                                if variant.name.name == variant_name {
                                    return Some(variant.fields.clone());
                                }
                            }
                            return None; // Variant not found
                        }
                    }
                }
            }
        }
        None // Enum not found
    }

    /// Validate a single match arm
    fn validate_match_arm(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        binding_count: usize,
        span: Span,
        variants: &std::collections::HashMap<String, (usize, Span)>,
    ) {
        // Check if variant exists
        match variants.get(variant_name) {
            Some((expected_arity, _)) => {
                // Check arity matches
                if *expected_arity != binding_count {
                    self.errors.push(CompilerError::VariantArityMismatch {
                        variant: variant_name.to_string(),
                        expected: *expected_arity,
                        actual: binding_count,
                        span,
                    });
                }
            }
            None => {
                // Variant doesn't exist in enum
                self.errors.push(CompilerError::UnknownEnumVariant {
                    variant: variant_name.to_string(),
                    enum_name: enum_name.to_string(),
                    span,
                });
            }
        }
    }

    /// Pass 4: Validate trait implementations
    /// Check that structs implement all required fields from their traits
    fn validate_trait_implementations(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    self.validate_struct_trait_implementation(struct_def);
                }
            }
        }
    }

    /// Validate that a struct implements all required fields from its traits
    fn validate_struct_trait_implementation(&mut self, struct_def: &StructDef) {
        // For each implemented trait
        for trait_ref in &struct_def.traits {
            // Get all required fields from this trait (including composed traits)
            let required_fields = self.symbols.get_all_trait_fields(&trait_ref.name);

            // Check each required field
            for (field_name, required_type) in required_fields {
                // Look for the field in the struct
                match struct_def.fields.iter().find(|f| f.name.name == field_name) {
                    Some(struct_field) => {
                        // Field exists, check type matches
                        if !self.types_match(&struct_field.ty, &required_type) {
                            self.errors.push(CompilerError::TraitFieldTypeMismatch {
                                field: field_name.clone(),
                                trait_name: trait_ref.name.clone(),
                                expected: self.type_to_string(&required_type),
                                actual: self.type_to_string(&struct_field.ty),
                                span: struct_field.span,
                            });
                        }
                    }
                    None => {
                        // Field is missing
                        self.errors.push(CompilerError::MissingTraitField {
                            field: field_name.clone(),
                            trait_name: trait_ref.name.clone(),
                            span: struct_def.span,
                        });
                    }
                }
            }

            // Get all required mounting points from this trait (including composed traits)
            let required_mounts = self.symbols.get_all_trait_mounting_points(&trait_ref.name);

            // Check each required mounting point
            for (mount_name, required_type) in required_mounts {
                match struct_def
                    .mount_fields
                    .iter()
                    .find(|f| f.name.name == mount_name)
                {
                    Some(mount_field) => {
                        // Mounting point exists, check type matches
                        // Special case: `Never` type satisfies any mount point requirement.
                        // `Never` is a terminal type indicating "no child content", used by
                        // terminal components like Empty, EmptyShape, etc.
                        let is_never =
                            matches!(&mount_field.ty, Type::Primitive(PrimitiveType::Never));
                        if !is_never && !self.types_match(&mount_field.ty, &required_type) {
                            self.errors
                                .push(CompilerError::TraitMountingPointTypeMismatch {
                                    mount: mount_name.clone(),
                                    trait_name: trait_ref.name.clone(),
                                    expected: self.type_to_string(&required_type),
                                    actual: self.type_to_string(&mount_field.ty),
                                    span: mount_field.span,
                                });
                        }
                    }
                    None => {
                        // Mounting point is missing
                        self.errors.push(CompilerError::MissingTraitMountingPoint {
                            mount: mount_name.clone(),
                            trait_name: trait_ref.name.clone(),
                            span: struct_def.span,
                        });
                    }
                }
            }
        }
    }

    /// Check if two types match (structural equality)
    #[expect(
        clippy::only_used_in_recursion,
        reason = "self is required for recursive dispatch through the method"
    )]
    fn types_match(&self, ty1: &Type, ty2: &Type) -> bool {
        match (ty1, ty2) {
            (Type::Primitive(p1), Type::Primitive(p2)) => p1 == p2,
            (Type::Ident(i1), Type::Ident(i2)) => i1.name == i2.name,
            (Type::Array(elem1), Type::Array(elem2)) => self.types_match(elem1, elem2),
            (Type::Optional(inner1), Type::Optional(inner2)) => self.types_match(inner1, inner2),
            (
                Type::Generic {
                    name: n1, args: a1, ..
                },
                Type::Generic {
                    name: n2, args: a2, ..
                },
            ) => {
                // Generic types match if they have the same base type and matching arguments
                n1.name == n2.name
                    && a1.len() == a2.len()
                    && a1
                        .iter()
                        .zip(a2.iter())
                        .all(|(t1, t2)| self.types_match(t1, t2))
            }
            (Type::TypeParameter(p1), Type::TypeParameter(p2)) => p1.name == p2.name,
            _ => false,
        }
    }

    /// Convert a type to a string for error messages
    #[expect(
        clippy::only_used_in_recursion,
        reason = "self is required for recursive dispatch through the method"
    )]
    fn type_to_string(&self, ty: &Type) -> String {
        match ty {
            Type::Primitive(prim) => match prim {
                PrimitiveType::String => "String".to_string(),
                PrimitiveType::Number => "Number".to_string(),
                PrimitiveType::Boolean => "Boolean".to_string(),
                PrimitiveType::Path => "Path".to_string(),
                PrimitiveType::Regex => "Regex".to_string(),
                PrimitiveType::Never => "Never".to_string(),
                // GPU scalar types
                PrimitiveType::F32 => "f32".to_string(),
                PrimitiveType::I32 => "i32".to_string(),
                PrimitiveType::U32 => "u32".to_string(),
                PrimitiveType::Bool => "bool".to_string(),
                // GPU vector types (float)
                PrimitiveType::Vec2 => "vec2".to_string(),
                PrimitiveType::Vec3 => "vec3".to_string(),
                PrimitiveType::Vec4 => "vec4".to_string(),
                // GPU vector types (signed int)
                PrimitiveType::IVec2 => "ivec2".to_string(),
                PrimitiveType::IVec3 => "ivec3".to_string(),
                PrimitiveType::IVec4 => "ivec4".to_string(),
                // GPU vector types (unsigned int)
                PrimitiveType::UVec2 => "uvec2".to_string(),
                PrimitiveType::UVec3 => "uvec3".to_string(),
                PrimitiveType::UVec4 => "uvec4".to_string(),
                // GPU matrix types
                PrimitiveType::Mat2 => "mat2".to_string(),
                PrimitiveType::Mat3 => "mat3".to_string(),
                PrimitiveType::Mat4 => "mat4".to_string(),
            },
            Type::Ident(ident) => ident.name.clone(),
            Type::Array(element_type) => {
                format!("[{}]", self.type_to_string(element_type))
            }
            Type::Optional(inner_type) => {
                format!("{}?", self.type_to_string(inner_type))
            }
            Type::Tuple(fields) => {
                let field_types: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name.name, self.type_to_string(&f.ty)))
                    .collect();
                format!("({})", field_types.join(", "))
            }
            Type::Generic { name, args, .. } => {
                if args.is_empty() {
                    name.name.clone()
                } else {
                    let arg_types: Vec<String> =
                        args.iter().map(|arg| self.type_to_string(arg)).collect();
                    format!("{}<{}>", name.name, arg_types.join(", "))
                }
            }
            Type::TypeParameter(param) => param.name.clone(),
            Type::Dictionary { key, value } => {
                format!(
                    "[{}: {}]",
                    self.type_to_string(key),
                    self.type_to_string(value)
                )
            }
            Type::Closure { params, ret } => {
                if params.is_empty() {
                    format!("() -> {}", self.type_to_string(ret))
                } else if params.len() == 1 {
                    format!(
                        "{} -> {}",
                        self.type_to_string(&params[0]),
                        self.type_to_string(ret)
                    )
                } else {
                    let param_types: Vec<String> =
                        params.iter().map(|p| self.type_to_string(p)).collect();
                    format!("{} -> {}", param_types.join(", "), self.type_to_string(ret))
                }
            }
        }
    }

    /// Validate function return type matches the body expression type
    fn validate_function_return_type(&mut self, func: &crate::ast::FnDef, file: &File) {
        // Clear local let bindings for this function
        self.local_let_bindings.clear();

        // Register function parameters as local bindings
        // Function parameters are mutable by default (can be assigned to)
        for param in &func.params {
            let ty_str = if let Some(ty) = &param.ty {
                self.validate_type(ty);
                self.type_to_string(ty)
            } else {
                "Unknown".to_string()
            };
            // Register parameter as a local binding with its type (mutable=true for params)
            self.local_let_bindings
                .insert(param.name.name.clone(), (ty_str, true));
        }

        // Validate the function body expression
        self.validate_expr(&func.body, file);

        // If there's a declared return type, check it matches the body type
        if let Some(declared_return_type) = &func.return_type {
            let body_type = self.infer_type(&func.body, file);
            let expected_type = self.type_to_string(declared_return_type);

            // Check if types are compatible
            if !self.type_strings_compatible(&expected_type, &body_type) {
                self.errors.push(CompilerError::FunctionReturnTypeMismatch {
                    function: func.name.name.clone(),
                    expected: expected_type,
                    actual: body_type,
                    span: func.name.span,
                });
            }
        }

        // Clear local let bindings after function
        self.local_let_bindings.clear();
    }

    /// Validate a standalone function definition (outside of impl blocks)
    fn validate_standalone_function(&mut self, func: &crate::ast::FunctionDef, file: &File) {
        // Clear local let bindings for this function
        self.local_let_bindings.clear();

        // Register function parameters as local bindings
        // Function parameters are mutable by default (can be assigned to)
        for param in &func.params {
            let ty_str = if let Some(ty) = &param.ty {
                self.validate_type(ty);
                self.type_to_string(ty)
            } else {
                "Unknown".to_string()
            };
            // Register parameter as a local binding with its type (mutable=true for params)
            self.local_let_bindings
                .insert(param.name.name.clone(), (ty_str, true));
        }

        // Validate return type if declared
        if let Some(return_type) = &func.return_type {
            self.validate_type(return_type);
        }

        // Validate the function body expression
        self.validate_expr(&func.body, file);

        // If there's a declared return type, check it matches the body type
        if let Some(declared_return_type) = &func.return_type {
            let body_type = self.infer_type(&func.body, file);
            let expected_type = self.type_to_string(declared_return_type);

            // Check if types are compatible
            if !self.type_strings_compatible(&expected_type, &body_type) {
                self.errors.push(CompilerError::FunctionReturnTypeMismatch {
                    function: func.name.name.clone(),
                    expected: expected_type,
                    actual: body_type,
                    span: func.name.span,
                });
            }
        }

        // Clear local let bindings after function
        self.local_let_bindings.clear();
    }

    /// Check if two type strings are compatible
    ///
    /// This handles:
    /// - Exact matches
    /// - Number/f32/i32/u32 compatibility
    /// - Unknown/placeholder type params
    /// - InferredEnum matching enum types
    fn type_strings_compatible(&self, expected: &str, actual: &str) -> bool {
        // Exact match
        if expected == actual {
            return true;
        }

        // Allow placeholder types to match anything (incomplete type inference)
        if actual == "Unknown" || actual.ends_with("Result") || actual.starts_with("FunctionResult")
        {
            return true;
        }

        // InferredEnum is compatible with any declared enum type
        // This handles `.variant(...)` syntax where the enum type is inferred from context
        if actual == "InferredEnum" && self.symbols.enums.contains_key(expected) {
            return true;
        }

        // Number is compatible with f32/i32/u32 for GPU types
        if expected == "Number" && (actual == "f32" || actual == "i32" || actual == "u32") {
            return true;
        }
        if actual == "Number" && (expected == "f32" || expected == "i32" || expected == "u32") {
            return true;
        }

        // Boolean and bool are compatible
        if (expected == "Boolean" && actual == "bool")
            || (expected == "bool" && actual == "Boolean")
        {
            return true;
        }

        false
    }

    /// Pass 5: Detect circular dependencies
    /// Build dependency graphs and detect cycles
    fn detect_circular_dependencies(&mut self, file: &File) {
        // 5.1: Detect circular type dependencies
        self.detect_circular_type_dependencies(file);

        // 5.2: Detect circular let binding dependencies
        self.detect_circular_let_dependencies(file);
    }

    /// Pass 5.1: Detect circular type dependencies
    /// Build a type dependency graph and detect cycles
    fn detect_circular_type_dependencies(&mut self, file: &File) {
        let mut type_graph = TypeGraph::new();
        let mut type_spans: HashMap<String, Span> = HashMap::new();

        // Build the type dependency graph
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                match &**def {
                    Definition::Trait(trait_def) => {
                        let trait_name = trait_def.name.name.clone();
                        type_spans.insert(trait_name.clone(), trait_def.span);

                        // Add dependencies from trait inheritance (trait A: B)
                        for parent_trait in &trait_def.traits {
                            type_graph
                                .add_dependency(trait_name.clone(), parent_trait.name.clone());
                        }

                        // Add dependencies from trait fields
                        for field in &trait_def.fields {
                            Self::add_type_dependencies(&mut type_graph, &trait_name, &field.ty);
                        }

                        // Note: Mount points are NOT added to the dependency graph.
                        // Mount points are "slots" filled at runtime with child content,
                        // so self-referential mount points (e.g., `mount body: View` in View trait)
                        // are valid and don't create impossible-to-construct types.
                        // The recursion is always broken by terminal types like Empty.
                    }
                    Definition::Struct(struct_def) => {
                        let struct_name = struct_def.name.name.clone();
                        type_spans.insert(struct_name.clone(), struct_def.span);

                        // Add dependencies from struct fields
                        for field in &struct_def.fields {
                            Self::add_type_dependencies(&mut type_graph, &struct_name, &field.ty);
                        }

                        // Note: Mount points are NOT added to the dependency graph.
                        // See comment above in trait handling for rationale.
                    }
                    Definition::Enum(_) => {
                        // Enums don't create type dependencies (they have associated data, not fields)
                        // Associated data types are validated in Pass 2
                    }
                    Definition::Impl(_) => {
                        // Impl blocks don't create type dependencies
                        // Dependencies are already tracked via the struct definition
                    }
                    Definition::Module(_) => {
                        // Modules don't create type dependencies themselves
                        // Type dependencies are handled per nested definition
                    }
                    Definition::Function(_) => {
                        // Standalone functions don't create type dependencies
                    }
                }
            }
        }

        // Detect cycles
        let cycles = type_graph.find_cycles();

        // Report errors for each cycle found
        for cycle in cycles {
            if !cycle.is_empty() {
                // Get the span of the first type in the cycle
                let span = cycle
                    .first()
                    .and_then(|t| type_spans.get(t))
                    .copied()
                    .unwrap_or_default();

                // Format the cycle as "A -> B -> C -> A"
                let cycle_str = cycle.join(" -> ");

                self.errors.push(CompilerError::CircularDependency {
                    cycle: cycle_str,
                    span,
                });
            }
        }
    }

    /// Pass 5.2: Detect circular let binding dependencies
    /// Build a let binding dependency graph and detect cycles
    fn detect_circular_let_dependencies(&mut self, file: &File) {
        let mut let_graph = TypeGraph::new();
        let mut let_spans: HashMap<String, Span> = HashMap::new();

        // Build the let binding dependency graph
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                // Get all bindings from the pattern
                let bindings = collect_bindings_from_pattern(&let_binding.pattern);
                if bindings.is_empty() {
                    continue;
                }

                // Register each binding and store its span
                for binding in &bindings {
                    let_spans.insert(binding.name.clone(), binding.span);
                }

                // Extract all let binding references from the value expression
                let references = self.extract_let_references(&let_binding.value);

                // Add dependencies for each binding from the pattern
                // All bindings from a single let share the same dependencies
                for binding in &bindings {
                    for referenced_let in &references {
                        let_graph.add_dependency(&binding.name, referenced_let.clone());
                    }
                }
            }
        }

        // Detect cycles
        let cycles = let_graph.find_cycles();

        // Report errors for each cycle found
        for cycle in cycles {
            if !cycle.is_empty() {
                // Get the span of the first let binding in the cycle
                let span = cycle
                    .first()
                    .and_then(|l| let_spans.get(l))
                    .copied()
                    .unwrap_or_default();

                // Format the cycle as "a -> b -> c -> a"
                let cycle_str = cycle.join(" -> ");

                self.errors.push(CompilerError::CircularDependency {
                    cycle: cycle_str,
                    span,
                });
            }
        }
    }

    /// Extract all let binding references from an expression
    /// Returns a set of let binding names that this expression depends on
    fn extract_let_references(&self, expr: &Expr) -> HashSet<String> {
        let mut references = HashSet::new();

        match expr {
            Expr::Literal(_) => {
                // Literals don't reference let bindings
            }
            Expr::Array { elements, .. } => {
                // Recursively extract from array elements
                for elem in elements {
                    references.extend(self.extract_let_references(elem));
                }
            }
            Expr::Tuple { fields, .. } => {
                // Recursively extract from tuple field expressions
                for (_, field_expr) in fields {
                    references.extend(self.extract_let_references(field_expr));
                }
            }
            Expr::Reference { path, .. } => {
                // A simple reference (single identifier) might be a let binding
                if path.len() == 1 {
                    let name = &path[0].name;
                    // Check if it's a let binding (not a model/view/enum)
                    if self.symbols.is_let(name) {
                        references.insert(name.clone());
                    }
                }
                // Path references like User::admin or user.name don't reference let bindings directly
                // (they reference fields/variants, not let bindings)
            }
            Expr::Invocation {
                args,
                mounts,
                type_args: _,
                ..
            } => {
                // Extract from argument expressions
                for (_, arg_expr) in args {
                    references.extend(self.extract_let_references(arg_expr));
                }
                // Extract from mounting point expressions
                for (_, mount_expr) in mounts {
                    references.extend(self.extract_let_references(mount_expr));
                }
            }
            Expr::EnumInstantiation { data, .. } => {
                // Extract from named field expressions
                for (_, data_expr) in data {
                    references.extend(self.extract_let_references(data_expr));
                }
            }
            Expr::InferredEnumInstantiation { data, .. } => {
                // Extract from named field expressions
                for (_, data_expr) in data {
                    references.extend(self.extract_let_references(data_expr));
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                // Extract from both operands
                references.extend(self.extract_let_references(left));
                references.extend(self.extract_let_references(right));
            }
            Expr::UnaryOp { operand, .. } => {
                // Extract from operand
                references.extend(self.extract_let_references(operand));
            }
            Expr::ForExpr {
                collection, body, ..
            } => {
                // Extract from collection and body
                references.extend(self.extract_let_references(collection));
                references.extend(self.extract_let_references(body));
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Extract from condition and branches
                references.extend(self.extract_let_references(condition));
                references.extend(self.extract_let_references(then_branch));
                if let Some(else_expr) = else_branch {
                    references.extend(self.extract_let_references(else_expr));
                }
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                // Extract from scrutinee and arm bodies
                references.extend(self.extract_let_references(scrutinee));
                for arm in arms {
                    references.extend(self.extract_let_references(&arm.body));
                }
            }
            Expr::Group { expr, .. } => {
                // Extract from grouped expression
                references.extend(self.extract_let_references(expr));
            }
            Expr::DictLiteral { entries, .. } => {
                // Extract from all key-value expressions
                for (key, value) in entries {
                    references.extend(self.extract_let_references(key));
                    references.extend(self.extract_let_references(value));
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                // Extract from dictionary and key expressions
                references.extend(self.extract_let_references(dict));
                references.extend(self.extract_let_references(key));
            }
            Expr::FieldAccess { object, .. } => {
                // Extract from object expression
                references.extend(self.extract_let_references(object));
            }
            Expr::ClosureExpr { body, .. } => {
                // Extract from closure body
                references.extend(self.extract_let_references(body));
            }
            Expr::LetExpr { value, body, .. } => {
                // Extract from value and body expressions
                references.extend(self.extract_let_references(value));
                references.extend(self.extract_let_references(body));
            }
            Expr::MethodCall { receiver, args, .. } => {
                // Extract from receiver and argument expressions
                references.extend(self.extract_let_references(receiver));
                for arg in args {
                    references.extend(self.extract_let_references(arg));
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                // Extract from block statements and result
                for stmt in statements {
                    match stmt {
                        BlockStatement::Let { value, .. } => {
                            references.extend(self.extract_let_references(value));
                        }
                        BlockStatement::Assign { target, value, .. } => {
                            references.extend(self.extract_let_references(target));
                            references.extend(self.extract_let_references(value));
                        }
                        BlockStatement::Expr(expr) => {
                            references.extend(self.extract_let_references(expr));
                        }
                    }
                }
                references.extend(self.extract_let_references(result));
            }
        }

        references
    }

    /// Add type dependencies from a type to the graph
    /// Recursively extracts type names from arrays and optionals
    fn add_type_dependencies(graph: &mut TypeGraph, from: &str, ty: &Type) {
        match ty {
            Type::Primitive(_) => {
                // Primitive types don't create dependencies
            }
            Type::Ident(ident) => {
                // Direct type reference creates a dependency
                graph.add_dependency(from.to_string(), ident.name.clone());
            }
            Type::Array(element_ty) => {
                // Array element type creates a dependency
                // Note: Currently arrays don't break cycles, so [Node] still creates Node -> Node
                Self::add_type_dependencies(graph, from, element_ty);
            }
            Type::Optional(inner_ty) => {
                // Optional inner type creates a dependency
                Self::add_type_dependencies(graph, from, inner_ty);
            }
            Type::Tuple(fields) => {
                // Tuple field types create dependencies
                for field in fields {
                    Self::add_type_dependencies(graph, from, &field.ty);
                }
            }
            Type::Generic { name, args, .. } => {
                // Generic type: Container<T, U> creates dependency on the base type
                // and recursively on all type arguments
                graph.add_dependency(from.to_string(), name.name.clone());
                for arg in args {
                    Self::add_type_dependencies(graph, from, arg);
                }
            }
            Type::TypeParameter(_) => {
                // Type parameters (e.g., T in struct Container<T>) don't create dependencies
                // because they're resolved at instantiation time, not definition time
            }
            Type::Dictionary { key, value } => {
                // Recursively add dependencies for key and value types
                Self::add_type_dependencies(graph, from, key);
                Self::add_type_dependencies(graph, from, value);
            }
            Type::Closure { params, ret } => {
                // Recursively add dependencies for parameter and return types
                for param in params {
                    Self::add_type_dependencies(graph, from, param);
                }
                Self::add_type_dependencies(graph, from, ret);
            }
        }
    }

    /// Infer the type of an expression (simplified type inference)
    #[expect(
        clippy::only_used_in_recursion,
        reason = "self is required for recursive dispatch through the method"
    )]
    fn infer_type(&self, expr: &Expr, file: &File) -> String {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::String(_) => "String".to_string(),
                Literal::Number(_) => "Number".to_string(),
                Literal::UnsignedInt(_) => "u32".to_string(),
                Literal::SignedInt(_) => "i32".to_string(),
                Literal::Boolean(_) => "Boolean".to_string(),
                Literal::Regex { .. } => "Regex".to_string(),
                Literal::Path(_) => "Path".to_string(),
                Literal::Nil => "nil".to_string(),
            },
            Expr::Array { elements, .. } => {
                // Infer element type from first element
                if let Some(first) = elements.first() {
                    let elem_type = self.infer_type(first, file);
                    format!("[{}]", elem_type)
                } else {
                    "[Unknown]".to_string()
                }
            }
            Expr::Tuple { fields, .. } => {
                // Infer tuple type from field types
                let field_types: Vec<String> = fields
                    .iter()
                    .map(|(name, expr)| {
                        let ty = self.infer_type(expr, file);
                        format!("{}: {}", name.name, ty)
                    })
                    .collect();
                format!("({})", field_types.join(", "))
            }
            Expr::Invocation {
                path,
                type_args,
                args,
                ..
            } => {
                // Join path to get the name
                let name = path
                    .iter()
                    .map(|id| id.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");

                // Check if this is a struct instantiation or function call
                if self.symbols.is_struct(&name) {
                    // Struct instantiation - return the struct type
                    if type_args.is_empty() {
                        name
                    } else {
                        let arg_types: Vec<String> =
                            type_args.iter().map(|ty| self.type_to_string(ty)).collect();
                        format!("{}<{}>", name, arg_types.join(", "))
                    }
                } else {
                    // Function call - infer return type from builtin or user-defined function
                    // For builtins, we can look up the return type based on argument types
                    if crate::builtins::BuiltinRegistry::global().is_builtin(&name) {
                        // Get argument types for builtin function resolution
                        let arg_types: Vec<String> = args
                            .iter()
                            .map(|(_, expr)| self.infer_type(expr, file))
                            .collect();
                        // Try to resolve builtin return type
                        if let Some(ret_type) = crate::builtins::BuiltinRegistry::global()
                            .resolve_return_type(&name, &arg_types)
                        {
                            ret_type
                        } else {
                            "Number".to_string() // Fallback for builtin functions
                        }
                    } else {
                        // User-defined function - would need function table lookup
                        // For now, return a generic type
                        "Function".to_string()
                    }
                }
            }
            Expr::EnumInstantiation { enum_name, .. } => enum_name.name.clone(),
            Expr::InferredEnumInstantiation { .. } => {
                // Type inference for inferred enum instantiation will be done in semantic analysis
                "InferredEnum".to_string()
            }
            Expr::Reference { path, .. } => {
                // Handle self.field references
                if !path.is_empty() && path[0].name == "self" {
                    if path.len() == 2 {
                        // self.field - look up field type in current impl struct
                        let field_name = &path[1].name;
                        if let Some(ref struct_name) = self.current_impl_struct {
                            if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                                // Check regular fields
                                for field in &struct_info.fields {
                                    if field.name == *field_name {
                                        return self.type_to_string(&field.ty);
                                    }
                                }
                                // Check mount fields
                                for field in &struct_info.mount_fields {
                                    if field.name == *field_name {
                                        return self.type_to_string(&field.ty);
                                    }
                                }
                            }
                        }
                    }
                    // For self without field, or self.field.subfield, return Unknown for now
                    return "Unknown".to_string();
                }

                // For references, resolve the type
                if path.len() == 1 {
                    // Simple reference - check if it's a let binding
                    let name = &path[0].name;
                    if let Some(let_type) = self.symbols.get_let_type(name) {
                        return let_type.to_string();
                    }
                    // Check if it's a local let binding or function parameter
                    if let Some((local_type, _mutable)) = self.local_let_bindings.get(name) {
                        return local_type.clone();
                    }
                    // Check if we're inside an impl block and this is a field reference
                    if let Some(ref struct_name) = self.current_impl_struct {
                        if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                            // Check regular fields
                            for field in &struct_info.fields {
                                if field.name == *name {
                                    return self.type_to_string(&field.ty);
                                }
                            }
                            // Check mount fields
                            for field in &struct_info.mount_fields {
                                if field.name == *name {
                                    return self.type_to_string(&field.ty);
                                }
                            }
                        }
                    }
                }
                // For other references (field access, enum variants, etc.),
                // return the last component for now
                path.last()
                    .map(|ident| ident.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string())
            }
            Expr::BinaryOp { left, op, .. } => {
                // Result type depends on operator
                match op {
                    BinaryOperator::Add
                    | BinaryOperator::Sub
                    | BinaryOperator::Mul
                    | BinaryOperator::Div
                    | BinaryOperator::Mod => self.infer_type(left, file), // Same as operand type
                    BinaryOperator::Lt
                    | BinaryOperator::Gt
                    | BinaryOperator::Le
                    | BinaryOperator::Ge
                    | BinaryOperator::Eq
                    | BinaryOperator::Ne
                    | BinaryOperator::And
                    | BinaryOperator::Or => "Boolean".to_string(),
                    BinaryOperator::Range => format!("Range<{}>", self.infer_type(left, file)),
                }
            }
            Expr::UnaryOp { op, operand, .. } => {
                // Result type depends on operator
                match op {
                    UnaryOperator::Neg => self.infer_type(operand, file), // Same as operand type
                    UnaryOperator::Not => "Boolean".to_string(),
                }
            }
            Expr::ForExpr { body, .. } => {
                // For expression returns an array of the body type
                let body_type = self.infer_type(body, file);
                format!("[{}]", body_type)
            }
            Expr::IfExpr { then_branch, .. } => {
                // Type is the type of the then branch
                self.infer_type(then_branch, file)
            }
            Expr::MatchExpr { arms, .. } => {
                // Type is the type of the first arm's body (simplified)
                arms.first()
                    .map(|arm| self.infer_type(&arm.body, file))
                    .unwrap_or_else(|| "Unknown".to_string())
            }
            Expr::Group { expr, .. } => self.infer_type(expr, file),
            Expr::DictLiteral { .. } => {
                // Dictionary literals - type would need to be inferred from entries
                "Dictionary".to_string()
            }
            Expr::DictAccess { .. } => {
                // Dictionary access returns the value type - simplified
                "Unknown".to_string()
            }
            Expr::FieldAccess { .. } => {
                // Field access returns the field type - simplified
                "Unknown".to_string()
            }
            Expr::ClosureExpr { .. } => {
                // Closures are function types
                "Closure".to_string()
            }
            Expr::LetExpr { body, .. } => {
                // Let expressions have the type of their body
                self.infer_type(body, file)
            }
            Expr::MethodCall { .. } => {
                // Method call return type - would need method signature lookup
                "Unknown".to_string()
            }
            Expr::Block { result, .. } => {
                // Block expressions have the type of their result expression
                self.infer_type(result, file)
            }
        }
    }

    /// Check if an expression is mutable
    /// An expression is mutable if:
    /// - It's a reference to a mutable let binding
    /// - It's a field access where the entire chain is mutable (upward propagation)
    /// - It's a context access that was marked as mutable
    /// - It's an array element where the array is mutable
    fn is_expr_mutable(&self, expr: &Expr, file: &File) -> bool {
        match expr {
            // Literal values are never mutable
            Expr::Literal(_) => false,

            // References can be mutable if they refer to mutable let bindings or fields
            Expr::Reference { path, .. } => {
                if path.is_empty() {
                    return false;
                }

                // Check if this is a reference to a let binding
                if path.len() == 1 {
                    return self.is_let_mutable(&path[0].name, file);
                }

                // For field access like `user.email`, check if:
                // 1. The root (user) is mutable
                // 2. The field (email) is mutable
                // Both must be true (upward propagation)
                let root_name = &path[0].name;
                let is_root_mutable = self.is_let_mutable(root_name, file);

                if !is_root_mutable {
                    return false;
                }

                // Check if all fields in the chain are mutable
                // For user.profile.email, we need: user is mut, profile field is mut, email field is mut
                self.is_field_chain_mutable(&path[0].name, &path[1..], file)
            }

            // Array elements are mutable if the array expression is mutable
            Expr::Array { .. } => false, // Array literals are not mutable

            // Tuple literals are not mutable
            Expr::Tuple { .. } => false,

            // Invocation (struct instantiation or function call) returns new values, not mutable
            Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. } => false,

            // Binary and unary operations are not mutable
            Expr::BinaryOp { .. } => false,
            Expr::UnaryOp { .. } => false,

            // For/If/Match expressions result in new values, not mutable
            Expr::ForExpr { .. } | Expr::IfExpr { .. } | Expr::MatchExpr { .. } => false,

            // Grouped expressions delegate to inner expression
            Expr::Group { expr, .. } => self.is_expr_mutable(expr, file),

            // Dictionary expressions are not mutable
            Expr::DictLiteral { .. } => false,
            Expr::DictAccess { .. } => false,

            // Field access depends on the object
            Expr::FieldAccess { object, .. } => self.is_expr_mutable(object, file),

            // Closure expressions are not mutable
            Expr::ClosureExpr { .. } => false,

            // Let expressions delegate to their body
            Expr::LetExpr { body, .. } => self.is_expr_mutable(body, file),

            // Method calls return new values, not mutable
            Expr::MethodCall { .. } => false,

            // Block expressions delegate to their result
            Expr::Block { result, .. } => self.is_expr_mutable(result, file),
        }
    }

    /// Check if a let binding is mutable
    fn is_let_mutable(&self, name: &str, file: &File) -> bool {
        // First check local let bindings (function params, block lets)
        if let Some((_, mutable)) = self.local_let_bindings.get(name) {
            return *mutable;
        }

        // Then check file-level let bindings
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                // Check if the name is in any binding from this pattern
                for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                    if binding.name == name {
                        return let_binding.mutable;
                    }
                }
            }
        }
        false
    }

    /// Check if a field access chain is mutable
    /// For path like ["profile", "email"], check that both profile and email fields are mutable
    fn is_field_chain_mutable(
        &self,
        root_name: &str,
        field_path: &[crate::ast::Ident],
        file: &File,
    ) -> bool {
        if field_path.is_empty() {
            return true;
        }

        // Get the type of the root to find which struct it refers to
        let root_type = self.get_let_type(root_name, file);

        // Check each field in the chain
        let mut current_type = root_type;
        for field_ident in field_path {
            // Check if the current field is mutable in its type
            if !self.is_struct_field_mutable(&current_type, &field_ident.name, file) {
                return false;
            }

            // Get the type of this field to continue checking the chain
            current_type = self.get_field_type(&current_type, &field_ident.name, file);
        }

        true
    }

    /// Get the type of a let binding
    fn get_let_type(&self, name: &str, file: &File) -> String {
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                // Check if the name is in any binding from this pattern
                for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                    if binding.name == name {
                        return self.infer_type(&let_binding.value, file);
                    }
                }
            }
        }
        "Unknown".to_string()
    }

    /// Check if a struct field is mutable
    fn is_struct_field_mutable(&self, type_name: &str, field_name: &str, file: &File) -> bool {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == type_name {
                        for field in &struct_def.fields {
                            if field.name.name == field_name {
                                return field.mutable;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Get the type of a struct field
    fn get_field_type(&self, type_name: &str, field_name: &str, file: &File) -> String {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == type_name {
                        for field in &struct_def.fields {
                            if field.name.name == field_name {
                                return self.type_to_string(&field.ty);
                            }
                        }
                    }
                }
            }
        }
        "Unknown".to_string()
    }

    /// Push a generic scope for a definition with generic parameters
    fn push_generic_scope(&mut self, generics: &[crate::ast::GenericParam]) {
        let mut scope = GenericScope {
            params: HashMap::new(),
        };

        for param in generics {
            let constraints: Vec<String> = param
                .constraints
                .iter()
                .map(|c| match c {
                    crate::ast::GenericConstraint::Trait(ident) => ident.name.clone(),
                })
                .collect();

            scope.params.insert(param.name.name.clone(), constraints);
        }

        self.generic_scopes.push(scope);
    }

    /// Pop the current generic scope
    fn pop_generic_scope(&mut self) {
        self.generic_scopes.pop();
    }

    /// Check if a name is a type parameter in the current generic scopes
    fn is_type_parameter(&self, name: &str) -> bool {
        // Search from the most recent scope backwards
        for scope in self.generic_scopes.iter().rev() {
            if scope.params.contains_key(name) {
                return true;
            }
        }
        false
    }

    /// Get the constraints for a type parameter if it's in scope
    fn get_type_parameter_constraints(&self, name: &str) -> Option<Vec<String>> {
        // Search from the most recent scope backwards
        for scope in self.generic_scopes.iter().rev() {
            if let Some(constraints) = scope.params.get(name) {
                return Some(constraints.clone());
            }
        }
        None
    }

    /// Resolve a nested module type path (e.g., ["outer", "inner", "Type"])
    /// Returns Some(error_message) if the type doesn't exist, None if valid
    fn resolve_nested_module_type(&self, parts: &[&str], _span: Span) -> Option<String> {
        if parts.is_empty() {
            return Some("empty module path".to_string());
        }

        // The last part is the type name, the rest are module names
        let Some((type_name, module_parts)) = parts.split_last() else {
            return Some("empty module path".to_string());
        };

        // Traverse nested modules
        let mut current_symbols = &self.symbols;
        let mut path_so_far = String::new();

        for (i, module_name) in module_parts.iter().enumerate() {
            if i > 0 {
                path_so_far.push_str("::");
            }
            path_so_far.push_str(module_name);

            if let Some(module_info) = current_symbols.modules.get(*module_name) {
                current_symbols = &module_info.symbols;
            } else {
                return Some(format!("module '{}' not found", path_so_far));
            }
        }

        // Check if type exists in the final module
        if !current_symbols.is_type(type_name) && !current_symbols.is_trait(type_name) {
            return Some(format!(
                "type '{}' not found in module '{}'",
                type_name, path_so_far
            ));
        }

        None // Type is valid
    }

    /// Check if a method exists on a given type
    ///
    /// Handles:
    /// 1. Builtin methods on primitive types (vec3.normalize(), mat4.transpose(), etc.)
    /// 2. User-defined methods in impl blocks
    fn method_exists_on_type(&self, type_name: &str, method_name: &str, file: &File) -> bool {
        // Skip validation for unknown types (chained method calls where we can't infer intermediate types)
        if type_name == "Unknown" || type_name.contains("Unknown") {
            return true;
        }

        // Check if it's a primitive GPU type with builtin methods
        if let Some(prim) = self.string_to_primitive_type(type_name) {
            if crate::builtins::resolve_method_type(prim, method_name).is_some() {
                return true;
            }
        }

        // Check if the method is a common builtin that works on numbers/vectors
        // This handles chained calls where type inference might not propagate correctly
        let common_builtins = [
            "abs",
            "sign",
            "floor",
            "ceil",
            "round",
            "trunc",
            "fract",
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "exp",
            "log",
            "sqrt",
            "pow",
            "min",
            "max",
            "clamp",
            "normalize",
            "length",
            "distance",
            "dot",
            "cross",
            "saturate",
            "radians",
            "degrees",
        ];
        if common_builtins.contains(&method_name) {
            return true;
        }

        // Check if it's a struct with an impl block containing the method
        if self.symbols.is_struct(type_name) {
            // Check impl blocks in the current file
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Impl(impl_def) = &**def {
                        if impl_def.name.name == type_name {
                            for func in &impl_def.functions {
                                if func.name.name == method_name {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            // Also check if there's an impl in the symbol table
            if self.symbols.impls.contains_key(type_name) {
                // The impl exists, but we need to check for the method
                // For now, if we find an impl, we assume the method might exist
                // (the impl block methods aren't stored in the symbol table currently)
            }
        }

        false
    }

    /// Convert a type string to a PrimitiveType if applicable
    fn string_to_primitive_type(&self, type_name: &str) -> Option<PrimitiveType> {
        match type_name {
            "f32" | "Number" => Some(PrimitiveType::F32),
            "i32" => Some(PrimitiveType::I32),
            "u32" => Some(PrimitiveType::U32),
            "bool" | "Boolean" => Some(PrimitiveType::Bool),
            "vec2" => Some(PrimitiveType::Vec2),
            "vec3" => Some(PrimitiveType::Vec3),
            "vec4" => Some(PrimitiveType::Vec4),
            "ivec2" => Some(PrimitiveType::IVec2),
            "ivec3" => Some(PrimitiveType::IVec3),
            "ivec4" => Some(PrimitiveType::IVec4),
            "uvec2" => Some(PrimitiveType::UVec2),
            "uvec3" => Some(PrimitiveType::UVec3),
            "uvec4" => Some(PrimitiveType::UVec4),
            "mat2" => Some(PrimitiveType::Mat2),
            "mat3" => Some(PrimitiveType::Mat3),
            "mat4" => Some(PrimitiveType::Mat4),
            _ => None,
        }
    }

    /// Check if a type satisfies a trait constraint
    ///
    /// A type satisfies a trait constraint if:
    /// 1. It's a struct that implements the trait (via : Trait or impl Trait for Struct)
    /// 2. It's an enum that implements the trait
    /// 3. It's a type parameter that has the constraint in scope
    fn type_satisfies_trait_constraint(&self, ty: &Type, trait_name: &str) -> bool {
        match ty {
            Type::Ident(ident) => {
                // Check if struct implements the trait
                if let Some(struct_info) = self.symbols.get_struct(&ident.name) {
                    // Check inline traits (struct Foo: Trait)
                    if struct_info.traits.iter().any(|t| t == trait_name) {
                        return true;
                    }
                }
                // Check trait impls (impl Trait for Struct)
                let all_traits = self.symbols.get_all_traits_for_struct(&ident.name);
                if all_traits.contains(&trait_name.to_string()) {
                    return true;
                }
                // Check if enum implements the trait
                let enum_traits = self.symbols.get_all_traits_for_enum(&ident.name);
                if enum_traits.contains(&trait_name.to_string()) {
                    return true;
                }
                false
            }
            Type::Generic { name, .. } => {
                // For generic types, check if the base type implements the trait
                if let Some(struct_info) = self.symbols.get_struct(&name.name) {
                    if struct_info.traits.iter().any(|t| t == trait_name) {
                        return true;
                    }
                }
                let all_traits = self.symbols.get_all_traits_for_struct(&name.name);
                all_traits.contains(&trait_name.to_string())
            }
            Type::TypeParameter(param) => {
                // Check if the type parameter has the constraint in scope
                if let Some(constraints) = self.get_type_parameter_constraints(&param.name) {
                    return constraints.contains(&trait_name.to_string());
                }
                false
            }
            // Primitives don't implement user-defined traits
            Type::Primitive(_) => false,
            // Arrays, optionals, tuples, etc. don't implement user-defined traits
            Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::Closure { .. } => false,
        }
    }

    /// Get a reference to the symbol table for querying
    /// This is primarily used by LSP features for completion, hover, etc.
    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    /// Get all cached IR modules from imports.
    ///
    /// Returns a map from file path to IrModule for all modules that were
    /// analyzed during import resolution. Used by WGSL codegen to generate
    /// impl blocks from imported types.
    ///
    /// # Returns
    ///
    /// Reference to the cached IR modules. Empty if no imports were processed.
    pub fn imported_ir_modules(&self) -> &HashMap<PathBuf, crate::ir::IrModule> {
        // TODO: Implement - currently returns empty cache
        // Will be populated during parse_and_analyze_module()
        &self.module_ir_cache
    }
}

// Note: No Default implementation since a ModuleResolver is required
