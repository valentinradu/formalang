//! Phase 1: collect every generic instantiation reachable from the
//! module — the seed of the specialisation worklist.

use std::collections::HashSet;

use crate::ir::{GenericBase, IrGenericParam, IrModule, ResolvedType};

use super::specialise::Instantiation;
use super::walkers::walk_module_types;

/// A set of instantiations that keeps the order in which they were
/// first found.
///
/// The worklist makes the specialisations in this order, and each one
/// takes the next free id. A `HashSet` gave a new order on each run, so
/// one source gave a different module each time.
#[derive(Default)]
pub(super) struct Found {
    seen: HashSet<Instantiation>,
    order: Vec<Instantiation>,
}

impl Found {
    fn insert(&mut self, inst: Instantiation) {
        if self.seen.insert(inst.clone()) {
            self.order.push(inst);
        }
    }

    /// The instantiations, in the order they were first found.
    pub(super) fn into_vec(self) -> Vec<Instantiation> {
        self.order
    }
}

/// Walk every type slot in the module and gather `(base, type_args)` keys
/// for every generic instantiation. Generic-trait constraints and impl
/// trait references aren't reached by the type walker, so they're added
/// in a separate pass at the bottom.
pub(super) fn collect_all_instantiations(module: &IrModule) -> Vec<Instantiation> {
    let mut out = Found::default();
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
    out.into_vec()
}

fn collect_constraints(params: &[IrGenericParam], out: &mut Found) {
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

pub(super) fn collect_from_type(ty: &ResolvedType, out: &mut Found) {
    match ty {
        ResolvedType::Generic { base, args } => {
            for a in args {
                collect_from_type(a, out);
            }
            out.insert((*base, args.clone()));
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                collect_from_type(t, out);
            }
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
