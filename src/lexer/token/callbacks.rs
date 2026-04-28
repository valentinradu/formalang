//! Logos callbacks and parsing helpers used by the `Token` enum.

use logos::Skip;

use super::Token;

/// Strip the `///` prefix and a single leading space from a doc-comment
/// slice. Returns the remaining text (trimmed of trailing whitespace).
pub(super) fn parse_doc_comment(lex: &logos::Lexer<'_, Token>) -> String {
    let raw = lex.slice();
    let body = raw.strip_prefix("///").unwrap_or(raw);
    let body = body.strip_prefix(' ').unwrap_or(body);
    body.trim_end().to_string()
}

/// Strip the `//!` prefix and a single leading space from an inner
/// doc-comment slice. Returns the remaining text (trimmed of trailing
/// whitespace).
pub(super) fn parse_inner_doc_comment(lex: &logos::Lexer<'_, Token>) -> String {
    let raw = lex.slice();
    let body = raw.strip_prefix("//!").unwrap_or(raw);
    let body = body.strip_prefix(' ').unwrap_or(body);
    body.trim_end().to_string()
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
    let end = opening_span.end.saturating_add(len);
    lex.extras
        .unterminated_block_comments
        .push((opening_span.start, end));
    lex.bump(len);
    Skip
}

/// Parse a numeric literal slice into its `f64` value plus optional width-tag
/// suffix.
///
/// The slice may end in one of `I32`, `I64`, `F32`, `F64`; the digits before
/// the suffix are stripped of underscores and parsed via `f64::parse`. Returns
/// `None` on parse failure so logos emits an error that the lexer converts
/// into [`crate::error::CompilerError::InvalidNumber`].
pub(super) fn parse_number(s: &str) -> Option<crate::ast::NumberLiteral> {
    use crate::ast::{NumberLiteral, NumberSourceKind};

    let (digits, suffix) = strip_numeric_suffix(s);
    // The source kind is determined syntactically: a `.` or `e`/`E` in the
    // digit slice means float syntax (`3.14`, `1e5`); otherwise integer.
    let kind = if digits.bytes().any(|b| b == b'.' || b == b'e' || b == b'E') {
        NumberSourceKind::Float
    } else {
        NumberSourceKind::Integer
    };
    let cleaned: String = digits.chars().filter(|c| *c != '_').collect();
    cleaned
        .parse::<f64>()
        .ok()
        .map(|value| NumberLiteral::from_lex(value, suffix, kind))
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

pub(super) fn parse_string(lex: &mut logos::Lexer<'_, Token>) -> String {
    let s = lex.slice();
    let content = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or_default();
    let (text, bad) = process_escapes(content);
    record_bad_escapes(lex, bad);
    text
}

pub(super) fn parse_multiline_string(lex: &mut logos::Lexer<'_, Token>) -> String {
    let s = lex.slice();
    let content = s
        .strip_prefix("\"\"\"")
        .and_then(|s| s.strip_suffix("\"\"\""))
        .unwrap_or_default();
    let (text, bad) = process_escapes(content);
    record_bad_escapes(lex, bad);
    text
}

/// Push each bad `\uXXXX` hex string accumulated by [`process_escapes`]
/// into the lexer's extras alongside the enclosing string literal's
/// byte range, so the wrapping [`Lexer`](crate::lexer::Lexer) can surface a
/// real [`CompilerError::InvalidUnicodeEscape`](crate::CompilerError).
fn record_bad_escapes(lex: &mut logos::Lexer<'_, Token>, bad: Vec<String>) {
    if bad.is_empty() {
        return;
    }
    let span = lex.span();
    for hex in bad {
        lex.extras
            .invalid_unicode_escapes
            .push((span.start, span.end, hex));
    }
}

/// Decode escape sequences in a string-literal body.
///
/// Returns the decoded text and a list of bad `\uXXXX` hex strings —
/// any escape whose code point is invalid (e.g. a UTF-16 surrogate
/// `\uD800..\uDFFF`). The replacement character `U+FFFD` is substituted
/// in the decoded output for each bad escape so positions in the rest
/// of the literal stay sensible.
fn process_escapes(s: &str) -> (String, Vec<String>) {
    let mut result = String::new();
    let mut bad_escapes: Vec<String> = Vec::new();
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(c @ ('"' | '\\')) => result.push(c),
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('u') => {
                    // The lexer regex guarantees four hex digits follow,
                    // so `from_str_radix` won't fail; the only failure
                    // mode is `from_u32` rejecting a surrogate code point
                    // in 0xD800..=0xDFFF.
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(unicode_char) = char::from_u32(code) {
                            result.push(unicode_char);
                        } else {
                            bad_escapes.push(hex);
                            result.push('\u{FFFD}');
                        }
                    } else {
                        bad_escapes.push(hex);
                        result.push('\u{FFFD}');
                    }
                }
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }

    (result, bad_escapes)
}

/// Parse a regex token slice into pattern and flags.
#[must_use]
pub fn parse_regex(s: &str) -> Option<(String, String)> {
    let content = s.strip_prefix("r/")?;
    let last_slash = content.rfind('/')?;
    let (pattern, rest) = content.split_at(last_slash);
    let flags = rest.strip_prefix('/').unwrap_or_default();

    Some((pattern.to_string(), flags.to_string()))
}
