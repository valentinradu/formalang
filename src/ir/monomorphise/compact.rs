//! Phase 3: compact the module by dropping the original generic structs,
//! enums, traits, and impls (which were specialised in earlier phases),
//! then remap every surviving id to its new post-compaction position.

use crate::error::CompilerError;
use crate::ir::{EnumId, GenericBase, IrExpr, IrModule, ResolvedType, StructId, TraitId};
use crate::location::Span;

use super::expr_walk::iter_expr_children_mut;
use super::walkers::walk_module_types_mut;

/// Build an old-id → new-id remap table for structs. Structs with non-empty
/// `generic_params` become `None` (they will be dropped on compaction);
/// surviving structs map to their new post-compaction position.
pub(super) fn build_struct_remap(module: &IrModule) -> Vec<Option<StructId>> {
    let mut out = Vec::with_capacity(module.structs.len());
    let mut next: u32 = 0;
    for s in &module.structs {
        if s.generic_params.is_empty() {
            out.push(Some(StructId(next)));
            next = next.saturating_add(1);
        } else {
            out.push(None);
        }
    }
    out
}

/// Matching remap for enums.
pub(super) fn build_enum_remap(module: &IrModule) -> Vec<Option<EnumId>> {
    let mut out = Vec::with_capacity(module.enums.len());
    let mut next: u32 = 0;
    for e in &module.enums {
        if e.generic_params.is_empty() {
            out.push(Some(EnumId(next)));
            next = next.saturating_add(1);
        } else {
            out.push(None);
        }
    }
    out
}

/// Phase F: matching remap for traits. Generic traits are dropped
/// post-specialisation (every reference to them was rewritten to
/// the specialised clone in `rewrite_trait_refs`); surviving traits
/// shift down to fill the gaps.
pub(super) fn build_trait_remap(module: &IrModule) -> Vec<Option<TraitId>> {
    let mut out = Vec::with_capacity(module.traits.len());
    let mut next: u32 = 0;
    for t in &module.traits {
        if t.generic_params.is_empty() {
            out.push(Some(TraitId(next)));
            next = next.saturating_add(1);
        } else {
            out.push(None);
        }
    }
    out
}

/// Drop impls whose target is a generic struct or enum that got specialised
/// (and therefore survives in `module.impls` only through its Phase-2b
/// clones). Returns the old-index → new-index mapping for surviving
/// impls so callers can rewrite `DispatchKind::Static { impl_id }`
/// references to match the compacted vector.
pub(super) fn drop_specialised_generic_impls(
    module: &mut IrModule,
    struct_remap: &[Option<StructId>],
    enum_remap: &[Option<EnumId>],
) -> Vec<Option<usize>> {
    let keep: Vec<bool> = module
        .impls
        .iter()
        .map(|imp| match imp.target {
            crate::ir::ImplTarget::Struct(id) => struct_remap
                .get(id.0 as usize)
                .copied()
                .is_none_or(|slot| slot.is_some()),
            crate::ir::ImplTarget::Enum(id) => enum_remap
                .get(id.0 as usize)
                .copied()
                .is_none_or(|slot| slot.is_some()),
        })
        .collect();
    let mut new_index: Vec<Option<usize>> = Vec::with_capacity(keep.len());
    let mut next: usize = 0;
    for &k in &keep {
        if k {
            new_index.push(Some(next));
            next = next.saturating_add(1);
        } else {
            new_index.push(None);
        }
    }
    let mut idx = 0;
    module.impls.retain(|_| {
        let k = keep.get(idx).copied().unwrap_or(false);
        idx = idx.saturating_add(1);
        k
    });
    new_index
}

/// Rewrite every `DispatchKind::Static { impl_id }` so it points at the
/// compacted impl index. Called after `drop_specialised_generic_impls`.
fn impl_index_rewrite_expr(expr: &mut IrExpr, remap: &[Option<usize>]) {
    use crate::ir::{DispatchKind, ImplId};
    for child in iter_expr_children_mut(expr) {
        impl_index_rewrite_expr(child, remap);
    }
    if let IrExpr::MethodCall {
        dispatch: DispatchKind::Static { impl_id },
        ..
    } = expr
    {
        if let Some(Some(new)) = remap.get(impl_id.0 as usize).copied() {
            *impl_id = ImplId(u32::try_from(new).unwrap_or(u32::MAX));
        }
    }
}

