//! Match-arm and pattern parsers, lifted out of the main expression
//! combinator. Both are entry points called by other parsers.

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{Expr, Ident, MatchArm, Pattern};
use crate::lexer::Token;

use super::super::{ident_parser, span_from_simple};

/// Parse a match arm: `pattern: expr`.
pub(in crate::parser) fn match_arm_parser<'tokens, I>(
    expr: impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone,
) -> impl Parser<'tokens, I, MatchArm, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    pattern_parser()
        .then_ignore(just(Token::Colon))
        .then(expr)
        .map_with(|(pattern, body), e| MatchArm {
            pattern,
            body,
            span: span_from_simple(e.span()),
        })
        .labelled("match arm (pattern: expression)")
}

/// Parse a pattern: `variant`, `variant(b1, b2)`, `.variant`,
/// `.variant(b1, b2)`, or `_`. A binding in the list can be `_`, for
/// example `.image(_, size)`.
pub(in crate::parser) fn pattern_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Pattern, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let wildcard = just(Token::Underscore).to(Pattern::Wildcard);

    // A field binding is a name, or `_` to ignore the field. The `_`
    // binds nothing: the checks and the lowering skip the name `_`, as
    // they do for `let _ = ...`.
    let field_binding = choice((
        ident_parser(),
        just(Token::Underscore).map_with(|_, e| Ident::new("_", span_from_simple(e.span()))),
    ));

    let variant = choice((
        // Short form: .variant or .variant(bindings)
        just(Token::Dot).ignore_then(ident_parser()),
        // Full form: variant or variant(bindings)
        ident_parser(),
    ))
    .then(
        field_binding
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .or_not(),
    )
    .map(|(name, bindings)| Pattern::Variant {
        name,
        bindings: bindings.unwrap_or_default(),
    });

    choice((wildcard, variant))
}
