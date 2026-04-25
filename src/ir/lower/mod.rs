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

mod expr;
mod types;

use crate::ast::{
    self, BindingPattern, Definition, EnumDef, File, FnDef, FunctionDef, GenericConstraint,
    ImplDef, LetBinding, Literal, ParamConvention, PrimitiveType, Statement, StructDef,
    StructField, TraitDef, Type,
};
use crate::error::CompilerError;
use crate::semantic::{EnumInfo, StructInfo, SymbolKind, SymbolTable};

use super::{
    simple_type_name, ImportedKind, IrEnum, IrEnumVariant, IrExpr, IrField, IrFunction,
    IrFunctionParam, IrFunctionSig, IrGenericParam, IrImpl, IrImport, IrImportItem, IrLet,
    IrModule, IrStruct, IrTrait, ResolvedType, TraitId,
};
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
    /// captures inherit the outer binding's convention (audit finding #32).
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
    /// instead of `Span::default()`. See audit finding #31.
    pub(super) current_span: crate::location::Span,
    /// Audit2 B19: when a closure literal is being lowered as the
    /// argument to a function call (or assigned to a closure-typed
    /// struct field, or passed as a method argument), this carries the
    /// expected closure type from the call/assignment context. The
    /// closure lowerer reads it to fill in any param/return types that
    /// the AST didn't annotate, so `array.map(x -> x + 1)` lowers with
    /// `x: Number` instead of `x: TypeParam("Unknown")`.
    pub(super) expected_closure_type: Option<ResolvedType>,
}

