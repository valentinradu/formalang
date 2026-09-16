//! The expression half of the defunctionalisation rewrite.
//!
//! Two arms do the work — `ClosureRef` becomes a tag and
//! `CallClosure` becomes a direct call — and the rest is plain
//! recursion that carries the type rewrite into every slot.

use crate::ir::{FieldIdx, IrBlockStatement, IrExpr, IrMatchArm, ResolvedType};

use super::rewrite::ty;
use super::synthesis::Plan;
use super::ENV_FIELD_NAME;

#[expect(
    clippy::too_many_lines,
    reason = "one arm per expression shape; the two rewrites sit among plain recursion"
)]
pub(super) fn expr(e: IrExpr, plan: &Plan) -> IrExpr {
    match e {
        // A closure value becomes the tag that names its lifted
        // function, carrying the env struct unchanged.
        IrExpr::ClosureRef {
            funcref,
            env_struct,
            ty: closure_ty,
            span,
        } => {
            let Some(shape) = plan.for_type(&closure_ty) else {
                return IrExpr::ClosureRef {
                    funcref,
                    env_struct,
                    ty: closure_ty,
                    span,
                };
            };
            let name = funcref.last().cloned().unwrap_or_default();
            let Some((variant, variant_idx)) = shape.variants.get(&name).cloned() else {
                return IrExpr::ClosureRef {
                    funcref,
                    env_struct,
                    ty: closure_ty,
                    span,
                };
            };
            let env = expr(*env_struct, plan);
            IrExpr::EnumInst {
                enum_id: Some(shape.enum_id),
                variant,
                variant_idx,
                fields: vec![(ENV_FIELD_NAME.to_string(), FieldIdx(0), env)],
                ty: shape.enum_ty.clone(),
                span,
            }
        }
        // An indirect call becomes a direct call to the dispatch
        // function, with the tag as its first argument.
        IrExpr::CallClosure {
            closure,
            args,
            ty: return_ty,
            span,
        } => {
            let closure_ty = closure.ty().clone();
            let Some(shape) = plan.for_type(&closure_ty) else {
                return IrExpr::CallClosure {
                    closure: Box::new(expr(*closure, plan)),
                    args: args.into_iter().map(|(n, a)| (n, expr(a, plan))).collect(),
                    ty: return_ty,
                    span,
                };
            };
            let mut call_args = vec![(Some("f".to_string()), expr(*closure, plan))];
            for (position, (_, arg)) in args.into_iter().enumerate() {
                call_args.push((Some(format!("p{position}")), expr(arg, plan)));
            }
            IrExpr::FunctionCall {
                path: vec![shape.call_fn.clone()],
                function_id: None,
                args: call_args,
                ty: return_ty,
                span,
            }
        }

        IrExpr::Literal {
            value,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::Literal {
                value,
                ty: slot,
                span,
            }
        }
        IrExpr::Reference {
            path,
            target,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::Reference {
                path,
                target,
                ty: slot,
                span,
            }
        }
        IrExpr::LetRef {
            name,
            binding_id,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::LetRef {
                name,
                binding_id,
                ty: slot,
                span,
            }
        }
        IrExpr::SelfFieldRef {
            field,
            field_idx,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::SelfFieldRef {
                field,
                field_idx,
                ty: slot,
                span,
            }
        }
        IrExpr::StructInst {
            struct_id,
            mut type_args,
            fields,
            ty: mut slot,
            span,
        } => {
            for arg in &mut type_args {
                ty_of(arg, plan);
            }
            ty_of(&mut slot, plan);
            IrExpr::StructInst {
                struct_id,
                type_args,
                fields: fields
                    .into_iter()
                    .map(|(n, i, v)| (n, i, expr(v, plan)))
                    .collect(),
                ty: slot,
                span,
            }
        }
        IrExpr::EnumInst {
            enum_id,
            variant,
            variant_idx,
            fields,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::EnumInst {
                enum_id,
                variant,
                variant_idx,
                fields: fields
                    .into_iter()
                    .map(|(n, i, v)| (n, i, expr(v, plan)))
                    .collect(),
                ty: slot,
                span,
            }
        }
        IrExpr::Array {
            elements,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::Array {
                elements: elements.into_iter().map(|v| expr(v, plan)).collect(),
                ty: slot,
                span,
            }
        }
        IrExpr::Tuple {
            fields,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::Tuple {
                fields: fields
                    .into_iter()
                    .map(|(n, v)| (n, expr(v, plan)))
                    .collect(),
                ty: slot,
                span,
            }
        }
        IrExpr::FieldAccess {
            object,
            field,
            field_idx,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::FieldAccess {
                object: Box::new(expr(*object, plan)),
                field,
                field_idx,
                ty: slot,
                span,
            }
        }
        IrExpr::BinaryOp {
            left,
            op,
            right,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::BinaryOp {
                left: Box::new(expr(*left, plan)),
                op,
                right: Box::new(expr(*right, plan)),
                ty: slot,
                span,
            }
        }
        IrExpr::UnaryOp {
            op,
            operand,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::UnaryOp {
                op,
                operand: Box::new(expr(*operand, plan)),
                ty: slot,
                span,
            }
        }
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::If {
                condition: Box::new(expr(*condition, plan)),
                then_branch: Box::new(expr(*then_branch, plan)),
                else_branch: else_branch.map(|b| Box::new(expr(*b, plan))),
                ty: slot,
                span,
            }
        }
        IrExpr::For {
            var,
            mut var_ty,
            var_binding_id,
            collection,
            body,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut var_ty, plan);
            ty_of(&mut slot, plan);
            IrExpr::For {
                var,
                var_ty,
                var_binding_id,
                collection: Box::new(expr(*collection, plan)),
                body: Box::new(expr(*body, plan)),
                ty: slot,
                span,
            }
        }
        IrExpr::Match {
            scrutinee,
            arms,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::Match {
                scrutinee: Box::new(expr(*scrutinee, plan)),
                arms: arms
                    .into_iter()
                    .map(|mut arm| {
                        for (_, _, binding_ty) in &mut arm.bindings {
                            ty_of(binding_ty, plan);
                        }
                        IrMatchArm {
                            body: expr(arm.body, plan),
                            ..arm
                        }
                    })
                    .collect(),
                ty: slot,
                span,
            }
        }
        IrExpr::FunctionCall {
            path,
            function_id,
            args,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::FunctionCall {
                path,
                function_id,
                args: args.into_iter().map(|(n, a)| (n, expr(a, plan))).collect(),
                ty: slot,
                span,
            }
        }
        IrExpr::MethodCall {
            receiver,
            method,
            method_idx,
            args,
            dispatch,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::MethodCall {
                receiver: Box::new(expr(*receiver, plan)),
                method,
                method_idx,
                args: args.into_iter().map(|(n, a)| (n, expr(a, plan))).collect(),
                dispatch,
                ty: slot,
                span,
            }
        }
        IrExpr::DictLiteral {
            entries,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::DictLiteral {
                entries: entries
                    .into_iter()
                    .map(|(k, v)| (expr(k, plan), expr(v, plan)))
                    .collect(),
                ty: slot,
                span,
            }
        }
        IrExpr::DictAccess {
            dict,
            key,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::DictAccess {
                dict: Box::new(expr(*dict, plan)),
                key: Box::new(expr(*key, plan)),
                ty: slot,
                span,
            }
        }
        IrExpr::Block {
            statements,
            result,
            ty: mut slot,
            span,
        } => {
            ty_of(&mut slot, plan);
            IrExpr::Block {
                statements: statements.into_iter().map(|s| statement(s, plan)).collect(),
                result: Box::new(expr(*result, plan)),
                ty: slot,
                span,
            }
        }
        // Closure conversion removed every `Closure` node before this
        // pass runs, so one here is already an error the previous pass
        // reported. Pass it through untouched.
        IrExpr::Closure { .. } => e,
    }
}

fn statement(s: IrBlockStatement, plan: &Plan) -> IrBlockStatement {
    match s {
        IrBlockStatement::Let {
            binding_id,
            name,
            mutable,
            ty: mut slot,
            value,
            span,
        } => {
            if let Some(slot_ty) = &mut slot {
                ty_of(slot_ty, plan);
            }
            IrBlockStatement::Let {
                binding_id,
                name,
                mutable,
                ty: slot,
                value: expr(value, plan),
                span,
            }
        }
        IrBlockStatement::Assign {
            target,
            value,
            span,
        } => IrBlockStatement::Assign {
            target: expr(target, plan),
            value: expr(value, plan),
            span,
        },
        IrBlockStatement::Expr(e) => IrBlockStatement::Expr(expr(e, plan)),
    }
}

/// Shorthand so each expression arm reads as one line.
fn ty_of(slot: &mut ResolvedType, plan: &Plan) {
    ty(slot, plan);
}
