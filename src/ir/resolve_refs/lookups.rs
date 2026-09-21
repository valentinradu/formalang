//! Index-lookup helpers used by `resolve_expr` to populate `FieldIdx`,
//! `VariantIdx`, and `MethodIdx` on the IR variants.
//!
//! Each function returns `Some(idx)` when the lookup succeeds and
//! `None` when the receiver type / dispatch kind / etc. is in a state
//! the pass declines to resolve (typically because an upstream stage
//! left a sentinel that the pass intentionally leaves alone).

use crate::ir::IrExpr;
use crate::ir::{DispatchKind, IrModule, ResolvedType};

pub(super) fn lookup_method_idx(
    dispatch: &DispatchKind,
    method: &str,
    args: &[(Option<String>, IrExpr)],
    module: &IrModule,
) -> Option<u32> {
    // A type may declare several methods of one name, the way a module
    // may declare several functions of one name. Taking the first of
    // the right name made every call to the second unreachable — it
    // compiled, and answered from the first. The call's labels and
    // count say which is meant, the same way they do for a free
    // function.
    let labels: Vec<Option<String>> = args.iter().map(|(label, _)| label.clone()).collect();

    #[expect(
        clippy::cast_possible_truncation,
        reason = "method count is bounded upstream"
    )]
    match dispatch {
        DispatchKind::Static { impl_id } => {
            let imp = module.impls.get(impl_id.0 as usize)?;
            crate::ir::overload::method_index(
                &imp.functions,
                |f| f.name.as_str(),
                |f| f.params.as_slice(),
                method,
                &labels,
                args.len(),
            )
            .map(|i| i as u32)
        }
        DispatchKind::Virtual { trait_id, .. } => {
            let t = module.get_trait(*trait_id)?;
            crate::ir::overload::method_index(
                &t.methods,
                |m| m.name.as_str(),
                |m| m.params.as_slice(),
                method,
                &labels,
                args.len(),
            )
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
