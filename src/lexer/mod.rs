mod token;

pub use token::Token;

use token::BadEscape;

use crate::error::CompilerError;
use crate::location::Span;
use logos::Logos;

/// Lexer for `FormaLang` source code
#[derive(Debug)]
pub struct Lexer<'source> {
    inner: logos::Lexer<'source, Token>,
    source: &'source str,
    /// Errors accumulated during lexing. Each malformed token is reported, and
    /// the lexer continues scanning rather than silently dropping it.
    errors: Vec<CompilerError>,
}

impl<'source> Lexer<'source> {
    #[must_use]
    pub fn new(source: &'source str) -> Self {
        let mut inner = Token::lexer(source);

        // A leading UTF-8 byte-order mark is not part of the program.
        // Several editors write one, and the user cannot see it, so
        // reporting it as an invalid character points them at a
        // character that is not on their screen.
        //
        // Skip it by advancing the lexer rather than by trimming the
        // source, so every span still indexes the original bytes and a
        // diagnostic snippet still lines up. A mark anywhere else stays
        // an error: there it is a stray zero-width character, not an
        // encoding marker.
        if source.starts_with('\u{feff}') {
            inner.bump('\u{feff}'.len_utf8());
        }

        Self {
            inner,
            source,
            errors: Vec::new(),
        }
    }

    /// Get the next token with its span.
    ///
    /// On a lexer error, records a [`CompilerError`] and continues scanning.
    pub fn next_token(&mut self) -> Option<(Token, Span)> {
        loop {
            let token = self.inner.next()?;
            let range = self.inner.span();
            let span = Span::from_range(range.start, range.end);

            match token {
                Ok(tok) => return Some((tok, span)),
                Err(()) => {
                    self.errors
                        .push(Self::classify_error(self.source, range.start, range.end));
                    // Continue and return the next successful token.
                }
            }
        }
    }

    /// Classify a lexer error span into a specific [`CompilerError`] variant.
    ///
    /// This inspects the offending source slice and distinguishes between:
    /// - [`CompilerError::UnterminatedString`] — a `"` that never closes,
    /// - [`CompilerError::InvalidNumber`] — a digit-led slice that is not a valid numeric literal,
    /// - [`CompilerError::InvalidCharacter`] — anything else (default fall-back).
    fn classify_error(source: &str, start: usize, end: usize) -> CompilerError {
        let span = Span::from_range(start, end);
        // Logos always produces byte ranges within `source`; fall back to empty
        // only as a defensive measure if that invariant is ever broken.
        let slice = source.get(start..end).unwrap_or_default();

        let first = slice.chars().next();

        match first {
            Some('"') => CompilerError::UnterminatedString { span },
            Some(c) if c.is_ascii_digit() => CompilerError::InvalidNumber {
                value: slice.to_string(),
                span,
            },
            Some(c) => CompilerError::InvalidCharacter { character: c, span },
            None => CompilerError::InvalidCharacter {
                character: '\u{0}',
                span,
            },
        }
    }

    /// Get current span
    #[must_use]
    pub fn span(&self) -> Span {
        let range = self.inner.span();
        Span::from_range(range.start, range.end)
    }

    /// Take accumulated errors, leaving the lexer's error list empty.
    pub fn take_errors(&mut self) -> Vec<CompilerError> {
        std::mem::take(&mut self.errors)
    }

    /// Tokenize entire source (useful for testing and debugging).
    ///
    /// Lexer errors are silently dropped; use [`tokenize_all_with_errors`](Self::tokenize_all_with_errors)
    /// to recover them.
    #[must_use]
    pub fn tokenize_all(source: &'source str) -> Vec<(Token, Span)> {
        Self::tokenize_all_with_errors(source).0
    }

