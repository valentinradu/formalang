//! Literal parser for primitive token-level literals.

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{Expr, Literal};
use crate::lexer::Token;

use super::super::span_from_simple;

/// Parse a literal expression. Each branch produces a raw `Literal`; the
/// outer `map_with` attaches the source span so diagnostics and LSP hover
/// point at the right location.
pub(super) fn literal_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let literal_value = choice((
        select! { Token::String(s) => Literal::String(s) },
        select! { Token::Number(n) => Literal::Number(n) },
        just(Token::True).to(Literal::Boolean(true)),
        just(Token::False).to(Literal::Boolean(false)),
        just(Token::Nil).to(Literal::Nil),
    ));
    literal_value.map_with(|value, e| Expr::Literal {
        value,
        span: span_from_simple(e.span()),
    })
}

/// Parse a bracketed literal: an array, a dictionary, or the empty
/// dictionary `[:]`.
///
/// `expr` is the recursive expression parser, threaded in because this
/// parser is built inside `recursive(|expr| ...)`.
pub(super) fn bracket_literal_parser<'tokens, I>(
    expr: impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone + 'tokens,
) -> impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // One item of a bracketed literal: an expression, and the
    // `: value` that turns it into a dictionary entry.
    //
    // An array element and a dictionary entry share their whole
    // prefix. Parsing them as two alternatives of a `choice` made
    // the parser read every element twice — once for the entry
    // that then failed on the missing colon, once for the element
    // — and that doubling compounds with each level of nesting, so
    // the cost was 2^depth. Read each item once and decide after.
    let bracket_item = expr
        .clone()
        .then(just(Token::Colon).ignore_then(expr).or_not());

    // Array or dictionary literal:
    //   `[]`                    the empty array
    //   `[:]`                   the empty dictionary
    //   `[a, b]`                an array
    //   `[k: v, k2: v2]`        a dictionary
    choice((
        // The empty dictionary is spelled `[:]`, because `[]` is
        // the empty array. It has no item to read, so it needs its
        // own alternative.
        just(Token::LBracket)
            .ignore_then(just(Token::Colon))
            .ignore_then(just(Token::RBracket))
            .map_with(|_, e| Expr::DictLiteral {
                entries: vec![],
                span: span_from_simple(e.span()),
            }),
        bracket_item
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .try_map(|items, span: SimpleSpan| {
                let entries = items.iter().filter(|(_, value)| value.is_some()).count();
                if entries == 0 {
                    Ok(Expr::Array {
                        elements: items.into_iter().map(|(element, _)| element).collect(),
                        span: span_from_simple(span),
                    })
                } else if entries == items.len() {
                    Ok(Expr::DictLiteral {
                        entries: items
                            .into_iter()
                            .filter_map(|(key, value)| value.map(|value| (key, value)))
                            .collect(),
                        span: span_from_simple(span),
                    })
                } else {
                    Err(Rich::custom(
                        span,
                        "a bracketed literal must hold either array elements or \
                         dictionary entries, not both",
                    ))
                }
            }),
    ))
}
