//! Phase 2 / 2b / 2c: rewrite IR references after specialisation.
//!
//! - [`rewrite_module`] turns every `ResolvedType::Generic` into the
//!   concrete `Struct/Enum/Trait` id of its cloned specialisation, plus
//!   matching rewrites of `IrTraitRef` slots (constraints and impl trait
//!   refs).
//! - [`specialise_impls`] clones each generic-targeting impl block once
//!   per specialised target with the body's `TypeParam` slots
//!   substituted, returning the `(orig_idx, spec_target) → new_idx` map.
//! - [`rewrite_dispatch_impl_ids`] retargets `DispatchKind::Static` calls
//!   onto the cloned impls.
//! - [`devirtualise_concrete_receivers`] resolves any
//!   `DispatchKind::Virtual` whose receiver is now a concrete type to
//!   `Static` against the specialised impl.

use std::collections::HashMap;

use crate::ir::{GenericBase, IrExpr, IrGenericParam, IrImpl, IrModule, IrTraitRef, ResolvedType};

use super::expr_walk::iter_expr_children_mut;
use super::specialise::{substitute_expr_types, substitute_type, Instantiation};
use super::walkers::{walk_function_types_mut, walk_module_types_mut};

/// Map from `(original impl index, specialised target)` to the new
/// impl index in `module.impls` after Phase 2b.
pub(super) type ImplRemap = HashMap<(usize, GenericBase), usize>;

/// Phase 2: rewrite every `ResolvedType::Generic` to its specialised
/// concrete base.
pub(super) fn rewrite_module(module: &mut IrModule, mapping: &HashMap<Instantiation, GenericBase>) {
    {
        let rewrite = |ty: &mut ResolvedType| rewrite_type(ty, mapping);
        walk_module_types_mut(module, rewrite);
    }
    // Phase E: rewrite generic-trait references on IrTraitRef slots
    // that don't live inside ResolvedType. After this, every
    // constraint and impl-trait-ref with non-empty args points at
    // its specialised trait id with the args slot cleared (the
    // specialised trait isn't generic any more).
    rewrite_trait_refs(module, mapping);
}

fn rewrite_trait_ref(tr: &mut IrTraitRef, mapping: &HashMap<Instantiation, GenericBase>) {
    if tr.args.is_empty() {
        return;
    }
    // Rewrite nested generic args first so the lookup key matches
    // the post-rewrite shape stored in `mapping`.
    for a in &mut tr.args {
        rewrite_type(a, mapping);
    }
    let key = (GenericBase::Trait(tr.trait_id), tr.args.clone());
    if let Some(GenericBase::Trait(new_id)) = mapping.get(&key).copied() {
        tr.trait_id = new_id;
        tr.args.clear();
    }
}

fn rewrite_trait_refs(module: &mut IrModule, mapping: &HashMap<Instantiation, GenericBase>) {
    let rewrite_params = |params: &mut [IrGenericParam],
                          mapping: &HashMap<Instantiation, GenericBase>| {
        for p in params {
            for c in &mut p.constraints {
                rewrite_trait_ref(c, mapping);
            }
        }
    };
    for s in &mut module.structs {
        for tr in &mut s.traits {
            rewrite_trait_ref(tr, mapping);
        }
        rewrite_params(&mut s.generic_params, mapping);
    }
    for e in &mut module.enums {
        rewrite_params(&mut e.generic_params, mapping);
    }
    for t in &mut module.traits {
        rewrite_params(&mut t.generic_params, mapping);
    }
    for imp in &mut module.impls {
        rewrite_params(&mut imp.generic_params, mapping);
        if let Some(tr) = &mut imp.trait_ref {
            rewrite_trait_ref(tr, mapping);
        }
    }
    for f in &mut module.functions {
        rewrite_params(&mut f.generic_params, mapping);
    }
}

