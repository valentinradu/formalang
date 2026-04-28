//! Capture context and the helper that rewrites a captured-name read
//! into an `__env.<name>` field-access.

use std::collections::HashSet;

use crate::ast::ParamConvention;
use crate::ir::{IrExpr, ResolvedType};

use super::ENV_PARAM_NAME;

/// Capture context threaded through the recursive walk. A node's
/// context determines whether a `Reference` / `LetRef` of a given
/// name should be rewritten to `__env.<name>` (for the *current*
/// enclosing closure's captures) or left as-is (parameter or local
/// binding).
#[derive(Clone, Default)]
pub(super) struct CaptureCtx {
    /// Names captured by the immediately-enclosing closure (or empty
    /// at module level).
    captured_names: HashSet<String>,
    /// Resolved type of the current closure's env struct, used as the
    /// `ty` of the synthesized `Reference { path: [__env] }`. `None`
    /// at module level (no env in scope).
    env_ty: Option<ResolvedType>,
    /// Names introduced by `let` / `match` / `for` since the
    /// enclosing closure boundary. References to these shadow
    /// captures of the same name.
    bound: HashSet<String>,
}

impl CaptureCtx {
    pub(super) fn module_level() -> Self {
        Self::default()
    }

    pub(super) fn for_closure(
        captures: &[(String, ParamConvention, ResolvedType)],
        env_ty: ResolvedType,
    ) -> Self {
        Self {
            captured_names: captures.iter().map(|(n, _, _)| n.clone()).collect(),
            env_ty: Some(env_ty),
            bound: HashSet::new(),
        }
    }

    pub(super) fn is_captured(&self, name: &str) -> bool {
        self.captured_names.contains(name) && !self.bound.contains(name)
    }

    /// Add `name` to the set of locally-bound names that shadow any
    /// like-named capture for the remainder of this scope.
    pub(super) fn bind(&mut self, name: String) {
        self.bound.insert(name);
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
            ty: env_ty.cloned().unwrap_or(ResolvedType::Error),
        }),
        field,
        ty,
    }
}
