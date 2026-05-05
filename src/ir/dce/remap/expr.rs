use crate::ir::IrExpr;

use super::ty::remap_type;
use super::IdRemap;

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive match over every IrExpr variant; splitting would hide the structural walk"
)]
pub(super) fn remap_expr(expr: &mut IrExpr, remap: &IdRemap) {
    remap_type(expr.ty_mut(), remap);
    match expr {
        IrExpr::StructInst {
            struct_id,
            type_args,
            fields,
            ..
        } => {
            if let Some(id) = struct_id {
                if let Some(new) = remap.struct_of(*id) {
                    *id = new;
                }
            }
            for t in type_args {
                remap_type(t, remap);
            }
            for (_, _, e) in fields {
                remap_expr(e, remap);
            }
        }
        IrExpr::EnumInst {
            enum_id, fields, ..
        } => {
            if let Some(id) = enum_id {
                if let Some(new) = remap.enum_of(*id) {
                    *id = new;
                }
            }
            for (_, _, e) in fields {
                remap_expr(e, remap);
            }
        }
        IrExpr::BinaryOp { left, right, .. } => {
            remap_expr(left, remap);
            remap_expr(right, remap);
        }
        IrExpr::UnaryOp { operand, .. } => remap_expr(operand, remap),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            remap_expr(condition, remap);
            remap_expr(then_branch, remap);
            if let Some(eb) = else_branch {
                remap_expr(eb, remap);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                remap_expr(e, remap);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                remap_expr(e, remap);
            }
        }
        IrExpr::FieldAccess { object, .. } => remap_expr(object, remap),
        IrExpr::For {
            var_ty,
            collection,
            body,
            ..
        } => {
            remap_type(var_ty, remap);
            remap_expr(collection, remap);
            remap_expr(body, remap);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            remap_expr(scrutinee, remap);
            for arm in arms {
                for (_, _, t) in &mut arm.bindings {
                    remap_type(t, remap);
                }
                remap_expr(&mut arm.body, remap);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                remap_expr(e, remap);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            remap_expr(closure, remap);
            for (_, e) in args {
                remap_expr(e, remap);
            }
        }
        IrExpr::MethodCall {
            receiver,
            args,
            dispatch,
            ..
        } => {
            remap_expr(receiver, remap);
            for (_, e) in args {
                remap_expr(e, remap);
            }
            if let crate::ir::DispatchKind::Virtual { trait_id, .. } = dispatch {
                if let Some(new) = remap.trait_of(*trait_id) {
                    *trait_id = new;
                }
            }
        }
        IrExpr::Closure {
            params,
            captures,
            body,
            ..
        } => {
            for (_, _, _, t) in params {
                remap_type(t, remap);
            }
            for (_, _, _, t) in captures {
                remap_type(t, remap);
            }
            remap_expr(body, remap);
        }
        IrExpr::ClosureRef { env_struct, ty, .. } => {
            remap_type(ty, remap);
            remap_expr(env_struct, remap);
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                remap_expr(k, remap);
                remap_expr(v, remap);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            remap_expr(dict, remap);
            remap_expr(key, remap);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements.iter_mut() {
                remap_block_statement(stmt, remap);
            }
            remap_expr(result, remap);
        }
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
    }
}

pub(super) fn remap_block_statement(stmt: &mut crate::ir::IrBlockStatement, remap: &IdRemap) {
    use crate::ir::IrBlockStatement;
    match stmt {
        IrBlockStatement::Let { ty, value, .. } => {
            if let Some(t) = ty {
                remap_type(t, remap);
            }
            remap_expr(value, remap);
        }
        IrBlockStatement::Assign { target, value, .. } => {
            remap_expr(target, remap);
            remap_expr(value, remap);
        }
        IrBlockStatement::Expr(e) => remap_expr(e, remap),
    }
}
