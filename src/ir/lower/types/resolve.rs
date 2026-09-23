use super::super::IrLowerer;
use crate::ir::ResolvedType;

/// Replace each `TypeParam(name)` reference inside `ty` with the
/// matching entry from `subs`. Used during match-pattern lowering on a
/// generic-instantiated enum so binding payload types carry concrete
/// substitutions (e.g. `T -> I32`) before reaching the IR.
fn substitute_typeparams(
    ty: &mut ResolvedType,
    subs: &std::collections::HashMap<String, ResolvedType>,
) {
    match ty {
        ResolvedType::TypeParam(name) => {
            if let Some(concrete) = subs.get(name) {
                *ty = concrete.clone();
            }
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                substitute_typeparams(t, subs);
            }
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                substitute_typeparams(t, subs);
            }
            substitute_typeparams(return_ty, subs);
        }
        ResolvedType::Generic { args, .. } => {
            for a in args {
                substitute_typeparams(a, subs);
            }
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                substitute_typeparams(a, subs);
            }
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Error => {}
    }
}

impl IrLowerer<'_> {
    /// Get field type from a resolved type.
    pub(in crate::ir::lower) fn get_field_type_from_resolved(
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
    pub(in crate::ir::lower) fn get_variant_fields(
        &self,
        enum_ty: &ResolvedType,
        variant_name: &str,
    ) -> Vec<ResolvedType> {
        // Receiver-side type arguments for a `Generic` scrutinee; used to
        // substitute the variant payload's `TypeParam` slots with concrete
        // types so match-arm bindings carry the right type post-lowering.
        // Optional<T> hits this path naturally now: it's the prelude-defined
        // generic enum, so `.some(T)` / `.none` resolve through the normal
        // enum lookup with substitution.
        let receiver_args: &[ResolvedType] = match enum_ty {
            ResolvedType::Generic { args, .. } => args,
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => &[],
        };
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
            | ResolvedType::Tuple(_)
            | ResolvedType::Closure { .. }
            | ResolvedType::External { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::Error => None,
        };
        if let Some(id) = enum_id {
            if let Some(enum_def) = self.module.get_enum(id) {
                if let Some(variant) = enum_def.variants.iter().find(|v| v.name == variant_name) {
                    let param_names: Vec<String> = if receiver_args.is_empty() {
                        Vec::new()
                    } else {
                        enum_def
                            .generic_params
                            .iter()
                            .map(|p| p.name.clone())
                            .collect()
                    };
                    return variant
                        .fields
                        .iter()
                        .map(|f| {
                            let mut ty = f.ty.clone();
                            if !param_names.is_empty() {
                                let subs: std::collections::HashMap<String, ResolvedType> =
                                    param_names
                                        .iter()
                                        .cloned()
                                        .zip(receiver_args.iter().cloned())
                                        .collect();
                                substitute_typeparams(&mut ty, &subs);
                            }
                            ty
                        })
                        .collect();
                }
            }
        }
        Vec::new()
    }
}
