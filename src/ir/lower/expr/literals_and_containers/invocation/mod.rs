//! Lowering for the bare-call form `Path(args)`: struct instantiation,
//! external-type instantiation, closure-binding calls and regular function
//! calls (with default-substitution + let-wrapper for forward references
//! to preceding non-defaulted params).

mod closure_call;
mod dispatch;
mod function_call;
mod struct_call;

use crate::ir::{IrBlockStatement, IrExpr, ResolvedType};
use std::collections::{HashMap, HashSet};

/// True if `expr` (or any sub-expression) is an `IrExpr::Reference`
/// with a single-segment path matching one of `names`. Used to
/// decide whether a substituted default needs the let-wrapper that
/// binds preceding non-defaulted params to the call-site values.
pub(super) fn expr_references_any_name(expr: &IrExpr, names: &HashSet<String>) -> bool {
    match expr {
        IrExpr::Reference { path, .. } => {
            path.first().is_some_and(|seg| names.contains(seg.as_str()))
        }
        IrExpr::LetRef { name, .. } => names.contains(name.as_str()),
        IrExpr::BinaryOp { left, right, .. } => {
            expr_references_any_name(left, names) || expr_references_any_name(right, names)
        }
        IrExpr::UnaryOp { operand, .. } => expr_references_any_name(operand, names),
        IrExpr::FieldAccess { object, .. } => expr_references_any_name(object, names),
        IrExpr::FunctionCall { args, .. } | IrExpr::MethodCall { args, .. } => {
            args.iter().any(|(_, a)| expr_references_any_name(a, names))
        }
        IrExpr::CallClosure { closure, args, .. } => {
            expr_references_any_name(closure, names)
                || args.iter().any(|(_, a)| expr_references_any_name(a, names))
        }
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_references_any_name(condition, names)
                || expr_references_any_name(then_branch, names)
                || else_branch
                    .as_deref()
                    .is_some_and(|e| expr_references_any_name(e, names))
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            expr_references_any_name(scrutinee, names)
                || arms
                    .iter()
                    .any(|arm| expr_references_any_name(&arm.body, names))
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            statements.iter().any(|s| match s {
                IrBlockStatement::Let { value, .. } => expr_references_any_name(value, names),
                IrBlockStatement::Assign { target, value, .. } => {
                    expr_references_any_name(target, names)
                        || expr_references_any_name(value, names)
                }
                IrBlockStatement::Expr(e) => expr_references_any_name(e, names),
            }) || expr_references_any_name(result, names)
        }
        IrExpr::Array { elements, .. } => {
            elements.iter().any(|e| expr_references_any_name(e, names))
        }
        IrExpr::Tuple { fields, .. } => fields
            .iter()
            .any(|(_, e)| expr_references_any_name(e, names)),
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => fields
            .iter()
            .any(|(_, _, e)| expr_references_any_name(e, names)),
        IrExpr::DictLiteral { entries, .. } => entries
            .iter()
            .any(|(k, v)| expr_references_any_name(k, names) || expr_references_any_name(v, names)),
        IrExpr::DictAccess { dict, key, .. } => {
            expr_references_any_name(dict, names) || expr_references_any_name(key, names)
        }
        IrExpr::For {
            collection, body, ..
        } => expr_references_any_name(collection, names) || expr_references_any_name(body, names),
        IrExpr::Closure { body, .. } => expr_references_any_name(body, names),
        IrExpr::ClosureRef { env_struct, .. } => expr_references_any_name(env_struct, names),
        IrExpr::Literal { .. } | IrExpr::SelfFieldRef { .. } => false,
    }
}

/// Walk `pattern` (a struct field's declared type containing `TypeParam`s)
/// alongside `concrete` (a lowered argument's type) and record each
/// `TypeParam(name) -> concrete` binding into `out`. Conflicts (the same
/// name bound to two different types) and shape mismatches are silently
/// skipped; caller checks coverage afterwards.
pub(super) fn unify_type_args(
    pattern: &ResolvedType,
    concrete: &ResolvedType,
    out: &mut HashMap<String, ResolvedType>,
) {
    match (pattern, concrete) {
        (ResolvedType::TypeParam(name), other) => {
            out.entry(name.clone()).or_insert_with(|| other.clone());
        }
        (ResolvedType::Tuple(p_fields), ResolvedType::Tuple(c_fields)) => {
            for ((_, pt), (_, ct)) in p_fields.iter().zip(c_fields.iter()) {
                unify_type_args(pt, ct, out);
            }
        }
        (
            ResolvedType::Generic {
                args: p_args,
                base: p_base,
            },
            ResolvedType::Generic {
                args: c_args,
                base: c_base,
            },
        ) if p_base == c_base => {
            for (pa, ca) in p_args.iter().zip(c_args.iter()) {
                unify_type_args(pa, ca, out);
            }
        }
        (
            ResolvedType::Closure {
                param_tys: p_params,
                return_ty: p_ret,
            },
            ResolvedType::Closure {
                param_tys: c_params,
                return_ty: c_ret,
            },
        ) => {
            for ((_, pt), (_, ct)) in p_params.iter().zip(c_params.iter()) {
                unify_type_args(pt, ct, out);
            }
            unify_type_args(p_ret, c_ret, out);
        }
        _ => {}
    }
}
