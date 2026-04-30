//! Recursive `IrExpr` walker for `ResolveReferencesPass` — the giant
//! match. Pulled out of `mod.rs` to keep the pass file under the
//! 500-LOC ceiling.

use super::lookups::{lookup_method_idx, struct_field_idx};
use super::walkers::{self, resolve_match_arm};
use super::{BindingKind, FnResolver};
use crate::error::CompilerError;
use crate::ir::{FieldIdx, IrExpr, MethodIdx, ReferenceTarget, ResolvedType, VariantIdx};

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive walk over IrExpr variants"
)]
pub(super) fn resolve_expr(expr: &mut IrExpr, r: &mut FnResolver<'_>) {
    match expr {
        IrExpr::Literal { .. } | IrExpr::SelfFieldRef { .. } => {}
        IrExpr::Reference { path, target, ty } => {
            *target = walkers::resolve_path(path, r);
            // Promote a remaining `Unresolved` to a typed
            // `UndefinedReference` error — but only when the upstream
            // didn't already mark the reference's type as `Error`,
            // which is how lowering signals "I already pushed a
            // CompilerError for this site". Without the gate we'd
            // double-count any unbound name.
            if matches!(target, ReferenceTarget::Unresolved) && !matches!(ty, ResolvedType::Error) {
                r.errors.push(CompilerError::UndefinedReference {
                    name: path.join("::"),
                    span: crate::location::Span::default(),
                });
            }
        }
        IrExpr::LetRef {
            name, binding_id, ..
        } => {
            if let Some((id, _)) = r.lookup(name) {
                *binding_id = id;
            }
        }
        IrExpr::FunctionCall {
            path,
            function_id,
            args,
            ..
        } => {
            if function_id.is_none() {
                *function_id = walkers::resolve_function_call_id(path, r);
            }
            for (_, arg) in args {
                resolve_expr(arg, r);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            resolve_expr(closure, r);
            for (_, arg) in args {
                resolve_expr(arg, r);
            }
        }
        IrExpr::MethodCall {
            receiver,
            method,
            method_idx,
            dispatch,
            args,
            ..
        } => {
            if let Some(idx) = lookup_method_idx(dispatch, method, r.module) {
                *method_idx = MethodIdx(idx);
            }
            resolve_expr(receiver, r);
            for (_, arg) in args {
                resolve_expr(arg, r);
            }
        }
        IrExpr::FieldAccess {
            object,
            field,
            field_idx,
            ..
        } => {
            resolve_expr(object, r);
            if let Some(idx) = struct_field_idx(object.ty(), field, r.module) {
                *field_idx = FieldIdx(idx);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, fexpr) in fields {
                resolve_expr(fexpr, r);
            }
        }
        IrExpr::StructInst {
            struct_id, fields, ..
        } => {
            for (name, idx, fexpr) in fields.iter_mut() {
                resolve_expr(fexpr, r);
                if let Some(sid) = struct_id {
                    if let Some(found) = r
                        .module
                        .get_struct(*sid)
                        .and_then(|s| s.fields.iter().position(|f| f.name == *name))
                    {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "field count is bounded upstream"
                        )]
                        let new_idx = FieldIdx(found as u32);
                        *idx = new_idx;
                    }
                }
            }
        }
        IrExpr::EnumInst {
            enum_id,
            variant,
            variant_idx,
            fields,
            ..
        } => {
            if let Some(eid) = enum_id {
                if let Some(found) = r
                    .module
                    .get_enum(*eid)
                    .and_then(|e| e.variants.iter().position(|v| v.name == *variant))
                {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "variant count is bounded upstream"
                    )]
                    let new_idx = VariantIdx(found as u32);
                    *variant_idx = new_idx;
                }
            }
            for (fname, fidx, fexpr) in fields.iter_mut() {
                resolve_expr(fexpr, r);
                if let Some(eid) = enum_id {
                    if let Some(found_field) = r
                        .module
                        .get_enum(*eid)
                        .and_then(|e| {
                            e.variants
                                .iter()
                                .find(|v| v.name == *variant)
                                .map(|v| v.fields.iter().position(|f| f.name == *fname))
                        })
                        .flatten()
                    {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "field count is bounded upstream"
                        )]
                        let new_field_idx = FieldIdx(found_field as u32);
                        *fidx = new_field_idx;
                    }
                }
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                resolve_expr(e, r);
            }
        }
        IrExpr::BinaryOp { left, right, .. } => {
            resolve_expr(left, r);
            resolve_expr(right, r);
        }
        IrExpr::UnaryOp { operand, .. } => {
            resolve_expr(operand, r);
        }
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            resolve_expr(condition, r);
            r.push_scope();
            resolve_expr(then_branch, r);
            r.pop_scope();
            if let Some(eb) = else_branch.as_mut() {
                r.push_scope();
                resolve_expr(eb, r);
                r.pop_scope();
            }
        }
        IrExpr::For {
            var,
            var_binding_id,
            collection,
            body,
            ..
        } => {
            resolve_expr(collection, r);
            r.push_scope();
            let id = r.fresh();
            *var_binding_id = id;
            r.bind(var.clone(), id, BindingKind::Local);
            resolve_expr(body, r);
            r.pop_scope();
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            resolve_expr(scrutinee, r);
            let scrutinee_ty = scrutinee.ty().clone();
            for arm in arms {
                resolve_match_arm(arm, &scrutinee_ty, r);
            }
        }
        IrExpr::Closure {
            params,
            captures,
            body,
            ..
        } => {
            // Capture binding-id resolution: each capture's
            // `outer_binding_id` must point at the introducing
            // binding *in the enclosing scope*, which we look up
            // BEFORE pushing the closure's own scope frame.
            for (cap_bid, cap_name, _, _) in captures.iter_mut() {
                if let Some((id, _)) = r.lookup(cap_name) {
                    *cap_bid = id;
                }
            }
            r.push_scope();
            for (_, param_bid, name, _) in params.iter_mut() {
                let id = r.fresh();
                *param_bid = id;
                r.bind(name.clone(), id, BindingKind::Local);
            }
            resolve_expr(body, r);
            r.pop_scope();
        }
        IrExpr::ClosureRef { env_struct, .. } => {
            resolve_expr(env_struct, r);
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                resolve_expr(k, r);
                resolve_expr(v, r);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            resolve_expr(dict, r);
            resolve_expr(key, r);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            r.push_scope();
            for stmt in statements {
                walkers::resolve_block_stmt(stmt, r);
            }
            resolve_expr(result, r);
            r.pop_scope();
        }
    }
}
