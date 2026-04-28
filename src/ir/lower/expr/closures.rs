//! Closure-literal lowering plus the free-variable walk used to compute
//! captures.

use crate::ast::{self, ClosureParam, Expr, ParamConvention};
use crate::ir::lower::IrLowerer;
use crate::ir::{IrBlockStatement, IrExpr, ResolvedType};
use std::collections::HashMap;

impl IrLowerer<'_> {
    /// Lower a closure expression.
    ///
    /// Lowers parameters and body to a `Closure` IR node, and collects the
    /// free variables (captures) referenced by the body. The regular lowering
    /// path handles all closure cases uniformly, including closures whose body
    /// is an enum variant construction.
    pub(super) fn lower_closure(
        &mut self,
        params: &[ClosureParam],
        return_type: Option<&ast::Type>,
        body: &Expr,
    ) -> IrExpr {
        // when the surrounding context (a call argument,
        // a closure-typed struct field, etc.) supplies an expected
        // closure type, fall back to its param/return types for any
        // closure-literal slots the AST didn't annotate. This turns
        // `array.map(x -> x + 1)` into a closure with concrete
        // `x: <element type>` instead of `ResolvedType::Error`.
        let expected = self.expected_closure_type.take();
        let expected_param_tys: Vec<Option<ResolvedType>> = match expected.as_ref() {
            Some(ResolvedType::Closure { param_tys, .. }) if param_tys.len() == params.len() => {
                param_tys.iter().map(|(_, t)| Some(t.clone())).collect()
            }
            _ => vec![None; params.len()],
        };
        let expected_return_ty: Option<ResolvedType> = match expected.as_ref() {
            Some(ResolvedType::Closure { return_ty, .. }) => Some((**return_ty).clone()),
            _ => None,
        };

        // General closure: lower params and body
        let lowered_params: Vec<(ParamConvention, String, ResolvedType)> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ty = p.ty.as_ref().map_or_else(
                    || {
                        expected_param_tys
                            .get(i)
                            .and_then(std::clone::Clone::clone)
                            .unwrap_or(ResolvedType::Error)
                    },
                    |t| self.lower_type(t),
                );
                (p.convention, p.name.name.clone(), ty)
            })
            .collect();

        // Push a binding-scope frame for the closure's own parameters so that
        // (a) References inside the body resolve to declared types and
        // (b) nested closures see them when computing their own captures.
        let mut closure_frame: HashMap<String, (ParamConvention, ResolvedType)> = HashMap::new();
        for (conv, name, ty) in &lowered_params {
            closure_frame.insert(name.clone(), (*conv, ty.clone()));
        }
        self.local_binding_scopes.push(closure_frame);

        // set `current_function_return_type` from the
        // closure's declared return type so an inferred-enum
        // `.variant` inside the body resolves against the closure's
        // own return type, not the surrounding context (which after B18
        // can be the *outer* type, e.g. the field's `Closure` type).
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = return_type.map(IrLowerer::type_name);

        let body_ir = self.lower_expr(body);

        self.current_function_return_type = saved_return_type;
        // prefer the declared return type when
        // present, then the expected return type from the surrounding
        // context, then the inferred body type (which may be `Unknown`
        // or narrower).
        let return_ty = return_type.map_or_else(
            || {
                if matches!(body_ir.ty(), ResolvedType::TypeParam(_)) {
                    expected_return_ty
                        .clone()
                        .unwrap_or_else(|| body_ir.ty().clone())
                } else {
                    body_ir.ty().clone()
                }
            },
            |t| self.lower_type(t),
        );

        // Pop the closure's own frame before resolving captures so that
        // capture lookups consult only the enclosing scopes.
        self.local_binding_scopes.pop();

        let param_names: std::collections::HashSet<String> =
            lowered_params.iter().map(|(_, n, _)| n.clone()).collect();
        let mut captures: Vec<(String, ResolvedType)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        collect_free_refs(&body_ir, &param_names, &mut captures, &mut seen);

        // each capture inherits the convention of the outer
        // binding it refers to — function parameter convention, mutable-let
        // in a block, outer closure parameter convention, or module-level
        // `let mut`. Bindings whose convention can't be located default to
        // `Let` (immutable view is the safest backend assumption).
        let captures_with_mode: Vec<(String, ParamConvention, ResolvedType)> = captures
            .into_iter()
            .map(|(name, ty)| {
                let convention = self
                    .lookup_local_binding_entry(&name)
                    .map(|(c, _)| *c)
                    .or_else(|| {
                        self.module.lets.iter().find(|l| l.name == name).map(|l| {
                            if l.mutable {
                                ParamConvention::Mut
                            } else {
                                ParamConvention::Let
                            }
                        })
                    })
                    .unwrap_or(ParamConvention::Let);
                (name, convention, ty)
            })
            .collect();

        let ty = ResolvedType::Closure {
            param_tys: lowered_params
                .iter()
                .map(|(c, _, t)| (*c, t.clone()))
                .collect(),
            return_ty: Box::new(return_ty),
        };

        IrExpr::Closure {
            params: lowered_params,
            captures: captures_with_mode,
            body: Box::new(body_ir),
            ty,
        }
    }
}