    /// Tokenize entire source, returning both tokens and accumulated errors.
    #[must_use]
    pub fn tokenize_all_with_errors(
        source: &'source str,
    ) -> (Vec<(Token, Span)>, Vec<CompilerError>) {
        let mut lexer = Self::new(source);
        let mut tokens = Vec::new();

        // One index for the whole pass. Calling `fill_span_positions`
        // per token built a fresh index each time, and each index costs
        // a pass over the source, which made lexing quadratic in the
        // source length.
        let index = crate::location::LineIndex::new(source);

        while let Some((token, span)) = lexer.next_token() {
            // Logos signals end-of-input by returning `None` from
            // `next_token` — there is no separate EOF sentinel token.
            // (`Token::Eof` and the dead
            // guard that previously matched it here.)
            //
            // Fill in line/column positions from byte offsets
            let span = index.fill_span(span);
            tokens.push((token, span));
        }

        // drain unterminated block-comment ranges accumulated
        // in `extras` and surface them as real `UnterminatedBlockComment`
        // diagnostics rather than letting the parser report a misleading
        // "unexpected end of input".
        for (start, end) in std::mem::take(&mut lexer.inner.extras.unterminated_block_comments) {
            lexer.errors.push(CompilerError::UnterminatedBlockComment {
                span: Span::from_range(start, end),
            });
        }

        // Drain the wrong escapes. The decoded string still holds a
        // U+FFFD in place of each one, so parsing can continue, but the
        // user is told what went wrong.
        for (start, end, escape) in std::mem::take(&mut lexer.inner.extras.bad_escapes) {
            let span = Span::from_range(start, end);
            lexer.errors.push(match escape {
                BadEscape::Unicode(value) => CompilerError::InvalidUnicodeEscape { value, span },
                BadEscape::Unknown(sequence) => CompilerError::InvalidEscape { sequence, span },
            });
        }

        // Drain the bidirectional control characters in comments and
        // strings. See `CompilerError::BidirectionalControl`.
        for (start, end, character) in std::mem::take(&mut lexer.inner.extras.bidi_controls) {
            lexer.errors.push(CompilerError::BidirectionalControl {
                character,
                span: Span::from_range(start, end),
            });
        }

        let errors = lexer
            .take_errors()
            .into_iter()
            .map(|e| fill_error_span_positions(e, &index))
            .collect();

        (drop_continuation_newlines(&tokens), errors)
    }
}

/// Whether a token can be the last one of a statement.
///
/// A newline only ends a statement when what came before it is
/// complete. A line ending in `+`, `,` or `{` is plainly unfinished, so
/// the newline after it continues.
const fn can_end_a_statement(token: &Token) -> bool {
    matches!(
        token,
        Token::Ident(_)
            | Token::String(_)
            | Token::Number(_)
            | Token::True
            | Token::False
            | Token::Nil
            | Token::SelfKeyword
            | Token::RParen
            | Token::RBracket
            | Token::RBrace
            | Token::Question
    )
}

/// Whether a token can only continue what came before it, so a newline
/// in front of it never ends a statement.
///
/// `.` keeps leading-dot continuation working — a match arm written as
/// `.variant: body` on its own line, and a method chain broken across
/// lines. `else` can only follow an `if`, and `{` opens a body for the
/// line above.
const fn only_continues(token: &Token) -> bool {
    matches!(token, Token::Dot | Token::Else | Token::LBrace)
}

/// Whether a token can only start a definition or a statement, and
/// never continues an expression.
const fn only_starts_a_definition(token: &Token) -> bool {
    matches!(
        token,
        Token::Pub
            | Token::Use
            | Token::Let
            | Token::Struct
            | Token::Enum
            | Token::Trait
            | Token::Impl
            | Token::Fn
            | Token::Extern
            | Token::Module
            | Token::Inline
            | Token::NoInline
            | Token::Cold
            | Token::DocComment(_)
            | Token::InnerDocComment(_)
    )
}

/// Whether a token can end a definition, but also continues an
/// expression as an operator.
///
/// A type such as `Box<I32>` ends with `>`, and `use a::*` ends with
/// `*`. A newline after one of them ends the definition only when the
/// next line starts a new definition.
const fn ends_a_definition_or_continues(token: &Token) -> bool {
    matches!(token, Token::Gt | Token::Star)
}

