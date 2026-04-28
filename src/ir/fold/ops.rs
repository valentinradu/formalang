//! Pure-function folders for binary and unary operations on literals.
//! Both return `None` when an operand combination has no fold rule (the
//! caller leaves the original `BinaryOp` / `UnaryOp` in place).

use crate::ast::{BinaryOperator, Literal, PrimitiveType, UnaryOperator};
use crate::ir::{IrExpr, ResolvedType};

/// Try to fold a binary operation on two literal values.
pub(super) fn fold_binary_op(
    left: &Literal,
    op: BinaryOperator,
    right: &Literal,
    ty: &ResolvedType,
) -> Option<IrExpr> {
    match (left, right) {
        (Literal::Number(l), Literal::Number(r)) => fold_numeric_pair(*l, op, *r, ty),
        (Literal::Boolean(l), Literal::Boolean(r)) => fold_boolean_pair(*l, op, *r),
        (Literal::String(l), Literal::String(r)) => fold_string_pair(l, op, r),
        _ => None,
    }
}

fn fold_numeric_pair(
    l: crate::ast::NumberLiteral,
    op: BinaryOperator,
    r: crate::ast::NumberLiteral,
    ty: &ResolvedType,
) -> Option<IrExpr> {
    // Suffix preservation: arithmetic results carry the left operand's
    // suffix. Mismatched-suffix mixing isn't yet type-checked by semantic.
    let combine =
        |v: f64| Literal::Number(crate::ast::NumberLiteral::from_lex(v, l.suffix, l.kind));
    let lv = l.value;
    let rv = r.value;
    let result = match op {
        BinaryOperator::Add => Some(combine(lv + rv)),
        BinaryOperator::Sub => Some(combine(lv - rv)),
        BinaryOperator::Mul => Some(combine(lv * rv)),
        BinaryOperator::Div if rv != 0.0 => Some(combine(lv / rv)),
        #[expect(
            clippy::modulo_arithmetic,
            reason = "f64 modulo with rv != 0 guard mirrors BinaryOp::Mod runtime semantics"
        )]
        BinaryOperator::Mod if rv != 0.0 => Some(combine(lv % rv)),
        BinaryOperator::Lt => Some(Literal::Boolean(lv < rv)),
        BinaryOperator::Le => Some(Literal::Boolean(lv <= rv)),
        BinaryOperator::Gt => Some(Literal::Boolean(lv > rv)),
        BinaryOperator::Ge => Some(Literal::Boolean(lv >= rv)),
        // IEEE 754 equality (NaN != NaN, +0.0 == -0.0) — matches `f64::eq`
        // and the ordering ops above. A bit-level comparison would
        // disagree on signed zero.
        #[expect(
            clippy::float_cmp,
            reason = "IEEE 754 equality is intentional for constant folding"
        )]
        BinaryOperator::Eq => Some(Literal::Boolean(lv == rv)),
        #[expect(
            clippy::float_cmp,
            reason = "IEEE 754 inequality is intentional for constant folding"
        )]
        BinaryOperator::Ne => Some(Literal::Boolean(lv != rv)),
        BinaryOperator::Div
        | BinaryOperator::Mod
        | BinaryOperator::And
        | BinaryOperator::Or
        | BinaryOperator::Range => None,
    };

    result.map(|value| {
        let result_ty = match &value {
            Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
            Literal::String(_)
            | Literal::Number(_)
            | Literal::Regex { .. }
            | Literal::Path(_)
            | Literal::Nil => ty.clone(),
        };
        IrExpr::Literal {
            value,
            ty: result_ty,
        }
    })
}

fn fold_boolean_pair(l: bool, op: BinaryOperator, r: bool) -> Option<IrExpr> {
    let result = match op {
        BinaryOperator::And => Some(Literal::Boolean(l && r)),
        BinaryOperator::Or => Some(Literal::Boolean(l || r)),
        BinaryOperator::Eq => Some(Literal::Boolean(l == r)),
        BinaryOperator::Ne => Some(Literal::Boolean(l != r)),
        BinaryOperator::Add
        | BinaryOperator::Sub
        | BinaryOperator::Mul
        | BinaryOperator::Div
        | BinaryOperator::Mod
        | BinaryOperator::Lt
        | BinaryOperator::Gt
        | BinaryOperator::Le
        | BinaryOperator::Ge
        | BinaryOperator::Range => None,
    };
    result.map(|value| IrExpr::Literal {
        value,
        ty: ResolvedType::Primitive(PrimitiveType::Boolean),
    })
}

fn fold_string_pair(l: &str, op: BinaryOperator, r: &str) -> Option<IrExpr> {
    if op == BinaryOperator::Add {
        Some(IrExpr::Literal {
            value: Literal::String(format!("{l}{r}")),
            ty: ResolvedType::Primitive(PrimitiveType::String),
        })
    } else {
        None
    }
}

pub(super) fn fold_unary_op(
    op: UnaryOperator,
    operand: &Literal,
    ty: &ResolvedType,
) -> Option<IrExpr> {
    match operand {
        Literal::Number(n) => {
            if op == UnaryOperator::Neg {
                Some(IrExpr::Literal {
                    value: Literal::Number(crate::ast::NumberLiteral::from_lex(
                        -n.value, n.suffix, n.kind,
                    )),
                    ty: ty.clone(),
                })
            } else {
                None
            }
        }
        Literal::Boolean(b) => {
            if op == UnaryOperator::Not {
                Some(IrExpr::Literal {
                    value: Literal::Boolean(!b),
                    ty: ResolvedType::Primitive(PrimitiveType::Boolean),
                })
            } else {
                None
            }
        }
        Literal::String(_) | Literal::Regex { .. } | Literal::Path(_) | Literal::Nil => None,
    }
}
