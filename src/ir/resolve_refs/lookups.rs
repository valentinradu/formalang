//! Index-lookup helpers used by `resolve_expr` to populate `FieldIdx`,
//! `VariantIdx`, and `MethodIdx` on the IR variants.
//!
//! Each function returns `Some(idx)` when the lookup succeeds and
//! `None` when the receiver type / dispatch kind / etc. is in a state
//! the pass declines to resolve (typically because an upstream stage
//! left a sentinel that the pass intentionally leaves alone).

use crate::ir::{DispatchKind, IrModule, ResolvedType};

pub(super) fn lookup_method_idx(
    dispatch: &DispatchKind,
    method: &str,
    module: &IrModule,
) -> Option<u32> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "method count is bounded upstream"
    )]
    match dispatch {
        DispatchKind::Static { impl_id } => {
            let imp = module.impls.get(impl_id.0 as usize)?;
            imp.functions
                .iter()
                .position(|f| f.name == method)
                .map(|i| i as u32)
        }
        DispatchKind::Virtual { trait_id, .. } => {
            let t = module.get_trait(*trait_id)?;
            t.methods
                .iter()
                .position(|m| m.name == method)
                .map(|i| i as u32)
        }
    }
}

pub(super) fn struct_field_idx(ty: &ResolvedType, field: &str, module: &IrModule) -> Option<u32> {
    let &ResolvedType::Struct(sid) = ty else {
        return None;
    };
    let s = module.get_struct(sid)?;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "field count is bounded upstream"
    )]
    s.fields
        .iter()
        .position(|f| f.name == field)
        .map(|i| i as u32)
}

pub(super) fn match_variant_idx(
    scrutinee_ty: &ResolvedType,
    variant: &str,
    module: &IrModule,
) -> Option<u32> {
    let &ResolvedType::Enum(enum_id) = scrutinee_ty else {
        return None;
    };
    let e = module.get_enum(enum_id)?;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "variant count is bounded upstream"
    )]
    e.variants
        .iter()
        .position(|v| v.name == variant)
        .map(|i| i as u32)
}
