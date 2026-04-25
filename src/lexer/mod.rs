mod token;

pub use token::{parse_regex, Token};

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
        Self {
            inner: Token::lexer(source),
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

        while let Some((token, span)) = lexer.next_token() {
            // Logos signals end-of-input by returning `None` from
            // `next_token` — there is no separate EOF sentinel token.
            // (Audit finding #48 removed `Token::Eof` and the dead
            // guard that previously matched it here.)
            //
            // Fill in line/column positions from byte offsets
            let span = crate::location::fill_span_positions(span, source);
            tokens.push((token, span));
        }

        // Audit2 B3: drain unterminated block-comment ranges accumulated
        // in `extras` and surface them as real `UnterminatedBlockComment`
        // diagnostics rather than letting the parser report a misleading
        // "unexpected end of input".
        for (start, end) in std::mem::take(&mut lexer.inner.extras.unterminated_block_comments) {
            lexer.errors.push(CompilerError::UnterminatedBlockComment {
                span: Span::from_range(start, end),
            });
        }

        // Audit2 B4: drain bad `\uXXXX` escape ranges and surface them as
        // `InvalidUnicodeEscape` diagnostics. The decoded string still
        // contains a U+FFFD replacement so downstream parsing can
        // continue, but the user is told what went wrong.
        for (start, end, hex) in std::mem::take(&mut lexer.inner.extras.invalid_unicode_escapes) {
            lexer.errors.push(CompilerError::InvalidUnicodeEscape {
                value: hex,
                span: Span::from_range(start, end),
            });
        }

        let errors = lexer
            .take_errors()
            .into_iter()
            .map(|e| fill_error_span_positions(e, source))
            .collect();

        (tokens, errors)
    }
}

/// Return the given error with its span upgraded to have line/column info.
///
/// Only lexer-produced variants ([`CompilerError::InvalidCharacter`],
/// [`CompilerError::UnterminatedString`], [`CompilerError::InvalidNumber`],
/// [`CompilerError::UnterminatedBlockComment`]) are produced by
/// [`Lexer::classify_error`] / [`tokenize_all_with_errors`]; any other variant
/// would indicate a bug in the lexer's error-classification logic and is
/// returned unchanged.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Lexer::classify_error only produces a small set of lexer-error variants; enumerating every CompilerError variant would be noisy without adding safety"
)]
fn fill_error_span_positions(error: CompilerError, source: &str) -> CompilerError {
    let span = crate::location::fill_span_positions(error.span(), source);
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
        CompilerError::InvalidNumber { value, .. } => CompilerError::InvalidNumber { value, span },
        other => other,
    }
}
