//! A limit on how deep a program nests, checked on the tokens before
//! the parser runs.
//!
//! The parser is recursive, and so are the phases after it. A program
//! that nests deep enough overflows the stack, and a stack overflow
//! aborts the whole process: for an editor that holds the compiler, it
//! is a crash of the editor. So the parser refuses such a program with
//! an error before it starts.
//!
//! The check is one pass over the tokens, so its time grows with the
//! length of the input. It does not parse. It computes a score that is
//! never lower than the depth of the tree that the parser would build:
//!
//! - each open `(`, `[` or `{` adds one level;
//! - inside a level, each operator and each keyword that starts a
//!   nested expression adds one more, because a chain such as
//!   `1 + 1 + 1` or `if a { } else if b { }` builds a tree as deep as
//!   the chain is long;
//! - a `,` or a line break ends one item of a level, so the next item
//!   starts again from the depth of the level.
//!
//! The score counts some tokens that do not nest, so it can be higher
//! than the real depth. The limit leaves room for that: a program whose
//! expressions are within the limit of the semantic pass is below it.

use crate::lexer::Token;
use crate::location::Span;

/// The highest nesting score that the parser accepts.
///
/// The semantic pass refuses an expression deeper than 500. This limit
/// is higher, because the score can count a level more than once. It is
/// not higher still, because the phases after the parser recurse too:
/// in a debug build on a stack of 8 megabytes, the semantic pass overflows on
/// an expression about 1400 deep, before its own limit applies.
pub(super) const MAX_NESTING: usize = 1024;

/// The stack of the parser thread: a base, and this much more for each
/// unit of the nesting score of the program.
///
/// The parser is recursive, and a debug build uses much more stack for
/// each level than a release build. A thread of the default size (8
/// megabytes for the main thread on Linux, 2 megabytes for other
/// threads) overflowed on a program nested about 60 deep. The worst
/// construct measured, a method call whose argument is a block, needs
/// about 250 kilobytes for each unit of the score in a debug build;
/// this size is twice that.
///
/// The operating system commits the pages of a stack only when they are
/// used, so the size costs address space, not memory. A program at the
/// nesting limit gets a stack of about 520 megabytes.
const PARSER_STACK_BASE: usize = 8 * 1024 * 1024;
const PARSER_STACK_PER_LEVEL: usize = 512 * 1024;

/// Run `parse` on a thread with a stack for a program of nesting score
/// `score`, and return what it returns.
///
/// When the thread cannot start, for example on a target without
/// threads, `parse` runs on the thread of the caller. A panic on the
/// parser thread continues on the thread of the caller.
pub(super) fn on_parser_stack<T, F>(score: usize, parse: F) -> T
where
    T: Send,
    F: Fn() -> T + Send + Copy,
{
    let stack_size = PARSER_STACK_PER_LEVEL
        .saturating_mul(score)
        .saturating_add(PARSER_STACK_BASE);
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("formalang-parser".to_owned())
            .stack_size(stack_size)
            .spawn_scoped(scope, parse)
            .map_or_else(
                |_| parse(),
                |handle| {
                    handle
                        .join()
                        .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
                },
            )
    })
}

/// The highest nesting score of the program, or the span of the first
/// token at which the score passes [`MAX_NESTING`].
pub(super) fn nesting_score(tokens: &[(Token, Span)]) -> Result<usize, Span> {
    // The count of operators in the current item of each open level.
    // The first entry is the top level, which is never closed.
    let mut items: Vec<usize> = vec![0];
    // The score: the open levels plus every count in `items`.
    let mut score: usize = 0;
    let mut highest: usize = 0;
    for (token, span) in tokens {
        if matches!(token, Token::LParen | Token::LBracket | Token::LBrace) {
            items.push(0);
            score = score.saturating_add(1);
        } else if matches!(token, Token::RParen | Token::RBracket | Token::RBrace) {
            // An unbalanced closer leaves the top level alone; the parser
            // reports it.
            if items.len() > 1 {
                let count = items.pop().unwrap_or(0);
                score = score.saturating_sub(count.saturating_add(1));
            }
        } else if matches!(token, Token::Comma | Token::Newline) {
            if let Some(count) = items.last_mut() {
                score = score.saturating_sub(*count);
                *count = 0;
            }
        } else if deepens(token) {
            if let Some(count) = items.last_mut() {
                *count = count.saturating_add(1);
            }
            score = score.saturating_add(1);
        }
        if score > MAX_NESTING {
            return Err(*span);
        }
        highest = highest.max(score);
    }
    Ok(highest)
}

/// Whether `token` can make the tree one level deeper inside its item:
/// an operator, a `.` of a field or a method, or a keyword that starts
/// a nested expression.
const fn deepens(token: &Token) -> bool {
    matches!(
        token,
        Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::EqEq
            | Token::Ne
            | Token::Lt
            | Token::Gt
            | Token::Le
            | Token::Ge
            | Token::And
            | Token::Or
            | Token::Bang
            | Token::DotDot
            | Token::Dot
            | Token::Question
            | Token::Arrow
            | Token::Equals
            | Token::If
            | Token::Else
            | Token::Match
            | Token::For
            | Token::Let
    )
}

#[cfg(test)]
mod tests {
    use super::{nesting_score, MAX_NESTING};
    use crate::lexer::Lexer;

    fn too_deep(source: &str) -> bool {
        nesting_score(&Lexer::tokenize_all(source)).is_err()
    }

    #[test]
    fn the_score_counts_the_open_levels() {
        let tokens = Lexer::tokenize_all("pub let x = ((1))\n");
        assert_eq!(nesting_score(&tokens), Ok(4));
    }

    #[test]
    fn a_flat_program_passes() {
        // The check reads tokens only, so the names need not differ.
        let source = "pub let x: I32 = 1 + 2 * 3
"
        .repeat(10_000);
        assert!(!too_deep(&source));
    }

    #[test]
    fn a_long_array_passes() {
        let items = vec!["1 + 1"; 100_000].join(", ");
        assert!(!too_deep(&format!("pub let x = [{items}]\n")));
    }

    /// `let` and `=` count one each, so the parentheses can take the
    /// rest of the limit.
    #[test]
    fn parentheses_at_the_limit_pass_and_one_more_fails() {
        let nest = |n: usize| format!("pub let x = {}1{}\n", "(".repeat(n), ")".repeat(n));
        assert!(!too_deep(&nest(MAX_NESTING - 2)));
        assert!(too_deep(&nest(MAX_NESTING - 1)));
    }

    #[test]
    fn a_long_operator_chain_fails() {
        let chain = vec!["1"; MAX_NESTING + 2].join(" + ");
        assert!(too_deep(&format!("pub let x = {chain}\n")));
    }

    #[test]
    fn a_long_else_if_chain_fails() {
        let chain = " else if a == 1 { 1 }".repeat(MAX_NESTING);
        let source =
            format!("pub fn f(a: I32) -> I32 {{\n    if a == 0 {{ 0 }}{chain} else {{ 1 }}\n}}\n");
        assert!(too_deep(&source));
    }

    #[test]
    fn unclosed_blocks_fail() {
        assert!(too_deep(&"pub fn f() -> I32 {\n".repeat(MAX_NESTING + 1)));
    }
}
