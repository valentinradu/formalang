//! IR lowering pass: AST + `SymbolTable` → `IrModule`
//!
//! The IR layer intentionally consumes the semantic analyzer's
//! [`SymbolTable`](crate::semantic::SymbolTable) along with its public
//! shape types ([`StructInfo`](crate::semantic::StructInfo),
//! [`EnumInfo`](crate::semantic::EnumInfo), etc.). Those types are the
//! narrow contract between the two phases and are re-exported from
//! [`crate::semantic`] for that purpose; IR lowering should access them
//! through the re-exports rather than reaching into
//! `crate::semantic::symbol_table` directly.

mod definitions;
mod destructuring;
mod expr;
mod let_and_module;
mod register;
#[cfg(test)]
mod tests;
mod types;

use crate::ast::{File, ParamConvention, Statement};
use crate::error::CompilerError;
use crate::semantic::{SymbolKind, SymbolTable};

use super::{ImportedKind, IrGenericParam, IrImport, IrImportItem, IrModule, ResolvedType};
use std::collections::HashMap;

/// Lower an AST and symbol table into an IR module.
///
/// This is the main entry point for the lowering pass. It takes a validated AST
/// and its corresponding symbol table and produces an IR module with resolved types.
///
/// # Arguments
///
/// * `ast` - The validated AST from the semantic analyzer
/// * `symbols` - The symbol table built during semantic analysis
///
/// # Returns
///
/// * `Ok(IrModule)` - The lowered IR module
/// * `Err(Vec<CompilerError>)` - Errors encountered during lowering
///
/// # Errors
///
/// Returns a list of [`CompilerError`] if type resolution or lowering fails for
/// any definition or expression in the file.
///
/// # Example
///
/// ```
/// use formalang::{compile_with_analyzer, ir::lower_to_ir};
///
/// let source = "pub struct User { name: String }";
/// let (ast, analyzer) = compile_with_analyzer(source).unwrap();
/// let ir = lower_to_ir(&ast, analyzer.symbols()).unwrap();
/// assert_eq!(ir.structs.len(), 1);
/// ```
pub fn lower_to_ir(ast: &File, symbols: &SymbolTable) -> Result<IrModule, Vec<CompilerError>> {
    let mut lowerer = IrLowerer::new(symbols);
    lowerer.lower_file(ast)?;
    Ok(lowerer.module)
}

