use super::token::TokenKind;
use crate::error::CompilerError;
use crate::location::{Location, Span};
use logos::Logos;

/// Tracks indentation levels and emits INDENT/DEDENT tokens
pub struct IndentationTracker<'source> {
    /// The underlying logos lexer
    lexer: logos::Lexer<'source, TokenKind>,
    /// Stack of indentation levels (in spaces)
    indent_stack: Vec<usize>,
    /// Pending tokens to emit before next lexer token
    pending_tokens: Vec<(TokenKind, Span)>,
    /// Current location tracking
    current_line: usize,
    current_column: usize,
    /// Source text for indentation measurement
    source: &'source str,
    /// Are we at the start of a line?
    at_line_start: bool,
}

impl<'source> IndentationTracker<'source> {
    pub fn new(source: &'source str) -> Self {
        Self {
            lexer: TokenKind::lexer(source),
            indent_stack: vec![0], // Start with zero indentation
            pending_tokens: Vec::new(),
            current_line: 1,
            current_column: 1,
            source,
            at_line_start: true,
        }
    }

    /// Get the next token with indentation handling
    pub fn next_token(&mut self) -> Option<Result<(TokenKind, Span), CompilerError>> {
        // First, emit any pending tokens (INDENT/DEDENT)
        if let Some((token, span)) = self.pending_tokens.pop() {
            return Some(Ok((token, span)));
        }

        // Get next token from logos
        let token_result = self.lexer.next()?;

        let token = match token_result {
            Ok(t) => t,
            Err(_) => {
                let span = self.current_span();
                return Some(Err(CompilerError::InvalidCharacter {
                    character: self.lexer.slice().chars().next().unwrap_or('�'),
                    span,
                }));
            }
        };

        let span = self.current_span();

        // Handle newlines specially - check indentation on next line
        if matches!(token, TokenKind::Newline) {
            self.current_line += 1;
            self.current_column = 1;
            self.at_line_start = true;
            return Some(Ok((TokenKind::Newline, span)));
        }

        // If we're at line start and see a real token, check indentation
        if self.at_line_start && !matches!(token, TokenKind::Eof) {
            self.at_line_start = false;

            // Measure indentation of current line
            if let Some(indent_result) = self.measure_current_indentation() {
                match indent_result {
                    Ok(indent_level) => {
                        // Compare to current indentation level
                        // Should never be empty (initialized with [0]), but handle defensively
                        let current_level = *self.indent_stack.last().unwrap_or(&0);

                        if indent_level > current_level {
                            // Indent
                            self.indent_stack.push(indent_level);
                            self.pending_tokens.push((token, span));
                            return Some(Ok((TokenKind::Indent, span)));
                        } else if indent_level < current_level {
                            // Dedent (possibly multiple levels)
                            let mut dedent_count = 0;
                            while let Some(&level) = self.indent_stack.last() {
                                if level <= indent_level {
                                    break;
                                }
                                self.indent_stack.pop();
                                dedent_count += 1;
                            }

                            // Check if indentation matches a previous level
                            if self.indent_stack.last() != Some(&indent_level) {
                                return Some(Err(CompilerError::InvalidIndentation { span }));
                            }

                            // Emit DEDENT tokens (in reverse order)
                            self.pending_tokens.push((token, span));
                            for _ in 0..dedent_count - 1 {
                                self.pending_tokens.push((TokenKind::Dedent, span));
                            }
                            return Some(Ok((TokenKind::Dedent, span)));
                        }
                    }
                    Err(err) => return Some(Err(err)),
                }
            }
        }

        // Update location
        self.update_location();

        Some(Ok((token, span)))
    }

    /// Measure indentation level of current line in spaces
    fn measure_current_indentation(&self) -> Option<Result<usize, CompilerError>> {
        let start = self.lexer.span().start;

        // Find start of current line
        let line_start = self.source[..start]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(0);

        let mut indent = 0;
        let mut has_tabs = false;
        let mut has_spaces = false;

        for ch in self.source[line_start..start].chars() {
            match ch {
                ' ' => {
                    has_spaces = true;
                    indent += 1;
                }
                '\t' => {
                    has_tabs = true;
                    indent += 4; // Count tab as 4 spaces
                }
                _ => break,
            }
        }

        // Check for mixed indentation
        if has_tabs && has_spaces {
            let span = self.current_span();
            return Some(Err(CompilerError::MixedIndentation { span }));
        }

        Some(Ok(indent))
    }

    /// Get current span from lexer
    fn current_span(&self) -> Span {
        let range = self.lexer.span();
        Span::new(
            Location::new(range.start, self.current_line, self.current_column),
            Location::new(
                range.end,
                self.current_line,
                self.current_column + (range.end - range.start),
            ),
        )
    }

    /// Update location tracking based on lexer position
    fn update_location(&mut self) {
        let slice = self.lexer.slice();
        for ch in slice.chars() {
            if ch == '\n' {
                self.current_line += 1;
                self.current_column = 1;
            } else {
                self.current_column += 1;
            }
        }
    }

    /// Consume all tokens and return them as a Vec (useful for testing)
    pub fn collect_all(mut self) -> Result<Vec<(TokenKind, Span)>, Vec<CompilerError>> {
        let mut tokens = Vec::new();
        let mut errors = Vec::new();

        while let Some(result) = self.next_token() {
            match result {
                Ok((TokenKind::Eof, _)) => break,
                Ok(token) => tokens.push(token),
                Err(err) => errors.push(err),
            }
        }

        if errors.is_empty() {
            Ok(tokens)
        } else {
            Err(errors)
        }
    }
}

