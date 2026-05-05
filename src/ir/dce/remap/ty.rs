use super::IdRemap;

pub(in crate::ir::dce) fn remap_type(ty: &mut crate::ir::ResolvedType, remap: &IdRemap) {
    use crate::ir::ResolvedType;
    match ty {
        ResolvedType::Struct(id) => {
            if let Some(new) = remap.struct_of(*id) {
                *id = new;
            }
        }
        ResolvedType::Trait(id) => {
            if let Some(new) = remap.trait_of(*id) {
                *id = new;
            }
        }
        ResolvedType::Enum(id) => {
            if let Some(new) = remap.enum_of(*id) {
                *id = new;
            }
        }
        ResolvedType::Generic { base, args } => {
            match base {
                crate::ir::GenericBase::Struct(id) => {
                    if let Some(new) = remap.struct_of(*id) {
                        *id = new;
                    }
                }
                crate::ir::GenericBase::Enum(id) => {
                    if let Some(new) = remap.enum_of(*id) {
                        *id = new;
                    }
                }
                crate::ir::GenericBase::Trait(id) => {
                    if let Some(new) = remap.trait_of(*id) {
                        *id = new;
                    }
                }
            }
            for a in args {
                remap_type(a, remap);
            }
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                remap_type(t, remap);
            }
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                remap_type(t, remap);
            }
            remap_type(return_ty, remap);
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                remap_type(a, remap);
            }
        }
        ResolvedType::Primitive(_) | ResolvedType::TypeParam(_) | ResolvedType::Error => {}
    }
}
