//! Expression-level dead-code elimination — collapses `if true / if false`
//! branches and recursively rewrites every `IrExpr` variant. Pulled out
//! of `mod.rs` to keep the module file under the 500-LOC ceiling.

use crate::ast::Literal;
use crate::ir::IrExpr;

/// Eliminate dead code from an expression.
///
/// This removes unreachable branches based on constant conditions.
#[expect(
    clippy::too_many_lines,
    reason = "exhaustive match over all IrExpr variants"
)]
pub fn eliminate_dead_code_expr(expr: IrExpr) -> IrExpr {
    match expr {
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ty,
        } => {
            let cond = eliminate_dead_code_expr(*condition);
            if let IrExpr::Literal {
                value: Literal::Boolean(b),
                ..
            } = &cond
            {
                if *b {
                    return eliminate_dead_code_expr(*then_branch);
                } else if let Some(else_b) = else_branch {
                    return eliminate_dead_code_expr(*else_b);
                }
            }
            IrExpr::If {
                condition: Box::new(cond),
                then_branch: Box::new(eliminate_dead_code_expr(*then_branch)),
                else_branch: else_branch.map(|e| Box::new(eliminate_dead_code_expr(*e))),
                ty,
            }
        }
        IrExpr::BinaryOp {
            left,
            op,
            right,
            ty,
        } => IrExpr::BinaryOp {
            left: Box::new(eliminate_dead_code_expr(*left)),
            op,
            right: Box::new(eliminate_dead_code_expr(*right)),
            ty,
        },
        IrExpr::Array { elements, ty } => IrExpr::Array {
            elements: elements.into_iter().map(eliminate_dead_code_expr).collect(),
            ty,
        },
        IrExpr::Tuple { fields, ty } => IrExpr::Tuple {
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, eliminate_dead_code_expr(e)))
                .collect(),
            ty,
        },
        IrExpr::StructInst {
            struct_id,
            type_args,
            fields,
            ty,
        } => IrExpr::StructInst {
            struct_id,
            type_args,
            fields: fields
                .into_iter()
                .map(|(n, idx, e)| (n, idx, eliminate_dead_code_expr(e)))
                .collect(),
            ty,
        },
        IrExpr::For {
            var,
            var_ty,
            var_binding_id,
            collection,
            body,
            ty,
        } => IrExpr::For {
            var,
            var_ty,
            var_binding_id,
            collection: Box::new(eliminate_dead_code_expr(*collection)),
            body: Box::new(eliminate_dead_code_expr(*body)),
            ty,
        },
        IrExpr::Match {
            scrutinee,
            arms,
            ty,
        } => IrExpr::Match {
            scrutinee: Box::new(eliminate_dead_code_expr(*scrutinee)),
            arms: arms
                .into_iter()
                .map(|arm| crate::ir::IrMatchArm {
                    variant: arm.variant,
                    variant_idx: arm.variant_idx,
                    is_wildcard: arm.is_wildcard,
                    bindings: arm.bindings,
                    body: eliminate_dead_code_expr(arm.body),
                })
                .collect(),
            ty,
        },
        IrExpr::FunctionCall {
            path,
            function_id,
            args,
            ty,
        } => IrExpr::FunctionCall {
            path,
            function_id,
            args: args
                .into_iter()
                .map(|(name, e)| (name, eliminate_dead_code_expr(e)))
                .collect(),
            ty,
        },
        IrExpr::CallClosure { closure, args, ty } => IrExpr::CallClosure {
            closure: Box::new(eliminate_dead_code_expr(*closure)),
            args: args
                .into_iter()
                .map(|(name, e)| (name, eliminate_dead_code_expr(e)))
                .collect(),
            ty,
        },
        IrExpr::MethodCall {
            receiver,
            method,
            method_idx,
            args,
            dispatch,
            ty,
        } => IrExpr::MethodCall {
            receiver: Box::new(eliminate_dead_code_expr(*receiver)),
            method,
            method_idx,
            args: args
                .into_iter()
                .map(|(name, e)| (name, eliminate_dead_code_expr(e)))
                .collect(),
            dispatch,
            ty,
        },
        IrExpr::EnumInst {
            enum_id,
            variant,
            variant_idx,
            fields,
            ty,
        } => IrExpr::EnumInst {
            enum_id,
            variant,
            variant_idx,
            fields: fields
                .into_iter()
                .map(|(n, idx, e)| (n, idx, eliminate_dead_code_expr(e)))
                .collect(),
            ty,
        },
        IrExpr::DictLiteral { entries, ty } => IrExpr::DictLiteral {
            entries: entries
                .into_iter()
                .map(|(k, v)| (eliminate_dead_code_expr(k), eliminate_dead_code_expr(v)))
                .collect(),
            ty,
        },
        IrExpr::DictAccess { dict, key, ty } => IrExpr::DictAccess {
            dict: Box::new(eliminate_dead_code_expr(*dict)),
            key: Box::new(eliminate_dead_code_expr(*key)),
            ty,
        },
        IrExpr::Block {
            statements,
            result,
            ty,
        } => IrExpr::Block {
            statements: statements
                .into_iter()
                .map(|stmt| stmt.map_exprs(eliminate_dead_code_expr))
                .collect(),
            result: Box::new(eliminate_dead_code_expr(*result)),
            ty,
        },
        e @ (IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::FieldAccess { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::UnaryOp { .. }
        | IrExpr::Closure { .. }
        | IrExpr::ClosureRef { .. }) => e,
    }
}
