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
            "Never" => ResolvedType::Primitive(PrimitiveType::Never),
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
                crate::ir::GenericBase::Struct(_) => None,
            },
            ResolvedType::TypeParam(name) if name == "self" => self
                .current_impl_struct
                .as_ref()
                .and_then(|impl_name| self.module.enum_id(impl_name)),
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Array(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::External { .. }
            | ResolvedType::TypeParam(_) => None,
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
}
