//! Pure-function folders for binary and unary operations on literals.
//! Both return `None` when an operand combination has no fold rule (the
//! caller leaves the original `BinaryOp` / `UnaryOp` in place).

use crate::ast::{
    BinaryOperator, Literal, NumberLiteral, NumberValue, PrimitiveType, UnaryOperator,
};
use crate::ir::{IrExpr, ResolvedType};

/// Try to fold a binary operation on two literal values.
///
/// `operand_ty` is the type of the two operands. A numeric fold computes
/// in the width of that type, not in the width of the result.
pub(super) fn fold_binary_op(
    left: &Literal,
    op: BinaryOperator,
    right: &Literal,
    operand_ty: &ResolvedType,
    ty: &ResolvedType,
    span: crate::ir::IrSpan,
) -> Option<IrExpr> {
    match (left, right) {
        (Literal::Number(l), Literal::Number(r)) => {
            fold_numeric_pair(*l, op, *r, operand_ty, ty, span)
        }
        (Literal::Boolean(l), Literal::Boolean(r)) => fold_boolean_pair(*l, op, *r, span),
        (Literal::String(l), Literal::String(r)) => fold_string_pair(l, op, r, span),
        _ => None,
    }
}

/// The numeric width that a fold computes in.
#[derive(Clone, Copy)]
enum Width {
    I32,
    I64,
    F32,
    F64,
}

impl Width {
    /// The width of `ty`, or `None` when `ty` is not a numeric
    /// primitive. The fold does not touch an operation of an unknown
    /// width.
    const fn of(ty: &ResolvedType) -> Option<Self> {
        let ResolvedType::Primitive(primitive) = ty else {
            return None;
        };
        match primitive {
            PrimitiveType::I32 => Some(Self::I32),
            PrimitiveType::I64 => Some(Self::I64),
            PrimitiveType::F32 => Some(Self::F32),
            PrimitiveType::F64 => Some(Self::F64),
            PrimitiveType::String | PrimitiveType::Boolean | PrimitiveType::Never => None,
        }
    }
}

fn fold_numeric_pair(
    l: NumberLiteral,
    op: BinaryOperator,
    r: NumberLiteral,
    operand_ty: &ResolvedType,
    ty: &ResolvedType,
    span: crate::ir::IrSpan,
) -> Option<IrExpr> {
    let result = match Width::of(operand_ty)? {
        Width::I32 => {
            let li = i32::try_from(integer_value(l)?).ok()?;
            let ri = i32::try_from(integer_value(r)?).ok()?;
            fold_integer_pair(l, li, op, ri)
        }
        Width::I64 => {
            let li = i64::try_from(integer_value(l)?).ok()?;
            let ri = i64::try_from(integer_value(r)?).ok()?;
            fold_integer_pair(l, li, op, ri)
        }
        Width::F32 => fold_f32_pair(l, op, r),
        Width::F64 => fold_float_pair(l, l.value.as_f64(), op, r.value.as_f64()),
    }?;
    Some(build_numeric_result(result, ty, span))
}

/// The value of an integer literal, or `None` for a float literal.
const fn integer_value(n: NumberLiteral) -> Option<i128> {
    match n.value {
        NumberValue::Integer(v) => Some(v),
        NumberValue::Float(_) => None,
    }
}

/// The checked integer operations of one width.
trait CheckedInt: Copy + Ord + Into<i128> {
    fn add(self, other: Self) -> Option<Self>;
    fn sub(self, other: Self) -> Option<Self>;
    fn mul(self, other: Self) -> Option<Self>;
    fn div(self, other: Self) -> Option<Self>;
    fn rem(self, other: Self) -> Option<Self>;
}

macro_rules! checked_int {
    ($t:ty) => {
        impl CheckedInt for $t {
            fn add(self, other: Self) -> Option<Self> {
                self.checked_add(other)
            }
            fn sub(self, other: Self) -> Option<Self> {
                self.checked_sub(other)
            }
            fn mul(self, other: Self) -> Option<Self> {
                self.checked_mul(other)
            }
            fn div(self, other: Self) -> Option<Self> {
                self.checked_div(other)
            }
            fn rem(self, other: Self) -> Option<Self> {
                self.checked_rem(other)
            }
        }
    };
}

checked_int!(i32);
checked_int!(i64);

/// Fold two integers in the width of `T`. An operation that overflows
/// `T`, or that divides by zero, stays unfolded, so the backend decides
/// what it gives.
fn fold_integer_pair<T: CheckedInt>(
    l: NumberLiteral,
    li: T,
    op: BinaryOperator,
    ri: T,
) -> Option<Literal> {
    // The result carries the suffix of the left operand.
    let combine = |v: T| {
        Literal::Number(NumberLiteral::from_lex(
            NumberValue::Integer(v.into()),
            l.suffix,
            l.kind,
        ))
    };
    match op {
        BinaryOperator::Add => li.add(ri).map(combine),
        BinaryOperator::Sub => li.sub(ri).map(combine),
        BinaryOperator::Mul => li.mul(ri).map(combine),
        BinaryOperator::Div => li.div(ri).map(combine),
        BinaryOperator::Mod => li.rem(ri).map(combine),
        BinaryOperator::Lt => Some(Literal::Boolean(li < ri)),
        BinaryOperator::Le => Some(Literal::Boolean(li <= ri)),
        BinaryOperator::Gt => Some(Literal::Boolean(li > ri)),
        BinaryOperator::Ge => Some(Literal::Boolean(li >= ri)),
        BinaryOperator::Eq => Some(Literal::Boolean(li == ri)),
        BinaryOperator::Ne => Some(Literal::Boolean(li != ri)),
        BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Range => None,
    }
}

