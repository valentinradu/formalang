//! Pratt-style operator layer for expressions: unary, arithmetic,
//! comparison, equality, logical, range, plus the postfix `[]` and
//! method/field-access dispatch.

use chumsky::input::ValueInput;
use chumsky::pratt::{infix, left, postfix, prefix};
use chumsky::prelude::*;

use crate::ast::{BinaryOperator, Expr, Ident, UnaryOperator};
use crate::lexer::Token;

use super::super::{ident_parser, span_from_simple};

type MethodCallArgs = Vec<(Option<Ident>, Expr)>;

/// Wrap an atom parser with the full pratt-style operator stack.
#[expect(
    clippy::too_many_lines,
    reason = "one entry per operator precedence band — splitting would scatter the precedence story"
)]
pub(super) fn apply_operators<'tokens, I>(
    atom: impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone + 'tokens,
    expr: impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone + 'tokens,
    invocation_args: impl Parser<'tokens, I, MethodCallArgs, extra::Err<Rich<'tokens, Token>>>
        + Clone
        + 'tokens,
) -> impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    atom.pratt((
        // Unary operators (highest precedence: 9)
        prefix(9, just(Token::Minus), |_, operand, e| Expr::UnaryOp {
            op: UnaryOperator::Neg,
            operand: Box::new(operand),
            span: span_from_simple(e.span()),
        }),
        prefix(9, just(Token::Bang), |_, operand, e| Expr::UnaryOp {
            op: UnaryOperator::Not,
            operand: Box::new(operand),
            span: span_from_simple(e.span()),
        }),
        // Multiplication, division, modulo (precedence: 6)
        infix(left(6), just(Token::Star), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Mul,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        infix(left(6), just(Token::Slash), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Div,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        infix(left(6), just(Token::Percent), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Mod,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        // Addition, subtraction (precedence: 5)
        infix(left(5), just(Token::Plus), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Add,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        infix(left(5), just(Token::Minus), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Sub,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        // Comparison (precedence: 4)
        infix(left(4), just(Token::Lt), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Lt,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        infix(left(4), just(Token::Gt), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Gt,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        infix(left(4), just(Token::Le), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Le,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        infix(left(4), just(Token::Ge), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Ge,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        // Equality (precedence: 3)
        infix(left(3), just(Token::EqEq), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Eq,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        infix(left(3), just(Token::Ne), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Ne,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        // Logical AND (precedence: 2)
        infix(left(2), just(Token::And), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::And,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        // Logical OR (precedence: 1)
        infix(left(1), just(Token::Or), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Or,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        // Range (precedence: 0 — arithmetic binds tighter)
        infix(left(0), just(Token::DotDot), |l, _, r, e| Expr::BinaryOp {
            left: Box::new(l),
            op: BinaryOperator::Range,
            right: Box::new(r),
            span: span_from_simple(e.span()),
        }),
        // Dictionary/array access: `expr[key]` (precedence: 10).
        postfix(
            10,
            expr.delimited_by(just(Token::LBracket), just(Token::RBracket)),
            |dict, key, e| Expr::DictAccess {
                dict: Box::new(dict),
                key: Box::new(key),
                span: span_from_simple(e.span()),
            },
        ),
        // Method call: `expr.method(args)` (precedence: 11). Must come
        // before field access so it wins on `expr.ident(...)` shapes.
        postfix(
            11,
            just(Token::Dot)
                .ignore_then(ident_parser())
                .then(invocation_args),
            |receiver, (method, args): (Ident, MethodCallArgs), e| Expr::MethodCall {
                receiver: Box::new(receiver),
                method,
                args,
                span: span_from_simple(e.span()),
            },
        ),
        // Field access: `expr.field` (precedence: 10). Reference paths
        // get the field appended; other shapes wrap in `FieldAccess`.
        // Enum instantiation `Type.variant(args)` is parsed as an atom
        // and so doesn't reach here.
        postfix(
            10,
            just(Token::Dot).ignore_then(ident_parser()),
            |object, field, e| match object {
                Expr::Reference { mut path, .. } => {
                    path.push(field);
                    Expr::Reference {
                        path,
                        span: span_from_simple(e.span()),
                    }
                }
                Expr::Literal { .. }
                | Expr::Invocation { .. }
                | Expr::EnumInstantiation { .. }
                | Expr::InferredEnumInstantiation { .. }
                | Expr::Array { .. }
                | Expr::Tuple { .. }
                | Expr::BinaryOp { .. }
                | Expr::UnaryOp { .. }
                | Expr::ForExpr { .. }
                | Expr::IfExpr { .. }
                | Expr::MatchExpr { .. }
                | Expr::Group { .. }
                | Expr::DictLiteral { .. }
                | Expr::DictAccess { .. }
                | Expr::FieldAccess { .. }
                | Expr::ClosureExpr { .. }
                | Expr::LetExpr { .. }
                | Expr::MethodCall { .. }
                | Expr::Block { .. } => Expr::FieldAccess {
                    object: Box::new(object),
                    field,
                    span: span_from_simple(e.span()),
                },
            },
        ),
    ))
}
