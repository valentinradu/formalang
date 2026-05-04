//! Phase 2: rewrite every leftover `External { type_args, .. }` whose
//! `(module_path, name, type_args)` was specialised in Phase 1a, to
//! point at the cloned local Struct/Enum/Trait.

use std::collections::HashMap;

use crate::ir::{IrModule, ResolvedType};

use super::super::walkers::walk_module_types_mut;
use super::specialise::ExternalInstantiation;

pub(in crate::ir::monomorphise) fn rewrite_external_references(
    module: &mut IrModule,
    mapping: &HashMap<ExternalInstantiation, ResolvedType>,
) {
    walk_module_types_mut(module, |ty| rewrite_external_type(ty, mapping));
}

fn rewrite_external_type(
    ty: &mut ResolvedType,
    mapping: &HashMap<ExternalInstantiation, ResolvedType>,
) {
    match ty {
        ResolvedType::External {
            module_path,
            name,
            type_args,
            ..
        } => {
            for a in type_args.iter_mut() {
                rewrite_external_type(a, mapping);
            }
            // Both generic and non-generic externals are eligible; the
            // collection step now enqueues both.
            let key = (module_path.clone(), name.clone(), type_args.clone());
            if let Some(new_ty) = mapping.get(&key) {
                *ty = new_ty.clone();
            }
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                rewrite_external_type(t, mapping);
            }
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                rewrite_external_type(t, mapping);
            }
            rewrite_external_type(return_ty, mapping);
        }
        ResolvedType::Generic { args, .. } => {
            for a in args {
                rewrite_external_type(a, mapping);
            }
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::TypeParam(_)
        | ResolvedType::Error => {}
    }
}
