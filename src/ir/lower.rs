//! IR lowering pass: AST + `SymbolTable` → `IrModule`

use crate::ast::{
    self, BinaryOperator, BindingPattern, BlockStatement, ClosureParam, Definition, EnumDef, Expr,
    File, FnDef, FunctionDef, GenericConstraint, ImplDef, LetBinding, Literal, PrimitiveType,
    Statement, StructDef, StructField, TraitDef, Type, UnaryOperator,
};
use crate::builtins::resolve_method_type;
use crate::error::CompilerError;
use crate::semantic::symbol_table::SymbolTable;

use super::{
    simple_type_name, EventBindingSource, EventFieldBinding, ExternalKind, IrBlockStatement,
    IrEnum, IrEnumVariant, IrExpr, IrField, IrFunction, IrFunctionParam, IrGenericParam, IrImpl,
    IrImport, IrImportItem, IrLet, IrMatchArm, IrModule, IrStruct, IrTrait, ResolvedType, TraitId,
};
use crate::semantic::symbol_table::SymbolKind;
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
    module: IrModule,
    symbols: &'a SymbolTable,
    errors: Vec<CompilerError>,
    /// Track imports by module path for aggregation: (`module_path`, `source_file`) -> items
    imports_by_module: HashMap<Vec<String>, (Vec<IrImportItem>, std::path::PathBuf)>,
    /// Current struct being processed in an impl block (for self references)
    current_impl_struct: Option<String>,
    /// Current module prefix for nested definitions (e.g., "`outer::inner`")
    current_module_prefix: String,
    /// Current function's return type for inferring enum types
    current_function_return_type: Option<String>,
}

