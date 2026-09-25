//! Logos callbacks for string literals, and the check for
//! bidirectional control characters in strings and comments.
//!
//! A string literal is scanned by hand, not by a regular expression.
//! The regular expression that did this before had two defects. A
//! long literal overflowed the stack, and a literal with a wrong
//! escape did not match at all, so the lexer reported a string that
//! its quote closes as unterminated.

use logos::Lexer;

use super::Token;

/// One wrong escape in a string literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BadEscape {
    /// `\u` that is not followed by four hex digits, or whose four hex
    /// digits are not a Unicode scalar value. Holds the text after
    /// `\u`, at most four characters.
    Unicode(String),
    /// A backslash followed by a character that starts no escape.
    /// Holds the whole sequence, for example `\q`.
    Unknown(String),
}

/// Whether `c` changes the direction of the text around it.
///
/// These characters make an editor show a program in an order that is
/// not the order the compiler reads (CVE-2021-42574, "Trojan Source").
/// The compiler refuses them in comments and in string literals, as
/// `rustc` does. An escape such as `\u202E` stays allowed, because the
/// source then shows what the string holds.
pub(super) const fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Record each bidirectional control character in `text`. `base` is
/// the byte offset of `text` in the source.
pub(super) fn record_bidi_controls(lex: &mut Lexer<'_, Token>, base: usize, text: &str) {
    for (offset, c) in text.char_indices() {
        if is_bidi_control(c) {
            let start = base.saturating_add(offset);
            let end = start.saturating_add(c.len_utf8());
            lex.extras.bidi_controls.push((start, end, c));
        }
    }
}

/// Scan a single-line string literal. Logos has matched the opening
/// `"`.
///
/// Returns `None` for a literal that a line break or the end of the
/// input cuts off. Logos then reports the consumed text as an error,
/// and the lexer names it an unterminated string. The line break is
/// not consumed, so it still ends the statement.
pub(super) fn lex_string(lex: &mut Lexer<'_, Token>) -> Option<String> {
    let content_start = lex.span().end;
    let rest = lex.remainder();
    let mut chars = rest.char_indices();
    let mut close = None;
    let mut stop = rest.len();
    while let Some((offset, c)) = chars.next() {
        match c {
            '"' => {
                close = Some(offset);
                break;
            }
            '\n' => {
                stop = offset;
                break;
            }
            '\\' => match chars.next() {
                Some((next, '\n')) => {
                    stop = next;
                    break;
                }
                Some(_) => {}
                None => break,
            },
            _ => {}
        }
    }
    let Some(close) = close else {
        lex.bump(stop);
        return None;
    };
    let content = rest.get(..close).unwrap_or_default().to_owned();
    lex.bump(close.saturating_add(1));
    Some(finish_literal(lex, content_start, &content))
}

/// Scan a multi-line string literal. Logos has matched the opening
/// `"""`. The literal ends at the first `"""` that no backslash
/// escapes. A literal that the end of the input cuts off returns
/// `None`, as in [`lex_string`].
pub(super) fn lex_multiline_string(lex: &mut Lexer<'_, Token>) -> Option<String> {
    let content_start = lex.span().end;
    let rest = lex.remainder();
    let mut chars = rest.char_indices();
    let mut close = None;
    while let Some((offset, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '"' if rest.get(offset..).is_some_and(|s| s.starts_with("\"\"\"")) => {
                close = Some(offset);
                break;
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        lex.bump(rest.len());
        return None;
    };
    let content = rest.get(..close).unwrap_or_default().to_owned();
    lex.bump(close.saturating_add(3));
    Some(finish_literal(lex, content_start, &content))
}

/// Decode the escapes of a closed literal, and record each wrong
/// escape and each bidirectional control character.
fn finish_literal(lex: &mut Lexer<'_, Token>, content_start: usize, content: &str) -> String {
    record_bidi_controls(lex, content_start, content);
    let (text, bad) = process_escapes(content);
    if !bad.is_empty() {
        let span = lex.span();
        for escape in bad {
            lex.extras.bad_escapes.push((span.start, span.end, escape));
        }
    }
    text
}

/// Decode the escape sequences in the body of a string literal.
///
/// The escapes are `\"`, `\\`, `\n`, `\t`, `\r` and `\uXXXX` with four
/// hex digits. Returns the decoded text and each wrong escape. The
/// decoded text holds U+FFFD in place of a wrong escape, so the rest
/// of the literal keeps its positions.
pub(super) fn process_escapes(s: &str) -> (String, Vec<BadEscape>) {
    let mut result = String::with_capacity(s.len());
    let mut bad = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars.next() {
            Some(c @ ('"' | '\\')) => result.push(c),
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('r') => result.push('\r'),
            Some('u') => {
                let mut hex = String::new();
                let mut taken = 0_u8;
                while taken < 4 {
                    match chars.peek() {
                        Some(&c) if c != '"' && c != '\\' => {
                            hex.push(c);
                            taken = taken.saturating_add(1);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                let decoded = if hex.len() == 4 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                    u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                } else {
                    None
                };
                if let Some(c) = decoded {
                    result.push(c);
                } else {
                    bad.push(BadEscape::Unicode(hex));
                    result.push('\u{FFFD}');
                }
            }
            Some(c) => {
                bad.push(BadEscape::Unknown(format!("\\{c}")));
                result.push('\u{FFFD}');
            }
            None => {
                bad.push(BadEscape::Unknown("\\".to_owned()));
                result.push('\u{FFFD}');
            }
        }
    }

    (result, bad)
}

#[cfg(test)]
mod tests {
    use super::{is_bidi_control, process_escapes, BadEscape};

    #[test]
    fn the_known_escapes_decode() {
        let (text, bad) = process_escapes(r#"a\"b\\c\nd\te\rfA"#);
        assert_eq!(text, "a\"b\\c\nd\te\rfA");
        assert!(bad.is_empty());
    }

    #[test]
    fn a_short_unicode_escape_is_wrong() {
        let (_, bad) = process_escapes(r"\u41");
        assert_eq!(bad, vec![BadEscape::Unicode("41".to_owned())]);
    }

    #[test]
    fn a_braced_unicode_escape_is_wrong() {
        let (_, bad) = process_escapes(r"\u{41}");
        assert_eq!(bad, vec![BadEscape::Unicode("{41}".to_owned())]);
    }

    #[test]
    fn a_surrogate_is_wrong() {
        let (_, bad) = process_escapes(r"\uD800");
        assert_eq!(bad, vec![BadEscape::Unicode("D800".to_owned())]);
    }

    #[test]
    fn an_unknown_escape_is_wrong() {
        let (text, bad) = process_escapes(r"a\qb");
        assert_eq!(text, "a\u{FFFD}b");
        assert_eq!(bad, vec![BadEscape::Unknown(r"\q".to_owned())]);
    }

    #[test]
    fn the_bidi_controls_are_known() {
        for c in ['\u{202A}', '\u{202E}', '\u{2066}', '\u{2069}'] {
            assert!(is_bidi_control(c));
        }
        for c in ['\u{2029}', '\u{202F}', '\u{2065}', '\u{206A}', 'a'] {
            assert!(!is_bidi_control(c));
        }
    }
}
