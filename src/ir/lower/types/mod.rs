//! Type lowering and type-resolution helpers for the IR lowering pass.

mod prelude;
mod resolve;

use super::IrLowerer;
use crate::ast::{self, GenericConstraint, StructField, Type};
use crate::error::CompilerError;
use crate::ir::{simple_type_name, IrField, IrGenericParam, ResolvedType};

impl IrLowerer<'_> {
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
                            let scoped = self.scoped_type_name(&name.name);
                            self.module.trait_id(&scoped).map(|trait_id| {
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
        // The field's declared type is the expected type for its
        // default, so a `.variant` literal inside resolves against the
        // field's enum.
        let field_ty = self.lower_type(&f.ty);
        let default = f
            .default
            .as_ref()
            .map(|e| self.lower_with_expected_value(e, Some(&field_ty)));
        IrField {
            name: f.name.name.clone(),
            ty: field_ty,
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
                // A short name in an inline `mod` means that module's type.
                let name = &self.scoped_type_name(&ident.name);

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
                let scoped = self.scoped_type_name(&name.name);
                // Local generic struct
                if let Some(id) = self.module.struct_id(&scoped) {
                    return ResolvedType::Generic {
                        base: crate::ir::GenericBase::Struct(id),
                        args: type_args,
                    };
                }
                // Local generic enum
                if let Some(id) = self.module.enum_id(&scoped) {
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
