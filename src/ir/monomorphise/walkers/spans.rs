//! Mutable walks over every `IrSpan` reachable from a function or expression.
//! Used by file-id remapping to relocate spans from a clone-source module's
//! `file_table` index space onto the host module's.

use crate::ir::{IrBlockStatement, IrExpr, IrFunction, IrSpan};

/// Visit every `IrSpan` reachable through `f` (param spans, return-type
/// — which has no span — body expressions, default-value expressions).
/// The function header span itself is *not* visited; callers handle it
/// explicitly so they can decide whether the function is a clone whose
/// outer span needs remapping.
pub(in crate::ir::monomorphise) fn walk_function_spans_mut(
    f: &mut IrFunction,
    visit: &mut impl FnMut(&mut IrSpan),
) {
    for p in &mut f.params {
        visit(&mut p.span);
        if let Some(d) = &mut p.default {
            walk_expr_spans_mut(d, visit);
        }
    }
    if let Some(body) = &mut f.body {
        walk_expr_spans_mut(body, visit);
    }
}

pub(in crate::ir::monomorphise) fn walk_expr_spans_mut(
    expr: &mut IrExpr,
    visit: &mut impl FnMut(&mut IrSpan),
) {
    walk_expr_spans_mut_inner(expr, visit);
}

fn walk_expr_spans_mut_inner(expr: &mut IrExpr, visit: &mut impl FnMut(&mut IrSpan)) {
    visit(expr.span_mut());
    match expr {
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            for (_, _, e) in fields {
                walk_expr_spans_mut_inner(e, visit);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                walk_expr_spans_mut_inner(e, visit);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                walk_expr_spans_mut_inner(e, visit);
            }
        }
        IrExpr::FieldAccess { object, .. } => walk_expr_spans_mut_inner(object, visit),
        IrExpr::BinaryOp { left, right, .. } => {
            walk_expr_spans_mut_inner(left, visit);
            walk_expr_spans_mut_inner(right, visit);
        }
        IrExpr::UnaryOp { operand, .. } => walk_expr_spans_mut_inner(operand, visit),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            walk_expr_spans_mut_inner(condition, visit);
            walk_expr_spans_mut_inner(then_branch, visit);
            if let Some(e) = else_branch {
                walk_expr_spans_mut_inner(e, visit);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            walk_expr_spans_mut_inner(collection, visit);
            walk_expr_spans_mut_inner(body, visit);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr_spans_mut_inner(scrutinee, visit);
            for arm in arms {
                walk_expr_spans_mut_inner(&mut arm.body, visit);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, a) in args {
                walk_expr_spans_mut_inner(a, visit);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            walk_expr_spans_mut_inner(closure, visit);
            for (_, a) in args {
                walk_expr_spans_mut_inner(a, visit);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            walk_expr_spans_mut_inner(receiver, visit);
            for (_, a) in args {
                walk_expr_spans_mut_inner(a, visit);
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr_spans_mut_inner(k, visit);
                walk_expr_spans_mut_inner(v, visit);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            walk_expr_spans_mut_inner(dict, visit);
            walk_expr_spans_mut_inner(key, visit);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                walk_block_stmt_spans_mut(stmt, visit);
            }
            walk_expr_spans_mut_inner(result, visit);
        }
        IrExpr::Closure { body, .. } => walk_expr_spans_mut_inner(body, visit),
        IrExpr::ClosureRef { env_struct, .. } => walk_expr_spans_mut_inner(env_struct, visit),
    }
}

fn walk_block_stmt_spans_mut(stmt: &mut IrBlockStatement, visit: &mut impl FnMut(&mut IrSpan)) {
    match stmt {
        IrBlockStatement::Let { value, span, .. } => {
            visit(span);
            walk_expr_spans_mut_inner(value, visit);
        }
        IrBlockStatement::Assign {
            target,
            value,
            span,
        } => {
            visit(span);
            walk_expr_spans_mut_inner(target, visit);
            walk_expr_spans_mut_inner(value, visit);
        }
        IrBlockStatement::Expr(e) => walk_expr_spans_mut_inner(e, visit),
    }
}