impl<'a> IrLowerer<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            module: IrModule::new(),
            symbols,
            errors: Vec::new(),
            imports_by_module: HashMap::new(),
            current_impl_struct: None,
            current_module_prefix: String::new(),
            current_function_return_type: None,
        }
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
    #[expect(clippy::too_many_lines, reason = "large match expression — splitting would reduce clarity")]
    #[expect(
        clippy::cast_precision_loss,
        reason = "array destructuring indices are small source-code positions that fit exactly in f64 mantissa"
    )]
    fn lower_let_binding(&mut self, let_binding: &LetBinding) {
        match &let_binding.pattern {
            BindingPattern::Simple(ident) => {
                let ty = if let Some(type_ann) = &let_binding.type_annotation {
                    self.lower_type(type_ann)
                } else if let Some(let_type) = self.symbols.get_let_type(&ident.name) {
                    self.string_to_resolved_type(let_type)
                } else {
                    // Infer from expression
                    let expr = self.lower_expr(&let_binding.value);
                    expr.ty().clone()
                };

                let value = self.lower_expr(&let_binding.value);

                self.module.add_let(IrLet {
                    name: ident.name.clone(),
                    visibility: let_binding.visibility,
                    mutable: let_binding.mutable,
                    ty,
                    value,
                });
            }
            BindingPattern::Array { elements, .. } => {
                // For array destructuring, create a let for each element
                let value_expr = self.lower_expr(&let_binding.value);
                let elem_ty = match value_expr.ty() {
                    ResolvedType::Array(inner) => (**inner).clone(),
                    ResolvedType::Primitive(_)
                    | ResolvedType::Struct(_)
                    | ResolvedType::Trait(_)
                    | ResolvedType::Enum(_)
                    | ResolvedType::Optional(_)
                    | ResolvedType::Tuple(_)
                    | ResolvedType::Generic { .. }
                    | ResolvedType::TypeParam(_)
                    | ResolvedType::External { .. }
                    | ResolvedType::EventMapping { .. }
                    | ResolvedType::Dictionary { .. }
                    | ResolvedType::Closure { .. } => {
                        ResolvedType::TypeParam("Unknown".to_string())
                    }
                };

                for (i, element) in elements.iter().enumerate() {
                    if let Some(name) = Self::extract_binding_name(element) {
                        // Create an indexed reference expression
                        let index_expr = IrExpr::Literal {
                            value: Literal::Number(i as f64),
                            ty: ResolvedType::Primitive(PrimitiveType::Number),
                        };
                        // For now, just store the element type - the value is complex
                        self.module.add_let(IrLet {
                            name,
                            visibility: let_binding.visibility,
                            mutable: let_binding.mutable,
                            ty: elem_ty.clone(),
                            value: index_expr,
                        });
                    }
                }
            }
            BindingPattern::Struct { fields, .. } => {
                // For struct destructuring, create a let for each field
                let value_expr = self.lower_expr(&let_binding.value);

                for field in fields {
                    let field_name = field.name.name.clone();
                    let binding_name = field
                        .alias
                        .as_ref().map_or_else(|| field_name.clone(), |a| a.name.clone());

                    // Try to get the field type from the struct
                    let field_ty = self.get_field_type_from_resolved(value_expr.ty(), &field_name);

                    self.module.add_let(IrLet {
                        name: binding_name,
                        visibility: let_binding.visibility,
                        mutable: let_binding.mutable,
                        ty: field_ty,
                        value: IrExpr::Reference {
                            path: vec![field_name],
                            ty: ResolvedType::TypeParam("StructField".to_string()),
                        },
                    });
                }
            }
            BindingPattern::Tuple { elements, .. } => {
                // For tuple destructuring, create a let for each element
                let value_expr = self.lower_expr(&let_binding.value);
                let tuple_types = match value_expr.ty() {
                    ResolvedType::Tuple(fields) => fields.clone(),
                    ResolvedType::Primitive(_)
                    | ResolvedType::Struct(_)
                    | ResolvedType::Trait(_)
                    | ResolvedType::Enum(_)
                    | ResolvedType::Array(_)
                    | ResolvedType::Optional(_)
                    | ResolvedType::Generic { .. }
                    | ResolvedType::TypeParam(_)
                    | ResolvedType::External { .. }
                    | ResolvedType::EventMapping { .. }
                    | ResolvedType::Dictionary { .. }
                    | ResolvedType::Closure { .. } => Vec::new(),
                };

                for (i, element) in elements.iter().enumerate() {
                    if let Some(name) = Self::extract_simple_binding_name(element) {
                        let ty = tuple_types
                            .get(i).map_or_else(|| ResolvedType::TypeParam("Unknown".to_string()), |(_, t)| t.clone());

                        self.module.add_let(IrLet {
                            name,
                            visibility: let_binding.visibility,
                            mutable: let_binding.mutable,
                            ty,
                            value: IrExpr::Literal {
                                value: Literal::Number(i as f64),
                                ty: ResolvedType::Primitive(PrimitiveType::Number),
                            },
                        });
                    }
                }
            }
        }
    }

    /// Extract binding name from an array pattern element
    fn extract_binding_name(element: &ast::ArrayPatternElement) -> Option<String> {
        match element {
            ast::ArrayPatternElement::Binding(pattern) => Self::extract_simple_binding_name(pattern),
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

    /// Get field type from a resolved type
    fn get_field_type_from_resolved(&self, ty: &ResolvedType, field_name: &str) -> ResolvedType {
        if let ResolvedType::Struct(id) = ty {
            if let Some(struct_def) = self.module.get_struct(*id) {
                if let Some(field) = struct_def.fields.iter().find(|f| f.name == field_name) {
                    return field.ty.clone();
                }
            }
        }
        ResolvedType::TypeParam("Unknown".to_string())
    }

    /// Resolve the type of a self.field reference using current impl context
    fn resolve_self_field_type(&mut self, field_name: &str) -> ResolvedType {
        if let Some(struct_name) = self.current_impl_struct.clone() {
            // Look up the struct in the symbol table
            if let Some(struct_info) = self.symbols.structs.get(&struct_name) {
                // Check regular fields
                if let Some(field) = struct_info.fields.iter().find(|f| f.name == field_name) {
                    let ty = field.ty.clone();
                    return self.lower_type(&ty);
                }
                // Check mount fields
                if let Some(field) = struct_info
                    .mount_fields
                    .iter()
                    .find(|f| f.name == field_name)
                {
                    let ty = field.ty.clone();
                    return self.lower_type(&ty);
                }
            }
        }
        ResolvedType::TypeParam(format!("self.{field_name}"))
    }

    /// Resolve the type of `self` in an impl block context.
    /// Returns the `ResolvedType` for the struct or enum being implemented.
    fn resolve_impl_self_type(&self, impl_name: &str) -> ResolvedType {
        // First try as a struct
        if let Some(id) = self.module.struct_id(impl_name) {
            return ResolvedType::Struct(id);
        }
        // Then try as an enum
        if let Some(id) = self.module.enum_id(impl_name) {
            return ResolvedType::Enum(id);
        }
        // Fall back to TypeParam if not found
        ResolvedType::TypeParam("self".to_string())
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
                // Track this import for codegen (to find impl blocks)
                self.try_track_external_import(name, ExternalKind::Struct);
            }
        }

        // Register imported enums (top-level)
        for (name, enum_info) in &self.symbols.enums {
            // Check if this is an imported symbol
            if self.symbols.get_module_origin(name).is_some() {
                self.register_enum(name, enum_info);
                // Track this import for codegen (to find impl blocks)
                self.try_track_external_import(name, ExternalKind::Enum);
            }
        }

        // Register types from imported nested modules (e.g., fill::Solid)
        for (module_name, module_info) in &self.symbols.modules {
            self.register_module_types(module_name, &module_info.symbols);
        }
    }

    /// Register types from a nested module recursively
    fn register_module_types(&mut self, module_prefix: &str, module_symbols: &SymbolTable) {
        // Register traits from this module (placeholder — filled in the second pass)
        for (name, trait_info) in &module_symbols.traits {
            let qualified_name = format!("{module_prefix}::{name}");
            if let Err(e) = self.module.add_trait(
                qualified_name.clone(),
                IrTrait {
                    name: qualified_name,
                    visibility: trait_info.visibility,
                    composed_traits: Vec::new(),
                    fields: Vec::new(),
                    mount_fields: Vec::new(),
                    generic_params: Vec::new(),
                },
            ) {
                self.errors.push(e);
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

    /// Helper method to register an enum
    /// Note: `EnumInfo` doesn't preserve variant field details, so we create placeholder variants
    fn register_enum(&mut self, name: &str, enum_info: &crate::semantic::symbol_table::EnumInfo) {
        let generic_params = self.lower_generic_params(&enum_info.generics);

        // EnumInfo only stores variant names and arity, not field details
        // We create placeholder variants with empty fields
        let variants: Vec<IrEnumVariant> = enum_info
            .variants
            .keys()
            .map(|variant_name| IrEnumVariant {
                name: variant_name.clone(),
                fields: Vec::new(), // Field details not available in EnumInfo
            })
            .collect();

        if let Err(e) = self.module.add_enum(
            name.to_string(),
            IrEnum {
                name: name.to_string(),
                visibility: enum_info.visibility,
                variants,
                generic_params,
            },
        ) {
            self.errors.push(e);
        }
    }

    /// Helper method to register a struct with full field information
    fn register_struct(
        &mut self,
        name: &str,
        struct_info: &crate::semantic::symbol_table::StructInfo,
    ) {
        // Convert fields from StructInfo to IrField
        let fields: Vec<IrField> = struct_info
            .fields
            .iter()
            .map(|f| IrField {
                name: f.name.clone(),
                ty: self.lower_type(&f.ty),
                mutable: false,
                optional: false,
                default: None,
            })
            .collect();

        // Convert mount_fields from StructInfo to IrField
        let mount_fields: Vec<IrField> = struct_info
            .mount_fields
            .iter()
            .map(|f| IrField {
                name: f.name.clone(),
                ty: self.lower_type(&f.ty),
                mutable: false,
                optional: false,
                default: None,
            })
            .collect();

        // Convert trait names to trait IDs
        // Use get_all_traits_for_struct to include both inline traits and impl blocks
        let all_trait_names = self.symbols.get_all_traits_for_struct(name);
        let traits: Vec<TraitId> = all_trait_names
            .iter()
            .filter_map(|trait_name| self.module.trait_id(trait_name))
            .collect();

        // Convert generic params
        let generic_params = self.lower_generic_params(&struct_info.generics);

        if let Err(e) = self.module.add_struct(
            name.to_string(),
            IrStruct {
                name: name.to_string(),
                visibility: struct_info.visibility,
                traits,
                fields,
                mount_fields,
                generic_params,
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
                        mount_fields: Vec::new(),
                        generic_params: Vec::new(),
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
                        mount_fields: Vec::new(),
                        generic_params: Vec::new(),
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
        let fields: Vec<IrField> = t.fields.iter().map(|f| self.lower_field_def(f)).collect();
        let mount_fields: Vec<IrField> = t
            .mount_fields
            .iter()
            .map(|f| self.lower_field_def(f))
            .collect();

        // Update the trait in place
        // id was obtained from trait_id() which guarantees it is a valid index
        #[expect(clippy::indexing_slicing, reason = "id obtained from trait_id() guarantees valid index")]
        let trait_def = &mut self.module.traits[id.0 as usize];
        trait_def.name = qualified_name;
        trait_def.visibility = t.visibility;
        trait_def.composed_traits = composed_traits;
        trait_def.generic_params = generic_params;
        trait_def.fields = fields;
        trait_def.mount_fields = mount_fields;
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
        let fields: Vec<IrField> = s
            .fields
            .iter()
            .map(|f| self.lower_struct_field(f))
            .collect();
        let mount_fields: Vec<IrField> = s
            .mount_fields
            .iter()
            .map(|f| self.lower_struct_field(f))
            .collect();

        // Update the struct in place
        // id was obtained from struct_id() which guarantees it is a valid index
        #[expect(clippy::indexing_slicing, reason = "id obtained from struct_id() guarantees valid index")]
        let struct_def = &mut self.module.structs[id.0 as usize];
        struct_def.name = qualified_name;
        struct_def.visibility = s.visibility;
        struct_def.traits = traits;
        struct_def.generic_params = generic_params;
        struct_def.fields = fields;
        struct_def.mount_fields = mount_fields;
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
                    })
                    .collect(),
            })
            .collect();

        // Update the enum in place
        // id was obtained from enum_id() which guarantees it is a valid index
        #[expect(clippy::indexing_slicing, reason = "id obtained from enum_id() guarantees valid index")]
        let enum_def = &mut self.module.enums[id.0 as usize];
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

        let fields: Vec<IrField> = t.fields.iter().map(|f| self.lower_field_def(f)).collect();

        let mount_fields: Vec<IrField> = t
            .mount_fields
            .iter()
            .map(|f| self.lower_field_def(f))
            .collect();

        // Update the trait in place
        // id was obtained from trait_id() which guarantees it is a valid index
        #[expect(clippy::indexing_slicing, reason = "id obtained from trait_id() guarantees valid index")]
        let trait_def = &mut self.module.traits[id.0 as usize];
        trait_def.composed_traits = composed_traits;
        trait_def.fields = fields;
        trait_def.mount_fields = mount_fields;
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
                self.try_track_external_import(trait_name, ExternalKind::Trait);
                self.module.trait_id(trait_name)
            })
            .collect();

        let generic_params = self.lower_generic_params(&s.generics);

        let fields: Vec<IrField> = s
            .fields
            .iter()
            .map(|f| self.lower_struct_field(f))
            .collect();

        let mount_fields: Vec<IrField> = s
            .mount_fields
            .iter()
            .map(|f| self.lower_struct_field(f))
            .collect();

        // Update the struct in place
        // id was obtained from struct_id() which guarantees it is a valid index
        #[expect(clippy::indexing_slicing, reason = "id obtained from struct_id() guarantees valid index")]
        let struct_def = &mut self.module.structs[id.0 as usize];
        struct_def.traits = traits;
        struct_def.fields = fields;
        struct_def.mount_fields = mount_fields;
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

        let variants: Vec<IrEnumVariant> = e
            .variants
            .iter()
            .map(|v| IrEnumVariant {
                name: v.name.name.clone(),
                fields: v.fields.iter().map(|f| self.lower_field_def(f)).collect(),
            })
            .collect();

        // Update the enum in place
        // id was obtained from enum_id() which guarantees it is a valid index
        #[expect(clippy::indexing_slicing, reason = "id obtained from enum_id() guarantees valid index")]
        let enum_def = &mut self.module.enums[id.0 as usize];
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

        let functions: Vec<IrFunction> = i.functions.iter().map(|f| self.lower_fn_def(f)).collect();

        // Clear the context
        self.current_impl_struct = None;

        self.module.add_impl(IrImpl { target, functions });
    }

    fn lower_function(&mut self, f: &FunctionDef) {
        let params: Vec<IrFunctionParam> = f
            .params
            .iter()
            .map(|p| IrFunctionParam {
                name: p.name.name.clone(),
                ty: p.ty.as_ref().map(|t| self.lower_type(t)),
                default: p.default.as_ref().map(|e| self.lower_expr(e)),
            })
            .collect();

        let return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Set return type context for inferred enum resolution
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = f.return_type.as_ref().map(Self::type_name);

        let body = self.lower_expr(&f.body);

        // Restore previous return type context
        self.current_function_return_type = saved_return_type;

        if let Err(e) = self.module.add_function(
            f.name.name.clone(),
            IrFunction {
                name: f.name.name.clone(),
                params,
                return_type,
                body,
            },
        ) {
            self.errors.push(e);
        }
    }

    fn lower_fn_def(&mut self, f: &FnDef) -> IrFunction {
        let params: Vec<IrFunctionParam> = f
            .params
            .iter()
            .map(|p| IrFunctionParam {
                name: p.name.name.clone(),
                ty: p.ty.as_ref().map(|t| self.lower_type(t)),
                default: p.default.as_ref().map(|e| self.lower_expr(e)),
            })
            .collect();

        let return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // Set return type context for inferred enum resolution
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = f.return_type.as_ref().map(Self::type_name);

        let body = self.lower_expr(&f.body);

        // Restore previous return type context
        self.current_function_return_type = saved_return_type;

        IrFunction {
            name: f.name.name.clone(),
            params,
            return_type,
            body,
        }
    }

    /// Extract the type name from an AST type (for return type context)
    fn type_name(ty: &ast::Type) -> String {
        match ty {
            ast::Type::Primitive(prim) => format!("{prim:?}"),
            ast::Type::Optional(inner) => Self::type_name(inner),
            ast::Type::Array(_) => "Array".to_string(),
            ast::Type::Tuple(_) => "Tuple".to_string(),
            ast::Type::Dictionary { .. } => "Dictionary".to_string(),
            ast::Type::Closure { .. } => "Closure".to_string(),
            ast::Type::Ident(name)
            | ast::Type::Generic { name, .. }
            | ast::Type::TypeParameter(name) => name.name.clone(),
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
        IrField {
            name: f.name.name.clone(),
            ty: self.lower_type(&f.ty),
            mutable: f.mutable,
            optional: false,
            default: None,
        }
    }

    fn lower_struct_field(&mut self, f: &StructField) -> IrField {
        IrField {
            name: f.name.name.clone(),
            ty: self.lower_type(&f.ty),
            mutable: f.mutable,
            optional: f.optional,
            default: f.default.as_ref().map(|e| self.lower_expr(e)),
        }
    }

    fn lower_type(&mut self, ty: &Type) -> ResolvedType {
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
                } else {
                    // Might be a type parameter
                    ResolvedType::TypeParam(name.clone())
                }
            }

            Type::Generic { name, args, .. } => {
                let type_args: Vec<ResolvedType> =
                    args.iter().map(|t| self.lower_type(t)).collect();

                // Check if this is an external generic type
                if let Some(external) = self.try_external_type(&name.name, type_args.clone()) {
                    return external;
                }
                // Otherwise try local types
                self.module.struct_id(&name.name).map_or_else(
                    || ResolvedType::TypeParam(name.name.clone()),
                    |base| ResolvedType::Generic {
                        base,
                        args: type_args,
                    },
                )
            }

            Type::Array(inner) => ResolvedType::Array(Box::new(self.lower_type(inner))),

            Type::Optional(inner) => ResolvedType::Optional(Box::new(self.lower_type(inner))),

            Type::Tuple(fields) => ResolvedType::Tuple(
                fields
                    .iter()
                    .map(|f| (f.name.name.clone(), self.lower_type(&f.ty)))
                    .collect(),
            ),

            Type::TypeParameter(ident) => ResolvedType::TypeParam(ident.name.clone()),

            Type::Dictionary { key, value } => ResolvedType::Dictionary {
                key_ty: Box::new(self.lower_type(key)),
                value_ty: Box::new(self.lower_type(value)),
            },

            Type::Closure { params, ret } => ResolvedType::Closure {
                param_tys: params.iter().map(|p| self.lower_type(p)).collect(),
                return_ty: Box::new(self.lower_type(ret)),
            },
        }
    }

    /// Track an external import if the given name is imported from another module.
    /// This is used for cases where we can't create a full External type (e.g., trait implementations).
    fn try_track_external_import(&mut self, name: &str, expected_kind: ExternalKind) {
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
    fn try_external_type(
        &mut self,
        name: &str,
        type_args: Vec<ResolvedType>,
    ) -> Option<ResolvedType> {
        // Check if this symbol was imported from another module
        let module_path = self.symbols.get_module_logical_path(name)?;
        let kind = self.symbols.get_symbol_kind(name)?;

        let external_kind = match kind {
            SymbolKind::Struct => ExternalKind::Struct,
            SymbolKind::Trait => ExternalKind::Trait,
            SymbolKind::Enum => ExternalKind::Enum,
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

    #[expect(clippy::too_many_lines, reason = "large match expression — splitting would reduce clarity")]
    fn lower_expr(&mut self, expr: &Expr) -> IrExpr {
        match expr {
            Expr::Literal(lit) => IrExpr::Literal {
                value: lit.clone(),
                ty: Self::literal_type(lit),
            },

            Expr::Invocation {
                path,
                type_args,
                args,
                mounts,
                ..
            } => {
                // Join path into a single name for lookup
                let name = path
                    .iter()
                    .map(|id| id.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");

                let type_args_resolved: Vec<ResolvedType> =
                    type_args.iter().map(|t| self.lower_type(t)).collect();

                // Check if this is a struct instantiation or function call
                // For now, we try struct first, then fall back to function call
                if let Some(id) = self.module.struct_id(&name) {
                    // Struct instantiation
                    let ty = if type_args_resolved.is_empty() {
                        ResolvedType::Struct(id)
                    } else {
                        ResolvedType::Generic {
                            base: id,
                            args: type_args_resolved.clone(),
                        }
                    };
                    // Extract named args for struct (semantic analysis verified all are named)
                    let named_fields: Vec<(String, IrExpr)> = args
                        .iter()
                        .filter_map(|(name_opt, expr)| {
                            name_opt
                                .as_ref()
                                .map(|n| (n.name.clone(), self.lower_expr(expr)))
                        })
                        .collect();
                    IrExpr::StructInst {
                        struct_id: Some(id),
                        type_args: type_args_resolved,
                        fields: named_fields,
                        mounts: mounts
                            .iter()
                            .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                            .collect(),
                        ty,
                    }
                } else if let Some(external_ty) =
                    self.try_external_type(&name, type_args_resolved.clone())
                {
                    // External struct from unregistered module
                    let named_fields: Vec<(String, IrExpr)> = args
                        .iter()
                        .filter_map(|(name_opt, expr)| {
                            name_opt
                                .as_ref()
                                .map(|n| (n.name.clone(), self.lower_expr(expr)))
                        })
                        .collect();
                    IrExpr::StructInst {
                        struct_id: None,
                        type_args: type_args_resolved,
                        fields: named_fields,
                        mounts: mounts
                            .iter()
                            .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                            .collect(),
                        ty: external_ty,
                    }
                } else {
                    // Not a struct - treat as function call
                    let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();
                    let lowered_args: Vec<(Option<String>, IrExpr)> = args
                        .iter()
                        .map(|(name_opt, expr)| {
                            let arg_name = name_opt.as_ref().map(|n| n.name.clone());
                            (arg_name, self.lower_expr(expr))
                        })
                        .collect();

                    // Try to resolve the function return type
                    let fn_name = path_strs.last().map_or("", std::string::String::as_str);
                    let ty = self.resolve_function_return_type(fn_name, &lowered_args);

                    IrExpr::FunctionCall {
                        path: path_strs,
                        args: lowered_args,
                        ty,
                    }
                }
            }

            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                ..
            } => {
                // Check if we have an enum ID (local or imported)
                let (enum_id, ty) = self.module.enum_id(&enum_name.name).map_or_else(
                    || {
                        // External enum from unregistered module - no valid ID, or unknown enum
                        self.try_external_type(&enum_name.name, vec![]).map_or_else(
                            || (None, ResolvedType::TypeParam(enum_name.name.clone())),
                            |external_ty| (None, external_ty),
                        )
                    },
                    |id| (Some(id), ResolvedType::Enum(id)),
                );

                IrExpr::EnumInst {
                    enum_id,
                    variant: variant.name.clone(),
                    fields: data
                        .iter()
                        .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                        .collect(),
                    ty,
                }
            }

            Expr::InferredEnumInstantiation { variant, data, .. } => {
                // Try to resolve the enum type from the current function's return type context
                let (enum_id, ty) =
                    self.current_function_return_type.clone().map_or_else(
                        || (None, ResolvedType::TypeParam("InferredEnum".to_string())),
                        |return_type_name| {
                            // Check if the return type is an enum
                            self.module.enum_id(&return_type_name).map_or_else(
                                || {
                                    // Return type is not an enum we can resolve, or external
                                    self.try_external_type(&return_type_name, vec![]).map_or_else(
                                        || (None, ResolvedType::TypeParam("InferredEnum".to_string())),
                                        |external_ty| (None, external_ty),
                                    )
                                },
                                |id| (Some(id), ResolvedType::Enum(id)),
                            )
                        },
                    );

                IrExpr::EnumInst {
                    enum_id,
                    variant: variant.name.clone(),
                    fields: data
                        .iter()
                        .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                        .collect(),
                    ty,
                }
            }

            Expr::Array { elements, .. } => {
                let lowered: Vec<IrExpr> = elements.iter().map(|e| self.lower_expr(e)).collect();
                let elem_ty = lowered
                    .first().map_or_else(|| ResolvedType::TypeParam("UnknownElement".to_string()), |e| e.ty().clone());

                IrExpr::Array {
                    elements: lowered,
                    ty: ResolvedType::Array(Box::new(elem_ty)),
                }
            }

            Expr::Tuple { fields, .. } => {
                let lowered: Vec<(String, IrExpr)> = fields
                    .iter()
                    .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                    .collect();

                let tuple_types: Vec<(String, ResolvedType)> = lowered
                    .iter()
                    .map(|(n, e)| (n.clone(), e.ty().clone()))
                    .collect();

                IrExpr::Tuple {
                    fields: lowered,
                    ty: ResolvedType::Tuple(tuple_types),
                }
            }

            Expr::Reference { path, .. } => {
                let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();

                // Check for self.field pattern — bounds verified by len() == 2 check
                #[expect(clippy::indexing_slicing, reason = "len == 2 check above guarantees indices 0 and 1")]
                if path_strs.len() == 2 && path_strs[0] == "self" {
                    let field_name = &path_strs[1];
                    let ty = self.resolve_self_field_type(field_name);
                    return IrExpr::SelfFieldRef {
                        field: field_name.clone(),
                        ty,
                    };
                }

                // Check for bare "self" in impl context — bounds verified by len() == 1 check
                #[expect(clippy::indexing_slicing, reason = "len == 1 check above guarantees index 0")]
                if path_strs.len() == 1 && path_strs[0] == "self" {
                    if let Some(ref impl_name) = self.current_impl_struct {
                        let ty = self.resolve_impl_self_type(impl_name);
                        return IrExpr::Reference {
                            path: path_strs,
                            ty,
                        };
                    }
                }

                // Check for module-level let binding reference
                if path_strs.len() == 1 {
                    // bounds verified by len() == 1 check above
                    #[expect(clippy::indexing_slicing, reason = "len == 1 check above guarantees index 0")]
                    let name = &path_strs[0];
                    if let Some(let_type) = self.symbols.get_let_type(name) {
                        let ty = self.string_to_resolved_type(let_type);
                        return IrExpr::LetRef {
                            name: name.clone(),
                            ty,
                        };
                    }
                }

                // Fall back to generic reference for other cases
                let ty = if path_strs.len() == 1 {
                    // bounds verified by len() == 1 check above
                    #[expect(clippy::indexing_slicing, reason = "len == 1 check above guarantees index 0")]
                    let t = ResolvedType::TypeParam(path_strs[0].clone());
                    t
                } else {
                    ResolvedType::TypeParam(path_strs.join("."))
                };

                IrExpr::Reference {
                    path: path_strs,
                    ty,
                }
            }

            Expr::BinaryOp {
                left, op, right, ..
            } => {
                let left_ir = self.lower_expr(left);
                let right_ir = self.lower_expr(right);

                let ty = match op {
                    BinaryOperator::Eq
                    | BinaryOperator::Ne
                    | BinaryOperator::Lt
                    | BinaryOperator::Le
                    | BinaryOperator::Gt
                    | BinaryOperator::Ge
                    | BinaryOperator::And
                    | BinaryOperator::Or => ResolvedType::Primitive(PrimitiveType::Boolean),
                    BinaryOperator::Add
                    | BinaryOperator::Sub
                    | BinaryOperator::Mul
                    | BinaryOperator::Div
                    | BinaryOperator::Mod
                    | BinaryOperator::Range => left_ir.ty().clone(),
                };

                IrExpr::BinaryOp {
                    left: Box::new(left_ir),
                    op: *op,
                    right: Box::new(right_ir),
                    ty,
                }
            }

            Expr::UnaryOp { op, operand, .. } => {
                let operand_ir = self.lower_expr(operand);

                let ty = match op {
                    UnaryOperator::Not => ResolvedType::Primitive(PrimitiveType::Boolean),
                    UnaryOperator::Neg => operand_ir.ty().clone(),
                };

                IrExpr::UnaryOp {
                    op: *op,
                    operand: Box::new(operand_ir),
                    ty,
                }
            }

            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let then_ir = self.lower_expr(then_branch);
                let ty = then_ir.ty().clone();

                IrExpr::If {
                    condition: Box::new(self.lower_expr(condition)),
                    then_branch: Box::new(then_ir),
                    else_branch: else_branch.as_ref().map(|e| Box::new(self.lower_expr(e))),
                    ty,
                }
            }

            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => {
                let collection_ir = self.lower_expr(collection);
                let body_ir = self.lower_expr(body);

                // Extract element type from collection
                let var_ty = match collection_ir.ty() {
                    ResolvedType::Array(inner) => (**inner).clone(),
                    ResolvedType::Primitive(_)
                    | ResolvedType::Struct(_)
                    | ResolvedType::Trait(_)
                    | ResolvedType::Enum(_)
                    | ResolvedType::Optional(_)
                    | ResolvedType::Tuple(_)
                    | ResolvedType::Generic { .. }
                    | ResolvedType::TypeParam(_)
                    | ResolvedType::External { .. }
                    | ResolvedType::EventMapping { .. }
                    | ResolvedType::Dictionary { .. }
                    | ResolvedType::Closure { .. } => {
                        ResolvedType::TypeParam("UnknownElement".to_string())
                    }
                };

                IrExpr::For {
                    var: var.name.clone(),
                    var_ty,
                    collection: Box::new(collection_ir),
                    body: Box::new(body_ir.clone()),
                    ty: ResolvedType::Array(Box::new(body_ir.ty().clone())),
                }
            }

            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                let scrutinee_ir = self.lower_expr(scrutinee);

                let arms_ir: Vec<IrMatchArm> = arms
                    .iter()
                    .map(|arm| {
                        let bindings = self.extract_pattern_bindings(&arm.pattern, &scrutinee_ir);
                        IrMatchArm {
                            variant: match &arm.pattern {
                                ast::Pattern::Variant { name, .. } => name.name.clone(),
                                ast::Pattern::Wildcard => String::new(),
                            },
                            is_wildcard: matches!(&arm.pattern, ast::Pattern::Wildcard),
                            bindings,
                            body: self.lower_expr(&arm.body),
                        }
                    })
                    .collect();

                let ty = arms_ir
                    .first().map_or_else(|| ResolvedType::TypeParam("Unknown".to_string()), |a| a.body.ty().clone());

                IrExpr::Match {
                    scrutinee: Box::new(scrutinee_ir),
                    arms: arms_ir,
                    ty,
                }
            }

            Expr::Group { expr, .. } => self.lower_expr(expr),

            // Let expressions lower to their body
            Expr::LetExpr { body, .. } => self.lower_expr(body),

            Expr::DictLiteral { entries, .. } => {
                let lowered_entries: Vec<(IrExpr, IrExpr)> = entries
                    .iter()
                    .map(|(k, v)| (self.lower_expr(k), self.lower_expr(v)))
                    .collect();

                // Infer dictionary type from entries
                let ty = if let Some((k, v)) = lowered_entries.first() {
                    ResolvedType::Dictionary {
                        key_ty: Box::new(k.ty().clone()),
                        value_ty: Box::new(v.ty().clone()),
                    }
                } else {
                    // Empty dictionary - use placeholder types
                    ResolvedType::Dictionary {
                        key_ty: Box::new(ResolvedType::TypeParam("K".to_string())),
                        value_ty: Box::new(ResolvedType::TypeParam("V".to_string())),
                    }
                };

                IrExpr::DictLiteral {
                    entries: lowered_entries,
                    ty,
                }
            }

            Expr::DictAccess { dict, key, .. } => {
                let dict_ir = self.lower_expr(dict);
                let key_ir = self.lower_expr(key);

                // Extract value type from dictionary type
                let ty = match dict_ir.ty() {
                    ResolvedType::Dictionary { value_ty, .. } => (**value_ty).clone(),
                    ResolvedType::Primitive(_)
                    | ResolvedType::Struct(_)
                    | ResolvedType::Trait(_)
                    | ResolvedType::Enum(_)
                    | ResolvedType::Array(_)
                    | ResolvedType::Optional(_)
                    | ResolvedType::Tuple(_)
                    | ResolvedType::Generic { .. }
                    | ResolvedType::TypeParam(_)
                    | ResolvedType::External { .. }
                    | ResolvedType::EventMapping { .. }
                    | ResolvedType::Closure { .. } => {
                        ResolvedType::TypeParam("DictValue".to_string())
                    }
                };

                IrExpr::DictAccess {
                    dict: Box::new(dict_ir),
                    key: Box::new(key_ir),
                    ty,
                }
            }

            Expr::ClosureExpr { params, body, .. } => self.lower_closure(params, body),

            Expr::FieldAccess { object, field, .. } => {
                let object_ir = self.lower_expr(object);

                // Resolve the field type based on the object's type
                let ty = self.resolve_field_type(object_ir.ty(), &field.name);

                IrExpr::FieldAccess {
                    object: Box::new(object_ir),
                    field: field.name.clone(),
                    ty,
                }
            }

            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let receiver_ir = self.lower_expr(receiver);
                // Lower positional arguments (methods use positional args, so name is None)
                let lowered_args: Vec<(Option<String>, IrExpr)> = args
                    .iter()
                    .map(|expr| (None, self.lower_expr(expr)))
                    .collect();

                // Resolve method return type based on receiver type
                let ty = self.resolve_method_return_type(receiver_ir.ty(), &method.name);

                IrExpr::MethodCall {
                    receiver: Box::new(receiver_ir),
                    method: method.name.clone(),
                    args: lowered_args,
                    ty,
                }
            }

            Expr::Block {
                statements, result, ..
            } => {
                // Lower block statements
                let ir_statements: Vec<IrBlockStatement> = statements
                    .iter()
                    .map(|stmt| self.lower_block_statement(stmt))
                    .collect();

                // Lower result expression
                let ir_result = self.lower_expr(result);
                let ty = ir_result.ty().clone();

                // If no statements, just return the result directly
                if ir_statements.is_empty() {
                    return ir_result;
                }

                IrExpr::Block {
                    statements: ir_statements,
                    result: Box::new(ir_result),
                    ty,
                }
            }
        }
    }

    /// Lower an AST block statement to an IR block statement.
    fn lower_block_statement(&mut self, stmt: &BlockStatement) -> IrBlockStatement {
        match stmt {
            BlockStatement::Let {
                mutable,
                pattern,
                ty,
                value,
                ..
            } => {
                // Handle binding patterns
                let name = match pattern {
                    BindingPattern::Simple(ident) => ident.name.clone(),
                    BindingPattern::Tuple { elements, .. } => {
                        // For tuple destructuring, extract first simple name or use placeholder
                        elements
                            .iter()
                            .find_map(|p| match p {
                                BindingPattern::Simple(ident) => Some(ident.name.clone()),
                                BindingPattern::Array { .. }
                                | BindingPattern::Struct { .. }
                                | BindingPattern::Tuple { .. } => None,
                            })
                            .unwrap_or_else(|| "_tuple".to_string())
                    }
                    BindingPattern::Struct { fields, .. } => {
                        // For struct destructuring, use first field name or placeholder
                        fields
                            .first().map_or_else(|| "_struct".to_string(), |f| f.name.name.clone())
                    }
                    BindingPattern::Array { elements, .. } => {
                        // For array destructuring, use first binding name or placeholder
                        elements
                            .iter()
                            .find_map(|elem| match elem {
                                crate::ast::ArrayPatternElement::Binding(
                                    BindingPattern::Simple(ident),
                                ) => Some(ident.name.clone()),
                                crate::ast::ArrayPatternElement::Binding(_)
                                | crate::ast::ArrayPatternElement::Rest(_)
                                | crate::ast::ArrayPatternElement::Wildcard => None,
                            })
                            .unwrap_or_else(|| "_array".to_string())
                    }
                };
                let ir_ty = ty.as_ref().map(|t| self.lower_type(t));
                let ir_value = self.lower_expr(value);

                IrBlockStatement::Let {
                    name,
                    mutable: *mutable,
                    ty: ir_ty,
                    value: ir_value,
                }
            }
            BlockStatement::Assign { target, value, .. } => {
                let ir_target = self.lower_expr(target);
                let ir_value = self.lower_expr(value);

                IrBlockStatement::Assign {
                    target: ir_target,
                    value: ir_value,
                }
            }
            BlockStatement::Expr(expr) => {
                let ir_expr = self.lower_expr(expr);
                IrBlockStatement::Expr(ir_expr)
            }
        }
    }

    fn literal_type(lit: &Literal) -> ResolvedType {
        match lit {
            Literal::String(_) => ResolvedType::Primitive(PrimitiveType::String),
            Literal::Number(_) => ResolvedType::Primitive(PrimitiveType::Number),
            Literal::UnsignedInt(_) => ResolvedType::Primitive(PrimitiveType::U32),
            Literal::SignedInt(_) => ResolvedType::Primitive(PrimitiveType::I32),
            Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
            Literal::Path(_) => ResolvedType::Primitive(PrimitiveType::Path),
            Literal::Regex { .. } => ResolvedType::Primitive(PrimitiveType::Regex),
            Literal::Nil => ResolvedType::TypeParam("Nil".to_string()),
        }
    }

    /// Resolve the type of a field access on an expression.
    ///
    /// Handles:
    /// 1. Vector component access (vec2.x, vec3.y, etc.) -> f32/i32/u32
    /// 2. Struct field access -> field type
    fn resolve_field_type(&self, object_ty: &ResolvedType, field_name: &str) -> ResolvedType {
        match object_ty {
            // Vector component access
            ResolvedType::Primitive(PrimitiveType::Vec2 | PrimitiveType::Vec3 |
PrimitiveType::Vec4) => match field_name {
                "x" | "y" | "z" | "w" | "r" | "g" | "b" | "a" => {
                    ResolvedType::Primitive(PrimitiveType::F32)
                }
                _ => ResolvedType::TypeParam(field_name.to_string()),
            },
            ResolvedType::Primitive(PrimitiveType::IVec2 | PrimitiveType::IVec3 |
PrimitiveType::IVec4) => match field_name {
                "x" | "y" | "z" | "w" => ResolvedType::Primitive(PrimitiveType::I32),
                _ => ResolvedType::TypeParam(field_name.to_string()),
            },
            ResolvedType::Primitive(PrimitiveType::UVec2 | PrimitiveType::UVec3 |
PrimitiveType::UVec4) => match field_name {
                "x" | "y" | "z" | "w" => ResolvedType::Primitive(PrimitiveType::U32),
                _ => ResolvedType::TypeParam(field_name.to_string()),
            },
            // Struct field access
            ResolvedType::Struct(struct_id) => {
                if let Some(struct_def) = self.module.get_struct(*struct_id) {
                    for field in &struct_def.fields {
                        if field.name == field_name {
                            return field.ty.clone();
                        }
                    }
                }
                ResolvedType::TypeParam(field_name.to_string())
            }
            // Default: return a placeholder type
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::EventMapping { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. } => ResolvedType::TypeParam(field_name.to_string()),
        }
    }

    /// Resolve the return type of a method call.
    ///
    /// Handles:
    /// 1. Builtin methods on GPU types (e.g., `vec3.normalize()` -> Vec3)
    /// 2. User-defined methods in impl blocks
    fn resolve_method_return_type(
        &self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> ResolvedType {
        // Try builtin method resolution for primitive types
        if let ResolvedType::Primitive(prim) = receiver_ty {
            if let Some(return_prim) = resolve_method_type(*prim, method_name) {
                return ResolvedType::Primitive(return_prim);
            }
        }

        // Try to find method in impl blocks for struct types
        if let ResolvedType::Struct(struct_id) = receiver_ty {
            for impl_block in &self.module.impls {
                if impl_block.struct_id() == Some(*struct_id) {
                    for func in &impl_block.functions {
                        if func.name == method_name {
                            // Return the function's return type, or the body type if unspecified
                            return func
                                .return_type
                                .clone()
                                .unwrap_or_else(|| func.body.ty().clone());
                        }
                    }
                }
            }
        }

        // Try to find method in impl blocks for enum types
        if let ResolvedType::Enum(enum_id) = receiver_ty {
            for impl_block in &self.module.impls {
                if impl_block.enum_id() == Some(*enum_id) {
                    for func in &impl_block.functions {
                        if func.name == method_name {
                            return func
                                .return_type
                                .clone()
                                .unwrap_or_else(|| func.body.ty().clone());
                        }
                    }
                }
            }
        }

        // Fallback: placeholder type
        ResolvedType::TypeParam(format!("{method_name}Result"))
    }

    /// Resolve the return type of a function call.
    ///
    /// Handles:
    /// 1. User-defined standalone functions in `IrModule::functions`
    /// 2. Builtin functions (math, GPU intrinsics, etc.)
    /// 3. Falls back to void for unknown functions
    fn resolve_function_return_type(
        &self,
        fn_name: &str,
        _args: &[(Option<String>, IrExpr)],
    ) -> ResolvedType {
        // Check if it's a user-defined function
        if let Some(func_id) = self.module.function_id(fn_name) {
            if let Some(func) = self.module.get_function(func_id) {
                // Return the declared return type, or infer from body
                return func
                    .return_type
                    .clone()
                    .unwrap_or_else(|| func.body.ty().clone());
            }
        }

        // Check builtin functions registry
        if let Some(return_ty) = Self::resolve_builtin_function_type(fn_name) {
            return return_ty;
        }

        // Fallback: void type for unknown functions
        ResolvedType::Primitive(PrimitiveType::Never)
    }

    /// Resolve the return type of a builtin function.
    ///
    /// Returns the appropriate type for common builtin/intrinsic functions.
    fn resolve_builtin_function_type(fn_name: &str) -> Option<ResolvedType> {
        use PrimitiveType::{Number, Vec2, Vec3, Vec4, IVec2, IVec3, IVec4, UVec2, UVec3, UVec4, Mat2, Mat3, Mat4, I32, U32, Boolean};

        // Math functions (return same type as input, typically f32)
        let math_float_fns = [
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "sinh",
            "cosh",
            "tanh",
            "exp",
            "exp2",
            "log",
            "log2",
            "sqrt",
            "inverseSqrt",
            "abs",
            "sign",
            "floor",
            "ceil",
            "round",
            "trunc",
            "fract",
            "saturate",
            "radians",
            "degrees",
        ];
        if math_float_fns.contains(&fn_name) {
            return Some(ResolvedType::Primitive(Number));
        }

        // Two-argument math functions
        let math_binary_fns = ["pow", "min", "max", "step", "mod", "atan2"];
        if math_binary_fns.contains(&fn_name) {
            return Some(ResolvedType::Primitive(Number));
        }

        // Vector constructors
        match fn_name {
            "vec2" => return Some(ResolvedType::Primitive(Vec2)),
            "vec3" => return Some(ResolvedType::Primitive(Vec3)),
            "vec4" => return Some(ResolvedType::Primitive(Vec4)),
            "ivec2" => return Some(ResolvedType::Primitive(IVec2)),
            "ivec3" => return Some(ResolvedType::Primitive(IVec3)),
            "ivec4" => return Some(ResolvedType::Primitive(IVec4)),
            "uvec2" => return Some(ResolvedType::Primitive(UVec2)),
            "uvec3" => return Some(ResolvedType::Primitive(UVec3)),
            "uvec4" => return Some(ResolvedType::Primitive(UVec4)),
            "mat2" => return Some(ResolvedType::Primitive(Mat2)),
            "mat3" => return Some(ResolvedType::Primitive(Mat3)),
            "mat4" => return Some(ResolvedType::Primitive(Mat4)),
            _ => {}
        }

        // Type casts
        match fn_name {
            "f32" | "float" => return Some(ResolvedType::Primitive(Number)),
            "i32" | "int" => return Some(ResolvedType::Primitive(I32)),
            "u32" | "uint" => return Some(ResolvedType::Primitive(U32)),
            "bool" => return Some(ResolvedType::Primitive(Boolean)),
            _ => {}
        }

        // Vector operations that return scalars
        match fn_name {
            "length" | "distance" | "dot" => return Some(ResolvedType::Primitive(Number)),
            _ => {}
        }

        // Vector operations that return vectors (input-dependent, approximate as Vec3)
        let vec_to_vec_fns = ["normalize", "cross", "reflect", "refract", "faceforward"];
        if vec_to_vec_fns.contains(&fn_name) {
            return Some(ResolvedType::Primitive(Vec3));
        }

        // Mix/lerp returns same type as input
        if fn_name == "mix" || fn_name == "lerp" || fn_name == "smoothstep" || fn_name == "clamp" {
            return Some(ResolvedType::Primitive(Number));
        }

        None
    }

    /// Lower a closure expression.
    ///
    /// Closures are classified into two types:
    /// 1. Event mappings: 0-1 params, body is enum instantiation → `EventMapping`
    /// 2. General closures: arbitrary params/body → `Closure`
    fn lower_closure(&mut self, params: &[ClosureParam], body: &Expr) -> IrExpr {
        // Check if this is an event mapping (enum body with 0-1 params)
        let is_event_mapping = params.len() <= 1
            && matches!(
                body,
                Expr::EnumInstantiation { .. } | Expr::InferredEnumInstantiation { .. }
            );

        if is_event_mapping {
            return self.lower_event_mapping(params, body);
        }

        // General closure: lower params and body
        let lowered_params: Vec<(String, ResolvedType)> = params
            .iter()
            .map(|p| {
                let ty =
                    p.ty.as_ref()
                        .map_or_else(|| ResolvedType::TypeParam("Unknown".to_string()), |t| self.lower_type(t));
                (p.name.name.clone(), ty)
            })
            .collect();

        let body_ir = self.lower_expr(body);
        let return_ty = body_ir.ty().clone();

        let ty = ResolvedType::Closure {
            param_tys: lowered_params.iter().map(|(_, t)| t.clone()).collect(),
            return_ty: Box::new(return_ty),
        };

        IrExpr::Closure {
            params: lowered_params,
            body: Box::new(body_ir),
            ty,
        }
    }

    /// Lower a closure expression to an event mapping.
    ///
    /// Event mappings are restricted closures that:
    /// - Have zero or one parameter
    /// - Return an enum variant instantiation
    /// - Cannot capture variables from outer scope
    ///
    /// # Examples
    ///
    /// - `() -> .submit` → `EventMapping` with no param, variant "submit"
    /// - `x -> .changed(value: x)` → `EventMapping` with param "x", variant "changed", binding value→x
    fn lower_event_mapping(&mut self, params: &[ClosureParam], body: &Expr) -> IrExpr {
        // Validate: 0 or 1 parameter
        if params.len() > 1 {
            // For now, return a placeholder for invalid event mappings
            return IrExpr::Literal {
                value: Literal::Nil,
                ty: ResolvedType::TypeParam("InvalidEventMapping".to_string()),
            };
        }

        // Extract parameter name and type
        let param = params.first().map(|p| p.name.name.clone());
        let param_ty = params
            .first()
            .and_then(|p| p.ty.as_ref())
            .map(|t| Box::new(self.lower_type(t)));

        // Body must be an enum variant instantiation
        match body {
            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                ..
            } => {
                // Resolve the enum type
                let (enum_id, return_ty) = self.resolve_event_enum_type(&enum_name.name);

                // Extract field bindings - check if they reference the parameter
                let field_bindings = Self::extract_event_field_bindings(data, param.as_deref());

                // Build the event mapping type
                let ty = ResolvedType::EventMapping {
                    param_ty,
                    return_ty: Box::new(return_ty),
                };

                IrExpr::EventMapping {
                    enum_id,
                    variant: variant.name.clone(),
                    param,
                    field_bindings,
                    ty,
                }
            }
            // Inferred enum instantiation: .variant or .variant(field: value)
            Expr::InferredEnumInstantiation { variant, data, .. } => {
                // Extract field bindings
                let field_bindings = Self::extract_event_field_bindings(data, param.as_deref());

                let ty = ResolvedType::EventMapping {
                    param_ty,
                    return_ty: Box::new(ResolvedType::TypeParam("InferredEvent".to_string())),
                };

                IrExpr::EventMapping {
                    enum_id: None,
                    variant: variant.name.clone(),
                    param,
                    field_bindings,
                    ty,
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => {
                // Invalid: body is not an enum variant
                IrExpr::Literal {
                    value: Literal::Nil,
                    ty: ResolvedType::TypeParam("InvalidEventMapping".to_string()),
                }
            }
        }
    }

    /// Resolve enum type for event mapping, returning (`enum_id`, `resolved_type`).
    fn resolve_event_enum_type(&self, enum_name: &str) -> (Option<super::EnumId>, ResolvedType) {
        self.module.enum_id(enum_name).map_or_else(
            || (None, ResolvedType::TypeParam(enum_name.to_string())),
            |enum_id| (Some(enum_id), ResolvedType::Enum(enum_id)),
        )
    }

    /// Extract field bindings from enum variant fields.
    ///
    /// Checks if field values reference the event mapping parameter.
    fn extract_event_field_bindings(
        fields: &[(ast::Ident, Expr)],
        param_name: Option<&str>,
    ) -> Vec<EventFieldBinding> {
        fields
            .iter()
            .map(|(field_name, value)| {
                let source = match value {
                    // Field references the parameter: `value: x`
                    // path[0] is bounds-safe: guarded by path.len() == 1 in the match guard
                    #[expect(clippy::indexing_slicing, reason = "len == 1 guard above guarantees index 0")]
                    Expr::Reference { path, .. }
                        if path.len() == 1 && param_name.is_some_and(|p| path[0].name == p) =>
                    {
                        EventBindingSource::Param(path[0].name.clone())
                    }
                    // Field has a literal value: `value: 42`
                    Expr::Literal(lit) => EventBindingSource::Literal(lit.clone()),
                    // For other expressions, treat as referencing param (best effort)
                    Expr::Invocation { .. }
                    | Expr::EnumInstantiation { .. }
                    | Expr::InferredEnumInstantiation { .. }
                    | Expr::Array { .. }
                    | Expr::Tuple { .. }
                    | Expr::Reference { .. }
                    | Expr::BinaryOp { .. }
                    | Expr::UnaryOp { .. }
                    | Expr::ForExpr { .. }
                    | Expr::IfExpr { .. }
                    | Expr::MatchExpr { .. }
                    | Expr::Group { .. }
                    | Expr::DictLiteral { .. }
                    | Expr::DictAccess { .. }
                    | Expr::FieldAccess { .. }
                    | Expr::ClosureExpr { .. }
                    | Expr::LetExpr { .. }
                    | Expr::MethodCall { .. }
                    | Expr::Block { .. } => {
                        param_name.map_or(
                            EventBindingSource::Literal(Literal::Nil),
                            |p| EventBindingSource::Param(p.to_string()),
                        )
                    }
                };

                EventFieldBinding {
                    field_name: field_name.name.clone(),
                    source,
                }
            })
            .collect()
    }

    fn string_to_resolved_type(&self, type_str: &str) -> ResolvedType {
        match type_str {
            "String" => ResolvedType::Primitive(PrimitiveType::String),
            "Number" => ResolvedType::Primitive(PrimitiveType::Number),
            "Boolean" => ResolvedType::Primitive(PrimitiveType::Boolean),
            "Path" => ResolvedType::Primitive(PrimitiveType::Path),
            "Regex" => ResolvedType::Primitive(PrimitiveType::Regex),
            name => self.module.struct_id(name).map_or_else(
                || {
                    self.module.enum_id(name).map_or_else(
                        || {
                            self.module
                                .trait_id(name)
                                .map_or_else(|| ResolvedType::TypeParam(name.to_string()), ResolvedType::Trait)
                        },
                        ResolvedType::Enum,
                    )
                },
                ResolvedType::Struct,
            ),
        }
    }

    fn extract_pattern_bindings(
        &self,
        pattern: &ast::Pattern,
        scrutinee: &IrExpr,
    ) -> Vec<(String, ResolvedType)> {
        match pattern {
            ast::Pattern::Variant { name, bindings } => {
                // Try to find variant field types from the enum
                let variant_fields = self.get_variant_fields(scrutinee.ty(), &name.name);

                bindings
                    .iter()
                    .enumerate()
                    .map(|(i, ident)| {
                        let ty = variant_fields
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| ResolvedType::TypeParam("Unknown".to_string()));
                        (ident.name.clone(), ty)
                    })
                    .collect()
            }
            ast::Pattern::Wildcard => {
                // Wildcard has no bindings
                Vec::new()
            }
        }
    }

    fn get_variant_fields(&self, enum_ty: &ResolvedType, variant_name: &str) -> Vec<ResolvedType> {
        // Handle direct enum type
        if let ResolvedType::Enum(id) = enum_ty {
            if let Some(enum_def) = self.module.get_enum(*id) {
                if let Some(variant) = enum_def.variants.iter().find(|v| v.name == variant_name) {
                    return variant.fields.iter().map(|f| f.ty.clone()).collect();
                }
            }
        }
        // Handle TypeParam("self") in impl context - resolve to actual enum type
        if let ResolvedType::TypeParam(name) = enum_ty {
            if name == "self" {
                if let Some(ref impl_name) = self.current_impl_struct {
                    if let Some(id) = self.module.enum_id(impl_name) {
                        if let Some(enum_def) = self.module.get_enum(id) {
                            if let Some(variant) =
                                enum_def.variants.iter().find(|v| v.name == variant_name)
                            {
                                return variant.fields.iter().map(|f| f.ty.clone()).collect();
                            }
                        }
                    }
                }
            }
        }
        Vec::new()
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
