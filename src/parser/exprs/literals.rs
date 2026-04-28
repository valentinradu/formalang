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
        select! { Token::Regex(s) => {
            if let Some((pattern, flags)) = crate::lexer::parse_regex(&s) {
                Literal::Regex { pattern, flags }
            } else {
                Literal::Regex { pattern: String::new(), flags: String::new() }
            }
        }},
        select! { Token::Path(p) => Literal::Path(p) },
        just(Token::True).to(Literal::Boolean(true)),
        just(Token::False).to(Literal::Boolean(false)),
        just(Token::Nil).to(Literal::Nil),
    ));
    literal_value.map_with(|value, e| Expr::Literal {
        value,
        span: span_from_simple(e.span()),
    })
}