impl<'a> IrLowerer<'a> {
    /// Record an internal-compiler-error indicating that an ID produced earlier
    /// in the lowering pass no longer resolves to a definition. This only fires
    /// on invariant violations (e.g. a caller mutating an IR vector between
    /// registration and write-back); we surface it as a loud compilation
    /// failure rather than panicking.
    fn record_missing_id(&mut self, kind: &'static str, id: u32) {
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
    /// helper. See audit finding #8.
    pub(super) fn internal_error_type(&mut self, detail: String) -> ResolvedType {
        self.errors.push(CompilerError::InternalError {
            detail,
            span: self.current_span,
        });
        ResolvedType::TypeParam("Unknown".to_string())
    }

    /// Like `internal_error_type`, but skips the error push when the
    /// offending type is already a `TypeParam` — that indicates an upstream
    /// lowering step already produced a placeholder (unresolved path, tuple
    /// type rendered as a string, etc.) and will have errored on its own
    /// behalf. This avoids a cascade of secondary errors until the upstream
    /// `TypeParam` sources are removed (audit finding #8 follow-up).
    pub(super) fn internal_error_type_if_concrete(
        &mut self,
        bad_ty: &ResolvedType,
        detail: String,
    ) -> ResolvedType {
        if matches!(bad_ty, ResolvedType::TypeParam(_)) {
            ResolvedType::TypeParam("Unknown".to_string())
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

    /// Lower a module-level let binding
    fn lower_let_binding(&mut self, let_binding: &LetBinding) {
        match &let_binding.pattern {
            BindingPattern::Simple(ident) => self.lower_simple_let(let_binding, &ident.name),
            BindingPattern::Array { elements, .. } => {
                self.lower_array_destructuring_let(let_binding, elements);
            }
            BindingPattern::Struct { fields, .. } => {
                self.lower_struct_destructuring_let(let_binding, fields);
            }
            BindingPattern::Tuple { elements, .. } => {
                self.lower_tuple_destructuring_let(let_binding, elements);
            }
        }
    }

    /// Lower a simple `let name = value` binding.
    fn lower_simple_let(&mut self, let_binding: &LetBinding, ident_name: &str) {
        // Audit2 B18: thread the let's annotation as the inferred-enum
        // target so `.variant` literals in the value resolve to the
        // declared enum (e.g. `let s: Status = .pending`) instead of
        // lowering to `TypeParam("InferredEnum")`.
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type =
            let_binding.type_annotation.as_ref().map(Self::type_name);
        let mut value = self.lower_expr(&let_binding.value);
        self.current_function_return_type = saved_return_type;
        let ty = if let Some(type_ann) = &let_binding.type_annotation {
            self.lower_type(type_ann)
        } else {
            self.symbols
                .get_let_type(ident_name)
                .map(str::to_string)
                .and_then(|s| self.string_to_resolved_type(&s))
                .unwrap_or_else(|| value.ty().clone())
        };
        // Audit #41: an empty array literal lowers to `Array(Never)`
        // because it has no elements to seed the element type from.
        // When the binding is annotated `[T]`, retype the value's
        // `Array(Never)` to `Array(T)` so backends and downstream IR
        // passes see a concrete element type instead of Never.
        if let (IrExpr::Array { elements, ty: vty }, ResolvedType::Array(annotated_elem)) =
            (&mut value, &ty)
        {
            if elements.is_empty()
                && matches!(
                    vty,
                    ResolvedType::Array(boxed)
                        if matches!(**boxed, ResolvedType::Primitive(PrimitiveType::Never))
                )
            {
                *vty = ResolvedType::Array(annotated_elem.clone());
            }
        }
        self.module.add_let(IrLet {
            name: ident_name.to_string(),
            visibility: let_binding.visibility,
            mutable: let_binding.mutable,
            ty,
            value,
            doc: let_binding.doc.clone(),
        });
    }

    /// Lower an array destructuring let binding: `let [a, b, c] = value`.
    fn lower_array_destructuring_let(
        &mut self,
        let_binding: &LetBinding,
        elements: &[ast::ArrayPatternElement],
    ) {
        let value_expr = self.lower_expr(&let_binding.value);
        let bad_recv = value_expr.ty().clone();
        let elem_ty = if let ResolvedType::Array(inner) = &bad_recv {
            (**inner).clone()
        } else {
            self.internal_error_type_if_concrete(
                &bad_recv,
                format!("array-destructuring let receiver lowered to non-array type {bad_recv:?}"),
            )
        };
        for (i, element) in elements.iter().enumerate() {
            if let Some(name) = Self::extract_binding_name(element) {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "array destructuring indices are small source-code positions that fit exactly in f64 mantissa"
                )]
                let index_key = IrExpr::Literal {
                    value: Literal::Number(i as f64),
                    ty: ResolvedType::Primitive(PrimitiveType::Number),
                };
                // `arr[i]` — dictionary-access is the IR node for index access
                let access_expr = IrExpr::DictAccess {
                    dict: Box::new(value_expr.clone()),
                    key: Box::new(index_key),
                    ty: elem_ty.clone(),
                };
                self.module.add_let(IrLet {
                    name,
                    visibility: let_binding.visibility,
                    mutable: let_binding.mutable,
                    ty: elem_ty.clone(),
                    value: access_expr,
                    doc: let_binding.doc.clone(),
                });
            }
        }
    }

    /// Lower a struct destructuring let binding: `let { field, other: alias } = value`.
    fn lower_struct_destructuring_let(
        &mut self,
        let_binding: &LetBinding,
        fields: &[ast::StructPatternField],
    ) {
        let value_expr = self.lower_expr(&let_binding.value);
        for field in fields {
            let field_name = field.name.name.clone();
            let binding_name = field
                .alias
                .as_ref()
                .map_or_else(|| field_name.clone(), |a| a.name.clone());
            let field_ty = self.get_field_type_from_resolved(value_expr.ty(), &field_name);
            // `value.field_name`
            let access_expr = IrExpr::FieldAccess {
                object: Box::new(value_expr.clone()),
                field: field_name,
                ty: field_ty.clone(),
            };
            self.module.add_let(IrLet {
                name: binding_name,
                visibility: let_binding.visibility,
                mutable: let_binding.mutable,
                ty: field_ty,
                value: access_expr,
                doc: let_binding.doc.clone(),
            });
        }
    }

    /// Lower a tuple destructuring let binding: `let (a, b) = value`.
    fn lower_tuple_destructuring_let(
        &mut self,
        let_binding: &LetBinding,
        elements: &[BindingPattern],
    ) {
        let value_expr = self.lower_expr(&let_binding.value);
        let bad_recv = value_expr.ty().clone();
        let tuple_types = if let ResolvedType::Tuple(fields) = &bad_recv {
            fields.clone()
        } else {
            let _ = self.internal_error_type_if_concrete(
                &bad_recv,
                format!("tuple-destructuring let receiver lowered to non-tuple type {bad_recv:?}"),
            );
            Vec::new()
        };
        let overflow_ty = if elements.len() > tuple_types.len() && !tuple_types.is_empty() {
            self.internal_error_type(format!(
                "tuple-destructuring pattern binds {} names but the receiver has {} fields",
                elements.len(),
                tuple_types.len(),
            ))
        } else {
            ResolvedType::TypeParam("Unknown".to_string())
        };
        for (i, element) in elements.iter().enumerate() {
            if let Some(name) = Self::extract_simple_binding_name(element) {
                let (field_name, ty) = tuple_types.get(i).map_or_else(
                    || (i.to_string(), overflow_ty.clone()),
                    |(n, t)| (n.clone(), t.clone()),
                );
                // `value.field_name` — tuple fields are accessed by their declared name
                let access_expr = IrExpr::FieldAccess {
                    object: Box::new(value_expr.clone()),
                    field: field_name,
                    ty: ty.clone(),
                };
                self.module.add_let(IrLet {
                    name,
                    visibility: let_binding.visibility,
                    mutable: let_binding.mutable,
                    ty,
                    value: access_expr,
                    doc: let_binding.doc.clone(),
                });
            }
        }
    }

    /// Extract binding name from an array pattern element
    fn extract_binding_name(element: &ast::ArrayPatternElement) -> Option<String> {
        match element {
            ast::ArrayPatternElement::Binding(pattern) => {
                Self::extract_simple_binding_name(pattern)
            }
            ast::ArrayPatternElement::Rest(Some(ident)) => Some(ident.name.clone()),
            ast::ArrayPatternElement::Rest(None) | ast::ArrayPatternElement::Wildcard => None,
        }
    }

    /// Extract binding name from a simple binding pattern
    fn extract_simple_binding_name(pattern: &BindingPattern) -> Option<String> {
        match pattern {
            BindingPattern::Simple(ident) => Some(ident.name.clone()),
            BindingPattern::Array { .. }
            | BindingPattern::Struct { .. }
            | BindingPattern::Tuple { .. } => None,
        }
    }

    /// Resolve the type of a self.field reference using current impl context.
    /// Searches the top-level symbol table first, then any current module
    /// prefix (so impl blocks inside `pub mod foo { ... }` resolve correctly).
    pub(super) fn resolve_self_field_type(&mut self, field_name: &str) -> ResolvedType {
        if let Some(struct_name) = self.current_impl_struct.clone() {
            if let Some(ty) = self.find_struct_field_ty(&struct_name, field_name) {
                return ty;
            }
        }
        self.internal_error_type(format!(
            "`self.{field_name}` has no matching field on the current impl target; semantic should have caught this"
        ))
    }

    /// Look up a struct's field type by searching the top-level symbol
    /// table, then walking the current module prefix if any. Returns the
    /// lowered `ResolvedType` if found.
    fn find_struct_field_ty(
        &mut self,
        struct_name: &str,
        field_name: &str,
    ) -> Option<ResolvedType> {
        if let Some(struct_info) = self.symbols.structs.get(struct_name) {
            if let Some(field) = struct_info.fields.iter().find(|f| f.name == field_name) {
                let ty = field.ty.clone();
                return Some(self.lower_type(&ty));
            }
        }
        if !self.current_module_prefix.is_empty() {
            let parts: Vec<&str> = self.current_module_prefix.split("::").collect();
            let mut current = self.symbols;
            for part in &parts {
                match current.modules.get(*part) {
                    Some(info) => current = &info.symbols,
                    None => return None,
                }
            }
            if let Some(struct_info) = current.structs.get(struct_name) {
                if let Some(field) = struct_info.fields.iter().find(|f| f.name == field_name) {
                    let ty = field.ty.clone();
                    return Some(self.lower_type(&ty));
                }
            }
        }
        None
    }

    /// Resolve the type of `self` in an impl block context.
    /// Returns the `ResolvedType` for the struct or enum being implemented.
    pub(super) fn resolve_impl_self_type(&mut self, impl_name: &str) -> ResolvedType {
        // First try as a struct
        if let Some(id) = self.module.struct_id(impl_name) {
            return ResolvedType::Struct(id);
        }
        // Then try as an enum
        if let Some(id) = self.module.enum_id(impl_name) {
            return ResolvedType::Enum(id);
        }
        self.internal_error_type(format!(
            "impl-self type `{impl_name}` was not registered in the module before lowering referenced it",
        ))
    }

    /// Look up all traits for a struct that lives inside a module.
    ///
    /// `module_prefix` is a `"::"` separated path (e.g. `"shapes"` or `"a::b"`).
    /// Returns the trait names as stored in the nested symbol table, which are
    /// the *unqualified* trait names as written in source.
    fn get_traits_for_struct_in_module(
        &self,
        module_prefix: &str,
        struct_name: &str,
    ) -> Vec<String> {
        // Walk the module hierarchy following the prefix segments.
        let parts: Vec<&str> = module_prefix.split("::").collect();
        let mut current = self.symbols;
        for part in &parts {
            match current.modules.get(*part) {
                Some(info) => current = &info.symbols,
                None => return Vec::new(),
            }
        }
        current.get_all_traits_for_struct(struct_name)
    }

    /// Register imported structs and enums from the symbol table.
    /// This ensures that imported types have struct/enum IDs in the IR module,
    /// so when we instantiate them, `struct_id` is populated correctly.
    fn register_imported_types(&mut self) {
        // Register imported structs (top-level)
        for (name, struct_info) in &self.symbols.structs {
            // Check if this is an imported symbol
            if self.symbols.get_module_origin(name).is_some() {
                self.register_struct(name, struct_info);
                // Track this import for backend use (to find impl blocks)
                self.try_track_imported_type(name, ImportedKind::Struct);
            }
        }

        // Register imported enums (top-level)
        for (name, enum_info) in &self.symbols.enums {
            // Check if this is an imported symbol
            if self.symbols.get_module_origin(name).is_some() {
                self.register_enum(name, enum_info);
                // Track this import for backend use (to find impl blocks)
                self.try_track_imported_type(name, ImportedKind::Enum);
            }
        }

        // Register types from imported nested modules (e.g., fill::Solid)
        for (module_name, module_info) in &self.symbols.modules {
            self.register_module_types(module_name, &module_info.symbols);
        }
    }

    /// Register types from a nested module recursively
    fn register_module_types(&mut self, module_prefix: &str, module_symbols: &SymbolTable) {
        // Register traits from this module with their real shape. Composed
        // traits are filled in after all names exist, since composition can
        // forward-reference traits in the same module.
        let mut pending_trait_composition: Vec<(String, Vec<String>)> = Vec::new();
        for (name, trait_info) in &module_symbols.traits {
            let qualified_name = format!("{module_prefix}::{name}");
            let generic_params = self.lower_generic_params(&trait_info.generics);
            self.generic_scopes.push(generic_params.clone());
            let fields: Vec<IrField> = trait_info
                .fields
                .iter()
                .map(|f| IrField {
                    name: f.name.clone(),
                    ty: self.lower_type(&f.ty),
                    default: None,
                    optional: matches!(f.ty, ast::Type::Optional(_)),
                    mutable: false,
                    doc: f.doc.clone(),
                })
                .collect();
            let methods: Vec<IrFunctionSig> = trait_info
                .methods
                .iter()
                .map(|m| self.lower_fn_sig(m))
                .collect();
            self.generic_scopes.pop();
            if let Err(e) = self.module.add_trait(
                qualified_name.clone(),
                IrTrait {
                    name: qualified_name.clone(),
                    visibility: trait_info.visibility,
                    composed_traits: Vec::new(),
                    fields,
                    methods,
                    generic_params,
                    doc: None,
                },
            ) {
                self.errors.push(e);
            }
            if !trait_info.composed_traits.is_empty() {
                pending_trait_composition
                    .push((qualified_name, trait_info.composed_traits.clone()));
            }
        }

        // Resolve composed-trait references after all traits from this module
        // have been registered.
        for (qualified_name, composed_names) in pending_trait_composition {
            let composed: Vec<TraitId> = composed_names
                .iter()
                .filter_map(|c| {
                    // Prefer the module-qualified lookup, fall back to simple
                    // name for traits composed from the enclosing scope.
                    self.module
                        .trait_id(&format!("{module_prefix}::{c}"))
                        .or_else(|| self.module.trait_id(c))
                })
                .collect();
            if let Some(id) = self.module.trait_id(&qualified_name) {
                if let Some(trait_def) = self.module.trait_mut(id) {
                    trait_def.composed_traits = composed;
                }
            }
        }

        // Register structs from this module
        for (name, struct_info) in &module_symbols.structs {
            let qualified_name = format!("{module_prefix}::{name}");
            self.register_struct(&qualified_name, struct_info);
        }

        // Register enums from this module
        for (name, enum_info) in &module_symbols.enums {
            let qualified_name = format!("{module_prefix}::{name}");
            self.register_enum(&qualified_name, enum_info);
        }

        // Recursively register nested modules
        for (nested_name, nested_module_info) in &module_symbols.modules {
            let nested_prefix = format!("{module_prefix}::{nested_name}");
            self.register_module_types(&nested_prefix, &nested_module_info.symbols);
        }
    }

    /// Helper method to register an enum using `EnumInfo::variant_fields`
    /// so imported-module enums carry real variant shapes into the IR.
    fn register_enum(&mut self, name: &str, enum_info: &EnumInfo) {
        let generic_params = self.lower_generic_params(&enum_info.generics);
        self.generic_scopes.push(generic_params.clone());

        let variants: Vec<IrEnumVariant> = enum_info
            .variants
            .keys()
            .map(|variant_name| {
                let fields = enum_info
                    .variant_fields
                    .get(variant_name)
                    .map(|fs| {
                        fs.iter()
                            .map(|f| IrField {
                                name: f.name.clone(),
                                ty: self.lower_type(&f.ty),
                                default: None,
                                optional: matches!(f.ty, ast::Type::Optional(_)),
                                mutable: false,
                                doc: f.doc.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                IrEnumVariant {
                    name: variant_name.clone(),
                    fields,
                }
            })
            .collect();

        self.generic_scopes.pop();

        if let Err(e) = self.module.add_enum(
            name.to_string(),
            IrEnum {
                name: name.to_string(),
                visibility: enum_info.visibility,
                variants,
                generic_params,
                doc: None,
            },
        ) {
            self.errors.push(e);
        }
    }

    /// Helper method to register a struct with full field information
    fn register_struct(&mut self, name: &str, struct_info: &StructInfo) {
        // Convert generic params first so field types referencing `T`
        // resolve as in-scope params instead of triggering an
        // `UndefinedType` from the tightened `lower_type` fallback.
        let generic_params = self.lower_generic_params(&struct_info.generics);
        self.generic_scopes.push(generic_params.clone());

        let fields: Vec<IrField> = struct_info
            .fields
            .iter()
            .map(|f| {
                let optional = matches!(f.ty, ast::Type::Optional(_));
                IrField {
                    name: f.name.clone(),
                    ty: self.lower_type(&f.ty),
                    mutable: false,
                    optional,
                    default: None,
                    doc: f.doc.clone(),
                }
            })
            .collect();

        self.generic_scopes.pop();

        // Convert trait names to trait IDs
        // Use get_all_traits_for_struct to include both inline traits and impl blocks
        let all_trait_names = self.symbols.get_all_traits_for_struct(name);
        let traits: Vec<TraitId> = all_trait_names
            .iter()
            .filter_map(|trait_name| self.module.trait_id(trait_name))
            .collect();

        if let Err(e) = self.module.add_struct(
            name.to_string(),
            IrStruct {
                name: name.to_string(),
                visibility: struct_info.visibility,
                traits,
                fields,
                generic_params,
                doc: None,
            },
        ) {
            self.errors.push(e);
        }
    }

    /// First pass: register definitions to allocate IDs
    fn register_definition(&mut self, def: &Definition) {
        match def {
            Definition::Trait(t) => {
                let name = t.name.name.clone();
                // Create placeholder, will be filled in second pass
                if let Err(e) = self.module.add_trait(
                    name,
                    IrTrait {
                        name: t.name.name.clone(),
                        visibility: t.visibility,
                        composed_traits: Vec::new(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                        generic_params: Vec::new(),
                        doc: t.doc.clone(),
                    },
                ) {
                    self.errors.push(e);
                }
            }
            Definition::Struct(s) => {
                let name = s.name.name.clone();
                if let Err(e) = self.module.add_struct(
                    name,
                    IrStruct {
                        name: s.name.name.clone(),
                        visibility: s.visibility,
                        traits: Vec::new(),
                        fields: Vec::new(),
                        generic_params: Vec::new(),
                        doc: s.doc.clone(),
                    },
                ) {
                    self.errors.push(e);
                }
            }
            Definition::Enum(e) => {
                let name = e.name.name.clone();
                if let Err(e) = self.module.add_enum(
                    name,
                    IrEnum {
                        name: e.name.name.clone(),
                        visibility: e.visibility,
                        variants: Vec::new(),
                        generic_params: Vec::new(),
                        doc: e.doc.clone(),
                    },
                ) {
                    self.errors.push(e);
                }
            }
            Definition::Impl(_) | Definition::Module(_) | Definition::Function(_) => {
                // Impls are processed after structs.
                // Modules: nested definitions are registered by register_module_types
                //   (called from register_imported_types before the first pass).
                // Functions are processed in the second pass.
            }
        }
    }

    /// Second pass: lower definitions with full type resolution
    fn lower_definition(&mut self, def: &Definition) {
        match def {
            Definition::Trait(t) => self.lower_trait(t),
            Definition::Struct(s) => self.lower_struct(s),
            Definition::Enum(e) => self.lower_enum(e),
            Definition::Impl(i) => self.lower_impl(i),
            Definition::Function(f) => self.lower_function(f.as_ref()),
            Definition::Module(m) => {
                // Lower nested definitions within the module
                self.lower_module(&m.name.name, &m.definitions);
            }
        }
    }

    /// Lower definitions within a module
    /// This processes nested definitions with their qualified names
    fn lower_module(&mut self, module_name: &str, definitions: &[Definition]) {
        // Save current module prefix
        let saved_prefix = self.current_module_prefix.clone();

        // Update module prefix for nested definitions
        if self.current_module_prefix.is_empty() {
            self.current_module_prefix = module_name.to_string();
        } else {
            self.current_module_prefix = format!("{}::{}", self.current_module_prefix, module_name);
        }

        // Lower all definitions in the module
        for def in definitions {
            match def {
                Definition::Trait(t) => {
                    // Traits in modules use qualified names
                    self.lower_trait_with_prefix(t, &self.current_module_prefix.clone());
                }
                Definition::Struct(s) => {
                    // Structs in modules use qualified names
                    self.lower_struct_with_prefix(s, &self.current_module_prefix.clone());
                }
                Definition::Enum(e) => {
                    // Enums in modules use qualified names
                    self.lower_enum_with_prefix(e, &self.current_module_prefix.clone());
                }
                Definition::Impl(i) => {
                    // Impls in modules
                    self.lower_impl(i);
                }
                Definition::Function(f) => {
                    // Functions in modules
                    self.lower_function(f.as_ref());
                }
                Definition::Module(m) => {
                    // Recursively process nested modules
                    self.lower_module(&m.name.name, &m.definitions);
                }
            }
        }

        // Restore module prefix
        self.current_module_prefix = saved_prefix;
    }

    /// Lower trait with module prefix
    fn lower_trait_with_prefix(&mut self, t: &TraitDef, prefix: &str) {
        let qualified_name = format!("{}::{}", prefix, t.name.name);
        let Some(id) = self
            .module
            .trait_id(&qualified_name)
            .or_else(|| self.module.trait_id(&t.name.name))
        else {
            return; // Trait not registered, skip
        };

        let composed_traits: Vec<TraitId> = t
            .traits
            .iter()
            .filter_map(|ident| self.module.trait_id(&ident.name))
            .collect();

        let generic_params = self.lower_generic_params(&t.generics);
        self.generic_scopes.push(generic_params.clone());
        let fields: Vec<IrField> = t.fields.iter().map(|f| self.lower_field_def(f)).collect();
        let methods: Vec<IrFunctionSig> = t.methods.iter().map(|m| self.lower_fn_sig(m)).collect();
        self.generic_scopes.pop();

        let Some(trait_def) = self.module.trait_mut(id) else {
            self.record_missing_id("trait", id.0);
            return;
        };
        trait_def.name = qualified_name;
        trait_def.visibility = t.visibility;
        trait_def.composed_traits = composed_traits;
        trait_def.generic_params = generic_params;
        trait_def.fields = fields;
        trait_def.methods = methods;
    }

    /// Lower struct with module prefix
    fn lower_struct_with_prefix(&mut self, s: &StructDef, prefix: &str) {
        let qualified_name = format!("{}::{}", prefix, s.name.name);
        let Some(id) = self
            .module
            .struct_id(&qualified_name)
            .or_else(|| self.module.struct_id(&s.name.name))
        else {
            return; // Struct not registered, skip
        };

        // Look up the struct's traits from the correct (nested) symbol table.
        let all_trait_names = self.get_traits_for_struct_in_module(prefix, &s.name.name);
        let traits: Vec<TraitId> = all_trait_names
            .iter()
            .filter_map(|trait_name| {
                // The trait name from source is unqualified (e.g. "Drawable").
                // It was registered in the IR as a qualified name (e.g. "shapes::Drawable").
                // Try the qualified form first, fall back to unqualified.
                let qualified = format!("{prefix}::{trait_name}");
                self.module
                    .trait_id(&qualified)
                    .or_else(|| self.module.trait_id(trait_name))
            })
            .collect();

        let generic_params = self.lower_generic_params(&s.generics);
        self.generic_scopes.push(generic_params.clone());
        let fields: Vec<IrField> = s
            .fields
            .iter()
            .map(|f| self.lower_struct_field(f))
            .collect();
        self.generic_scopes.pop();

        let Some(struct_def) = self.module.struct_mut(id) else {
            self.record_missing_id("struct", id.0);
            return;
        };
        struct_def.name = qualified_name;
        struct_def.visibility = s.visibility;
        struct_def.traits = traits;
        struct_def.generic_params = generic_params;
        struct_def.fields = fields;
    }

    /// Lower enum with module prefix
    fn lower_enum_with_prefix(&mut self, e: &EnumDef, prefix: &str) {
        let qualified_name = format!("{}::{}", prefix, e.name.name);
        let Some(id) = self
            .module
            .enum_id(&qualified_name)
            .or_else(|| self.module.enum_id(&e.name.name))
        else {
            return; // Enum not registered, skip
        };

        let generic_params = self.lower_generic_params(&e.generics);
        self.generic_scopes.push(generic_params.clone());
        let variants: Vec<IrEnumVariant> = e
            .variants
            .iter()
            .map(|v| IrEnumVariant {
                name: v.name.name.clone(),
                fields: v
                    .fields
                    .iter()
                    .map(|f| IrField {
                        name: f.name.name.clone(),
                        ty: self.lower_type(&f.ty),
                        default: None,
                        optional: false,
                        mutable: false,
                        doc: f.doc.clone(),
                    })
                    .collect(),
            })
            .collect();
        self.generic_scopes.pop();

        let Some(enum_def) = self.module.enum_mut(id) else {
            self.record_missing_id("enum", id.0);
            return;
        };
        enum_def.name = qualified_name;
        enum_def.visibility = e.visibility;
        enum_def.generic_params = generic_params;
        enum_def.variants = variants;
    }

    fn lower_trait(&mut self, t: &TraitDef) {
        let Some(id) = self.module.trait_id(&t.name.name) else {
            self.errors.push(CompilerError::UndefinedType {
                name: t.name.name.clone(),
                span: t.span,
            });
            return;
        };

        let composed_traits: Vec<TraitId> = t
            .traits
            .iter()
            .filter_map(|ident| self.module.trait_id(&ident.name))
            .collect();

        let generic_params = self.lower_generic_params(&t.generics);
        self.generic_scopes.push(generic_params.clone());

        let fields: Vec<IrField> = t.fields.iter().map(|f| self.lower_field_def(f)).collect();

        let methods: Vec<IrFunctionSig> = t.methods.iter().map(|m| self.lower_fn_sig(m)).collect();

        self.generic_scopes.pop();

        let Some(trait_def) = self.module.trait_mut(id) else {
            self.record_missing_id("trait", id.0);
            return;
        };
        trait_def.composed_traits = composed_traits;
        trait_def.fields = fields;
        trait_def.methods = methods;
        trait_def.generic_params = generic_params;
    }

    fn lower_struct(&mut self, s: &StructDef) {
        let Some(id) = self.module.struct_id(&s.name.name) else {
            self.errors.push(CompilerError::UndefinedType {
                name: s.name.name.clone(),
                span: s.span,
            });
            return;
        };

        // Get all traits from both inline definition and impl blocks
        let all_trait_names = self.symbols.get_all_traits_for_struct(&s.name.name);
        let traits: Vec<TraitId> = all_trait_names
            .iter()
            .filter_map(|trait_name| {
                // Check if this is an external trait and track the import
                self.try_track_imported_type(trait_name, ImportedKind::Trait);
                self.module.trait_id(trait_name)
            })
            .collect();

        let generic_params = self.lower_generic_params(&s.generics);
        self.generic_scopes.push(generic_params.clone());

        let fields: Vec<IrField> = s
            .fields
            .iter()
            .map(|f| self.lower_struct_field(f))
            .collect();

        self.generic_scopes.pop();

        let Some(struct_def) = self.module.struct_mut(id) else {
            self.record_missing_id("struct", id.0);
            return;
        };
        struct_def.traits = traits;
        struct_def.fields = fields;
        struct_def.generic_params = generic_params;
    }

    fn lower_enum(&mut self, e: &EnumDef) {
        let Some(id) = self.module.enum_id(&e.name.name) else {
            self.errors.push(CompilerError::UndefinedType {
                name: e.name.name.clone(),
                span: e.span,
            });
            return;
        };

        let generic_params = self.lower_generic_params(&e.generics);
        self.generic_scopes.push(generic_params.clone());

        let variants: Vec<IrEnumVariant> = e
            .variants
            .iter()
            .map(|v| IrEnumVariant {
                name: v.name.name.clone(),
                fields: v.fields.iter().map(|f| self.lower_field_def(f)).collect(),
            })
            .collect();

        self.generic_scopes.pop();

        let Some(enum_def) = self.module.enum_mut(id) else {
            self.record_missing_id("enum", id.0);
            return;
        };
        enum_def.variants = variants;
        enum_def.generic_params = generic_params;
    }

    fn lower_impl(&mut self, i: &ImplDef) {
        use super::ImplTarget;

        // Build qualified name if we're inside a module
        let qualified_name = if self.current_module_prefix.is_empty() {
            i.name.name.clone()
        } else {
            format!("{}::{}", self.current_module_prefix, i.name.name)
        };

        // Try to find struct first (qualified then unqualified), then enum
        let target = if let Some(id) = self.module.struct_id(&qualified_name) {
            ImplTarget::Struct(id)
        } else if let Some(id) = self.module.struct_id(&i.name.name) {
            ImplTarget::Struct(id)
        } else if let Some(id) = self.module.enum_id(&qualified_name) {
            ImplTarget::Enum(id)
        } else if let Some(id) = self.module.enum_id(&i.name.name) {
            ImplTarget::Enum(id)
        } else {
            return; // Error would have been caught in semantic analysis
        };

        // Set current impl struct/enum for self reference resolution
        self.current_impl_struct = Some(i.name.name.clone());

        let generic_params = self.lower_generic_params(&i.generics);
        // The impl's methods reference type parameters whose trait
        // constraints are declared on the *target* struct/enum
        // (e.g. `struct Box<T: Foo>` plus `impl Box<T> { ... }`). The impl
        // header itself carries the param names without constraints, so we
        // merge constraints from the target definition under each matching
        // name so `find_trait_for_method` resolves correctly.
        let mut scope = generic_params.clone();
        let target_params: Vec<IrGenericParam> = match target {
            super::ImplTarget::Struct(id) => self
                .module
                .get_struct(id)
                .map(|s| s.generic_params.clone())
                .unwrap_or_default(),
            super::ImplTarget::Enum(id) => self
                .module
                .get_enum(id)
                .map(|e| e.generic_params.clone())
                .unwrap_or_default(),
        };
        for target_param in target_params {
            if let Some(existing) = scope.iter_mut().find(|q| q.name == target_param.name) {
                for c in target_param.constraints {
                    if !existing.constraints.contains(&c) {
                        existing.constraints.push(c);
                    }
                }
            } else {
                scope.push(target_param);
            }
        }
        self.generic_scopes.push(scope);

        // Pre-compute method return types so lowering a body can resolve
        // forward references like `self.other_method()` without needing
        // the impl to already be in `module.impls`. Must run *after* the
        // generic-scope push so a method returning `T` resolves the type
        // param against the impl/target scope instead of failing
        // `UndefinedType` lookup.
        let saved_impl_returns = self.current_impl_method_returns.take();
        let mut impl_returns: HashMap<String, Option<ResolvedType>> = HashMap::new();
        for f in &i.functions {
            let ret = f.return_type.as_ref().map(|t| self.lower_type(t));
            impl_returns.insert(f.name.name.clone(), ret);
        }
        self.current_impl_method_returns = Some(impl_returns);
        let functions: Vec<IrFunction> = i
            .functions
            .iter()
            .map(|f| self.lower_fn_def(f, i.is_extern))
            .collect();
        self.generic_scopes.pop();
        let trait_id = i
            .trait_name
            .as_ref()
            .and_then(|t| self.module.trait_id(&t.name));

        // Clear the context
        self.current_impl_struct = None;
        self.current_impl_method_returns = saved_impl_returns;

        if let Err(err) = self.module.add_impl(IrImpl {
            target,
            trait_id,
            is_extern: i.is_extern,
            generic_params,
            functions,
        }) {
            self.errors.push(err);
        }
    }

    fn lower_function(&mut self, f: &FunctionDef) {
        let generic_params = self.lower_generic_params(&f.generics);
        self.generic_scopes.push(generic_params.clone());
        let params: Vec<IrFunctionParam> = f
            .params
            .iter()
            .map(|p| IrFunctionParam {
                name: p.name.name.clone(),
                external_label: p.external_label.as_ref().map(|l| l.name.clone()),
                ty: p.ty.as_ref().map(|t| self.lower_type(t)),
                default: p.default.as_ref().map(|e| self.lower_expr(e)),
                convention: p.convention,
            })
            .collect();

        let return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Set return type context for inferred enum resolution
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = f.return_type.as_ref().map(Self::type_name);

        // Push a local scope so References inside the body resolve against
        // the parameters' declared types and so closure captures see the
        // parameter's convention (audit finding #32).
        let mut frame: HashMap<String, (ParamConvention, ResolvedType)> = HashMap::new();
        for p in &params {
            if let Some(ty) = &p.ty {
                frame.insert(p.name.clone(), (p.convention, ty.clone()));
            }
        }
        self.local_binding_scopes.push(frame);

        let body = f.body.as_ref().map(|b| self.lower_expr(b));
        // Audit #28: trust the AST's explicit `is_extern` flag rather
        // than re-deriving from `body.is_none()`. Under parser error
        // recovery the two can diverge; the semantic layer surfaces
        // that mismatch as `ExternFnWithBody` / `RegularFnWithoutBody`.
        let is_extern = f.is_extern;

        self.local_binding_scopes.pop();

        // Restore previous return type context
        self.current_function_return_type = saved_return_type;

        self.generic_scopes.pop();

        if let Err(e) = self.module.add_function(
            f.name.name.clone(),
            IrFunction {
                name: f.name.name.clone(),
                generic_params,
                params,
                return_type,
                body,
                is_extern,
                doc: f.doc.clone(),
            },
        ) {
            self.errors.push(e);
        }
    }

    fn lower_fn_def(&mut self, f: &FnDef, enclosing_is_extern: bool) -> IrFunction {
        let params: Vec<IrFunctionParam> = f
            .params
            .iter()
            .map(|p| IrFunctionParam {
                name: p.name.name.clone(),
                external_label: p.external_label.as_ref().map(|l| l.name.clone()),
                ty: p.ty.as_ref().map(|t| self.lower_type(t)),
                default: p.default.as_ref().map(|e| self.lower_expr(e)),
                convention: p.convention,
            })
            .collect();

        let return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Set return type context for inferred enum resolution
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = f.return_type.as_ref().map(Self::type_name);

        // Push a local scope so the body's References to parameters resolve
        // to the declared param types rather than TypeParam(name) placeholders,
        // and so closures inherit the parameter convention when capturing
        // (audit finding #32).
        let mut frame: HashMap<String, (ParamConvention, ResolvedType)> = HashMap::new();
        for p in &params {
            if let Some(ty) = &p.ty {
                frame.insert(p.name.clone(), (p.convention, ty.clone()));
            }
        }
        if let Some(impl_name) = self.current_impl_struct.clone() {
            if let Some(struct_id) = self.module.struct_id(&impl_name) {
                frame.insert(
                    "self".to_string(),
                    (ParamConvention::Let, ResolvedType::Struct(struct_id)),
                );
            } else if let Some(enum_id) = self.module.enum_id(&impl_name) {
                frame.insert(
                    "self".to_string(),
                    (ParamConvention::Let, ResolvedType::Enum(enum_id)),
                );
            }
        }
        self.local_binding_scopes.push(frame);

        let body = f.body.as_ref().map(|b| self.lower_expr(b));
        // Audit2 A1: source `is_extern` from the enclosing `ImplDef.is_extern`
        // rather than re-deriving from `body.is_none()`. The semantic layer
        // enforces body/extern consistency for valid programs, but under
        // parser error recovery a method may have `body: None` inside a
        // regular impl; we want the IR's `IrFunction.is_extern` to match
        // the containing `IrImpl.is_extern` definitionally.
        let is_extern = enclosing_is_extern;

        self.local_binding_scopes.pop();

        // Restore previous return type context
        self.current_function_return_type = saved_return_type;

        IrFunction {
            name: f.name.name.clone(),
            // Method-level generics aren't yet supported; enclosing type
            // generics live on the containing IrImpl.
            generic_params: Vec::new(),
            params,
            return_type,
            body,
            is_extern,
            doc: f.doc.clone(),
        }
    }

    fn lower_fn_sig(&mut self, sig: &ast::FnSig) -> IrFunctionSig {
        let params: Vec<IrFunctionParam> = sig
            .params
            .iter()
            .map(|p| IrFunctionParam {
                name: p.name.name.clone(),
                external_label: p.external_label.as_ref().map(|l| l.name.clone()),
                ty: p.ty.as_ref().map(|t| self.lower_type(t)),
                default: p.default.as_ref().map(|e| self.lower_expr(e)),
                convention: p.convention,
            })
            .collect();

        let return_type = sig.return_type.as_ref().map(|t| self.lower_type(t));

        IrFunctionSig {
            name: sig.name.name.clone(),
            params,
            return_type,
        }
    }

    /// Extract the type name from an AST type (for return type context)
    fn type_name(ty: &ast::Type) -> String {
        match ty {
            ast::Type::Primitive(prim) => match prim {
                ast::PrimitiveType::String => "String".to_string(),
                ast::PrimitiveType::Number => "Number".to_string(),
                ast::PrimitiveType::Boolean => "Boolean".to_string(),
                ast::PrimitiveType::Path => "Path".to_string(),
                ast::PrimitiveType::Regex => "Regex".to_string(),
                ast::PrimitiveType::Never => "Never".to_string(),
            },
            ast::Type::Optional(inner) => Self::type_name(inner),
            ast::Type::Array(_) => "Array".to_string(),
            ast::Type::Tuple(_) => "Tuple".to_string(),
            ast::Type::Dictionary { .. } => "Dictionary".to_string(),
            ast::Type::Closure { .. } => "Closure".to_string(),
            ast::Type::Ident(name) | ast::Type::Generic { name, .. } => name.name.clone(),
        }
    }

    fn lower_generic_params(&self, params: &[ast::GenericParam]) -> Vec<IrGenericParam> {
        params
            .iter()
            .map(|p| IrGenericParam {
                name: p.name.name.clone(),
                constraints: p
                    .constraints
                    .iter()
                    .filter_map(|c| match c {
                        GenericConstraint::Trait(ident) => self.module.trait_id(&ident.name),
                    })
                    .collect(),
            })
            .collect()
    }

    fn lower_field_def(&mut self, f: &ast::FieldDef) -> IrField {
        let optional = matches!(f.ty, ast::Type::Optional(_));
        IrField {
            name: f.name.name.clone(),
            ty: self.lower_type(&f.ty),
            mutable: f.mutable,
            optional,
            default: None,
            doc: f.doc.clone(),
        }
    }

    fn lower_struct_field(&mut self, f: &StructField) -> IrField {
        // Audit2 B18: thread the field's declared type as the
        // inferred-enum target so `.variant` literals inside the
        // default expression resolve to the field's enum type.
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = Some(Self::type_name(&f.ty));
        let default = f.default.as_ref().map(|e| self.lower_expr(e));
        self.current_function_return_type = saved_return_type;
        IrField {
            name: f.name.name.clone(),
            ty: self.lower_type(&f.ty),
            mutable: f.mutable,
            optional: f.optional,
            default,
            doc: f.doc.clone(),
        }
    }

    pub(super) fn lower_type(&mut self, ty: &Type) -> ResolvedType {
        match ty {
            Type::Primitive(p) => ResolvedType::Primitive(*p),

            Type::Ident(ident) => {
                let name = &ident.name;

                // For path-qualified names like "alignment::Horizontal",
                // try looking up just the last component
                let lookup_name = simple_type_name(name);

                // Check if this is an external type
                if let Some(external) = self.try_external_type(lookup_name, vec![]) {
                    return external;
                }
                // Otherwise try local types
                if let Some(id) = self.module.struct_id(lookup_name) {
                    ResolvedType::Struct(id)
                } else if let Some(id) = self.module.trait_id(lookup_name) {
                    ResolvedType::Trait(id)
                } else if let Some(id) = self.module.enum_id(lookup_name) {
                    ResolvedType::Enum(id)
                } else if self.is_generic_param_in_scope(name) {
                    ResolvedType::TypeParam(name.clone())
                } else {
                    // Tier-1 audit: surface unresolved type names loudly
                    // instead of silently lowering to `TypeParam(name)`.
                    // Semantic should normally catch this; reaching here
                    // means a typo, an unimported type, or an out-of-
                    // scope generic param.
                    self.errors.push(CompilerError::UndefinedType {
                        name: name.clone(),
                        span: ident.span,
                    });
                    ResolvedType::TypeParam("Unknown".to_string())
                }
            }

            Type::Generic { name, args, .. } => {
                let type_args: Vec<ResolvedType> =
                    args.iter().map(|t| self.lower_type(t)).collect();

                // Check if this is an external generic type
                if let Some(external) = self.try_external_type(&name.name, type_args.clone()) {
                    return external;
                }
                // Local generic struct
                if let Some(id) = self.module.struct_id(&name.name) {
                    return ResolvedType::Generic {
                        base: crate::ir::GenericBase::Struct(id),
                        args: type_args,
                    };
                }
                // Local generic enum
                if let Some(id) = self.module.enum_id(&name.name) {
                    return ResolvedType::Generic {
                        base: crate::ir::GenericBase::Enum(id),
                        args: type_args,
                    };
                }
                if self.is_generic_param_in_scope(&name.name) {
                    return ResolvedType::TypeParam(name.name.clone());
                }
                self.errors.push(CompilerError::UndefinedType {
                    name: name.name.clone(),
                    span: name.span,
                });
                ResolvedType::TypeParam("Unknown".to_string())
            }

            Type::Array(inner) => ResolvedType::Array(Box::new(self.lower_type(inner))),

            Type::Optional(inner) => ResolvedType::Optional(Box::new(self.lower_type(inner))),

            Type::Tuple(fields) => ResolvedType::Tuple(
                fields
                    .iter()
                    .map(|f| (f.name.name.clone(), self.lower_type(&f.ty)))
                    .collect(),
            ),

            Type::Dictionary { key, value } => ResolvedType::Dictionary {
                key_ty: Box::new(self.lower_type(key)),
                value_ty: Box::new(self.lower_type(value)),
            },

            Type::Closure { params, ret } => ResolvedType::Closure {
                param_tys: params
                    .iter()
                    .map(|(c, p)| (*c, self.lower_type(p)))
                    .collect(),
                return_ty: Box::new(self.lower_type(ret)),
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lower_empty_file() -> Result<(), Box<dyn std::error::Error>> {
        let ast = File {
            statements: vec![],
            span: crate::location::Span::default(),
            format_version: 1,
        };
        let symbols = SymbolTable::new();
        let result = lower_to_ir(&ast, &symbols);
        if result.is_err() {
            return Err(format!("Expected ok: {:?}", result.err()).into());
        }
        let module = result.map_err(|e| format!("{e:?}"))?;
        if !module.structs.is_empty() {
            return Err(format!("Expected empty structs, got {}", module.structs.len()).into());
        }
        if !module.traits.is_empty() {
            return Err(format!("Expected empty traits, got {}", module.traits.len()).into());
        }
        if !module.enums.is_empty() {
            return Err(format!("Expected empty enums, got {}", module.enums.len()).into());
        }
        Ok(())
    }
}
