// Semantic analysis (validation only - no evaluation or expansion)
// Pass 0: Resolve modules and imports
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

mod circular;
mod inference;
mod trait_check;
mod type_resolution;
mod validation;

/// Re-exports of the symbol-table shapes that IR lowering and downstream
/// tooling consume. These are the minimal public contract between the
/// semantic and IR layers.
pub use symbol_table::{
    EnumInfo, LetInfo, ModuleInfo, StructInfo, SymbolKind, SymbolTable, TraitInfo,
};

use crate::ast::{
    ArrayPatternElement, BindingPattern, Definition, File, ParamConvention, Statement, Type,
    UseItems, UseStmt,
};
use crate::error::CompilerError;
use crate::location::Span;
use import_graph::ImportGraph;
use module_resolver::{ModuleError, ModuleResolver};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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

/// Return true if `name` is the name of a built-in primitive type.
///
/// Primitive names are not lexer keywords — they parse as regular identifiers
/// and are mapped to `Type::Primitive` at type position by the parser. User
/// definitions that reuse these names must be rejected here with
/// `PrimitiveRedefinition` rather than silently shadowing the built-in.
pub(crate) fn is_primitive_name(name: &str) -> bool {
    matches!(
        name,
        "String" | "Number" | "Boolean" | "Path" | "Regex" | "Never"
    )
}

/// If `ty` is `[T]`, return `T`. Otherwise, return None.
fn strip_array_type(ty: &str) -> Option<&str> {
    let trimmed = ty.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed.contains(':') {
        Some(trimmed[1..trimmed.len().saturating_sub(1)].trim())
    } else {
        None
    }
}

/// Parse a tuple type string like `(a: Number, b: String)` into a flat list
/// of field type strings `["Number", "String"]`. Commas inside nested
/// generics/tuples/arrays are respected.
fn parse_tuple_field_types(ty: &str) -> Vec<String> {
    let trimmed = ty.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return Vec::new();
    }
    let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
    let mut fields = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' | '[' | '<' => depth = depth.saturating_add(1),
            ')' | ']' | '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                fields.push(inner[start..i].to_string());
                start = i.saturating_add(1);
            }
            _ => {}
        }
    }
    if start < inner.len() {
        fields.push(inner[start..].to_string());
    }
    fields
        .into_iter()
        .map(|part| {
            let p = part.trim();
            // Strip leading `name:` from "name: Type"
            p.split_once(':')
                .map_or_else(|| p.to_string(), |(_, ty)| ty.trim().to_string())
        })
        .collect()
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

