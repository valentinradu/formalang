//! Position utilities for LSP integration
//!
//! This module provides utilities for bridging between LSP positions (0-indexed)
//! and FormaLang's internal Location system (1-indexed).

use crate::location::{offset_to_location, Location, Span};

/// LSP Position (0-indexed line and character)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

impl LspPosition {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }

    /// Convert LSP position (0-indexed) to FormaLang Location (1-indexed) with source text
    pub fn to_location(&self, source: &str) -> Location {
        let offset = Self::to_offset(source, *self);
        offset_to_location(offset, source)
    }

    /// Convert LSP position to byte offset
    pub fn to_offset(source: &str, position: LspPosition) -> usize {
        let mut current_line = 0u32;
        let mut byte_offset = 0usize;

        for (idx, ch) in source.char_indices() {
            // If we've reached the target line
            if current_line == position.line {
                // Count characters from the start of this line
                for (char_count, (char_idx, _)) in source[byte_offset..].char_indices().enumerate()
                {
                    if char_count == usize::try_from(position.character).unwrap_or(usize::MAX) {
                        return byte_offset.saturating_add(char_idx);
                    }

                    // Stop at newline
                    if let Some('\n') = source[byte_offset.saturating_add(char_idx)..]
                        .chars()
                        .next()
                    {
                        break;
                    }
                }

                // Character position is beyond the line length, return end of line
                let line_end = source[byte_offset..]
                    .find('\n')
                    .map(|n| byte_offset.saturating_add(n))
                    .unwrap_or(source.len());
                return line_end;
            }

            // Move to next line on newline
            if ch == '\n' {
                current_line = current_line.saturating_add(1);
                byte_offset = idx.saturating_add(ch.len_utf8());
            }
        }

        // Position is beyond end of file
        source.len()
    }
}

impl From<Location> for LspPosition {
    /// Convert FormaLang Location (1-indexed) to LSP position (0-indexed)
    fn from(location: Location) -> Self {
        Self {
            line: u32::try_from(location.line.saturating_sub(1)).unwrap_or(u32::MAX),
            character: u32::try_from(location.column.saturating_sub(1)).unwrap_or(u32::MAX),
        }
    }
}

/// Check if a span contains a given byte offset
pub fn span_contains_offset(span: &Span, offset: usize) -> bool {
    span.start.offset <= offset && offset < span.end.offset
}

/// Check if a span contains a given LSP position
pub fn span_contains_lsp_position(span: &Span, position: LspPosition, source: &str) -> bool {
    let offset = LspPosition::to_offset(source, position);
    span_contains_offset(span, offset)
}

/// Get the line content at a given LSP position
pub fn get_line_at_position(source: &str, position: LspPosition) -> &str {
    let lines: Vec<&str> = source.lines().collect();
    if (position.line as usize) < lines.len() {
        lines[position.line as usize]
    } else {
        ""
    }
}

/// Get the word at a given offset (useful for symbol resolution)
/// Returns (word, start_offset, end_offset)
pub fn get_word_at_offset(source: &str, offset: usize) -> Option<(String, usize, usize)> {
    if offset > source.len() {
        return None;
    }

    // Find word boundaries (alphanumeric and underscore)
    let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

    let start = source[..offset]
        .rfind(|c: char| !is_word_char(c))
        .map(|i| i.saturating_add(1))
        .unwrap_or(0);

    let end = source[offset..]
        .find(|c: char| !is_word_char(c))
        .map(|i| offset.saturating_add(i))
        .unwrap_or(source.len());

    if start < end {
        let word = source[start..end].to_string();
        Some((word, start, end))
    } else {
        None
    }
}

/// Get the word at a given LSP position
pub fn get_word_at_lsp_position(
    source: &str,
    position: LspPosition,
) -> Option<(String, usize, usize)> {
    let offset = LspPosition::to_offset(source, position);
    get_word_at_offset(source, offset)
}
