//! Type lowering and type-resolution helpers for the IR lowering pass.

mod prelude;
mod resolve;

use super::IrLowerer;
use crate::ast::{self, GenericConstraint, StructField, Type};
use crate::error::CompilerError;
use crate::ir::{simple_type_name, IrField, IrGenericParam, ResolvedType};

impl IrLowerer<'_> {
    /// Extract the type name from an AST type (for return type context)
    pub(in crate::ir::lower) fn type_name(ty: &ast::Type) -> String {
        match ty {
            ast::Type::Primitive(prim) => match prim {
                ast::PrimitiveType::String => "String".to_string(),
                ast::PrimitiveType::I32 => "I32".to_string(),
                ast::PrimitiveType::I64 => "I64".to_string(),
                ast::PrimitiveType::F32 => "F32".to_string(),
                ast::PrimitiveType::F64 => "F64".to_string(),
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

    pub(in crate::ir::lower) fn lower_generic_params(
        &mut self,
        params: &[ast::GenericParam],
    ) -> Vec<IrGenericParam> {
        // Phase C: each constraint becomes an IrTraitRef carrying
        // both the trait id and any generic-trait args
        // (`<T: Container<I32>>`). Arg lowering goes through
        // `lower_type`, which is why this method now needs `&mut self`.
        params
            .iter()
            .map(|p| {
                let constraints: Vec<crate::ir::IrTraitRef> = p
                    .constraints
                    .iter()
                    .filter_map(|c| match c {
                        GenericConstraint::Trait { name, args } => {
                            self.module.trait_id(&name.name).map(|trait_id| {
                                let lowered_args: Vec<ResolvedType> =
                                    args.iter().map(|t| self.lower_type(t)).collect();
                                crate::ir::IrTraitRef {
                                    trait_id,
                                    args: lowered_args,
                                }
                            })
                        }
                    })
                    .collect();
                IrGenericParam {
                    name: p.name.name.clone(),
                    constraints,
                }
            })
            .collect()
    }

    pub(in crate::ir::lower) fn lower_field_def(&mut self, f: &ast::FieldDef) -> IrField {
        let optional = matches!(f.ty, ast::Type::Optional(_));
        IrField {
            name: f.name.name.clone(),
            ty: self.lower_type(&f.ty),
            mutable: f.mutable,
            optional,
            default: None,
            doc: f.doc.clone(),
            convention: ast::ParamConvention::default(),
            span: self.current_ir_span(),
        }
    }

    pub(in crate::ir::lower) fn lower_struct_field(&mut self, f: &StructField) -> IrField {
        // thread the field's declared type as the
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
            convention: ast::ParamConvention::default(),
            span: self.current_ir_span(),
        }
    }

    pub(in crate::ir::lower) fn lower_type(&mut self, ty: &Type) -> ResolvedType {
        match ty {
            Type::Primitive(p) => ResolvedType::Primitive(*p),

            Type::Ident(ident) => {
                let name = &ident.name;

                // For path-qualified names like `geom::Point`, the IR's
                // symbol table registers the type under the fully
                // qualified name. Try the full name first; fall back to
                // the last segment so single-name references and primitive-
                // name lookups still work.
                let lookup_name = simple_type_name(name);

                // Check if this is an external type
                if let Some(external) = self.try_external_type(lookup_name, vec![]) {
                    return external;
                }
                // Otherwise try local types; qualified name first, then
                // the simple name.
                if let Some(id) = self.module.struct_id(name) {
                    ResolvedType::Struct(id)
                } else if let Some(id) = self.module.trait_id(name) {
                    ResolvedType::Trait(id)
                } else if let Some(id) = self.module.enum_id(name) {
                    ResolvedType::Enum(id)
                } else if let Some(id) = self.module.struct_id(lookup_name) {
                    ResolvedType::Struct(id)
                } else if let Some(id) = self.module.trait_id(lookup_name) {
                    ResolvedType::Trait(id)
                } else if let Some(id) = self.module.enum_id(lookup_name) {
                    ResolvedType::Enum(id)
                } else if self.is_generic_param_in_scope(name) {
                    ResolvedType::TypeParam(name.clone())
                } else if let Some(path) = self.imported_source_context.clone() {
                    // CM gap: when register_imported_types is lowering an
                    // imported struct/enum's field types, an unresolved
                    // identifier most likely names a sibling type from the
                    // same source module that the entry didn't import.
                    // Default to External(<source>, name) so the
                    // MonomorphisePass can pull it in via Phase 1a.
                    ResolvedType::External {
                        module_path: path,
                        name: name.clone(),
                        kind: crate::ir::ImportedKind::Struct,
                        type_args: Vec::new(),
                    }
                } else {
                    // surface unresolved type names loudly
                    // instead of silently lowering to `TypeParam(name)`.
                    // Semantic should normally catch this; reaching here
                    // means a typo, an unimported type, or an out-of-
                    // scope generic param.
                    self.errors.push(CompilerError::UndefinedType {
                        name: name.clone(),
                        span: ident.span,
                    });
                    ResolvedType::Error
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
                if let Some(path) = self.imported_source_context.clone() {
                    return ResolvedType::External {
                        module_path: path,
                        name: name.name.clone(),
                        kind: crate::ir::ImportedKind::Struct,
                        type_args,
                    };
                }
                self.errors.push(CompilerError::UndefinedType {
                    name: name.name.clone(),
                    span: name.span,
                });
                ResolvedType::Error
            }

            Type::Array(inner) => {
                let elem = self.lower_type(inner);
                self.array_of(elem).unwrap_or(ResolvedType::Error)
            }

            Type::Optional(inner) => {
                let elem = self.lower_type(inner);
                self.optional_of(elem).unwrap_or(ResolvedType::Error)
            }

            Type::Tuple(fields) => ResolvedType::Tuple(
                fields
                    .iter()
                    .map(|f| (f.name.name.clone(), self.lower_type(&f.ty)))
                    .collect(),
            ),

            Type::Dictionary { key, value } => {
                let k = self.lower_type(key);
                let v = self.lower_type(value);
                self.dictionary_of(k, v).unwrap_or(ResolvedType::Error)
            }

            Type::Closure { params, ret } => ResolvedType::Closure {
                param_tys: params
                    .iter()
                    .map(|(c, p)| (*c, self.lower_type(p)))
                    .collect(),
                return_ty: Box::new(self.lower_type(ret)),
            },
        }
    }
}
