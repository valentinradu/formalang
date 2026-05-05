//! ID remapping after dead-code elimination.
//!
//! After unused [`StructId`] / [`TraitId`] / [`EnumId`] definitions are
//! removed from an [`IrModule`], every surviving reference must be rewritten
//! to the new contiguous id space. The functions in this module build the
//! [`IdRemap`] table and walk the module rewriting expression, statement,
//! type, and impl-target ids.

mod expr;
mod impl_index;
mod module;
mod ty;

use std::collections::HashSet;

use crate::ir::{EnumId, IrModule, StructId, TraitId};

use impl_index::{apply_impl_index_remap, build_impl_index_remap};
use module::remap_module;

pub(super) use ty::remap_type;

/// Mapping from old-to-new IDs after a DCE pass. `None` at an index means
/// the old definition was removed.
#[derive(Debug, Default)]
pub(super) struct IdRemap {
    pub(super) structs: Vec<Option<StructId>>,
    pub(super) traits: Vec<Option<TraitId>>,
    pub(super) enums: Vec<Option<EnumId>>,
}

impl IdRemap {
    pub(super) fn struct_of(&self, old: StructId) -> Option<StructId> {
        self.structs.get(old.0 as usize).copied().flatten()
    }

    pub(super) fn trait_of(&self, old: TraitId) -> Option<TraitId> {
        self.traits.get(old.0 as usize).copied().flatten()
    }

    pub(super) fn enum_of(&self, old: EnumId) -> Option<EnumId> {
        self.enums.get(old.0 as usize).copied().flatten()
    }
}

/// Remove unused struct/trait/enum definitions and every reference to them
/// across the whole IR module. Also drops impl blocks whose target is
/// removed, and rebuilds name-to-ID indices.
pub(super) fn remove_unused_definitions(
    module: &mut IrModule,
    used_structs: &HashSet<StructId>,
    used_traits: &HashSet<TraitId>,
    used_enums: &HashSet<EnumId>,
) {
    let remap = build_remap(module, used_structs, used_traits, used_enums);

    {
        let mut iter = remap.structs.iter();
        module
            .structs
            .retain(|_| iter.next().copied().flatten().is_some());
    }
    {
        let mut iter = remap.traits.iter();
        module
            .traits
            .retain(|_| iter.next().copied().flatten().is_some());
    }
    {
        let mut iter = remap.enums.iter();
        module
            .enums
            .retain(|_| iter.next().copied().flatten().is_some());
    }

    let impl_index_remap = build_impl_index_remap(module, &remap);
    {
        let mut iter = impl_index_remap.iter();
        module
            .impls
            .retain(|_| iter.next().copied().flatten().is_some());
    }

    remap_module(module, &remap);
    apply_impl_index_remap(module, &impl_index_remap);

    module.rebuild_indices();
}

fn build_remap(
    module: &IrModule,
    used_structs: &HashSet<StructId>,
    used_traits: &HashSet<TraitId>,
    used_enums: &HashSet<EnumId>,
) -> IdRemap {
    fn remap_slice<Id: Copy + Eq + std::hash::Hash>(
        count: usize,
        used: &HashSet<Id>,
        make: impl Fn(u32) -> Id,
    ) -> Vec<Option<Id>> {
        let mut out = Vec::with_capacity(count);
        let mut next: u32 = 0;
        for i in 0..count {
            let Ok(old_idx) = u32::try_from(i) else {
                out.push(None);
                continue;
            };
            let old = make(old_idx);
            if used.contains(&old) {
                out.push(Some(make(next)));
                let Some(n) = next.checked_add(1) else {
                    for _ in i.saturating_add(1)..count {
                        out.push(None);
                    }
                    break;
                };
                next = n;
            } else {
                out.push(None);
            }
        }
        out
    }

    IdRemap {
        structs: remap_slice(module.structs.len(), used_structs, StructId),
        traits: remap_slice(module.traits.len(), used_traits, TraitId),
        enums: remap_slice(module.enums.len(), used_enums, EnumId),
    }
}