/// Semantic analyser for a parsed `FormaLang` program.
///
/// `SemanticAnalyzer` runs name resolution, type inference, trait checking,
/// and module resolution against a given AST. It is the source of truth for
/// the compiler's symbol table and is consumed by IR lowering and by
/// LSP-style consumers that need completion, hover, or go-to-definition.
///
/// # Typical use
///
/// Most callers should use [`compile_with_analyzer`](crate::compile_with_analyzer),
/// which lexes, parses, and analyses in one step and returns both the AST
/// and the analyser. Direct construction via [`Self::new_with_file`] is
/// reserved for code that already has a parsed [`File`] in hand.
///
/// # Module resolution
///
/// Imports are resolved through the generic `R: ModuleResolver`. Use
/// [`FileSystemResolver`](crate::FileSystemResolver) for disk-backed files,
/// or provide your own resolver for in-memory or network-backed modules.
pub struct SemanticAnalyzer<R: ModuleResolver> {
    symbols: SymbolTable,
    errors: Vec<CompilerError>,
    resolver: R,
    import_graph: ImportGraph,
    /// Cache of parsed modules (path -> (AST, `SymbolTable`))
    module_cache: HashMap<PathBuf, (File, SymbolTable)>,
    /// Cache of IR modules for imported modules (keyed by file path)
    ///
    /// Populated during `parse_and_analyze_module()` to enable codegen
    /// backends to generate impl blocks from imported types.
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
    /// Bindings consumed by a `sink` parameter call — cannot be used after
    consumed_bindings: HashSet<String>,
    /// Scoped overrides used during inference. When inferring the body of
    /// a match arm or a similar pattern-introducing construct, the
    /// pattern's bindings (variant fields → element types) are pushed as
    /// a frame here. `infer_type_reference` consults this stack first
    /// (innermost frame wins) before falling back to `local_let_bindings`.
    /// Wrapped in `RefCell` so the read-only `infer_type` family doesn't
    /// have to thread `&mut self` through every helper.
    ///
    /// Audit finding #27.
    pub(super) inference_scope_stack: std::cell::RefCell<Vec<HashMap<String, String>>>,
    /// Conventions for closure-typed bindings: `binding_name` → param conventions in order
    closure_binding_conventions: HashMap<String, Vec<ParamConvention>>,
    /// Free-variable captures for closure-typed let bindings, used for
    /// escape-aware ownership propagation.
    ///
    /// `binding_name` → list of outer binding names referenced from the
    /// closure body (excluding the closure's own parameters and any bindings
    /// introduced locally inside it).
    ///
    /// Model: MVS-style ownership with closure escape analysis.
    ///
    /// - A closure's captures are borrowed (view) as long as the closure
    ///   stays in its defining scope — used at call sites to emit
    ///   `UseAfterSink` when an invoked closure references a binding that has
    ///   since been consumed by a sink parameter.
    /// - When a closure escapes (sink-pass to function/method, struct field
    ///   assignment, array/tuple/dict entry), ownership transfers with it:
    ///   each captured binding is marked consumed at the escape site.
    /// - Transitive: if closure A captures closure B and A escapes, B's
    ///   captures are also consumed.
    /// - Function-return escape: when a function's declared return type is
    ///   a closure type, the returned closure's captures are validated
    ///   against `current_fn_param_conventions`. Only `sink` parameters and
    ///   outer-scope bindings (module-level or wider) may be captured; local
    ///   `let` bindings and `let`/`mut` parameters would leave dangling
    ///   captures and are rejected with
    ///   `ClosureCaptureEscapesLocalBinding`. A `sink`-parameter capture
    ///   that escapes is marked consumed in the function's scope.
    ///
    /// Not covered:
    /// - Closures stored in arbitrary non-let places (e.g., assigned to a
    ///   struct field after construction via field assignment); only
    ///   construction-site field assignment is tracked.
    pub(super) closure_binding_captures: HashMap<String, Vec<String>>,
    /// All closure-binding captures created anywhere in the currently-validating
    /// function body, flat across nested block scopes. Cleared at function
    /// entry/exit. Used by the function-return escape check to classify captures
    /// when a named closure binding is returned (see `validate_function_return_escape`).
    pub(super) fn_scope_closure_captures: HashMap<String, Vec<String>>,
    /// Parameter conventions for the currently-validated function body.
    ///
    /// Populated on entry to a function body and cleared on exit. Used by the
    /// return-escape check to distinguish `sink` parameters (ownership
    /// transfers into a returned closure) from `let`/`mut` parameters (views
    /// that cannot escape) and from function-local `let` bindings.
    pub(super) current_fn_param_conventions: HashMap<String, ParamConvention>,
    /// Recursion depth counter for `validate_expr` (to prevent stack overflow)
    validate_expr_depth: usize,
}

