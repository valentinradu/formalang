//! Semantic analysis (validation only — no evaluation or expansion).
//!
//! The semantic layer runs six passes against a parsed `File`:
//!
//! - **Pass 0** — resolve modules and imports.
//! - **Pass 1** — build the symbol table (structs, traits, enums,
//!   impls, lets, functions, modules). Includes a sub-pass (`Pass 1.5`)
//!   that validates duplicate generic parameters before later passes
//!   consume them.
//! - **Pass 2** — resolve type references; map AST `Type::Ident` /
//!   `Type::Generic` to entries in the symbol table.
//! - **Pass 3** — validate expressions: operator typing, `for`/`if`/
//!   `match` shape, mutability/sink rules.
//! - **Pass 4** — validate trait composition (model traits' required
//!   field requirements; view traits are validated separately).
//! - **Pass 5** — detect circular dependencies in let-bindings and
//!   in struct/trait/enum field types.
//!
//! this file's overview comment used `//` (a regular line
//! comment) and didn't reach `cargo doc`. Promoted to `//!` so the
//! pass list shows up in the rendered API docs alongside the
//! `SemanticAnalyzer` type.

pub(crate) mod import_graph;
pub mod module_resolver;
pub mod node_finder;
pub mod position;
pub mod queries;
pub mod symbol_table;
pub(crate) mod type_graph;

mod circular;
mod helpers;
mod imports;
mod inference;
mod module_collect;
mod pass1_symbols;
mod pattern_types;
mod sem_type;
mod trait_check;
mod type_resolution;
mod validation;

#[cfg(test)]
mod tests;

/// Re-exports of the symbol-table shapes that IR lowering and downstream
/// tooling consume. These are the minimal public contract between the
/// semantic and IR layers.
pub use symbol_table::{
    EnumInfo, LetInfo, ModuleInfo, StructInfo, SymbolKind, SymbolTable, TraitInfo,
};

// Free helpers shared between sibling submodules. Kept at `super`
// visibility so files like `circular.rs`, `inference/mod.rs`, and
// `trait_check.rs` can continue to refer to them through `super::*`.
pub(crate) use helpers::is_primitive_name;
pub(super) use helpers::{collect_bindings_from_pattern, strip_array_type};

use module_resolver::ModuleResolver;
use pattern_types::GenericScope;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::ast::{File, ParamConvention};
use crate::error::CompilerError;
use import_graph::ImportGraph;

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
    ///
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

    /// Analyze a file and validate all semantic rules
    ///
    /// # Errors
    ///
    /// Returns `Err(Vec<CompilerError>)` if any semantic errors are found during analysis.
    pub fn analyze(&mut self, file: &File) -> Result<(), Vec<CompilerError>> {
        self.run_passes(file);

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
        self.run_passes(file);

        // Return errors if any
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    /// Drive all six semantic passes in order. Shared by [`Self::analyze`]
    /// and [`Self::analyze_and_classify`] so the two entry points stay in
    /// lockstep.
    fn run_passes(&mut self, file: &File) {
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
