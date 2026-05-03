//! Direct-child walker over `IrExpr`. Centralises the exhaustive variant
//! match so passes that recursively rewrite expressions can stay short and
//! avoid wildcard match arms.

use crate::ir::{IrBlockStatement, IrExpr};

/// Visit each direct child expression of `expr` with `visit`, without
/// touching `expr` itself. Centralises the exhaustive `IrExpr` match
/// so passes that recursively process expressions (e.g. path
/// qualification, id remapping) can stay short and avoid wildcard
/// match arms.
pub(crate) fn walk_expr_children_mut(expr: &mut IrExpr, visit: &mut impl FnMut(&mut IrExpr)) {
    match expr {
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            for (_, _, e) in fields {
                visit(e);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                visit(e);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                visit(e);
            }
        }
        IrExpr::FieldAccess { object, .. } => visit(object),
        IrExpr::BinaryOp { left, right, .. } => {
            visit(left);
            visit(right);
        }
        IrExpr::UnaryOp { operand, .. } => visit(operand),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visit(condition);
            visit(then_branch);
            if let Some(e) = else_branch {
                visit(e);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            visit(collection);
            visit(body);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            visit(scrutinee);
            for arm in arms {
                visit(&mut arm.body);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, a) in args {
                visit(a);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            visit(closure);
            for (_, a) in args {
                visit(a);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            visit(receiver);
            for (_, a) in args {
                visit(a);
            }
        }
        IrExpr::Closure { body, .. } => visit(body),
        IrExpr::ClosureRef { env_struct, .. } => visit(env_struct),
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                visit(k);
                visit(v);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            visit(dict);
            visit(key);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                walk_block_stmt_children_mut(stmt, visit);
            }
            visit(result);
        }
    }
}

fn walk_block_stmt_children_mut(stmt: &mut IrBlockStatement, visit: &mut impl FnMut(&mut IrExpr)) {
    match stmt {
        IrBlockStatement::Let { value, .. } => visit(value),
        IrBlockStatement::Assign { target, value, .. } => {
            visit(target);
            visit(value);
        }
        IrBlockStatement::Expr(e) => visit(e),
    }
}