pub(super) fn rewrite_type(ty: &mut ResolvedType, mapping: &HashMap<Instantiation, GenericBase>) {
    // Recurse first so nested generics inside args are resolved before we
    // try to look up the outer key (the mapping keys hold fully-rewritten
    // inner types, so we must rewrite inner before outer lookup).
    match ty {
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            rewrite_type(inner, mapping);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                rewrite_type(t, mapping);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            rewrite_type(key_ty, mapping);
            rewrite_type(value_ty, mapping);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                rewrite_type(t, mapping);
            }
            rewrite_type(return_ty, mapping);
        }
        ResolvedType::Generic { base, args } => {
            for a in args.iter_mut() {
                rewrite_type(a, mapping);
            }
            if let Some(&spec) = mapping.get(&(*base, args.clone())) {
                *ty = match spec {
                    GenericBase::Struct(id) => ResolvedType::Struct(id),
                    GenericBase::Enum(id) => ResolvedType::Enum(id),
                    GenericBase::Trait(id) => ResolvedType::Trait(id),
                };
            }
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                rewrite_type(a, mapping);
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

// Phase 2b: specialise impl blocks targeting generic structs/enums

/// For each impl block whose target is a generic struct/enum, append one
/// cloned impl per specialisation of that target (with `TypeParam`s
/// substituted for the concrete type args of that specialisation). The
/// originals are retained in `module.impls` for now; they are dropped in
/// Phase 3 by `drop_specialised_generic_impls`.
///
/// Dispatch sites (`DispatchKind::Static { impl_id }`) still reference the
/// original generic-impl slot after this runs. Backends that iterate
/// `module.impls` to locate methods on a specialised type will find them
/// correctly here; Phase 2c (`rewrite_dispatch_impl_ids`) uses the
/// returned [`ImplRemap`] to retarget `DispatchKind::Static { impl_id }`
/// sites onto the cloned impl for each specialisation.
pub(super) fn specialise_impls(
    module: &mut IrModule,
    mapping: &HashMap<Instantiation, GenericBase>,
) -> ImplRemap {
    // Group specialisations by original generic base.
    type Spec = (Vec<ResolvedType>, GenericBase);
    let mut by_base: HashMap<GenericBase, Vec<Spec>> = HashMap::new();
    for ((orig_base, args), spec_base) in mapping {
        by_base
            .entry(*orig_base)
            .or_default()
            .push((args.clone(), *spec_base));
    }
    let mut new_impls: Vec<IrImpl> = Vec::new();
    let mut impl_remap: ImplRemap = HashMap::new();

    for (orig_idx, imp) in module.impls.iter().enumerate() {
        let base = match imp.target {
            crate::ir::ImplTarget::Struct(id) => GenericBase::Struct(id),
            crate::ir::ImplTarget::Enum(id) => GenericBase::Enum(id),
        };
        let Some(specs) = by_base.get(&base) else {
            continue;
        };
        let generic_param_names: Vec<String> = match base {
            GenericBase::Struct(sid) => module
                .get_struct(sid)
                .map(|s| s.generic_params.iter().map(|p| p.name.clone()).collect())
                .unwrap_or_default(),
            GenericBase::Enum(eid) => module
                .get_enum(eid)
                .map(|e| e.generic_params.iter().map(|p| p.name.clone()).collect())
                .unwrap_or_default(),
            // An impl never targets a trait base directly — `imp.target`
            // is `ImplTarget::Struct(_)` or `ImplTarget::Enum(_)`. This
            // arm is unreachable but kept for match exhaustiveness.
            GenericBase::Trait(_) => Vec::new(),
        };
        if generic_param_names.is_empty() {
            continue;
        }
        for (args, spec_base) in specs {
            if generic_param_names.len() != args.len() {
                continue;
            }
            let subs: HashMap<String, ResolvedType> = generic_param_names
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect();
            let mut clone = imp.clone();
            clone.target = match spec_base {
                GenericBase::Struct(id) => crate::ir::ImplTarget::Struct(*id),
                GenericBase::Enum(id) => crate::ir::ImplTarget::Enum(*id),
                // Impl targets are struct/enum only — see above.
                GenericBase::Trait(_) => continue,
            };
            for func in &mut clone.functions {
                for param in &mut func.params {
                    if let Some(ty) = &mut param.ty {
                        substitute_type(ty, &subs);
                    }
                    if let Some(default) = &mut param.default {
                        substitute_expr_types(default, &subs);
                    }
                }
                if let Some(ret_ty) = &mut func.return_type {
                    substitute_type(ret_ty, &subs);
                }
                if let Some(body) = &mut func.body {
                    substitute_expr_types(body, &subs);
                }
            }
            walk_impl_types_mut(&mut clone, &mut |ty| rewrite_type(ty, mapping));
            // Record the (orig_idx, spec_target) → new_idx mapping so
            // dispatch-site rewriting can find the right clone.
            let new_idx = module.impls.len().saturating_add(new_impls.len());
            impl_remap.insert((orig_idx, *spec_base), new_idx);
            new_impls.push(clone);
        }
    }

    module.impls.extend(new_impls);
    impl_remap
}

/// `ImplRemap`-aware type-to-base extraction. Returns the
/// `GenericBase` of a concrete struct/enum receiver type (post Phase 2
/// rewrite). Returns `None` for non-nominal types.
pub(super) fn receiver_to_base(ty: &ResolvedType) -> Option<GenericBase> {
    match ty {
        ResolvedType::Struct(id) => Some(GenericBase::Struct(*id)),
        ResolvedType::Enum(id) => Some(GenericBase::Enum(*id)),
        ResolvedType::Optional(inner) => receiver_to_base(inner),
        ResolvedType::Primitive(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Array(_)
        | ResolvedType::Range(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::External { .. }
        | ResolvedType::Dictionary { .. }
        | ResolvedType::Closure { .. }
        | ResolvedType::Error => None,
    }
}

/// Rewrite `DispatchKind::Static { impl_id }` at every method-call
/// site so the id points at the per-specialisation clone created in
/// Phase 2b. Walks every expression in the module.
fn dispatch_rewrite_expr(expr: &mut IrExpr, impl_remap: &ImplRemap) {
    use crate::ir::{DispatchKind, ImplId};
    // Recurse first so nested method calls are rewritten too.
    for child in iter_expr_children_mut(expr) {
        dispatch_rewrite_expr(child, impl_remap);
    }
    if let IrExpr::MethodCall {
        receiver,
        dispatch: DispatchKind::Static { impl_id },
        ..
    } = expr
    {
        let old_idx = impl_id.0 as usize;
        if let Some(target_base) = receiver_to_base(receiver.ty()) {
            if let Some(&new_idx) = impl_remap.get(&(old_idx, target_base)) {
                *impl_id = ImplId(u32::try_from(new_idx).unwrap_or(u32::MAX));
            }
        }
    }
}

pub(super) fn rewrite_dispatch_impl_ids(module: &mut IrModule, impl_remap: &ImplRemap) {
    if impl_remap.is_empty() {
        return;
    }
    // Walk every expression in the module.
    for func in &mut module.functions {
        if let Some(body) = &mut func.body {
            dispatch_rewrite_expr(body, impl_remap);
        }
        for param in &mut func.params {
            if let Some(default) = &mut param.default {
                dispatch_rewrite_expr(default, impl_remap);
            }
        }
    }
    for imp in &mut module.impls {
        for func in &mut imp.functions {
            if let Some(body) = &mut func.body {
                dispatch_rewrite_expr(body, impl_remap);
            }
            for param in &mut func.params {
                if let Some(default) = &mut param.default {
                    dispatch_rewrite_expr(default, impl_remap);
                }
            }
        }
    }
    for s in &mut module.structs {
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                dispatch_rewrite_expr(default, impl_remap);
            }
        }
    }
    for e in &mut module.enums {
        for variant in &mut e.variants {
            for field in &mut variant.fields {
                if let Some(default) = &mut field.default {
                    dispatch_rewrite_expr(default, impl_remap);
                }
            }
        }
    }
    for l in &mut module.lets {
        dispatch_rewrite_expr(&mut l.value, impl_remap);
    }
}

/// Phase 2e devirtualisation: walk every method call and rewrite
/// `DispatchKind::Virtual` to `Static` when the receiver type is now
/// concrete (Struct/Enum). Reads `module.impls` to find the impl
/// providing the requested trait method on the receiver type.
///
/// Calls whose receiver is still a `TypeParam` (uninstantiated generic
/// function bodies) stay `Virtual` and are tolerated downstream —
/// those bodies are dropped during compaction or never reached by a
/// backend's specialisation root set.
pub(super) fn devirtualise_concrete_receivers(module: &mut IrModule) {
    // Clone the impls table so we can read it while mutating function
    // bodies. impls don't change shape during devirt; we only consult
    // them for `(target, trait_id, method_name)` lookup.
    let impls_snapshot = module.impls.clone();
    for func in &mut module.functions {
        if let Some(body) = &mut func.body {
            devirtualise_expr(body, &impls_snapshot);
        }
        for param in &mut func.params {
            if let Some(default) = &mut param.default {
                devirtualise_expr(default, &impls_snapshot);
            }
        }
    }
    for imp in &mut module.impls {
        for func in &mut imp.functions {
            if let Some(body) = &mut func.body {
                devirtualise_expr(body, &impls_snapshot);
            }
            for param in &mut func.params {
                if let Some(default) = &mut param.default {
                    devirtualise_expr(default, &impls_snapshot);
                }
            }
        }
    }
    for s in &mut module.structs {
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                devirtualise_expr(default, &impls_snapshot);
            }
        }
    }
    for e in &mut module.enums {
        for variant in &mut e.variants {
            for field in &mut variant.fields {
                if let Some(default) = &mut field.default {
                    devirtualise_expr(default, &impls_snapshot);
                }
            }
        }
    }
    for l in &mut module.lets {
        devirtualise_expr(&mut l.value, &impls_snapshot);
    }
}

