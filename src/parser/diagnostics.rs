//! Turning a `chumsky` parse failure into a readable message.

use chumsky::prelude::*;

use crate::lexer::Token;

/// Format a parse error with lowercase keywords and readable token names
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "RichPattern is defined in the chumsky library and cannot be exhaustively enumerated"
)]
pub(super) fn format_parse_error(error: &Rich<'_, Token>) -> String {
    use chumsky::error::RichPattern;

    let found = error
        .found()
        .map_or_else(|| "end of input".to_string(), |t| format!("{t}"));

    let expected: Vec<String> = error
        .expected()
        .map(|exp| match exp {
            RichPattern::Token(tok) => {
                // tok is a Maybe<Token, &Token> which derefs to &Token
                format!("{}", **tok)
            }
            RichPattern::Label(label) => label.to_string(),
            RichPattern::EndOfInput => "end of input".to_string(),
            _ => "<unknown>".to_string(),
        })
        .collect();

    let span = error.span();

    if expected.is_empty() {
        format!("found {} at {}..{}", found, span.start, span.end)
    } else if expected.len() == 1 {
        #[expect(
            clippy::indexing_slicing,
            reason = "bounds checked above: expected.len() == 1"
        )]
        let first = &expected[0];
        format!(
            "found {} at {}..{}, expected {}",
            found, span.start, span.end, first
        )
    } else {
        format!(
            "found {} at {}..{}, expected one of: {}",
            found,
            span.start,
            span.end,
            expected.join(", ")
        )
    }
}
