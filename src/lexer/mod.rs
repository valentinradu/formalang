mod token;

pub use token::{parse_regex, Token};

use crate::location::Span;
use logos::Logos;

/// Lexer for FormaLang source code
pub struct Lexer<'source> {
    lexer: logos::Lexer<'source, Token>,
}

impl<'source> Lexer<'source> {
    pub fn new(source: &'source str) -> Self {
        Self {
            lexer: Token::lexer(source),
        }
    }

    /// Get the next token with its span
    pub fn next_token(&mut self) -> Option<(Token, Span)> {
        let token = self.lexer.next()?;
        let span = self.lexer.span();
        let span = Span::from_range(span.start, span.end);

        match token {
            Ok(tok) => Some((tok, span)),
            Err(_) => {
                // Lexer error - skip this token and continue
                self.next_token()
            }
        }
    }

    /// Get current span
    pub fn span(&self) -> Span {
        let range = self.lexer.span();
        Span::from_range(range.start, range.end)
    }

    /// Tokenize entire source (useful for testing and debugging)
    pub fn tokenize_all(source: &'source str) -> Vec<(Token, Span)> {
        let mut lexer = Self::new(source);
        let mut tokens = Vec::new();

        while let Some((token, span)) = lexer.next_token() {
            if matches!(token, Token::Eof) {
                break;
            }
            // Fill in line/column positions from byte offsets
            let span = crate::location::fill_span_positions(span, source);
            tokens.push((token, span));
        }

        tokens
    }
}

