//! Newline handling.
//!
//! `FormaLang` separates statements with newlines and has no
//! terminator. The lexer keeps only the newlines that end a statement
//! and drops the rest, so a `Token::Newline` reaching the parser is
//! always a boundary — see
//! [`Lexer::tokenize_all_with_errors`](crate::lexer::Lexer::tokenize_all_with_errors).

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::lexer::Token;

/// Newlines, in any number, ignored.
///
/// The lexer keeps only the newlines that end a statement, so one
/// reaching the parser is always a boundary. A `{ ... }` list that is
/// not a block absorbs its newlines with this. In the fields of a
/// struct and the variants of an enum, a `,` separates two entries,
/// and a newline alone does not. In a trait, the `,` is optional. The
/// items of an impl block and of a `mod` have no separator.
pub(super) fn newlines<'tokens, I>(
) -> impl Parser<'tokens, I, (), extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    just(Token::Newline).repeated().ignored()
}