fn devirtualise_expr(expr: &mut IrExpr, impls: &[IrImpl]) {
    use crate::ir::{DispatchKind, ImplId};
    for child in iter_expr_children_mut(expr) {
        devirtualise_expr(child, impls);
    }
    let IrExpr::MethodCall {
        receiver,
        method,
        dispatch,
        ..
    } = expr
    else {
        return;
    };
    let DispatchKind::Virtual {
        trait_id: virt_trait_id,
        ..
    } = dispatch
    else {
        return;
    };
    let Some(target_base) = receiver_to_base(receiver.ty()) else {
        return;
    };
    let virt_trait_id = *virt_trait_id;
    let method_name_owned = method.clone();
    if let Some(impl_idx) = impls.iter().position(|imp| match imp.target {
        crate::ir::ImplTarget::Struct(id) => {
            target_base == GenericBase::Struct(id)
                && imp.trait_id() == Some(virt_trait_id)
                && imp.functions.iter().any(|f| f.name == method_name_owned)
        }
        crate::ir::ImplTarget::Enum(id) => {
            target_base == GenericBase::Enum(id)
                && imp.trait_id() == Some(virt_trait_id)
                && imp.functions.iter().any(|f| f.name == method_name_owned)
        }
    }) {
        let new_impl_id = ImplId(u32::try_from(impl_idx).unwrap_or(u32::MAX));
        *dispatch = DispatchKind::Static {
            impl_id: new_impl_id,
        };
    }
}

pub(super) fn walk_impl_types_mut(imp: &mut IrImpl, visit: &mut impl FnMut(&mut ResolvedType)) {
    for f in &mut imp.functions {
        walk_function_types_mut(f, visit);
    }
}
