//! Targeted tests for semantic/position.rs coverage.
//!
//! Covers uncovered branches:
//! - `span_contains_lsp_position`
//! - `get_line_at_position` (within bounds and beyond)
//! - `get_word_at_offset` (start of string, middle, end, non-word chars)
//! - `get_word_at_lsp_position`
//! - `LspPosition::to_offset` edge cases (beyond line length, beyond end of file)

use formalang::semantic::position::{
    get_line_at_position, get_word_at_lsp_position, get_word_at_offset, span_contains_lsp_position,
    span_contains_offset, LspPosition,
};
use formalang::{Location, Span};

const fn make_span(start_offset: usize, end_offset: usize) -> Span {
    Span {
        start: Location {
            offset: start_offset,
            line: 1,
            column: 1,
        },
        end: Location {
            offset: end_offset,
            line: 1,
            column: end_offset.saturating_sub(start_offset).saturating_add(1),
        },
    }
}

// =============================================================================
// span_contains_offset
// =============================================================================

#[test]
fn test_span_contains_offset_inside() -> Result<(), Box<dyn std::error::Error>> {
    let span = make_span(5, 15);
    if !span_contains_offset(&span, 10) {
        return Err("expected span to contain offset 10".into());
    }
    Ok(())
}

#[test]
fn test_span_contains_offset_at_start() -> Result<(), Box<dyn std::error::Error>> {
    let span = make_span(5, 15);
    if !span_contains_offset(&span, 5) {
        return Err("expected span to contain start offset 5".into());
    }
    Ok(())
}

#[test]
fn test_span_contains_offset_at_end_exclusive() -> Result<(), Box<dyn std::error::Error>> {
    let span = make_span(5, 15);
    // end is exclusive
    if span_contains_offset(&span, 15) {
        return Err("expected span end 15 to be exclusive".into());
    }
    Ok(())
}

#[test]
fn test_span_contains_offset_before() -> Result<(), Box<dyn std::error::Error>> {
    let span = make_span(5, 15);
    if span_contains_offset(&span, 4) {
        return Err("expected span to not contain offset 4".into());
    }
    Ok(())
}

// =============================================================================
// span_contains_lsp_position
// =============================================================================

#[test]
fn test_span_contains_lsp_position_inside() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = hello";
    // "hello" is at offset 8..13
    let span = make_span(8, 13);
    let pos = LspPosition::new(0, 10); // offset 10 is inside "hello"
    if !span_contains_lsp_position(&span, pos, source) {
        return Err("expected span to contain LSP position at offset 10".into());
    }
    Ok(())
}

#[test]
fn test_span_contains_lsp_position_outside() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = hello";
    let span = make_span(8, 13);
    let pos = LspPosition::new(0, 0); // offset 0, before the span
    if span_contains_lsp_position(&span, pos, source) {
        return Err("expected span to not contain LSP position at offset 0".into());
    }
    Ok(())
}

#[test]
fn test_span_contains_lsp_position_second_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1\nlet hello = 2";
    // "hello" starts at offset 14
    let span = make_span(14, 19);
    let pos = LspPosition::new(1, 4); // line 1, char 4 = offset 14
    if !span_contains_lsp_position(&span, pos, source) {
        return Err("expected span to contain LSP position on second line".into());
    }
    Ok(())
}

// =============================================================================
// get_line_at_position
// =============================================================================

#[test]
fn test_get_line_at_position_first_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1\nlet y = 2";
    let pos = LspPosition::new(0, 0);
    let line = get_line_at_position(source, pos);
    if line != "let x = 1" {
        return Err(format!("expected {:?} but got {:?}", "let x = 1", line).into());
    }
    Ok(())
}

#[test]
fn test_get_line_at_position_second_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1\nlet y = 2";
    let pos = LspPosition::new(1, 0);
    let line = get_line_at_position(source, pos);
    if line != "let y = 2" {
        return Err(format!("expected {:?} but got {:?}", "let y = 2", line).into());
    }
    Ok(())
}

#[test]
fn test_get_line_at_position_beyond_end() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1";
    let pos = LspPosition::new(99, 0); // Line 99 doesn't exist
    let line = get_line_at_position(source, pos);
    if !line.is_empty() {
        return Err(format!("expected empty string but got {line:?}").into());
    }
    Ok(())
}

#[test]
fn test_get_line_at_position_empty_source() -> Result<(), Box<dyn std::error::Error>> {
    let source = "";
    let pos = LspPosition::new(0, 0);
    let line = get_line_at_position(source, pos);
    if !line.is_empty() {
        return Err(format!("expected empty string but got {line:?}").into());
    }
    Ok(())
}

// =============================================================================
// get_word_at_offset
// =============================================================================