pub(super) fn apply_impl_index_remap(module: &mut IrModule, remap: &[Option<usize>]) {
    let identity = remap
        .iter()
        .enumerate()
        .all(|(i, s)| matches!(s, Some(j) if *j == i));
    if identity {
        return;
    }
    for func in &mut module.functions {
        if let Some(body) = &mut func.body {
            impl_index_rewrite_expr(body, remap);
        }
    }
    for imp in &mut module.impls {
        for func in &mut imp.functions {
            if let Some(body) = &mut func.body {
                impl_index_rewrite_expr(body, remap);
            }
        }
    }
    for s in &mut module.structs {
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                impl_index_rewrite_expr(default, remap);
            }
        }
    }
    for l in &mut module.lets {
        impl_index_rewrite_expr(&mut l.value, remap);
    }
}

/// Remap struct/enum IDs across the module after compaction.
///
/// Returns `Err` on out-of-bounds or dropped-slot impl targets — silently
/// no-op'ing them would leave dangling target IDs in the IR.
#[expect(
    clippy::too_many_lines,
    reason = "linear walk over every TraitId-bearing slot in the module"
)]
pub(super) fn apply_remaps(
    module: &mut IrModule,
    struct_remap: &[Option<StructId>],
    enum_remap: &[Option<EnumId>],
    trait_remap: &[Option<TraitId>],
) -> Result<(), Vec<CompilerError>> {
    walk_module_types_mut(module, |ty| {
        remap_type(ty, struct_remap, enum_remap, trait_remap);
    });
    // Phase F: walk every other slot that holds a TraitId outside
    // ResolvedType. Constraints, composed-trait lists, impl-trait
    // refs, and DispatchKind::Virtual all need their TraitIds
    // remapped (or dropped, if a generic-trait id slipped through —
    // by the time we reach apply_remaps, every constraint should
    // already point at a specialised, non-generic id, but we tolerate
    // None defensively).
    let mut errors: Vec<CompilerError> = Vec::new();
    let remap_trait_id_in_place = |id: &mut TraitId, errors: &mut Vec<CompilerError>| {
        match trait_remap.get(id.0 as usize).copied() {
            Some(Some(new)) => *id = new,
            Some(None) => errors.push(CompilerError::InternalError {
                detail: format!(
                    "monomorphise: stale TraitId({}) survived rewrite_trait_refs (generic trait dropped during compaction)",
                    id.0
                ),
                span: Span::default(),
            }),
            None => errors.push(CompilerError::InternalError {
                detail: format!(
                    "monomorphise: TraitId({}) out of bounds for trait remap table (len {})",
                    id.0,
                    trait_remap.len()
                ),
                span: Span::default(),
            }),
        }
    };
    for s in &mut module.structs {
        // Drop traits entries that point at dropped generic traits.
        // The symbol-table-driven `s.traits` index only ever held the
        // unqualified trait id (no args), so a generic-trait impl
        // (`impl Eq<I32> for Foo`) used to register both Eq AND
        // the relevant args at the impl level — but the index slot
        // can't tell them apart and ends up listing the generic id.
        // After rewrite_trait_refs, the impl's trait_ref points at
        // the specialised id; the struct.traits entry for the
        // generic id is stale and gets dropped here.
        s.traits.retain_mut(
            |tr| match trait_remap.get(tr.trait_id.0 as usize).copied() {
                Some(Some(new)) => {
                    tr.trait_id = new;
                    true
                }
                Some(None) | None => false,
            },
        );
        for gp in &mut s.generic_params {
            for c in &mut gp.constraints {
                remap_trait_id_in_place(&mut c.trait_id, &mut errors);
            }
        }
    }
    for t in &mut module.traits {
        for id in &mut t.composed_traits {
            remap_trait_id_in_place(id, &mut errors);
        }
        for gp in &mut t.generic_params {
            for c in &mut gp.constraints {
                remap_trait_id_in_place(&mut c.trait_id, &mut errors);
            }
        }
    }
    for e in &mut module.enums {
        for gp in &mut e.generic_params {
            for c in &mut gp.constraints {
                remap_trait_id_in_place(&mut c.trait_id, &mut errors);
            }
        }
    }
    for f in &mut module.functions {
        for gp in &mut f.generic_params {
            for c in &mut gp.constraints {
                remap_trait_id_in_place(&mut c.trait_id, &mut errors);
            }
        }
    }
    for imp in &mut module.impls {
        match &mut imp.target {
            crate::ir::ImplTarget::Struct(id) => match struct_remap.get(id.0 as usize).copied() {
                Some(Some(new)) => *id = new,
                Some(None) => errors.push(CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: impl block targets struct id {} which was dropped during compaction (drop_specialised_generic_impls missed it)",
                        id.0
                    ),
                    span: Span::default(),
                }),
                None => errors.push(CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: impl block targets struct id {} which is out of bounds for the remap table (len {})",
                        id.0,
                        struct_remap.len()
                    ),
                    span: Span::default(),
                }),
            },
            crate::ir::ImplTarget::Enum(id) => match enum_remap.get(id.0 as usize).copied() {
                Some(Some(new)) => *id = new,
                Some(None) => errors.push(CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: impl block targets enum id {} which was dropped during compaction (drop_specialised_generic_impls missed it)",
                        id.0
                    ),
                    span: Span::default(),
                }),
                None => errors.push(CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: impl block targets enum id {} which is out of bounds for the remap table (len {})",
                        id.0,
                        enum_remap.len()
                    ),
                    span: Span::default(),
                }),
            },
        }
        if let Some(tr) = &mut imp.trait_ref {
            remap_trait_id_in_place(&mut tr.trait_id, &mut errors);
        }
        for gp in &mut imp.generic_params {
            for c in &mut gp.constraints {
                remap_trait_id_in_place(&mut c.trait_id, &mut errors);
            }
        }
    }
    // DispatchKind::Virtual call sites carry a trait id too. Walk
    // every expression in the module.
    for f in &mut module.functions {
        if let Some(body) = &mut f.body {
            walk_dispatch(body, trait_remap, &mut errors);
        }
    }
    for imp in &mut module.impls {
        for f in &mut imp.functions {
            if let Some(body) = &mut f.body {
                walk_dispatch(body, trait_remap, &mut errors);
            }
        }
    }
    for l in &mut module.lets {
        walk_dispatch(&mut l.value, trait_remap, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn walk_dispatch(
    expr: &mut IrExpr,
    trait_remap: &[Option<TraitId>],
    errors: &mut Vec<CompilerError>,
) {
    for child in iter_expr_children_mut(expr) {
        walk_dispatch(child, trait_remap, errors);
    }
    if let IrExpr::MethodCall {
        dispatch: crate::ir::DispatchKind::Virtual { trait_id, .. },
        ..
    } = expr
    {
        match trait_remap.get(trait_id.0 as usize).copied() {
            Some(Some(new)) => *trait_id = new,
            Some(None) => errors.push(CompilerError::InternalError {
                detail: format!(
                    "monomorphise: Virtual dispatch references generic-trait id {} that was dropped",
                    trait_id.0
                ),
                span: Span::default(),
            }),
            None => errors.push(CompilerError::InternalError {
                detail: format!(
                    "monomorphise: Virtual dispatch trait id {} out of bounds for trait remap (len {})",
                    trait_id.0,
                    trait_remap.len()
                ),
                span: Span::default(),
            }),
        }
    }
}

fn remap_type(
    ty: &mut ResolvedType,
    struct_remap: &[Option<StructId>],
    enum_remap: &[Option<EnumId>],
    trait_remap: &[Option<TraitId>],
) {
    match ty {
        ResolvedType::Struct(id) => {
            if let Some(Some(new)) = struct_remap.get(id.0 as usize).copied() {
                *id = new;
            }
        }
        ResolvedType::Enum(id) => {
            if let Some(Some(new)) = enum_remap.get(id.0 as usize).copied() {
                *id = new;
            }
        }
        ResolvedType::Trait(id) => {
            if let Some(Some(new)) = trait_remap.get(id.0 as usize).copied() {
                *id = new;
            }
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            remap_type(inner, struct_remap, enum_remap, trait_remap);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                remap_type(t, struct_remap, enum_remap, trait_remap);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            remap_type(key_ty, struct_remap, enum_remap, trait_remap);
            remap_type(value_ty, struct_remap, enum_remap, trait_remap);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                remap_type(t, struct_remap, enum_remap, trait_remap);
            }
            remap_type(return_ty, struct_remap, enum_remap, trait_remap);
        }
        ResolvedType::Generic { base, args } => {
            // Defensive: by Phase 3 every Generic should have been
            // rewritten to a concrete Struct/Enum/Trait base, but
            // remap just in case a caller is inspecting mid-pass.
            match base {
                GenericBase::Struct(id) => {
                    if let Some(Some(new)) = struct_remap.get(id.0 as usize).copied() {
                        *id = new;
                    }
                }
                GenericBase::Enum(id) => {
                    if let Some(Some(new)) = enum_remap.get(id.0 as usize).copied() {
                        *id = new;
                    }
                }
                GenericBase::Trait(id) => {
                    if let Some(Some(new)) = trait_remap.get(id.0 as usize).copied() {
                        *id = new;
                    }
                }
            }
            for a in args {
                remap_type(a, struct_remap, enum_remap, trait_remap);
            }
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                remap_type(a, struct_remap, enum_remap, trait_remap);
            }
        }
        ResolvedType::Primitive(_) | ResolvedType::TypeParam(_) | ResolvedType::Error => {}
    }
}
