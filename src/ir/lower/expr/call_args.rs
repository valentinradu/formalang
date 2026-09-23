//! Lowering the arguments of a call.
//!
//! A free function and a method share the rule for a generic callee,
//! so it lives here once.

use std::collections::HashMap;

use crate::ast::Expr;
use crate::ir::lower::expr::operators::ArgSlot;
use crate::ir::lower::expr::type_params::unify_typeparam_in_resolved;
use crate::ir::lower::IrLowerer;
use crate::ir::monomorphise::specialise::substitute_type;
use crate::ir::{IrExpr, ResolvedType};

/// The lowered arguments of a call, each with its label.
type LoweredArgs = Vec<(Option<String>, IrExpr)>;

impl IrLowerer<'_> {
    /// Lower the arguments of a call against the callee's parameter
    /// slots, and bind the callee's type parameters from them.
    ///
    /// For a generic callee, a closure argument lowers last: the other
    /// arguments fix its type parameters first, so a closure for
    /// `f: (T) -> T` next to `v: 2` takes `(I32) -> I32`, not
    /// `(T) -> T`. The closure's own type is matched too, so a type
    /// parameter that only its return type mentions — `K` in
    /// `key: (T) -> K` — binds to the type that its body answers. The
    /// arguments keep their order in the call. `written` holds the type
    /// arguments that the call writes, `first<I32>(...)`: they bind
    /// before any argument lowers.
    pub(in crate::ir::lower::expr) fn lower_call_args(
        &mut self,
        args: &[(Option<crate::ast::Ident>, Expr)],
        expected_param_tys: &[ArgSlot],
        generic: bool,
        written: HashMap<String, ResolvedType>,
    ) -> (LoweredArgs, HashMap<String, ResolvedType>) {
        let is_closure = |e: &Expr| matches!(e, Expr::ClosureExpr { .. });
        let mut subs = written;
        let mut slots: Vec<Option<IrExpr>> = Vec::with_capacity(args.len());
        for (i, (label, expr)) in args.iter().enumerate() {
            if generic && is_closure(expr) {
                slots.push(None);
                continue;
            }
            let expected = Self::expected_arg_ty(expected_param_tys, i, label.as_ref());
            let lowered = self.lower_with_expected_value(expr, expected.as_ref());
            if let Some(declared) = &expected {
                unify_typeparam_in_resolved(declared, lowered.ty(), &mut subs);
            }
            slots.push(Some(lowered));
        }
        let mut lowered_args = Vec::with_capacity(args.len());
        for (i, ((label, expr), slot)) in args.iter().zip(slots).enumerate() {
            let lowered = slot.unwrap_or_else(|| {
                let expected =
                    Self::expected_arg_ty(expected_param_tys, i, label.as_ref()).map(|mut ty| {
                        substitute_type(&mut ty, &subs);
                        ty
                    });
                let lowered = self.lower_with_expected_value(expr, expected.as_ref());
                if let Some(declared) = &expected {
                    unify_typeparam_in_resolved(declared, lowered.ty(), &mut subs);
                }
                lowered
            });
            lowered_args.push((label.as_ref().map(|l| l.name.clone()), lowered));
        }
        (lowered_args, subs)
    }
}
