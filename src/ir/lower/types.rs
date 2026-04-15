//! Type resolution helpers for the IR lowering pass.

use super::IrLowerer;
use crate::ast::PrimitiveType;
use crate::ir::ResolvedType;

impl IrLowerer<'_> {
    /// Convert a string type name from the symbol table into a `ResolvedType`.
    pub(super) fn string_to_resolved_type(&self, type_str: &str) -> ResolvedType {
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
                            self.module.trait_id(name).map_or_else(
                                || ResolvedType::TypeParam(name.to_string()),
                                ResolvedType::Trait,
                            )
                        },
                        ResolvedType::Enum,
                    )
                },
                ResolvedType::Struct,
            ),
        }
    }

    /// Get field type from a resolved type.
    pub(super) fn get_field_type_from_resolved(
        &self,
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
        ResolvedType::TypeParam("Unknown".to_string())
    }

    /// Get the field types of a specific variant from an enum type.
    pub(super) fn get_variant_fields(
        &self,
        enum_ty: &ResolvedType,
        variant_name: &str,
    ) -> Vec<ResolvedType> {
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
