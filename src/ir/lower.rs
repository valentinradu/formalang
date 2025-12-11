//! IR lowering pass: AST + SymbolTable → IrModule

use crate::ast::{
    self, BinaryOperator, BindingPattern, Definition, EnumDef, Expr, File, GenericConstraint,
    ImplDef, LetBinding, Literal, PrimitiveType, Statement, StructDef, StructField, TraitDef, Type,
};
use crate::error::CompilerError;
use crate::semantic::symbol_table::SymbolTable;

use super::{
    ExternalKind, IrEnum, IrEnumVariant, IrExpr, IrField, IrGenericParam, IrImpl, IrImport,
    IrImportItem, IrLet, IrMatchArm, IrModule, IrStruct, IrTrait, ResolvedType, TraitId,
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
    /// Track imports by module path for aggregation
    imports_by_module: HashMap<Vec<String>, Vec<IrImportItem>>,
    /// Current struct being processed in an impl block (for self references)
    current_impl_struct: Option<String>,
}

impl<'a> IrLowerer<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            module: IrModule::new(),
            symbols,
            errors: Vec::new(),
            imports_by_module: HashMap::new(),
            current_impl_struct: None,
        }
    }

    fn lower_file(&mut self, file: &File) -> Result<(), Vec<CompilerError>> {
        // Pre-pass: register imported structs and enums so they have IDs
        self.register_imported_types();

        // First pass: register all definitions to get IDs
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                self.register_definition(def);
            }
        }

        // Second pass: lower all definitions with resolved types
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                self.lower_definition(def);
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
            .map(|(module_path, items)| IrImport { module_path, items })
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
                    _ => ResolvedType::TypeParam("Unknown".to_string()),
                };

                for (i, element) in elements.iter().enumerate() {
                    if let Some(name) = self.extract_binding_name(element) {
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
                        .as_ref()
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| field_name.clone());

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
                    _ => Vec::new(),
                };

                for (i, element) in elements.iter().enumerate() {
                    if let Some(name) = self.extract_simple_binding_name(element) {
                        let ty = tuple_types
                            .get(i)
                            .map(|(_, t)| t.clone())
                            .unwrap_or_else(|| ResolvedType::TypeParam("Unknown".to_string()));

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
    fn extract_binding_name(&self, element: &ast::ArrayPatternElement) -> Option<String> {
        match element {
            ast::ArrayPatternElement::Binding(pattern) => self.extract_simple_binding_name(pattern),
            ast::ArrayPatternElement::Rest(Some(ident)) => Some(ident.name.clone()),
            ast::ArrayPatternElement::Rest(None) | ast::ArrayPatternElement::Wildcard => None,
        }
    }

    /// Extract binding name from a simple binding pattern
    fn extract_simple_binding_name(&self, pattern: &BindingPattern) -> Option<String> {
        match pattern {
            BindingPattern::Simple(ident) => Some(ident.name.clone()),
            _ => None,
        }
    }

    /// Get field type from a resolved type
    fn get_field_type_from_resolved(&self, ty: &ResolvedType, field_name: &str) -> ResolvedType {
        if let ResolvedType::Struct(id) = ty {
            let struct_def = self.module.get_struct(*id);
            if let Some(field) = struct_def.fields.iter().find(|f| f.name == field_name) {
                return field.ty.clone();
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
        ResolvedType::TypeParam(format!("self.{}", field_name))
    }

    /// Register imported structs and enums from the symbol table.
    /// This ensures that imported types have struct/enum IDs in the IR module,
    /// so when we instantiate them, struct_id is populated correctly.
    fn register_imported_types(&mut self) {
        // Register imported structs (top-level)
        for (name, struct_info) in &self.symbols.structs {
            // Check if this is an imported symbol
            if self.symbols.get_module_origin(name).is_some() {
                self.register_struct(name, struct_info);
            }
        }

        // Register imported enums (top-level)
        for (name, enum_info) in &self.symbols.enums {
            // Check if this is an imported symbol
            if self.symbols.get_module_origin(name).is_some() {
                self.register_enum(name, enum_info);
            }
        }

        // Register types from imported nested modules (e.g., fill::Solid)
        for (module_name, module_info) in &self.symbols.modules {
            self.register_module_types(module_name, &module_info.symbols);
        }
    }

    /// Register types from a nested module recursively
    fn register_module_types(&mut self, module_prefix: &str, module_symbols: &SymbolTable) {
        // Register structs from this module
        for (name, struct_info) in &module_symbols.structs {
            let qualified_name = format!("{}::{}", module_prefix, name);
            self.register_struct(&qualified_name, struct_info);
        }

        // Register enums from this module
        for (name, enum_info) in &module_symbols.enums {
            let qualified_name = format!("{}::{}", module_prefix, name);
            self.register_enum(&qualified_name, enum_info);
        }

        // Recursively register nested modules
        for (nested_name, nested_module_info) in &module_symbols.modules {
            let nested_prefix = format!("{}::{}", module_prefix, nested_name);
            self.register_module_types(&nested_prefix, &nested_module_info.symbols);
        }
    }

    /// Helper method to register an enum
    /// Note: EnumInfo doesn't preserve variant field details, so we create placeholder variants
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

        self.module.add_enum(
            name.to_string(),
            IrEnum {
                name: name.to_string(),
                visibility: enum_info.visibility,
                variants,
                generic_params,
            },
        );
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
        let traits: Vec<TraitId> = struct_info
            .traits
            .iter()
            .filter_map(|trait_name| self.module.trait_id(trait_name))
            .collect();

        // Convert generic params
        let generic_params = self.lower_generic_params(&struct_info.generics);

        self.module.add_struct(
            name.to_string(),
            IrStruct {
                name: name.to_string(),
                visibility: struct_info.visibility,
                traits,
                fields,
                mount_fields,
                generic_params,
            },
        );
    }

    /// First pass: register definitions to allocate IDs
    fn register_definition(&mut self, def: &Definition) {
        match def {
            Definition::Trait(t) => {
                let name = t.name.name.clone();
                // Create placeholder, will be filled in second pass
                self.module.add_trait(
                    name,
                    IrTrait {
                        name: t.name.name.clone(),
                        visibility: t.visibility,
                        composed_traits: Vec::new(),
                        fields: Vec::new(),
                        mount_fields: Vec::new(),
                        generic_params: Vec::new(),
                    },
                );
            }
            Definition::Struct(s) => {
                let name = s.name.name.clone();
                self.module.add_struct(
                    name,
                    IrStruct {
                        name: s.name.name.clone(),
                        visibility: s.visibility,
                        traits: Vec::new(),
                        fields: Vec::new(),
                        mount_fields: Vec::new(),
                        generic_params: Vec::new(),
                    },
                );
            }
            Definition::Enum(e) => {
                let name = e.name.name.clone();
                self.module.add_enum(
                    name,
                    IrEnum {
                        name: e.name.name.clone(),
                        visibility: e.visibility,
                        variants: Vec::new(),
                        generic_params: Vec::new(),
                    },
                );
            }
            Definition::Impl(_) | Definition::Module(_) => {
                // Impls are processed after structs
                // Modules are flattened (nested definitions registered recursively)
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
            Definition::Module(_) => {
                // TODO: Handle nested modules
            }
        }
    }

    fn lower_trait(&mut self, t: &TraitDef) {
        let id = self
            .module
            .trait_id(&t.name.name)
            .expect("trait should be registered");

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
        let trait_def = &mut self.module.traits[id.0 as usize];
        trait_def.composed_traits = composed_traits;
        trait_def.fields = fields;
        trait_def.mount_fields = mount_fields;
        trait_def.generic_params = generic_params;
    }

    fn lower_struct(&mut self, s: &StructDef) {
        let id = self
            .module
            .struct_id(&s.name.name)
            .expect("struct should be registered");

        let traits: Vec<TraitId> = s
            .traits
            .iter()
            .filter_map(|ident| {
                // Check if this is an external trait and track the import
                self.try_track_external_import(&ident.name, ExternalKind::Trait);
                self.module.trait_id(&ident.name)
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
        let struct_def = &mut self.module.structs[id.0 as usize];
        struct_def.traits = traits;
        struct_def.fields = fields;
        struct_def.mount_fields = mount_fields;
        struct_def.generic_params = generic_params;
    }

    fn lower_enum(&mut self, e: &EnumDef) {
        let id = self
            .module
            .enum_id(&e.name.name)
            .expect("enum should be registered");

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
        let enum_def = &mut self.module.enums[id.0 as usize];
        enum_def.variants = variants;
        enum_def.generic_params = generic_params;
    }

    fn lower_impl(&mut self, i: &ImplDef) {
        let struct_id = match self.module.struct_id(&i.name.name) {
            Some(id) => id,
            None => return, // Error would have been caught in semantic analysis
        };

        // Set current impl struct for self reference resolution
        self.current_impl_struct = Some(i.name.name.clone());

        let defaults: Vec<(String, IrExpr)> = i
            .defaults
            .iter()
            .map(|(name, expr)| (name.name.clone(), self.lower_expr(expr)))
            .collect();

        // Clear the context
        self.current_impl_struct = None;

        self.module.add_impl(IrImpl {
            struct_id,
            defaults,
        });
    }

    fn lower_generic_params(&mut self, params: &[ast::GenericParam]) -> Vec<IrGenericParam> {
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
                // Check if this is an external type
                if let Some(external) = self.try_external_type(name, vec![]) {
                    return external;
                }
                // Otherwise try local types
                if let Some(id) = self.module.struct_id(name) {
                    ResolvedType::Struct(id)
                } else if let Some(id) = self.module.trait_id(name) {
                    ResolvedType::Trait(id)
                } else if let Some(id) = self.module.enum_id(name) {
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
                if let Some(base) = self.module.struct_id(&name.name) {
                    ResolvedType::Generic {
                        base,
                        args: type_args,
                    }
                } else {
                    // Fallback to type param if not found
                    ResolvedType::TypeParam(name.name.clone())
                }
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

            Type::Dictionary { .. } | Type::Closure { .. } => {
                // TODO: Add dictionary and closure types to IR
                ResolvedType::TypeParam("UnsupportedType".to_string())
            }
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

            self.imports_by_module
                .entry(module_path.clone())
                .or_default()
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
            _ => return None,
        };

        // Track this import
        let import_item = IrImportItem {
            name: name.to_string(),
            kind: external_kind.clone(),
        };

        self.imports_by_module
            .entry(module_path.clone())
            .or_default()
            .push(import_item);

        Some(ResolvedType::External {
            module_path: module_path.clone(),
            name: name.to_string(),
            kind: external_kind,
            type_args,
        })
    }

    fn lower_expr(&mut self, expr: &Expr) -> IrExpr {
        match expr {
            Expr::Literal(lit) => IrExpr::Literal {
                value: lit.clone(),
                ty: self.literal_type(lit),
            },

            Expr::StructInstantiation {
                name,
                type_args,
                args,
                mounts,
                ..
            } => {
                let type_args_resolved: Vec<ResolvedType> =
                    type_args.iter().map(|t| self.lower_type(t)).collect();

                // Check if we have a struct ID (local or imported)
                let (struct_id, ty) = if let Some(id) = self.module.struct_id(&name.name) {
                    // Local or imported struct with ID
                    let ty = if type_args_resolved.is_empty() {
                        ResolvedType::Struct(id)
                    } else {
                        ResolvedType::Generic {
                            base: id,
                            args: type_args_resolved.clone(),
                        }
                    };
                    (Some(id), ty)
                } else if let Some(external_ty) =
                    self.try_external_type(&name.name, type_args_resolved.clone())
                {
                    // External struct from unregistered module - no valid ID
                    (None, external_ty)
                } else {
                    // Unknown struct - this shouldn't happen after semantic analysis
                    // but handle gracefully
                    (None, ResolvedType::TypeParam(name.name.clone()))
                };

                IrExpr::StructInst {
                    struct_id,
                    type_args: type_args_resolved,
                    fields: args
                        .iter()
                        .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                        .collect(),
                    mounts: mounts
                        .iter()
                        .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                        .collect(),
                    ty,
                }
            }

            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                ..
            } => {
                // Check if we have an enum ID (local or imported)
                let (enum_id, ty) = if let Some(id) = self.module.enum_id(&enum_name.name) {
                    // Local or imported enum with ID
                    (Some(id), ResolvedType::Enum(id))
                } else if let Some(external_ty) = self.try_external_type(&enum_name.name, vec![]) {
                    // External enum from unregistered module - no valid ID
                    (None, external_ty)
                } else {
                    // Unknown enum - this shouldn't happen after semantic analysis
                    (None, ResolvedType::TypeParam(enum_name.name.clone()))
                };

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
                // For inferred enums, we'd need context to resolve the enum type
                // For now, use a placeholder
                IrExpr::EnumInst {
                    enum_id: None,
                    variant: variant.name.clone(),
                    fields: data
                        .iter()
                        .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                        .collect(),
                    ty: ResolvedType::TypeParam("InferredEnum".to_string()),
                }
            }

            Expr::Array { elements, .. } => {
                let lowered: Vec<IrExpr> = elements.iter().map(|e| self.lower_expr(e)).collect();
                let elem_ty = lowered
                    .first()
                    .map(|e| e.ty().clone())
                    .unwrap_or_else(|| ResolvedType::TypeParam("UnknownElement".to_string()));

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

                // Check for self.field pattern
                if path_strs.len() == 2 && path_strs[0] == "self" {
                    let field_name = &path_strs[1];
                    let ty = self.resolve_self_field_type(field_name);
                    return IrExpr::SelfFieldRef {
                        field: field_name.clone(),
                        ty,
                    };
                }

                // Check for module-level let binding reference
                if path_strs.len() == 1 {
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
                    ResolvedType::TypeParam(path_strs[0].clone())
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
                    _ => left_ir.ty().clone(),
                };

                IrExpr::BinaryOp {
                    left: Box::new(left_ir),
                    op: *op,
                    right: Box::new(right_ir),
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
                    _ => ResolvedType::TypeParam("UnknownElement".to_string()),
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
                            },
                            bindings,
                            body: self.lower_expr(&arm.body),
                        }
                    })
                    .collect();

                let ty = arms_ir
                    .first()
                    .map(|a| a.body.ty().clone())
                    .unwrap_or_else(|| ResolvedType::TypeParam("Unknown".to_string()));

                IrExpr::Match {
                    scrutinee: Box::new(scrutinee_ir),
                    arms: arms_ir,
                    ty,
                }
            }

            Expr::Group { expr, .. } => self.lower_expr(expr),

            // TODO: Handle these expression types
            Expr::ProvidesExpr { body, .. }
            | Expr::ConsumesExpr { body, .. }
            | Expr::LetExpr { body, .. } => self.lower_expr(body),

            Expr::DictLiteral { .. } | Expr::DictAccess { .. } | Expr::ClosureExpr { .. } => {
                // Return a placeholder for unsupported expressions
                IrExpr::Literal {
                    value: Literal::Nil,
                    ty: ResolvedType::TypeParam("UnsupportedExpr".to_string()),
                }
            }
        }
    }

    fn literal_type(&self, lit: &Literal) -> ResolvedType {
        match lit {
            Literal::String(_) => ResolvedType::Primitive(PrimitiveType::String),
            Literal::Number(_) => ResolvedType::Primitive(PrimitiveType::Number),
            Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
            Literal::Path(_) => ResolvedType::Primitive(PrimitiveType::Path),
            Literal::Regex { .. } => ResolvedType::Primitive(PrimitiveType::Regex),
            Literal::Nil => ResolvedType::TypeParam("Nil".to_string()),
        }
    }

    fn string_to_resolved_type(&self, type_str: &str) -> ResolvedType {
        match type_str {
            "String" => ResolvedType::Primitive(PrimitiveType::String),
            "Number" => ResolvedType::Primitive(PrimitiveType::Number),
            "Boolean" => ResolvedType::Primitive(PrimitiveType::Boolean),
            "Path" => ResolvedType::Primitive(PrimitiveType::Path),
            "Regex" => ResolvedType::Primitive(PrimitiveType::Regex),
            name => {
                if let Some(id) = self.module.struct_id(name) {
                    ResolvedType::Struct(id)
                } else if let Some(id) = self.module.enum_id(name) {
                    ResolvedType::Enum(id)
                } else if let Some(id) = self.module.trait_id(name) {
                    ResolvedType::Trait(id)
                } else {
                    ResolvedType::TypeParam(name.to_string())
                }
            }
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
        }
    }

    fn get_variant_fields(&self, enum_ty: &ResolvedType, variant_name: &str) -> Vec<ResolvedType> {
        if let ResolvedType::Enum(id) = enum_ty {
            let enum_def = self.module.get_enum(*id);
            if let Some(variant) = enum_def.variants.iter().find(|v| v.name == variant_name) {
                return variant.fields.iter().map(|f| f.ty.clone()).collect();
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lower_empty_file() {
        let ast = File {
            statements: vec![],
            span: crate::location::Span::default(),
        };
        let symbols = SymbolTable::new();
        let result = lower_to_ir(&ast, &symbols);
        assert!(result.is_ok());
        let module = result.unwrap();
        assert!(module.structs.is_empty());
        assert!(module.traits.is_empty());
        assert!(module.enums.is_empty());
    }
}
