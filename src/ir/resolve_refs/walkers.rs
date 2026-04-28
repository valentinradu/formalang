//! Sub-walkers for `resolve_expr` — block statements, match arms, and
//! the `resolve_path` driver. Pulled out of `mod.rs` to keep the
//! main pass file under the 500-LOC ceiling.

use super::lookups::match_variant_idx;
use super::{resolve_expr, BindingKind, FnResolver};
use crate::ir::{
    FunctionId, IrBlockStatement, IrMatchArm, ReferenceTarget, ResolvedType, VariantIdx,
};

/// Extract the module prefix of a qualified IR name. For
/// `"foo::bar::baz"` returns `"foo::bar"`; for a bare `"baz"`
/// returns `""`.
pub(super) fn module_prefix_of(qualified_name: &str) -> String {
    qualified_name
        .rsplit_once("::")
        .map_or_else(String::new, |(prefix, _)| prefix.to_string())
}

/// Late-bind a `FunctionCall` to a `FunctionId`. Single-segment
/// paths first try the enclosing module's qualified form (so
/// intra-module calls resolve to the local definition), falling back
/// to bare. Multi-segment paths use the joined name as-written
/// (matches the qualified registration of nested-module functions).
pub(super) fn resolve_function_call_id(path: &[String], r: &FnResolver<'_>) -> Option<FunctionId> {
    if path.len() == 1 {
        let bare = path.first().map(String::as_str).unwrap_or_default();
        let qualified = if r.module_prefix.is_empty() {
            None
        } else {
            Some(format!("{}::{}", r.module_prefix, bare))
        };
        qualified
            .as_deref()
            .and_then(|q| r.module.function_id(q))
            .or_else(|| r.module.function_id(bare))
    } else {
        r.module.function_id(&path.join("::"))
    }
}

pub(super) fn resolve_block_stmt(stmt: &mut IrBlockStatement, r: &mut FnResolver<'_>) {
    match stmt {
        IrBlockStatement::Let {
            binding_id,
            name,
            value,
            ..
        } => {
            resolve_expr(value, r);
            let id = r.fresh();
            *binding_id = id;
            r.bind(name.clone(), id, BindingKind::Local);
        }
        IrBlockStatement::Assign { target, value } => {
            resolve_expr(target, r);
            resolve_expr(value, r);
        }
        IrBlockStatement::Expr(e) => resolve_expr(e, r),
    }
}

pub(super) fn resolve_match_arm(
    arm: &mut IrMatchArm,
    scrutinee_ty: &ResolvedType,
    r: &mut FnResolver<'_>,
) {
    if !arm.is_wildcard {
        if let Some(idx) = match_variant_idx(scrutinee_ty, &arm.variant, r.module) {
            arm.variant_idx = VariantIdx(idx);
        }
    }
    r.push_scope();
    for (name, binding_id, _ty) in &mut arm.bindings {
        let id = r.fresh();
        *binding_id = id;
        r.bind(name.clone(), id, BindingKind::Local);
    }
    resolve_expr(&mut arm.body, r);
    r.pop_scope();
}

pub(super) fn resolve_path(path: &[String], r: &FnResolver<'_>) -> ReferenceTarget {
    if let [single] = path {
        if let Some((id, kind)) = r.lookup(single) {
            return match kind {
                BindingKind::Param => ReferenceTarget::Param(id),
                BindingKind::Local => ReferenceTarget::Local(id),
            };
        }
        if let Some(target) = r.symbols.by_name.get(single) {
            return target.clone();
        }
    } else if !path.is_empty() {
        // Multi-segment path. Items in `mod foo { struct Bar }` are
        // registered in the flat `IrModule.{structs, enums, …}` vectors
        // under the qualified name `"foo::Bar"`. Join the path segments
        // and look up directly. If the join doesn't match, fall back to
        // probing the first segment so we still resolve the reference
        // root (the trailing segments may be field accesses the AST
        // collapses into the same path).
        let joined = path.join("::");
        if let Some(target) = r.symbols.by_name.get(&joined) {
            return target.clone();
        }
        if let Some(first) = path.first() {
            if let Some(target) = r.symbols.by_name.get(first) {
                return target.clone();
            }
        }
    }
    ReferenceTarget::Unresolved
}
