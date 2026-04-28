//! Type lowering and type-resolution helpers for the IR lowering pass.

use super::IrLowerer;
use crate::ast::{self, GenericConstraint, PrimitiveType, StructField, Type};
use crate::error::CompilerError;
use crate::ir::{simple_type_name, IrField, IrGenericParam, ResolvedType};

/// A "simple" type name is a bare identifier (struct / enum / trait /
/// generic param). Composite stringifications produced by semantic's
/// `type_to_string` (`[T]`, `T?`, `T -> U`, `(a: T)`, `[K: V]`) contain
/// punctuation that disqualifies them.
fn is_simple_type_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
}

impl IrLowerer<'_> {
    /// Best-effort conversion of a stringified type from the symbol
    /// table into a `ResolvedType`.
    ///
    /// Semantic stores let / inference types via `type_to_string`, which
    /// produces full type expressions like `[I32]`, `String -> String`,
    /// or `(x: I32, y: I32)`. This helper only handles the
    /// *simple* name cases (primitives, named structs/enums/traits, in-
    /// scope generic params); for anything composite it returns `None`
    /// so callers can fall back to the value's already-lowered type.
    /// an unrecognised *simple identifier* now surfaces
    /// as `UndefinedType` rather than silently lowering to
    /// `TypeParam(name)`.
    pub(super) fn string_to_resolved_type(&mut self, type_str: &str) -> Option<ResolvedType> {
        match type_str {
            "String" => Some(ResolvedType::Primitive(PrimitiveType::String)),
            "I32" => Some(ResolvedType::Primitive(PrimitiveType::I32)),
            "I64" => Some(ResolvedType::Primitive(PrimitiveType::I64)),
            "F32" => Some(ResolvedType::Primitive(PrimitiveType::F32)),
            "F64" => Some(ResolvedType::Primitive(PrimitiveType::F64)),
            "Boolean" => Some(ResolvedType::Primitive(PrimitiveType::Boolean)),
            "Path" => Some(ResolvedType::Primitive(PrimitiveType::Path)),
            "Regex" => Some(ResolvedType::Primitive(PrimitiveType::Regex)),
            "Never" => Some(ResolvedType::Primitive(PrimitiveType::Never)),
            // Inference's stringified marker for the `nil` literal —
            // matches the IR representation in `lower_literal`.
            "Nil" => Some(ResolvedType::Optional(Box::new(ResolvedType::Primitive(
                PrimitiveType::Never,
            )))),
            name if is_simple_type_name(name) => {
                if let Some(id) = self.module.struct_id(name) {
                    Some(ResolvedType::Struct(id))
                } else if let Some(id) = self.module.enum_id(name) {
                    Some(ResolvedType::Enum(id))
                } else if let Some(id) = self.module.trait_id(name) {
                    Some(ResolvedType::Trait(id))
                } else if self.is_generic_param_in_scope(name) {
                    Some(ResolvedType::TypeParam(name.to_string()))
                } else {
                    self.errors
                        .push(crate::error::CompilerError::UndefinedType {
                            name: name.to_string(),
                            span: self.current_span,
                        });
                    Some(ResolvedType::Error)
                }
            }
            // Composite stringification (e.g. "[T]", "T?", "(a: T)",
            // "T -> U", "[K: V]"). The lowerer doesn't reparse these —
            // callers fall back to the value expression's resolved type.
            _ => None,
        }
    }

    /// Get field type from a resolved type.
    pub(super) fn get_field_type_from_resolved(
        &mut self,
        ty: &ResolvedType,
        field_name: &str,
    ) -> ResolvedType {
        if let ResolvedType::Struct(id) = ty {
            if let Some(struct_def) = self.module.get_struct(*id) {
                if let Some(field) = struct_def.fields.iter().find(|f| f.name == field_name) {
                    return field.ty.clone();
                }
            }
        }
        let bad = ty.clone();
        self.internal_error_type_if_concrete(
            &bad,
            format!(
                "get_field_type_from_resolved: no field `{field_name}` on type {bad:?}; semantic should have caught this"
            ),
        )
    }

    /// Get the field types of a specific variant from an enum type.
    ///
    /// Handles direct `Enum(id)`, a `Generic` whose base is an enum (so a
    /// match over e.g. `Option<T>` still finds its variants), and the
    /// `TypeParam("self")` impl-context fallback.
    pub(super) fn get_variant_fields(
        &self,
        enum_ty: &ResolvedType,
        variant_name: &str,
    ) -> Vec<ResolvedType> {
        let enum_id = match enum_ty {
            ResolvedType::Enum(id) => Some(*id),
            ResolvedType::Generic { base, .. } => match base {
                crate::ir::GenericBase::Enum(id) => Some(*id),
                crate::ir::GenericBase::Struct(_) | crate::ir::GenericBase::Trait(_) => None,
            },
            ResolvedType::TypeParam(name) if name == "self" => self
                .current_impl_struct
                .as_ref()
                .and_then(|impl_name| self.module.enum_id(impl_name)),
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Array(_)
            | ResolvedType::Range(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::External { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::Error => None,
        };
        if let Some(id) = enum_id {
            if let Some(enum_def) = self.module.get_enum(id) {
                if let Some(variant) = enum_def.variants.iter().find(|v| v.name == variant_name) {
                    return variant.fields.iter().map(|f| f.ty.clone()).collect();
                }
            }
        }
        Vec::new()
    }

    /// Extract the type name from an AST type (for return type context)
    pub(super) fn type_name(ty: &ast::Type) -> String {
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

    pub(super) fn lower_generic_params(
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

    pub(super) fn lower_field_def(&mut self, f: &ast::FieldDef) -> IrField {
        let optional = matches!(f.ty, ast::Type::Optional(_));
        IrField {
            name: f.name.name.clone(),
            ty: self.lower_type(&f.ty),
            mutable: f.mutable,
            optional,
            default: None,
            doc: f.doc.clone(),
            convention: ast::ParamConvention::default(),
        }
    }

    pub(super) fn lower_struct_field(&mut self, f: &StructField) -> IrField {
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
                self.errors.push(CompilerError::UndefinedType {
                    name: name.name.clone(),
                    span: name.span,
                });
                ResolvedType::Error
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
}
