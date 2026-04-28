//! Capture context and the helper that rewrites a captured-name read
//! into an `__env.<name>` field-access.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::ast::ParamConvention;
use crate::ir::{BindingId, IrExpr, ResolvedType};

use super::ENV_PARAM_NAME;

/// Capture context threaded through the recursive walk. A node's
/// context determines whether a `Reference` / `LetRef` whose
/// `BindingId` matches one of the enclosing closure's captures
/// should be rewritten to `__env.<name>`.
///
/// Detection is `BindingId`-based: the pass relies on
/// `ResolveReferencesPass` having run first so each `Reference` /
/// `LetRef` carries the introducing binding's id, and each capture
/// records the same id under `Closure.captures`. Shadowing is handled
/// by `BindingId` distinctness — a `let x = …` inside the closure body
/// gets a fresh id different from any captured outer id, so the inner
/// reference resolves to the new id and is *not* in the captured set.
/// No separate "shadowed names" tracking is needed.
#[derive(Clone, Default)]
pub(super) struct CaptureCtx {
    /// `BindingId`s captured by the immediately-enclosing closure
    /// (or empty at module level). Each id corresponds to an
    /// outer-scope `IrFunctionParam` / `IrBlockStatement::Let`.
    captured_bindings: HashSet<BindingId>,
    /// Map from captured `BindingId` to the source-level name the
    /// capture was introduced under; used to construct the
    /// `__env.<name>` field access on rewrite.
    capture_names: HashMap<BindingId, String>,
    /// Resolved type of the current closure's env struct, used as the
    /// `ty` of the synthesized `Reference { path: [__env] }`. `None`
    /// at module level (no env in scope).
    env_ty: Option<ResolvedType>,
}

impl CaptureCtx {
    pub(super) fn module_level() -> Self {
        Self::default()
    }

    pub(super) fn for_closure(
        captures: &[(BindingId, String, ParamConvention, ResolvedType)],
        env_ty: ResolvedType,
    ) -> Self {
        Self {
            captured_bindings: captures.iter().map(|(bid, _, _, _)| *bid).collect(),
            capture_names: captures
                .iter()
                .map(|(bid, name, _, _)| (*bid, name.clone()))
                .collect(),
            env_ty: Some(env_ty),
        }
    }

    /// Whether the given `BindingId` refers to one of the captures
    /// of the immediately-enclosing closure (and therefore needs
    /// rewriting to `__env.<name>`).
    pub(super) fn is_captured(&self, id: BindingId) -> bool {
        self.captured_bindings.contains(&id)
    }

    /// Look up the source-level name a captured `BindingId` was
    /// introduced under. `None` for ids that aren't in this context's
    /// captured set.
    pub(super) fn capture_name(&self, id: BindingId) -> Option<&str> {
        self.capture_names.get(&id).map(String::as_str)
    }

    /// Resolved type of the current closure's env struct, or `None`
    /// at module level.
    pub(super) const fn env_ty(&self) -> Option<&ResolvedType> {
        self.env_ty.as_ref()
    }
}

/// Build an `__env.<name>` field-access expression carrying the
/// captured value's resolved type. The `__env` reference itself is
/// typed as the env struct so backends can resolve its layout.
///
/// `env_ty` is `None` only at module level — and at module level
/// nothing is "captured", so this helper is never reached with
/// `env_ty == None` in practice. The fallback to
/// [`ResolvedType::Error`] keeps the function total without
/// panicking.
pub(super) fn env_field_access(
    field: String,
    ty: ResolvedType,
    env_ty: Option<&ResolvedType>,
) -> IrExpr {
    IrExpr::FieldAccess {
        object: Box::new(IrExpr::Reference {
            path: vec![ENV_PARAM_NAME.to_string()],
            target: crate::ir::ReferenceTarget::Unresolved,
            ty: env_ty.cloned().unwrap_or(ResolvedType::Error),
        }),
        field,
        field_idx: crate::ir::FieldIdx(0),
        ty,
    }
}
