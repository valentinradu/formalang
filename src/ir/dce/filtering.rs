//! Trait-id retention helpers used by the [`super::remap`] module.
//!
//! These predicates are passed to [`Vec::retain_mut`] when filtering
//! trait-reference collections such as `composed_traits` and the trait
//! constraints attached to generic parameters: a trait id whose definition
//! has been removed is dropped, while a surviving id is rewritten in place
//! to its new value.

use crate::ir::TraitId;

use super::remap::{remap_type, IdRemap};

/// Rewrite each surviving trait ID in `ids` and drop those whose trait was
/// removed. Returns `true` if the trait survived and has been updated.
pub(super) fn retain_trait_id(id: &mut TraitId, remap: &IdRemap) -> bool {
    remap.trait_of(*id).is_some_and(|new| {
        *id = new;
        true
    })
}

/// Same as `retain_trait_id` but for the [`crate::ir::IrTraitRef`] shape used
/// by generic-param constraints — also remaps any [`TraitId`] nested inside
/// the constraint's arg types.
pub(super) fn retain_trait_ref(constraint: &mut crate::ir::IrTraitRef, remap: &IdRemap) -> bool {
    let kept = remap.trait_of(constraint.trait_id).is_some_and(|new| {
        constraint.trait_id = new;
        true
    });
    if kept {
        for arg in &mut constraint.args {
            remap_type(arg, remap);
        }
    }
    kept
}
