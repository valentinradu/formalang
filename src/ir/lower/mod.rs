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

use crate::ast::{Definition, File, ParamConvention, Statement};
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
/// assert_eq!(ir.user_structs().count(), 1);
/// ```
pub fn lower_to_ir(ast: &File, symbols: &SymbolTable) -> Result<IrModule, Vec<CompilerError>> {
    let mut lowerer = IrLowerer::new(symbols);
    lowerer.lower_file(ast)?;
    Ok(lowerer.module)
}

/// Lower an AST to IR with a known source file path.
///
/// The path is registered in `IrModule.file_table` as the entry-point
/// file (id 1, since id 0 is reserved for synthetic nodes), and the
/// lowerer seeds `current_file` with that id so every lowered IR node
/// carries a non-synthetic `IrSpan.file`.
///
/// Use this entry point when emitting DWARF / source maps / line tables —
/// the resulting `IrModule` lets backends resolve every node's span
/// to a real file path via `IrModule.file_path(span.file)`.
///
/// # Errors
///
/// Same as [`lower_to_ir`]: returns the IR-lowering error list on
/// failure.
pub fn lower_to_ir_with_path(
    ast: &File,
    symbols: &SymbolTable,
    path: std::path::PathBuf,
) -> Result<IrModule, Vec<CompilerError>> {
    let mut lowerer = IrLowerer::new(symbols);
    lowerer.current_file = lowerer.module.register_file(path);
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
    /// Return type of the function, method, or closure whose body is
    /// being lowered. Seeds [`Self::expected_value_type`] for the body
    /// expression, so a tail-position `.variant` literal resolves
    /// against the declared return type.
    pub(super) current_function_return_type: Option<ResolvedType>,
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
    /// File id for the source being lowered. The lowerer registers the
    /// entry-point file as `FileId(1)` on construction; cross-module
    /// inlining (in `MonomorphisePass`) updates this for cloned items.
    pub(super) current_file: crate::ir::FileId,
    /// when a closure literal is being lowered as the
    /// argument to a function call (or assigned to a closure-typed
    /// struct field, or passed as a method argument), this carries the
    /// expected closure type from the call/assignment context. The
    /// closure lowerer reads it to fill in any param/return types that
    /// the AST didn't annotate, so `array.map(x -> x + 1)` lowers with
    /// `x: I32` instead of `x: ResolvedType::Error`.
    pub(super) expected_closure_type: Option<ResolvedType>,
    /// The type the next expression is expected to produce, when the
    /// surrounding context knows it: a `let` annotation, an array
    /// element slot, a struct or enum field, or a call argument.
    ///
    /// Two lowerings read it. Container literals peel one layer and
    /// pass the inner type down, so `let [f]: [I32 -> I32] = [|x| x]`
    /// produces `x: I32` instead of `x: Error`. Inferred-enum literals
    /// resolve `.variant` against it, so `let s: Status = .pending`
    /// and `f(kind: .pending)` both find `Status`.
    ///
    /// Consumed once: a lowering that reads it takes it, and the site
    /// that set it restores the previous value afterwards.
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
    /// The type of each module-level `let`, by name. The lowering
    /// records it before it lowers the definitions, because a function
    /// body can refer to a `let` that the lowering has not lowered yet.
    /// A name is absent when semantic inference did not settle its
    /// type; see [`crate::semantic::sem_type::SemType::to_ast`].
    pub(super) module_let_types: HashMap<String, ResolvedType>,
    /// The module-level `let` of each name whose type the pre-pass
    /// could not record: `let empty = []`, `let [first] = [[]]`. A
    /// reference before the `let` lowers computes the type from the
    /// value; see `module_let_type`.
    pub(super) deferred_module_lets: HashMap<String, crate::ast::LetBinding>,
    /// True while the declare pass runs: the function and impl
    /// lowerings then lower each signature with no body, into
    /// `declared_functions` and `declared_impls`.
    pub(super) signatures_only: bool,
    /// The signature of each free function, with no body, lowered
    /// before any body. A call that comes before its callee in the file
    /// reads the callee's parameters and return type here.
    pub(super) declared_functions: Vec<crate::ir::IrFunction>,
    /// The impl blocks with the signatures of their methods, with no
    /// bodies. A method call before the impl of its type reads them.
    pub(super) declared_impls: Vec<crate::ir::IrImpl>,
    /// While `register_imported_types` is lowering an imported struct's
    /// or enum's field types, this holds the source module's logical
    /// path. The `lower_type` fallback uses it to default unresolved
    /// type identifiers to `External(<source>, name)` instead of
    /// `UndefinedType` — sibling types in the same source module are
    /// reachable to the imported type even when the entry didn't
    /// import them, and the `MonomorphisePass` will pull them in via
    /// Phase 1a.
    pub(super) imported_source_context: Option<Vec<String>>,
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
            current_file: crate::ir::FileId::SYNTHETIC,
            expected_closure_type: None,
            expected_value_type: None,
            module_node_stack: Vec::new(),
            module_let_types: HashMap::new(),
            deferred_module_lets: HashMap::new(),
            signatures_only: false,
            declared_functions: Vec::new(),
            declared_impls: Vec::new(),
            imported_source_context: None,
        }
    }

    /// Build an `IrSpan` for the currently-lowered AST node from the
    /// stored `current_span` + `current_file`.
    pub(super) const fn current_ir_span(&self) -> crate::ir::IrSpan {
        crate::ir::IrSpan::new(self.current_span, self.current_file)
    }

    /// Look up a function by its source-level (single-segment) name
    /// using module-aware resolution: when called from inside
    /// `mod foo { … }`, prefer `"foo::name"` so intra-module calls
    /// resolve to the local definition; fall back to the bare name
    /// for top-level functions.
    pub(super) fn find_function_in_scope(&self, name: &str) -> Option<crate::ir::FunctionId> {
        if !self.current_module_prefix.is_empty() {
            let qualified = format!("{}::{}", self.current_module_prefix, name);
            if let Some(id) = self.module.function_id(&qualified) {
                return Some(id);
            }
        }
        self.module.function_id(name)
    }

    /// The id of the overload of `name` that the call's argument
    /// labels select.
    ///
    /// `IrModule.function_names` maps a name to one id, so a later
    /// overload overwrites an earlier one and every call to an
    /// overloaded name lowered to whichever was registered last. The
    /// semantic analyser resolved the overloads correctly, so the
    /// program compiled — and then a backend emitted a call to the
    /// wrong function. `format(value: x)` and
    /// `format(value: x, precision: p)` both became a call to the
    /// one-argument `format`.
    ///
    /// Selection mirrors the analyser: an overload is a candidate when
    /// it can take every label the call supplies and the call supplies
    /// every parameter it has no default for. Among candidates the one
    /// firing the fewest defaults wins; a tie keeps the first, which is
    /// the ambiguous case the analyser has already reported.
    pub(super) fn find_overload_in_scope(
        &self,
        name: &str,
        arg_labels: &[Option<String>],
        arg_count: usize,
    ) -> Option<crate::ir::FunctionId> {
        let qualified = if self.current_module_prefix.is_empty() {
            None
        } else {
            Some(format!("{}::{}", self.current_module_prefix, name))
        };

        let mut candidates: Vec<(crate::ir::FunctionId, &crate::ir::IrFunction)> = self
            .module
            .functions
            .iter()
            .enumerate()
            .filter(|(_, f)| qualified.as_deref() == Some(f.name.as_str()) || f.name == name)
            .filter_map(|(i, f)| u32::try_from(i).ok().map(|i| (crate::ir::FunctionId(i), f)))
            .collect();

        // A call inside `mod math` means `math::add` when that exists,
        // whatever a top-level `add` says. Narrowing here keeps that
        // lexical rule ahead of the label matching below.
        if let Some(prefixed) = qualified.as_deref() {
            if candidates.iter().any(|(_, f)| f.name == prefixed) {
                candidates.retain(|(_, f)| f.name == prefixed);
            }
        }

        if candidates.len() <= 1 {
            return candidates
                .first()
                .map(|(id, _)| *id)
                .or_else(|| self.find_function_in_scope(name));
        }

        // The same rule a method call uses — labels fit, count between
        // required and declared, fewest defaults fired. One copy, so
        // the two cannot drift and so one test covers both.
        let ordered = candidates;
        crate::ir::overload::choose(
            ordered.iter().map(|(_, f)| *f).enumerate(),
            |f| f.params.as_slice(),
            arg_labels,
            arg_count,
        )
        .and_then(|index| ordered.get(index).map(|(id, _)| *id))
        .or_else(|| self.find_function_in_scope(name))
    }

    /// The name under which the type `name`, written in the current
    /// module, is registered.
    ///
    /// A type in an inline `mod` is registered by its qualified name,
    /// `m::P`, and the code in `m` names it `P`. The current module
    /// comes first, then each enclosing module, then the top level.
    pub(super) fn scoped_type_name(&self, name: &str) -> String {
        let mut prefix = self.current_module_prefix.as_str();
        while !prefix.is_empty() {
            let qualified = format!("{prefix}::{name}");
            if self.module.struct_id(&qualified).is_some()
                || self.module.enum_id(&qualified).is_some()
                || self.module.trait_id(&qualified).is_some()
            {
                return qualified;
            }
            prefix = prefix.rsplit_once("::").map_or("", |(outer, _)| outer);
        }
        name.to_string()
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
    /// Used by `lower_type` to tell
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

        // Record the types of the module-level lets. The definitions
        // refer to them, but the lets are lowered last: their values can
        // use the structs and impls of the definitions.
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                self.record_module_let_types(let_binding);
            }
        }

        self.declare_signatures(file);

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

        // Finalize imports: convert the map to a vec of IrImport.
        //
        // Sorted by module path. Draining the `HashMap` directly put
        // the imports — and, through them, the `file_table` and the
        // order the monomorphiser inlines in — in hash order, so two
        // builds of one program produced different IR.
        let mut imports: Vec<IrImport> = self
            .imports_by_module
            .drain()
            .map(|(module_path, (items, source_file))| IrImport {
                module_path,
                items,
                source_file,
            })
            .collect();
        imports.sort_by(|a, b| a.module_path.cmp(&b.module_path));
        self.module.imports = imports;

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Lower the signature of each function and each impl method, with
    /// no body. The bodies lower in source order after this, so a call
    /// that comes before its callee reads the callee's signature from
    /// `declared_functions` or `declared_impls`.
    ///
    /// The errors and the imports that this pass records are dropped:
    /// the full lowering records them again.
    fn declare_signatures(&mut self, file: &File) {
        let errors_before = self.errors.len();
        let saved_imports = self.imports_by_module.clone();
        self.signatures_only = true;
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if matches!(
                    &**def,
                    Definition::Function(_) | Definition::Impl(_) | Definition::Module(_)
                ) {
                    self.lower_definition(def.as_ref());
                }
            }
        }
        self.signatures_only = false;
        self.errors.truncate(errors_before);
        self.imports_by_module = saved_imports;
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