#[test]
fn test_get_word_at_offset_start() -> Result<(), Box<dyn std::error::Error>> {
    let source = "hello world";
    // At start of "hello"
    let (word, start, end) = get_word_at_offset(source, 0).ok_or("expected Some at offset 0")?;
    if word != "hello" {
        return Err(format!("expected 'hello', got '{word}'").into());
    }
    if start != 0 {
        return Err(format!("expected start=0, got {start}").into());
    }
    if end != 5 {
        return Err(format!("expected end=5, got {end}").into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_middle_of_word() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let foobar = 1";
    // Offset 6 is inside "foobar" (starts at 4)
    let (word, ..) = get_word_at_offset(source, 6).ok_or("expected Some at offset 6")?;
    if word != "foobar" {
        return Err(format!("expected 'foobar', got '{word}'").into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_at_space() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1";
    // Offset 3 is the space after "let". rfind scans "let" (no non-word char) → start=0.
    // find scans " x = 1" and hits ' ' at index 0 → end=3. Returns "let".
    let (word, ..) = get_word_at_offset(source, 3)
        .ok_or("offset at space boundary should return word before space")?;
    if word != "let" {
        return Err(format!("expected 'let', got '{word}'").into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_at_end_of_source() -> Result<(), Box<dyn std::error::Error>> {
    let source = "abc";
    // offset == len is allowed (not > len), so returns the whole string as a word.
    let (word, ..) =
        get_word_at_offset(source, 3).ok_or("offset == len should still return the word")?;
    if word != "abc" {
        return Err(format!("expected 'abc', got '{word}'").into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_beyond_end() -> Result<(), Box<dyn std::error::Error>> {
    let source = "abc";
    let result = get_word_at_offset(source, 100);
    if result.is_some() {
        return Err("Offset beyond end should return None".into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_underscore() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let my_var = 0";
    let offset = source.find("my_var").ok_or("pattern not found")?;
    let (word, ..) = get_word_at_offset(source, offset + 2).ok_or("expected word at offset")?;
    if word != "my_var" {
        return Err(format!("expected 'my_var', got '{word}'").into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = "count = 42";
    let offset = source.find("42").ok_or("pattern not found")?;
    let (word, ..) = get_word_at_offset(source, offset).ok_or("expected word at offset")?;
    if word != "42" {
        return Err(format!("expected '42', got '{word}'").into());
    }
    Ok(())
}

// =============================================================================
// get_word_at_lsp_position
// =============================================================================

#[test]
fn test_get_word_at_lsp_position_first_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let hello = 1";
    let pos = LspPosition::new(0, 4); // character 4 is inside "hello"
    let (word, ..) = get_word_at_lsp_position(source, pos).ok_or("expected word at position")?;
    if word != "hello" {
        return Err(format!("expected 'hello', got '{word}'").into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_lsp_position_second_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1\nlet world = 2";
    let pos = LspPosition::new(1, 4); // "world" on second line
    let (word, ..) = get_word_at_lsp_position(source, pos).ok_or("expected word at position")?;
    if word != "world" {
        return Err(format!("expected 'world', got '{word}'").into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_lsp_position_at_space() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1";
    let pos = LspPosition::new(0, 3); // space after "let"
                                      // At a space boundary: get_word_at_lsp_position converts to offset 3 (space after "let"),
                                      // which get_word_at_offset handles the same way — returns "let" (word before the space).
    let (word, ..) = get_word_at_lsp_position(source, pos)
        .ok_or("expected word at space boundary via LSP position")?;
    if word != "let" {
        return Err(format!("expected 'let', got '{word}'").into());
    }
    Ok(())
}

// =============================================================================
// LspPosition::to_offset edge cases
// =============================================================================

#[test]
fn test_to_offset_beyond_line_length() -> Result<(), Box<dyn std::error::Error>> {
    // Character position beyond line length - should clamp to end of line
    let source = "abc\ndefgh";
    let pos = LspPosition::new(0, 100); // way beyond "abc" (3 chars)
    let offset = LspPosition::to_offset(source, pos);
    // End of line "abc" is at offset 3 (the \n position)
    if offset != 3 {
        return Err(format!(
            "Should return end of line for out-of-bounds char: expected 3, got {offset}"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_to_offset_last_line_no_newline() -> Result<(), Box<dyn std::error::Error>> {
    // Last line with no trailing newline
    let source = "first\nsecond";
    let pos = LspPosition::new(1, 3); // inside "second"
    let offset = LspPosition::to_offset(source, pos);
    if offset != 9 {
        return Err(format!("Should be offset 9 (6 + 3), got {offset}").into());
    }
    Ok(())
}

#[test]
fn test_to_location_wraps_offset() -> Result<(), Box<dyn std::error::Error>> {
    let source = "hello\nworld";
    let pos = LspPosition::new(1, 2);
    let loc = pos.to_location(source);
    // Line 1 (0-indexed) = line 2 (1-indexed), char 2 = column 3 (1-indexed)
    if loc.line != 2 {
        return Err(format!("expected line 2, got {}", loc.line).into());
    }
    if loc.column != 3 {
        return Err(format!("expected column 3, got {}", loc.column).into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_from_location_zero_indexed() -> Result<(), Box<dyn std::error::Error>> {
    let loc = Location {
        offset: 0,
        line: 1,
        column: 1,
    };
    let pos: LspPosition = loc.into();
    if pos.line != 0 {
        return Err(format!("expected line 0, got {}", pos.line).into());
    }
    if pos.character != 0 {
        return Err(format!("expected character 0, got {}", pos.character).into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_equality() -> Result<(), Box<dyn std::error::Error>> {
    let p1 = LspPosition::new(2, 5);
    let p2 = LspPosition::new(2, 5);
    if p1 != p2 {
        return Err(format!("expected {p1:?} == {p2:?}").into());
    }
    Ok(())
}
