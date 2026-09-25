//! Logos callbacks and parsing helpers used by the `Token` enum.

use logos::Skip;

use super::strings::record_bidi_controls;
use super::Token;

/// Strip the `///` prefix and a single leading space from a doc-comment
/// slice. Returns the remaining text (trimmed of trailing whitespace).
pub(super) fn parse_doc_comment(lex: &mut logos::Lexer<'_, Token>) -> String {
    check_comment(lex);
    let raw = lex.slice();
    let body = raw.strip_prefix("///").unwrap_or(raw);
    let body = body.strip_prefix(' ').unwrap_or(body);
    body.trim_end().to_string()
}

/// Strip the `//!` prefix and a single leading space from an inner
/// doc-comment slice. Returns the remaining text (trimmed of trailing
/// whitespace).
pub(super) fn parse_inner_doc_comment(lex: &mut logos::Lexer<'_, Token>) -> String {
    check_comment(lex);
    let raw = lex.slice();
    let body = raw.strip_prefix("//!").unwrap_or(raw);
    let body = body.strip_prefix(' ').unwrap_or(body);
    body.trim_end().to_string()
}

/// Skip a plain line comment, after the check on its text.
pub(super) fn skip_line_comment(lex: &mut logos::Lexer<'_, Token>) -> Skip {
    check_comment(lex);
    Skip
}

/// Record each bidirectional control character in the comment that
/// Logos has matched.
fn check_comment(lex: &mut logos::Lexer<'_, Token>) {
    let start = lex.span().start;
    let text = lex.slice();
    record_bidi_controls(lex, start, text);
}

/// Skip a nested block comment.
///
/// Called after Logos has matched the opening `/*`. Scans the remainder
/// while tracking nesting depth: every `/*` increments the counter and
/// every `*/` decrements it. Bumps the lexer cursor past the matching
/// closing `*/` on success.
///
/// On an unterminated comment, records the byte range of the opening
/// `/*` through end-of-input on the lexer's [`super::LexerExtras`] so the
/// wrapping [`Lexer`](crate::lexer::Lexer) can surface a real
/// [`CompilerError::UnterminatedBlockComment`](crate::CompilerError)
/// instead of a misleading "unexpected end of input" parse error.
pub(super) fn skip_block_comment(lex: &mut logos::Lexer<'_, Token>) -> Skip {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut depth: usize = 1;
    let mut i: usize = 0;
    let len = bytes.len();
    while i < len {
        let next_idx = i.saturating_add(1);
        let byte = bytes.get(i).copied().unwrap_or(0);
        let next = bytes.get(next_idx).copied().unwrap_or(0);
        if next_idx < len && byte == b'/' && next == b'*' {
            depth = depth.saturating_add(1);
            i = i.saturating_add(2);
        } else if next_idx < len && byte == b'*' && next == b'/' {
            depth = depth.saturating_sub(1);
            i = i.saturating_add(2);
            if depth == 0 {
                let start = lex.span().end;
                let text = remainder.get(..i).unwrap_or_default();
                record_bidi_controls(lex, start, text);
                lex.bump(i);
                return Skip;
            }
        } else {
            i = i.saturating_add(1);
        }
    }
    // Unterminated block comment: record the offending range (from the
    // opening `/*` through end-of-input) so the lexer can emit a real
    // diagnostic, then consume the rest of the input so Logos doesn't
    // loop on it.
    let opening_span = lex.span();
    record_bidi_controls(lex, opening_span.end, remainder);
    let end = opening_span.end.saturating_add(len);
    lex.extras
        .unterminated_block_comments
        .push((opening_span.start, end));
    lex.bump(len);
    Skip
}

/// Parse a numeric literal slice into its [`NumberValue`] payload plus
/// optional width-tag suffix.
///
/// The slice may end in one of `I32`, `I64`, `F32`, `F64`. Integer-syntax
/// digits (`42`, `1_000_000`) parse via `i128::from_str` so the exact value
/// round-trips into the IR; float-syntax digits (`3.14`, `1e5`) parse via
/// `f64::from_str`. Returns `None` on parse failure (unparseable, an
/// integer too large for `i128`, or a float that overflows to an
/// infinity) so logos emits an error that the lexer converts into
/// [`crate::error::CompilerError::InvalidNumber`].
pub(super) fn parse_number(s: &str) -> Option<crate::ast::NumberLiteral> {
    use crate::ast::{NumberLiteral, NumberSourceKind, NumberValue};

    let (digits, suffix) = strip_numeric_suffix(s);
    // The source kind is determined syntactically: a `.` or `e`/`E` in the
    // digit slice means float syntax (`3.14`, `1e5`); otherwise integer.
    let kind = if digits.bytes().any(|b| b == b'.' || b == b'e' || b == b'E') {
        NumberSourceKind::Float
    } else {
        NumberSourceKind::Integer
    };
    let cleaned: String = digits.chars().filter(|c| *c != '_').collect();
    let value = match kind {
        NumberSourceKind::Integer => NumberValue::Integer(cleaned.parse::<i128>().ok()?),
        NumberSourceKind::Float => {
            // `f64::from_str` reports overflow as an infinity rather
            // than an error, so `1e400` would parse. Reject it here,
            // the way the integer branch rejects a value too large for
            // `i128`. An infinity has no JSON form either, so letting
            // one through makes the serialised `IrModule` (the `serde`
            // feature) impossible to decode.
            //
            // Underflow is different and stays accepted: `1e-400`
            // becomes `0.0`, which is what IEEE 754 specifies and what
            // every other language does.
            let parsed = cleaned.parse::<f64>().ok()?;
            if !parsed.is_finite() {
                return None;
            }
            NumberValue::Float(parsed)
        }
    };
    Some(NumberLiteral::from_lex(value, suffix, kind))
}

/// Strip a trailing width-tag suffix (`I32`, `I64`, `F32`, `F64`) from a
/// numeric literal slice. Returns the digit prefix paired with the matched
/// suffix (or the original slice and `None` when no suffix is present).
fn strip_numeric_suffix(s: &str) -> (&str, Option<crate::ast::NumericSuffix>) {
    use crate::ast::NumericSuffix as N;

    const TABLE: [(&str, N); 4] = [
        ("I32", N::I32),
        ("I64", N::I64),
        ("F32", N::F32),
        ("F64", N::F64),
    ];
    TABLE
        .iter()
        .find_map(|&(text, suffix)| s.strip_suffix(text).map(|d| (d, Some(suffix))))
        .unwrap_or((s, None))
}