/// Internal state for the lowering pass.
struct IrLowerer<'a> {
    pub(super) module: IrModule,
    pub(super) symbols: &'a SymbolTable,
    pub(super) errors: Vec<CompilerError>,
    /// Track imports by module path for aggregation: (`module_path`, `source_file`) -> items
    pub(super) imports_by_module: HashMap<Vec<String>, (Vec<IrImportItem>, std::path::PathBuf)>,
    /// Current struct being processed in an impl block (for self references)
    pub(super) current_impl_struct: Option<String>,
    /// Current module prefix for nested definitions (e.g., "`outer::inner`")
    pub(super) current_module_prefix: String,
    /// Current function's return type for inferring enum types
    pub(super) current_function_return_type: Option<String>,
    /// Stack of local bindings in scope during lowering: each entry is a
    /// frame pushed when entering a function/closure/block body, mapping the
    /// binding name to its declared parameter convention and resolved type.
    /// Used so that a `Reference` to a parameter resolves to the concrete
    /// type instead of a `TypeParam(name)` placeholder, and so that closure
    /// captures inherit the outer binding's convention.
    pub(super) local_binding_scopes: Vec<HashMap<String, (ParamConvention, ResolvedType)>>,
    /// When lowering the body of an impl method, maps the current impl's
    /// methods to their declared return types so that forward references
    /// within the same impl block (`self.other_method()`) resolve without
    /// needing the impl to already be installed in `module.impls`.
    pub(super) current_impl_method_returns: Option<HashMap<String, Option<ResolvedType>>>,
    /// Stack of generic-parameter scopes active during lowering. Each frame
    /// records the param names in scope together with their trait
    /// constraints; used by `find_trait_for_method` to resolve which trait
    /// declares a method on a generic parameter (`T: Foo + Bar`).
    pub(super) generic_scopes: Vec<Vec<IrGenericParam>>,
    /// Span of the AST node currently being lowered. Updated at the top of
    /// `lower_expr` and a few other lowering entry points so that
    /// `InternalError` diagnostics can cite a meaningful source location
    /// instead of `Span::default()`.
    pub(super) current_span: crate::location::Span,
    /// when a closure literal is being lowered as the
    /// argument to a function call (or assigned to a closure-typed
    /// struct field, or passed as a method argument), this carries the
    /// expected closure type from the call/assignment context. The
    /// closure lowerer reads it to fill in any param/return types that
    /// the AST didn't annotate, so `array.map(x -> x + 1)` lowers with
    /// `x: I32` instead of `x: ResolvedType::Error`.
    pub(super) expected_closure_type: Option<ResolvedType>,
    /// When the surrounding context (e.g. a destructuring let with a
    /// type annotation) supplies the *aggregate* type that the next
    /// expression should produce, this carries it. Array- and
    /// tuple-literal lowering propagate it down to per-element
    /// closure-literal lowerings so
    /// `let [f]: [I32 -> I32] = [|x| x]` produces `x: I32` instead of
    /// `x: Error`. Consumed once.
    pub(super) expected_value_type: Option<ResolvedType>,
    /// Stack of currently-open module nodes during lowering. The
    /// outermost source module is at index 0; the deepest in-progress
    /// module is at the back. On entering `mod foo { ... }` we push a
    /// new [`crate::ir::IrModuleNode`]; on leaving, we pop it and
    /// attach it either to the parent node (if the stack is still non-
    /// empty) or to `module.modules`. Member IDs (struct/enum/trait/
    /// function) get appended to the topmost node as each definition
    /// is registered. Tier-1 item G.
    pub(super) module_node_stack: Vec<crate::ir::IrModuleNode>,
}

impl<'a> IrLowerer<'a> {
    /// Record an internal-compiler-error indicating that an ID produced earlier
    /// in the lowering pass no longer resolves to a definition. This only fires
    /// on invariant violations (e.g. a caller mutating an IR vector between
    /// registration and write-back); we surface it as a loud compilation
    /// failure rather than panicking.
    pub(super) fn record_missing_id(&mut self, kind: &'static str, id: u32) {
        self.errors.push(CompilerError::InternalError {
            detail: format!("{kind} id {id} produced by registration lookup is no longer valid"),
            span: crate::location::Span::default(),
        });
    }

    /// Record an `InternalError` at an IR-lowering site that should be
    /// unreachable under a passing semantic analysis, and return a
    /// placeholder `ResolvedType` so the surrounding lowering code can
    /// continue assembling the IR. The caller's error will surface via
    /// `self.errors` at the end of lowering; the returned placeholder only
    /// exists so we don't have to plumb `Result` through every lowering
    /// helper.
    pub(super) fn internal_error_type(&mut self, detail: String) -> ResolvedType {
        self.errors.push(CompilerError::InternalError {
            detail,
            span: self.current_span,
        });
        ResolvedType::Error
    }

    /// Like `internal_error_type`, but skips the error push when the
    /// offending type is already `ResolvedType::Error` — that indicates an
    /// upstream lowering step already pushed its own `CompilerError` and
    /// returned the placeholder. This avoids a cascade of secondary
    /// `InternalError` diagnostics for the same root cause.
    pub(super) fn internal_error_type_if_concrete(
        &mut self,
        bad_ty: &ResolvedType,
        detail: String,
    ) -> ResolvedType {
        if matches!(bad_ty, ResolvedType::Error) {
            ResolvedType::Error
        } else {
            self.internal_error_type(detail)
        }
    }

    fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            module: IrModule::new(),
            symbols,
            errors: Vec::new(),
            imports_by_module: HashMap::new(),
            current_impl_struct: None,
            current_module_prefix: String::new(),
            current_function_return_type: None,
            local_binding_scopes: Vec::new(),
            current_impl_method_returns: None,
            generic_scopes: Vec::new(),
            current_span: crate::location::Span::default(),
            expected_closure_type: None,
            expected_value_type: None,
            module_node_stack: Vec::new(),
        }
    }

    /// Look up a local binding's resolved type by name from the innermost
    /// scope outwards.
    pub(super) fn lookup_local_binding(&self, name: &str) -> Option<&ResolvedType> {
        self.lookup_local_binding_entry(name).map(|(_, ty)| ty)
    }

    /// Look up a local binding's full entry (convention + type) by name.
    pub(super) fn lookup_local_binding_entry(
        &self,
        name: &str,
    ) -> Option<&(ParamConvention, ResolvedType)> {
        for frame in self.local_binding_scopes.iter().rev() {
            if let Some(entry) = frame.get(name) {
                return Some(entry);
            }
        }
        None
    }

    /// Whether `name` matches a generic parameter declared in any
    /// currently-active generic scope (struct/enum/trait/impl/function).
    /// Used by `lower_type` and `string_to_resolved_type` to tell
    /// legitimate type-parameter references apart from references to
    /// names that fail to resolve to any known type.
    pub(super) fn is_generic_param_in_scope(&self, name: &str) -> bool {
        for frame in &self.generic_scopes {
            if frame.iter().any(|p| p.name == name) {
                return true;
            }
        }
        false
    }

    fn lower_file(&mut self, file: &File) -> Result<(), Vec<CompilerError>> {
        // Pre-pass: register imported structs and enums so they have IDs
        self.register_imported_types();

        // First pass: register all definitions to get IDs
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                self.register_definition(def.as_ref());
            }
        }

        // Second pass: lower all definitions with resolved types
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                self.lower_definition(def.as_ref());
            }
        }

        // Third pass: lower module-level let bindings
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                self.lower_let_binding(let_binding);
            }
        }

        // Finalize imports: convert the map to a vec of IrImport
        self.module.imports = self
            .imports_by_module
            .drain()
            .map(|(module_path, (items, source_file))| IrImport {
                module_path,
                items,
                source_file,
            })
            .collect();

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Track an external import if the given name is imported from another module.
    /// This is used for cases where we can't create a full External type (e.g., trait implementations).
    pub(super) fn try_track_imported_type(&mut self, name: &str, expected_kind: ImportedKind) {
        if let Some(module_path) = self.symbols.get_module_logical_path(name) {
            let import_item = IrImportItem {
                name: name.to_string(),
                kind: expected_kind,
            };

            // Get source file path for IR lookup during codegen
            let source_file = self
                .symbols
                .get_module_origin(name)
                .cloned()
                .unwrap_or_default();

            self.imports_by_module
                .entry(module_path.clone())
                .or_insert_with(|| (Vec::new(), source_file))
                .0
                .push(import_item);
        }
    }

    /// Try to create an external type reference.
    /// Returns Some(External) if the type is imported, None if it's local.
    pub(super) fn try_external_type(
        &mut self,
        name: &str,
        type_args: Vec<ResolvedType>,
    ) -> Option<ResolvedType> {
        // Check if this symbol was imported from another module
        let module_path = self.symbols.get_module_logical_path(name)?;
        let kind = self.symbols.get_symbol_kind(name)?;

        let external_kind = match kind {
            SymbolKind::Struct => ImportedKind::Struct,
            SymbolKind::Trait => ImportedKind::Trait,
            SymbolKind::Enum => ImportedKind::Enum,
            // Other kinds can't be used as types
            SymbolKind::Impl | SymbolKind::Let | SymbolKind::Module | SymbolKind::Function => {
                return None
            }
        };

        // Track this import
        let import_item = IrImportItem {
            name: name.to_string(),
            kind: external_kind.clone(),
        };

        // Get source file path for IR lookup during codegen
        let source_file = self
            .symbols
            .get_module_origin(name)
            .cloned()
            .unwrap_or_default();

        self.imports_by_module
            .entry(module_path.clone())
            .or_insert_with(|| (Vec::new(), source_file))
            .0
            .push(import_item);

        Some(ResolvedType::External {
            module_path: module_path.clone(),
            name: name.to_string(),
            kind: external_kind,
            type_args,
        })
    }
}