/// Fold two `F32` operands with binary32 arithmetic. Each operand
/// rounds to `f32` first, as a backend stores it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the rounding to binary32 is the point: an F32 value is an f32"
)]
fn fold_f32_pair(l: NumberLiteral, op: BinaryOperator, r: NumberLiteral) -> Option<Literal> {
    let lv = l.value.as_f64() as f32;
    let rv = r.value.as_f64() as f32;
    let combine = |v: f32| {
        v.is_finite().then(|| {
            Literal::Number(NumberLiteral::from_lex(
                NumberValue::Float(f64::from(v)),
                l.suffix,
                l.kind,
            ))
        })
    };
    float_op(lv, op, rv, combine)
}

/// Fold two `F64` operands with IEEE 754 binary64 arithmetic.
fn fold_float_pair(l: NumberLiteral, lv: f64, op: BinaryOperator, rv: f64) -> Option<Literal> {
    let combine = |v: f64| {
        v.is_finite().then(|| {
            Literal::Number(NumberLiteral::from_lex(
                NumberValue::Float(v),
                l.suffix,
                l.kind,
            ))
        })
    };
    float_op(lv, op, rv, combine)
}

/// One float operation in the width of `F`. A result that is not
/// finite stays unfolded: the IR JSON cannot hold it, and the backend
/// decides what an overflow gives.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "IEEE 754 float arithmetic cannot panic; a result that is not finite stays unfolded"
)]
fn float_op<F>(
    lv: F,
    op: BinaryOperator,
    rv: F,
    combine: impl Fn(F) -> Option<Literal>,
) -> Option<Literal>
where
    F: Copy
        + PartialOrd
        + core::ops::Add<Output = F>
        + core::ops::Sub<Output = F>
        + core::ops::Mul<Output = F>
        + core::ops::Div<Output = F>
        + core::ops::Rem<Output = F>
        + Default,
{
    let zero = F::default();
    match op {
        BinaryOperator::Add => combine(lv + rv),
        BinaryOperator::Sub => combine(lv - rv),
        BinaryOperator::Mul => combine(lv * rv),
        BinaryOperator::Div if rv != zero => combine(lv / rv),
        // A float `%` with a non-zero divisor, as the runtime computes it.
        BinaryOperator::Mod if rv != zero => combine(lv % rv),
        BinaryOperator::Lt => Some(Literal::Boolean(lv < rv)),
        BinaryOperator::Le => Some(Literal::Boolean(lv <= rv)),
        BinaryOperator::Gt => Some(Literal::Boolean(lv > rv)),
        BinaryOperator::Ge => Some(Literal::Boolean(lv >= rv)),
        // IEEE 754 equality: NaN is not equal to NaN, and +0.0 equals
        // -0.0. A bit-level comparison would disagree on signed zero.
        BinaryOperator::Eq => Some(Literal::Boolean(lv == rv)),
        BinaryOperator::Ne => Some(Literal::Boolean(lv != rv)),
        BinaryOperator::Div
        | BinaryOperator::Mod
        | BinaryOperator::And
        | BinaryOperator::Or
        | BinaryOperator::Range => None,
    }
}

fn build_numeric_result(value: Literal, ty: &ResolvedType, span: crate::ir::IrSpan) -> IrExpr {
    let result_ty = match &value {
        Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
        Literal::String(_) | Literal::Number(_) | Literal::Nil => ty.clone(),
    };
    IrExpr::Literal {
        value,
        ty: result_ty,
        span,
    }
}

fn fold_boolean_pair(
    l: bool,
    op: BinaryOperator,
    r: bool,
    span: crate::ir::IrSpan,
) -> Option<IrExpr> {
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
        span,
    })
}

fn fold_string_pair(
    l: &str,
    op: BinaryOperator,
    r: &str,
    span: crate::ir::IrSpan,
) -> Option<IrExpr> {
    if op == BinaryOperator::Add {
        Some(IrExpr::Literal {
            value: Literal::String(format!("{l}{r}")),
            ty: ResolvedType::Primitive(PrimitiveType::String),
            span,
        })
    } else {
        None
    }
}

pub(super) fn fold_unary_op(
    op: UnaryOperator,
    operand: &Literal,
    ty: &ResolvedType,
    span: crate::ir::IrSpan,
) -> Option<IrExpr> {
    match operand {
        Literal::Number(n) => {
            if op != UnaryOperator::Neg {
                return None;
            }
            // Negate in the width of the type. The negation of the lowest
            // value overflows that width, so it stays unfolded.
            let new_value = match (Width::of(ty)?, n.value) {
                (Width::I32, NumberValue::Integer(v)) => {
                    NumberValue::Integer(i32::try_from(v).ok()?.checked_neg()?.into())
                }
                (Width::I64, NumberValue::Integer(v)) => {
                    NumberValue::Integer(i64::try_from(v).ok()?.checked_neg()?.into())
                }
                (Width::F32 | Width::F64, value) => NumberValue::Float(-value.as_f64()),
                (Width::I32 | Width::I64, NumberValue::Float(_)) => return None,
            };
            Some(IrExpr::Literal {
                value: Literal::Number(NumberLiteral::from_lex(new_value, n.suffix, n.kind)),
                ty: ty.clone(),
                span,
            })
        }
        Literal::Boolean(b) => {
            if op == UnaryOperator::Not {
                Some(IrExpr::Literal {
                    value: Literal::Boolean(!b),
                    ty: ResolvedType::Primitive(PrimitiveType::Boolean),
                    span,
                })
            } else {
                None
            }
        }
        Literal::String(_) | Literal::Nil => None,
    }
}