impl<R: ModuleResolver> std::fmt::Debug for SemanticAnalyzer<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticAnalyzer")
            .field("symbols", &self.symbols)
            .field("errors", &self.errors)
            .finish_non_exhaustive()
    }
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
            consumed_bindings: HashSet::new(),
            inference_scope_stack: std::cell::RefCell::new(Vec::new()),
            closure_binding_conventions: HashMap::new(),
            closure_binding_captures: HashMap::new(),
            fn_scope_closure_captures: HashMap::new(),
            current_fn_param_conventions: HashMap::new(),
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
            consumed_bindings: HashSet::new(),
            inference_scope_stack: std::cell::RefCell::new(Vec::new()),
            closure_binding_conventions: HashMap::new(),
            closure_binding_captures: HashMap::new(),
            fn_scope_closure_captures: HashMap::new(),
            current_fn_param_conventions: HashMap::new(),
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
    fn module_error_to_compiler_error(
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

    /// Helper to collect a definition into a symbol table (static version for module parsing)
    fn collect_definition_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        def: &Definition,
    ) {
        match def {
            Definition::Trait(trait_def) => {
                Self::collect_trait_into(symbols, errors, trait_def);
            }
            Definition::Struct(struct_def) => {
                Self::collect_struct_into(symbols, errors, struct_def);
            }
            Definition::Impl(impl_def) => {
                Self::collect_impl_into(symbols, errors, impl_def);
            }
            Definition::Enum(enum_def) => {
                Self::collect_enum_into(symbols, errors, enum_def);
            }
            Definition::Module(module_def) => {
                Self::collect_module_into(symbols, errors, module_def);
            }
            Definition::Function(func_def) => {
                Self::collect_function_into(symbols, errors, func_def);
            }
        }
    }

    fn collect_trait_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        trait_def: &crate::ast::TraitDef,
    ) {
        if is_primitive_name(&trait_def.name.name) {
            errors.push(CompilerError::PrimitiveRedefinition {
                name: trait_def.name.name.clone(),
                span: trait_def.name.span,
            });
            return;
        }

        let fields: HashMap<String, Type> = trait_def
            .fields
            .iter()
            .map(|f| (f.name.name.clone(), f.ty.clone()))
            .collect();
        let composed_traits: Vec<String> =
            trait_def.traits.iter().map(|t| t.name.clone()).collect();

        if let Some((kind, _)) = symbols.define_trait(
            trait_def.name.name.clone(),
            trait_def.visibility,
            trait_def.span,
            trait_def.generics.clone(),
            fields,
            composed_traits,
            trait_def.methods.clone(),
        ) {
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

    fn collect_struct_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        struct_def: &crate::ast::StructDef,
    ) {
        use symbol_table::FieldInfo;

        if is_primitive_name(&struct_def.name.name) {
            errors.push(CompilerError::PrimitiveRedefinition {
                name: struct_def.name.name.clone(),
                span: struct_def.name.span,
            });
            return;
        }

        let fields: Vec<FieldInfo> = struct_def
            .fields
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
            fields,
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

    fn collect_impl_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        impl_def: &crate::ast::ImplDef,
    ) {
        use symbol_table::ImplInfo;
        if let Some(trait_ident) = &impl_def.trait_name {
            // Trait implementation: impl Trait for Struct
            // Existence validation is deferred to collect_definition.
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
            let info = ImplInfo {
                struct_name: impl_def.name.name.clone(),
                generics: impl_def.generics.clone(),
                span: impl_def.span,
            };
            if let Some((kind, _)) =
                symbols.define_impl(&impl_def.name.name, info, impl_def.is_extern)
            {
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

    fn collect_enum_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        enum_def: &crate::ast::EnumDef,
    ) {
        use symbol_table::FieldInfo;
        if is_primitive_name(&enum_def.name.name) {
            errors.push(CompilerError::PrimitiveRedefinition {
                name: enum_def.name.name.clone(),
                span: enum_def.name.span,
            });
            return;
        }

        let variants = enum_def
            .variants
            .iter()
            .map(|v| (v.name.name.clone(), (v.fields.len(), v.span)))
            .collect();
        let variant_fields: HashMap<String, Vec<FieldInfo>> = enum_def
            .variants
            .iter()
            .map(|v| {
                (
                    v.name.name.clone(),
                    v.fields
                        .iter()
                        .map(|f| FieldInfo {
                            name: f.name.name.clone(),
                            ty: f.ty.clone(),
                        })
                        .collect(),
                )
            })
            .collect();

        if let Some((kind, _)) = symbols.define_enum(
            enum_def.name.name.clone(),
            enum_def.visibility,
            enum_def.span,
            enum_def.generics.clone(),
            variants,
            variant_fields,
            Vec::new(),
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

    fn collect_module_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        module_def: &crate::ast::ModuleDef,
    ) {
        let mut module_symbols = SymbolTable::new();
        for nested_def in &module_def.definitions {
            Self::collect_definition_into(&mut module_symbols, errors, nested_def);
        }
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

    fn collect_function_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        func_def: &crate::ast::FunctionDef,
    ) {
        use symbol_table::ParamInfo;

        if is_primitive_name(&func_def.name.name) {
            errors.push(CompilerError::PrimitiveRedefinition {
                name: func_def.name.name.clone(),
                span: func_def.name.span,
            });
            return;
        }

        let params: Vec<ParamInfo> = func_def
            .params
            .iter()
            .map(|p| ParamInfo {
                convention: p.convention,
                external_label: p.external_label.clone(),
                name: p.name.clone(),
                ty: p.ty.clone(),
            })
            .collect();

        if let Some((kind, _)) = symbols.define_function(
            func_def.name.name.clone(),
            func_def.visibility,
            func_def.span,
            params,
            func_def.return_type.clone(),
            vec![],
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

    /// Import a single symbol from a module
    fn import_symbol(
        &mut self,
        name: &str,
        module_symbols: &SymbolTable,
        module_path: &std::path::Path,
        logical_path: Vec<String>,
        span: Span,
    ) {
        use symbol_table::ImportError;

        match self.symbols.import_symbol(
            name,
            module_symbols,
            module_path.to_path_buf(),
            logical_path,
        ) {
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
    ///
    /// # Errors
    ///
    /// Returns `Err(Vec<CompilerError>)` if any semantic errors are found during analysis.
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
    ///
    /// # Errors
    ///
    /// Returns `Err(Vec<CompilerError>)` if any semantic errors are found during analysis.
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
                    Definition::Function(func_def) => &func_def.generics,
                    // Module definitions don't carry generics themselves;
                    // nested definitions are validated via their own arms.
                    Definition::Module(_) => continue,
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
    /// Infer the type of each let binding from its value expression, preferring
    /// the explicit type annotation when one is present.
    fn infer_let_types(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                let source_type = let_binding.type_annotation.as_ref().map_or_else(
                    || self.infer_type(&let_binding.value, file),
                    Self::type_to_string,
                );
                // Each binding in a destructuring pattern gets the type of the
                // position it extracts (array element, tuple field, struct field).
                // Simple patterns get the full source type.
                let resolved = self.resolve_pattern_types(&let_binding.pattern, &source_type);
                for (name, ty) in resolved {
                    self.symbols.set_let_type(&name, ty);
                }
            }
        }
    }

    /// Resolve per-binding types for a destructuring pattern given the
    /// source type string. Falls back to the source type for bindings whose
    /// position cannot be resolved (e.g., unknown struct field).
    fn resolve_pattern_types(
        &self,
        pattern: &BindingPattern,
        source_ty: &str,
    ) -> Vec<(String, String)> {
        let mut out = Vec::new();
        self.collect_pattern_types_inner(pattern, source_ty, &mut out);
        out
    }

    fn collect_pattern_types_inner(
        &self,
        pattern: &BindingPattern,
        source_ty: &str,
        out: &mut Vec<(String, String)>,
    ) {
        match pattern {
            BindingPattern::Simple(ident) => {
                out.push((ident.name.clone(), source_ty.to_string()));
            }
            BindingPattern::Array { elements, .. } => {
                let element_ty = strip_array_type(source_ty).unwrap_or(source_ty);
                for element in elements {
                    match element {
                        ArrayPatternElement::Binding(inner) => {
                            self.collect_pattern_types_inner(inner, element_ty, out);
                        }
                        ArrayPatternElement::Rest(Some(ident)) => {
                            out.push((ident.name.clone(), source_ty.to_string()));
                        }
                        ArrayPatternElement::Rest(None) | ArrayPatternElement::Wildcard => {}
                    }
                }
            }
            BindingPattern::Tuple { elements, .. } => {
                let field_types = parse_tuple_field_types(source_ty);
                for (idx, element) in elements.iter().enumerate() {
                    let inner_ty = field_types
                        .get(idx)
                        .map_or(source_ty, std::string::String::as_str);
                    self.collect_pattern_types_inner(element, inner_ty, out);
                }
            }
            BindingPattern::Struct { fields, .. } => {
                for field in fields {
                    let binding_ident = field.alias.as_ref().unwrap_or(&field.name);
                    let field_ty = self
                        .symbols
                        .structs
                        .get(source_ty)
                        .and_then(|s| s.fields.iter().find(|f| f.name == field.name.name))
                        .map_or_else(|| source_ty.to_string(), |f| Self::type_to_string(&f.ty));
                    out.push((binding_ident.name.clone(), field_ty));
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
                        if is_primitive_name(&binding.name) {
                            self.errors.push(CompilerError::PrimitiveRedefinition {
                                name: binding.name.clone(),
                                span: binding.span,
                            });
                            continue;
                        }
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
            Definition::Trait(trait_def) => self.collect_definition_trait(trait_def),
            Definition::Struct(struct_def) => self.collect_definition_struct(struct_def),
            Definition::Impl(impl_def) => self.collect_definition_impl(impl_def),
            Definition::Enum(enum_def) => self.collect_definition_enum(enum_def),
            Definition::Module(module_def) => self.collect_definition_module(module_def),
            Definition::Function(func_def) => self.collect_definition_function(func_def),
        }
    }

    fn collect_definition_trait(&mut self, trait_def: &crate::ast::TraitDef) {
        if is_primitive_name(&trait_def.name.name) {
            self.errors.push(CompilerError::PrimitiveRedefinition {
                name: trait_def.name.name.clone(),
                span: trait_def.name.span,
            });
            return;
        }

        let fields: HashMap<String, Type> = trait_def
            .fields
            .iter()
            .map(|f| (f.name.name.clone(), f.ty.clone()))
            .collect();
        let composed_traits: Vec<String> =
            trait_def.traits.iter().map(|t| t.name.clone()).collect();

        if let Some((kind, _)) = self.symbols.define_trait(
            trait_def.name.name.clone(),
            trait_def.visibility,
            trait_def.span,
            trait_def.generics.clone(),
            fields,
            composed_traits,
            trait_def.methods.clone(),
        ) {
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

    fn collect_definition_struct(&mut self, struct_def: &crate::ast::StructDef) {
        use symbol_table::FieldInfo;

        if is_primitive_name(&struct_def.name.name) {
            self.errors.push(CompilerError::PrimitiveRedefinition {
                name: struct_def.name.name.clone(),
                span: struct_def.name.span,
            });
            return;
        }

        let fields: Vec<FieldInfo> = struct_def
            .fields
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
            fields,
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

    fn collect_definition_impl(&mut self, impl_def: &crate::ast::ImplDef) {
        use symbol_table::ImplInfo;

        // Validate function bodies vs extern status
        for func in &impl_def.functions {
            if impl_def.is_extern && func.body.is_some() {
                self.errors.push(CompilerError::ExternImplWithBody {
                    name: impl_def.name.name.clone(),
                    span: func.name.span,
                });
            } else if !impl_def.is_extern && func.body.is_none() {
                self.errors.push(CompilerError::RegularFnWithoutBody {
                    function: func.name.name.clone(),
                    span: func.name.span,
                });
            }
        }

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
            if !self.symbols.traits.contains_key(&trait_ident.name) {
                self.errors.push(CompilerError::UndefinedType {
                    name: trait_ident.name.clone(),
                    span: trait_ident.span,
                });
                return;
            }

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
            let info = ImplInfo {
                struct_name: impl_def.name.name.clone(),
                generics: impl_def.generics.clone(),
                span: impl_def.span,
            };
            if let Some((kind, _)) =
                self.symbols
                    .define_impl(&impl_def.name.name, info, impl_def.is_extern)
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

    fn collect_definition_enum(&mut self, enum_def: &crate::ast::EnumDef) {
        use symbol_table::FieldInfo;
        if is_primitive_name(&enum_def.name.name) {
            self.errors.push(CompilerError::PrimitiveRedefinition {
                name: enum_def.name.name.clone(),
                span: enum_def.name.span,
            });
            return;
        }

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

        let variants = enum_def
            .variants
            .iter()
            .map(|v| (v.name.name.clone(), (v.fields.len(), v.span)))
            .collect();
        let variant_fields: HashMap<String, Vec<FieldInfo>> = enum_def
            .variants
            .iter()
            .map(|v| {
                (
                    v.name.name.clone(),
                    v.fields
                        .iter()
                        .map(|f| FieldInfo {
                            name: f.name.name.clone(),
                            ty: f.ty.clone(),
                        })
                        .collect(),
                )
            })
            .collect();

        if let Some((kind, _)) = self.symbols.define_enum(
            enum_def.name.name.clone(),
            enum_def.visibility,
            enum_def.span,
            enum_def.generics.clone(),
            variants,
            variant_fields,
            Vec::new(),
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

    fn collect_definition_module(&mut self, module_def: &crate::ast::ModuleDef) {
        let mut module_symbols = SymbolTable::new();
        for nested_def in &module_def.definitions {
            Self::collect_definition_into(&mut module_symbols, &mut self.errors, nested_def);
        }
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

    fn collect_definition_function(&mut self, func_def: &crate::ast::FunctionDef) {
        use symbol_table::ParamInfo;

        if is_primitive_name(&func_def.name.name) {
            self.errors.push(CompilerError::PrimitiveRedefinition {
                name: func_def.name.name.clone(),
                span: func_def.name.span,
            });
            return;
        }

        // Audit finding #28: the AST has no separate `is_extern` flag on
        // `FunctionDef` — `extern_fn_parser` produces `body: None` and
        // `function_def_parser` always produces `body: Some(_)`. The
        // parser invariant therefore enforces extern/body consistency at
        // construction time; no additional semantic check is possible.
        // `ExternFnWithBody` / `RegularFnWithoutBody` remain reachable
        // through the impl-block path (`collect_definition_impl`) where
        // `ImplDef` does carry an explicit `is_extern`.

        let params: Vec<ParamInfo> = func_def
            .params
            .iter()
            .map(|p| ParamInfo {
                convention: p.convention,
                external_label: p.external_label.clone(),
                name: p.name.clone(),
                ty: p.ty.clone(),
            })
            .collect();

        if let Some((kind, _)) = self.symbols.define_function(
            func_def.name.name.clone(),
            func_def.visibility,
            func_def.span,
            params,
            func_def.return_type.clone(),
            vec![],
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

    /// Push a generic scope for an impl block that combines the impl's own
    /// `<T>` parameters with the constraints declared on the target
    /// struct/enum. `impl Sum<T>` carries the param name without
    /// constraints; the constraints (`T: Foo`) live on `struct Sum<T: Foo>`.
    /// Without merging, methods inside the impl can't see the trait
    /// bounds on T. Audit findings #12 and #4/#27.
    pub(super) fn push_impl_generic_scope(
        &mut self,
        impl_generics: &[crate::ast::GenericParam],
        target_name: &str,
    ) {
        let mut scope = GenericScope {
            params: HashMap::new(),
        };
        // Start with the impl's own generic param names (often constraint-less).
        for param in impl_generics {
            let constraints: Vec<String> = param
                .constraints
                .iter()
                .map(|c| match c {
                    crate::ast::GenericConstraint::Trait(ident) => ident.name.clone(),
                })
                .collect();
            scope.params.insert(param.name.name.clone(), constraints);
        }
        // Merge constraints from the target struct/enum's own generics.
        let target_generics = if let Some(s) = self.symbols.structs.get(target_name) {
            s.generics.clone()
        } else if let Some(e) = self.symbols.enums.get(target_name) {
            e.generics.clone()
        } else {
            Vec::new()
        };
        for param in &target_generics {
            let constraints: Vec<String> = param
                .constraints
                .iter()
                .map(|c| match c {
                    crate::ast::GenericConstraint::Trait(ident) => ident.name.clone(),
                })
                .collect();
            let entry = scope.params.entry(param.name.name.clone()).or_default();
            for c in constraints {
                if !entry.contains(&c) {
                    entry.push(c);
                }
            }
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

    /// Get a reference to the symbol table for querying
    /// This is primarily used by LSP features for completion, hover, etc.
    pub const fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    /// Return the cached AST+symbol-table pairs for every imported module.
    /// LSP features use this to answer queries about symbols that live
    /// outside the current file.
    pub const fn module_cache(&self) -> &HashMap<PathBuf, (File, SymbolTable)> {
        &self.module_cache
    }

    /// Get all cached IR modules from imports.
    ///
    /// Returns a map from file path to `IrModule` for all modules that were
    /// analyzed during import resolution. Used by codegen backends to generate
    /// impl blocks from imported types.
    ///
    /// The cache is populated as a side effect of `parse_and_analyze_module`:
    /// after each imported module is semantically analyzed, its AST is also
    /// lowered to IR and stored here keyed by its filesystem path.
    ///
    /// # Returns
    ///
    /// Reference to the cached IR modules. Empty if no imports were processed
    /// or if every imported module failed IR lowering.
    pub const fn imported_ir_modules(&self) -> &HashMap<PathBuf, crate::ir::IrModule> {
        &self.module_ir_cache
    }
}

// Note: No Default implementation since a ModuleResolver is required
