//! IR lowering pass: AST + SymbolTable → IrModule

use crate::ast::{
    self, BinaryOperator, Definition, EnumDef, Expr, File, GenericConstraint, ImplDef, Literal,
    PrimitiveType, Statement, StructDef, StructField, TraitDef, Type,
};
use crate::error::CompilerError;
use crate::semantic::symbol_table::SymbolTable;

use super::{
    EnumId, ExternalKind, IrEnum, IrEnumVariant, IrExpr, IrField, IrGenericParam, IrImpl, IrImport,
    IrImportItem, IrMatchArm, IrModule, IrStruct, IrTrait, ResolvedType, StructId, TraitId,
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
}

impl<'a> IrLowerer<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            module: IrModule::new(),
            symbols,
            errors: Vec::new(),
            imports_by_module: HashMap::new(),
        }
    }

    fn lower_file(&mut self, file: &File) -> Result<(), Vec<CompilerError>> {
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

        let defaults: Vec<(String, IrExpr)> = i
            .defaults
            .iter()
            .map(|(name, expr)| (name.name.clone(), self.lower_expr(expr)))
            .collect();

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
                let struct_id = self
                    .module
                    .struct_id(&name.name)
                    .unwrap_or(StructId(u32::MAX));

                let type_args_resolved: Vec<ResolvedType> =
                    type_args.iter().map(|t| self.lower_type(t)).collect();

                let ty = if type_args_resolved.is_empty() {
                    ResolvedType::Struct(struct_id)
                } else {
                    ResolvedType::Generic {
                        base: struct_id,
                        args: type_args_resolved.clone(),
                    }
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
                let enum_id = self
                    .module
                    .enum_id(&enum_name.name)
                    .unwrap_or(EnumId(u32::MAX));

                IrExpr::EnumInst {
                    enum_id,
                    variant: variant.name.clone(),
                    fields: data
                        .iter()
                        .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                        .collect(),
                    ty: ResolvedType::Enum(enum_id),
                }
            }

            Expr::InferredEnumInstantiation { variant, data, .. } => {
                // For inferred enums, we'd need context to resolve the enum type
                // For now, use a placeholder
                IrExpr::EnumInst {
                    enum_id: EnumId(u32::MAX),
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

                // Try to resolve the type from the symbol table
                let ty = if path_strs.len() == 1 {
                    let name = &path_strs[0];
                    if let Some(let_type) = self.symbols.get_let_type(name) {
                        self.string_to_resolved_type(let_type)
                    } else {
                        ResolvedType::TypeParam(name.clone())
                    }
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
