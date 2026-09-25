//! The end of a statement inside a function body or a block, and the
//! recovery for a statement that failed to parse.

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::lexer::Token;

/// Skip the tokens of a statement that failed to parse, and stop
/// before the next statement of the same block.
///
/// The first token is always consumed, so the parser moves on, but a
/// `}` is never consumed: it closes the enclosing block. After that,
/// skip to the next `let` or `}` of the same block.
///
/// A `( ... )`, `[ ... ]` or `{ ... }` group inside the failed
/// statement is skipped as a whole. Before, the skip stopped at the
/// first `let` inside such a group. Parsing then continued inside the
/// failed statement, so each nested group was parsed once for the
/// attempt and once more after the recovery. Each level of nesting
/// doubled the time, and a nest of unclosed `if` blocks took time that
/// grew three times for each level. An unclosed group now makes the
/// skip run to the end of the input, and the enclosing block then
/// reports its missing `}`.
pub(super) fn skip_failed_statement<'tokens, I>(
) -> impl Parser<'tokens, I, (), extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    custom(|inp| {
        let before = inp.cursor();
        let mut depth: usize = match inp.peek() {
            None | Some(Token::RBrace) => {
                let span = inp.span_since(&before);
                return Err(Rich::custom(span, "no statement to skip"));
            }
            Some(Token::LParen | Token::LBracket | Token::LBrace) => 1,
            Some(_) => 0,
        };
        inp.skip();
        loop {
            match inp.peek() {
                None => break,
                Some(Token::Let | Token::RBrace) if depth == 0 => break,
                Some(Token::LParen | Token::LBracket | Token::LBrace) => {
                    depth = depth.saturating_add(1);
                }
                Some(Token::RParen | Token::RBracket | Token::RBrace) => {
                    depth = depth.saturating_sub(1);
                }
                Some(_) => {}
            }
            inp.skip();
        }
        Ok(())
    })
}

/// The end of a statement inside a function body or a block: a line
/// break, the `}` that closes the block, or a `{` that opens a block
/// statement. None of them is consumed.
///
/// Statements have no terminator, so a line break separates two of
/// them. Without this check, `let a = 1 let b = 2` on one line parsed as
/// two statements.
///
/// The lexer drops a line break in front of a `{`, because a `{` at the
/// start of a line usually opens the body of the line above: `if a` on
/// one line and `{` on the next. So a block statement on its own line
/// reaches the parser with no line break in front of it, and a `{` must
/// end the statement before it too.
pub(super) fn statement_end<'tokens, I>(
) -> impl Parser<'tokens, I, (), extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    choice((
        just(Token::Newline).ignored(),
        just(Token::RBrace).ignored(),
        just(Token::LBrace).ignored(),
        end(),
    ))
    .rewind()
    .labelled("end of line")
}
