//! Phase 1: collect every generic instantiation reachable from the
//! module — the seed of the specialisation worklist.

use std::collections::HashSet;

use crate::ir::{GenericBase, IrGenericParam, IrModule, ResolvedType};

use super::specialise::Instantiation;
use super::walkers::walk_module_types;

/// Walk every type slot in the module and gather `(base, type_args)` keys
/// for every generic instantiation. Generic-trait constraints and impl
/// trait references aren't reached by the type walker, so they're added
/// in a separate pass at the bottom.
pub(super) fn collect_all_instantiations(module: &IrModule) -> HashSet<Instantiation> {
    let mut out = HashSet::new();
    let mut collector = |ty: &ResolvedType| collect_from_type(ty, &mut out);
    walk_module_types(module, &mut collector);

    // Phase E: generic-trait instantiations live on `IrTraitRef`
    // slots that aren't reached by `walk_module_types`:
    //   - constraints on every IrGenericParam in structs / enums /
    //     traits / impls / functions
    //   - the trait reference on every IrImpl
    // For each non-empty args list, schedule the trait specialisation.
    for s in &module.structs {
        collect_constraints(&s.generic_params, &mut out);
    }
    for e in &module.enums {
        collect_constraints(&e.generic_params, &mut out);
    }
    for t in &module.traits {
        collect_constraints(&t.generic_params, &mut out);
    }
    for imp in &module.impls {
        collect_constraints(&imp.generic_params, &mut out);
        if let Some(tr) = &imp.trait_ref {
            if !tr.args.is_empty() {
                out.insert((GenericBase::Trait(tr.trait_id), tr.args.clone()));
                for a in &tr.args {
                    collect_from_type(a, &mut out);
                }
            }
        }
    }
    for f in &module.functions {
        collect_constraints(&f.generic_params, &mut out);
    }
    out
}

fn collect_constraints(params: &[IrGenericParam], out: &mut HashSet<Instantiation>) {
    for p in params {
        for c in &p.constraints {
            if !c.args.is_empty() {
                out.insert((GenericBase::Trait(c.trait_id), c.args.clone()));
                for a in &c.args {
                    collect_from_type(a, out);
                }
            }
        }
    }
}

pub(super) fn collect_from_type(ty: &ResolvedType, out: &mut HashSet<Instantiation>) {
    match ty {
        ResolvedType::Generic { base, args } => {
            for a in args {
                collect_from_type(a, out);
            }
            out.insert((*base, args.clone()));
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            collect_from_type(inner, out);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                collect_from_type(t, out);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            collect_from_type(key_ty, out);
            collect_from_type(value_ty, out);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                collect_from_type(t, out);
            }
            collect_from_type(return_ty, out);
        }
        ResolvedType::External { type_args, .. } => {
            for t in type_args {
                collect_from_type(t, out);
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
