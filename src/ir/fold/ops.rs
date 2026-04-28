//! Pure-function folders for binary and unary operations on literals.
//! Both return `None` when an operand combination has no fold rule (the
//! caller leaves the original `BinaryOp` / `UnaryOp` in place).

use crate::ast::{
    BinaryOperator, Literal, NumberLiteral, NumberValue, PrimitiveType, UnaryOperator,
};
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
    l: NumberLiteral,
    op: BinaryOperator,
    r: NumberLiteral,
    ty: &ResolvedType,
) -> Option<IrExpr> {
    // Two-Integer pairs fold under exact i128 arithmetic so backends emitting
    // native integer instructions see literally what the source wrote. Any
    // operand carrying a Float payload falls back to f64 IEEE arithmetic.
    if let (NumberValue::Integer(li), NumberValue::Integer(ri)) = (l.value, r.value) {
        return fold_integer_pair(l, li, op, ri, ty);
    }
    fold_float_pair(l, l.value.as_f64(), op, r.value.as_f64(), ty)
}

fn fold_integer_pair(
    l: NumberLiteral,
    li: i128,
    op: BinaryOperator,
    ri: i128,
    ty: &ResolvedType,
) -> Option<IrExpr> {
    // Suffix preservation: arithmetic results carry the left operand's
    // suffix. Mismatched-suffix mixing isn't yet type-checked by semantic.
    let combine = |v: i128| {
        Literal::Number(NumberLiteral::from_lex(
            NumberValue::Integer(v),
            l.suffix,
            l.kind,
        ))
    };
    // Checked arithmetic — overflow leaves the BinaryOp unfolded so codegen
    // can decide what to emit (typically a hard error or wrap per target).
    let result = match op {
        BinaryOperator::Add => li.checked_add(ri).map(combine),
        BinaryOperator::Sub => li.checked_sub(ri).map(combine),
        BinaryOperator::Mul => li.checked_mul(ri).map(combine),
        BinaryOperator::Div if ri != 0 => li.checked_div(ri).map(combine),
        BinaryOperator::Mod if ri != 0 => li.checked_rem(ri).map(combine),
        BinaryOperator::Lt => Some(Literal::Boolean(li < ri)),
        BinaryOperator::Le => Some(Literal::Boolean(li <= ri)),
        BinaryOperator::Gt => Some(Literal::Boolean(li > ri)),
        BinaryOperator::Ge => Some(Literal::Boolean(li >= ri)),
        BinaryOperator::Eq => Some(Literal::Boolean(li == ri)),
        BinaryOperator::Ne => Some(Literal::Boolean(li != ri)),
        BinaryOperator::Div
        | BinaryOperator::Mod
        | BinaryOperator::And
        | BinaryOperator::Or
        | BinaryOperator::Range => None,
    };
    result.map(|value| build_numeric_result(value, ty))
}

fn fold_float_pair(
    l: NumberLiteral,
    lv: f64,
    op: BinaryOperator,
    rv: f64,
    ty: &ResolvedType,
) -> Option<IrExpr> {
    // Mixed Integer/Float (or two Floats) fall back to f64 IEEE 754 arithmetic.
    let combine = |v: f64| {
        Literal::Number(NumberLiteral::from_lex(
            NumberValue::Float(v),
            l.suffix,
            l.kind,
        ))
    };
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
    result.map(|value| build_numeric_result(value, ty))
}

fn build_numeric_result(value: Literal, ty: &ResolvedType) -> IrExpr {
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
                let new_value = match n.value {
                    // checked_neg avoids `i128::MIN.wrapping_neg()` silently
                    // wrapping back to MIN; on overflow leave the UnaryOp
                    // un-folded.
                    NumberValue::Integer(v) => NumberValue::Integer(v.checked_neg()?),
                    NumberValue::Float(f) => NumberValue::Float(-f),
                };
                Some(IrExpr::Literal {
                    value: Literal::Number(NumberLiteral::from_lex(new_value, n.suffix, n.kind)),
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
