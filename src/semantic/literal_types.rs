//! The type of a numeric literal that has no suffix.
//!
//! An unsuffixed literal takes its type from the position it is in,
//! when that position declares a type of the same kind: an integer
//! literal takes `I32` or `I64`, and a float literal takes `F32` or
//! `F64`. `let big: I64 = 3000000000` is an `I64`. With no such type,
//! the default applies: `I32` for integer syntax, `F64` for float
//! syntax. See `docs/user/types.md`.
//!
//! The check of the expressions records the type of each literal that
//! takes one from its position. [`apply_literal_types`] then writes each
//! type into the AST as a suffix, so IR lowering sees the same type.
//!
//! A minus sign directly before a numeric literal is part of the
//! literal: [`fold_negative_literals`] makes `-2147483648` one literal
//! before the check, so the range check sees the negative value.

use super::ast_walk::visit_exprs_mut;
use super::sem_type::SemType;
use crate::ast::{
    Expr, File, Literal, NumberLiteral, NumberSourceKind, NumberValue, NumericSuffix,
    PrimitiveType, UnaryOperator,
};
use std::collections::HashMap;

/// The identity of an expression node: its address. The AST does not
/// move between the check and [`apply_literal_types`], so the address
/// names one node for that time.
pub(super) fn node_key(expr: &Expr) -> usize {
    std::ptr::from_ref(expr).addr()
}

/// Make each `-<numeric literal>` one literal with a negative value.
pub(super) fn fold_negative_literals(file: &mut File) {
    visit_exprs_mut(file, &mut |expr| {
        let Expr::UnaryOp {
            op: UnaryOperator::Neg,
            operand,
            span,
        } = expr
        else {
            return;
        };
        let Expr::Literal {
            value: Literal::Number(n),
            ..
        } = &**operand
        else {
            return;
        };
        let value = match n.value {
            NumberValue::Integer(v) => match v.checked_neg() {
                Some(negated) => NumberValue::Integer(negated),
                None => return,
            },
            NumberValue::Float(v) => NumberValue::Float(-v),
        };
        *expr = Expr::Literal {
            value: Literal::Number(NumberLiteral {
                value,
                suffix: n.suffix,
                kind: n.kind,
            }),
            span: *span,
        };
    });
}

/// The type that an unsuffixed literal of `kind` takes in a position
/// that expects `expected`, or `None` when the default applies.
pub(super) fn contextual_type(
    kind: NumberSourceKind,
    expected: Option<&SemType>,
) -> Option<PrimitiveType> {
    let mut expected = expected?;
    while let SemType::Optional(inner) = expected {
        expected = inner;
    }
    let SemType::Primitive(target) = expected else {
        return None;
    };
    match (kind, target) {
        (NumberSourceKind::Integer, PrimitiveType::I32 | PrimitiveType::I64)
        | (NumberSourceKind::Float, PrimitiveType::F32 | PrimitiveType::F64) => Some(*target),
        _ => None,
    }
}

/// Write each recorded literal type into the AST as a suffix.
pub(super) fn apply_literal_types(file: &mut File, types: &HashMap<usize, PrimitiveType>) {
    if types.is_empty() {
        return;
    }
    visit_exprs_mut(file, &mut |expr| {
        let Some(target) = types.get(&node_key(expr)).copied() else {
            return;
        };
        if let Expr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            if n.suffix.is_none() {
                n.suffix = suffix_of(target);
            }
        }
    });
}

const fn suffix_of(target: PrimitiveType) -> Option<NumericSuffix> {
    match target {
        PrimitiveType::I32 => Some(NumericSuffix::I32),
        PrimitiveType::I64 => Some(NumericSuffix::I64),
        PrimitiveType::F32 => Some(NumericSuffix::F32),
        PrimitiveType::F64 => Some(NumericSuffix::F64),
        PrimitiveType::String | PrimitiveType::Boolean | PrimitiveType::Never => None,
    }
}