/// Drop every newline that continues a statement rather than ending
/// one, leaving the parser a stream where a `Newline` is always a
/// statement boundary.
///
/// A newline is kept when three things hold: no bracket is open, the
/// token before it can end a statement, and the token after it is not
/// one that can only continue. A newline after `>` or `*` is also kept
/// when no bracket is open and the next token starts a definition. Anything inside `(` or `[` is part of
/// one expression however many lines it spans, which is what lets
/// `assert(\n    condition: x\n        == 40\n)` keep working.
fn drop_continuation_newlines(tokens: &[(Token, Span)]) -> Vec<(Token, Span)> {
    let mut out: Vec<(Token, Span)> = Vec::with_capacity(tokens.len());
    let mut depth: usize = 0;
    // The bracket depth of each enclosing `{`. A block starts a fresh
    // count because its contents are statements again, however deep in
    // brackets the block itself sits, and the count is restored at the
    // matching `}`.
    let mut enclosing: Vec<usize> = Vec::new();

    for (index, (token, span)) in tokens.iter().enumerate() {
        if matches!(token, Token::Newline) {
            let previous = out.last().map(|(t, _)| t);
            let next = tokens
                .get(index.saturating_add(1)..)
                .and_then(|rest| rest.iter().find(|(t, _)| !matches!(t, Token::Newline)))
                .map(|(t, _)| t);

            let ends_a_statement = depth == 0
                && ((previous.is_some_and(can_end_a_statement)
                    && next.is_some_and(|t| !only_continues(t)))
                    || (previous.is_some_and(ends_a_definition_or_continues)
                        && next.is_some_and(only_starts_a_definition)));

            if ends_a_statement {
                out.push((token.clone(), *span));
            }
            continue;
        }

        // A `(` or `[` opens one expression, however many lines it
        // spans, so a newline inside one continues it.
        //
        // A `{` opens a body whose contents are statements, so a
        // newline inside one is a boundary again — including a block
        // written inside a call argument, which is where a closure
        // body usually goes. Counting only brackets left that body's
        // newlines dropped, and `a` on one line followed by `-1` on the
        // next silently became `a - 1`.
        if matches!(token, Token::LParen | Token::LBracket) {
            depth = depth.saturating_add(1);
        } else if matches!(token, Token::RParen | Token::RBracket) {
            depth = depth.saturating_sub(1);
        } else if matches!(token, Token::LBrace) {
            enclosing.push(depth);
            depth = 0;
        } else if matches!(token, Token::RBrace) {
            depth = enclosing.pop().unwrap_or(0);
        }
        out.push((token.clone(), *span));
    }

    out
}

/// Return the given error with its span upgraded to have line/column info.
///
/// Only lexer-produced variants ([`CompilerError::InvalidCharacter`],
/// [`CompilerError::UnterminatedString`], [`CompilerError::InvalidNumber`],
/// [`CompilerError::UnterminatedBlockComment`],
/// [`CompilerError::InvalidUnicodeEscape`], [`CompilerError::InvalidEscape`],
/// [`CompilerError::BidirectionalControl`]) are produced by
/// [`Lexer::classify_error`] / [`tokenize_all_with_errors`]; any other variant
/// would indicate a bug in the lexer's error-classification logic and is
/// returned unchanged.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Lexer::classify_error only produces a small set of lexer-error variants; enumerating every CompilerError variant would be noisy without adding safety"
)]
fn fill_error_span_positions(
    error: CompilerError,
    index: &crate::location::LineIndex<'_>,
) -> CompilerError {
    let span = index.fill_span(error.span());
    match error {
        CompilerError::InvalidCharacter { character, .. } => {
            CompilerError::InvalidCharacter { character, span }
        }
        CompilerError::UnterminatedString { .. } => CompilerError::UnterminatedString { span },
        CompilerError::UnterminatedBlockComment { .. } => {
            CompilerError::UnterminatedBlockComment { span }
        }
        CompilerError::InvalidUnicodeEscape { value, .. } => {
            CompilerError::InvalidUnicodeEscape { value, span }
        }
        CompilerError::InvalidEscape { sequence, .. } => {
            CompilerError::InvalidEscape { sequence, span }
        }
        CompilerError::BidirectionalControl { character, .. } => {
            CompilerError::BidirectionalControl { character, span }
        }
        CompilerError::InvalidNumber { value, .. } => CompilerError::InvalidNumber { value, span },
        other => other,
    }
}