/// Walk `expr` and collect every single-name `Reference` whose name is not
/// bound inside the expression itself — i.e. the closure's free variables.
///
/// Captures are appended to `out` in first-reference order and deduplicated
/// via `seen`. The caller seeds `bound` with the closure's own parameter
/// names; nested lets and inner closures extend it locally during the walk.
#[expect(
    clippy::too_many_lines,
    reason = "exhaustive dispatch over every IrExpr variant — extracting arms would hide the structural walk"
)]
fn collect_free_refs(
    expr: &IrExpr,
    bound: &std::collections::HashSet<String>,
    out: &mut Vec<(String, ResolvedType)>,
    seen: &mut std::collections::HashSet<String>,
) {
    match expr {
        IrExpr::Reference { path, ty, .. } => {
            if let [name] = path.as_slice() {
                if !bound.contains(name) && seen.insert(name.clone()) {
                    out.push((name.clone(), ty.clone()));
                }
            }
        }
        IrExpr::LetRef { name, ty, .. } => {
            if !bound.contains(name) && seen.insert(name.clone()) {
                out.push((name.clone(), ty.clone()));
            }
        }
        IrExpr::Literal { .. } | IrExpr::SelfFieldRef { .. } => {}
        IrExpr::StructInst { fields, .. }
        | IrExpr::EnumInst { fields, .. }
        | IrExpr::Tuple { fields, .. } => {
            for (_, field_expr) in fields {
                collect_free_refs(field_expr, bound, out, seen);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                collect_free_refs(e, bound, out, seen);
            }
        }
        IrExpr::FieldAccess { object, .. } => collect_free_refs(object, bound, out, seen),
        IrExpr::BinaryOp { left, right, .. } => {
            collect_free_refs(left, bound, out, seen);
            collect_free_refs(right, bound, out, seen);
        }
        IrExpr::UnaryOp { operand, .. } => collect_free_refs(operand, bound, out, seen),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_free_refs(condition, bound, out, seen);
            collect_free_refs(then_branch, bound, out, seen);
            if let Some(e) = else_branch {
                collect_free_refs(e, bound, out, seen);
            }
        }
        IrExpr::For {
            var,
            collection,
            body,
            ..
        } => {
            collect_free_refs(collection, bound, out, seen);
            let mut inner = bound.clone();
            inner.insert(var.clone());
            collect_free_refs(body, &inner, out, seen);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            collect_free_refs(scrutinee, bound, out, seen);
            for arm in arms {
                let mut inner = bound.clone();
                for (name, _, _) in &arm.bindings {
                    inner.insert(name.clone());
                }
                collect_free_refs(&arm.body, &inner, out, seen);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, a) in args {
                collect_free_refs(a, bound, out, seen);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            collect_free_refs(receiver, bound, out, seen);
            for (_, a) in args {
                collect_free_refs(a, bound, out, seen);
            }
        }
        IrExpr::Closure {
            params, captures, ..
        } => {
            // Inner closure: its own captures are already computed relative to
            // its body. Any capture that is bound in the outer scope is not
            // free at this level; the rest bubble up as outer-closure captures.
            let inner_params: std::collections::HashSet<String> =
                params.iter().map(|(_, n, _)| n.clone()).collect();
            for (name, _, ty) in captures {
                if !inner_params.contains(name)
                    && !bound.contains(name)
                    && seen.insert(name.clone())
                {
                    out.push((name.clone(), ty.clone()));
                }
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                collect_free_refs(k, bound, out, seen);
                collect_free_refs(v, bound, out, seen);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            collect_free_refs(dict, bound, out, seen);
            collect_free_refs(key, bound, out, seen);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            let mut inner = bound.clone();
            for stmt in statements {
                match stmt {
                    IrBlockStatement::Let { name, value, .. } => {
                        collect_free_refs(value, &inner, out, seen);
                        inner.insert(name.clone());
                    }
                    IrBlockStatement::Assign { target, value } => {
                        collect_free_refs(target, &inner, out, seen);
                        collect_free_refs(value, &inner, out, seen);
                    }
                    IrBlockStatement::Expr(e) => collect_free_refs(e, &inner, out, seen),
                }
            }
            collect_free_refs(result, &inner, out, seen);
        }
        IrExpr::ClosureRef { env_struct, .. } => {
            // `ClosureRef` is produced by `ClosureConversionPass`, which
            // runs after lowering — this helper shouldn't see one in
            // practice. Recurse into the env-struct expression so the
            // walk stays structurally sound if pass ordering ever shifts.
            collect_free_refs(env_struct, bound, out, seen);
        }
    }
}
