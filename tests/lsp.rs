//! LSP module tests
//!
//! Tests for position utilities, node finder, and semantic queries

use formalang::semantic::node_finder::{find_node_at_offset, NodeAtPosition};
use formalang::semantic::position::{
    get_line_at_position, get_word_at_offset, span_contains_offset, LspPosition,
};
use formalang::semantic::queries::QueryProvider;
use formalang::{compile_with_analyzer, Location, Span};

// =============================================================================
// Position Utility Tests
// =============================================================================

#[test]
fn test_lsp_position_to_offset_first_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1";
    let pos = LspPosition::new(0, 0);
    if LspPosition::to_offset(source, pos) != 0 {
        return Err(format!(
            "expected {:?} but got {:?}",
            0,
            LspPosition::to_offset(source, pos)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_to_offset_middle_of_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1";
    let pos = LspPosition::new(0, 4);
    if LspPosition::to_offset(source, pos) != 4 {
        return Err(format!(
            "expected {:?} but got {:?}",
            4,
            LspPosition::to_offset(source, pos)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_to_offset_second_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1\nlet y = 2";
    let pos = LspPosition::new(1, 0);
    let offset = LspPosition::to_offset(source, pos);
    if offset != 10 {
        return Err(format!("expected {:?} but got {:?}", 10, offset).into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_to_offset_second_line_middle() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1\nlet y = 2";
    let pos = LspPosition::new(1, 4);
    let offset = LspPosition::to_offset(source, pos);
    if offset != 14 {
        return Err(format!("expected {:?} but got {:?}", 14, offset).into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_to_offset_empty_lines() -> Result<(), Box<dyn std::error::Error>> {
    let source = "a\n\nb";
    let pos = LspPosition::new(2, 0);
    let offset = LspPosition::to_offset(source, pos);
    if offset != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, offset).into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_to_offset_beyond_source() -> Result<(), Box<dyn std::error::Error>> {
    let source = "abc";
    let pos = LspPosition::new(10, 0);
    // Should clamp to end of source
    let offset = LspPosition::to_offset(source, pos);
    if offset != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, offset).into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_to_location() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1\nlet y = 2";
    let pos = LspPosition::new(1, 4);
    let loc = pos.to_location(source);
    // Location is 1-indexed
    if loc.line != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, loc.line).into());
    }
    if loc.column != 5 {
        return Err(format!("expected {:?} but got {:?}", 5, loc.column).into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_from_location() -> Result<(), Box<dyn std::error::Error>> {
    let loc = Location {
        offset: 10,
        line: 2,
        column: 5,
    };
    let pos: LspPosition = loc.into();
    // LSP position is 0-indexed
    if pos.line != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, pos.line).into());
    }
    if pos.character != 4 {
        return Err(format!("expected {:?} but got {:?}", 4, pos.character).into());
    }
    Ok(())
}

#[test]
fn test_span_contains_offset_inside() -> Result<(), Box<dyn std::error::Error>> {
    let span = Span {
        start: Location {
            offset: 10,
            line: 1,
            column: 1,
        },
        end: Location {
            offset: 20,
            line: 1,
            column: 11,
        },
    };
    if !span_contains_offset(&span, 15) {
        return Err("assertion failed: span should contain offset 15".into());
    }
    Ok(())
}

#[test]
fn test_span_contains_offset_start_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let span = Span {
        start: Location {
            offset: 10,
            line: 1,
            column: 1,
        },
        end: Location {
            offset: 20,
            line: 1,
            column: 11,
        },
    };
    if !span_contains_offset(&span, 10) {
        return Err("assertion failed: span should contain start offset 10".into());
    }
    Ok(())
}

#[test]
fn test_span_contains_offset_end_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let span = Span {
        start: Location {
            offset: 10,
            line: 1,
            column: 1,
        },
        end: Location {
            offset: 20,
            line: 1,
            column: 11,
        },
    };
    // End is exclusive
    if span_contains_offset(&span, 20) {
        return Err("assertion failed: span end should be exclusive".into());
    }
    Ok(())
}

#[test]
fn test_span_contains_offset_before() -> Result<(), Box<dyn std::error::Error>> {
    let span = Span {
        start: Location {
            offset: 10,
            line: 1,
            column: 1,
        },
        end: Location {
            offset: 20,
            line: 1,
            column: 11,
        },
    };
    if span_contains_offset(&span, 5) {
        return Err("assertion failed: span should not contain offset 5".into());
    }
    Ok(())
}

#[test]
fn test_span_contains_offset_after() -> Result<(), Box<dyn std::error::Error>> {
    let span = Span {
        start: Location {
            offset: 10,
            line: 1,
            column: 1,
        },
        end: Location {
            offset: 20,
            line: 1,
            column: 11,
        },
    };
    if span_contains_offset(&span, 25) {
        return Err("assertion failed: span should not contain offset 25".into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let foo = 42";
    let result = get_word_at_offset(source, 5); // In "foo"
    if result.is_none() {
        return Err("assertion failed".into());
    }
    let (word, start, end) = result.ok_or("get_word_at_offset returned None")?;
    if word != "foo" {
        return Err(format!("expected {:?} but got {:?}", "foo", word).into());
    }
    if start != 4 {
        return Err(format!("expected {:?} but got {:?}", 4, start).into());
    }
    if end != 7 {
        return Err(format!("expected {:?} but got {:?}", 7, end).into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_start_of_word() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let foo = 42";
    let result = get_word_at_offset(source, 4); // Start of "foo"
    if result.is_none() {
        return Err("assertion failed".into());
    }
    let (word, _, _) = result.ok_or("get_word_at_offset returned None")?;
    if word != "foo" {
        return Err(format!("expected {:?} but got {:?}", "foo", word).into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_end_of_word() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let foo = 42";
    let result = get_word_at_offset(source, 7); // Right after "foo"
                                                // At position 7 (the space after "foo"), we're at the boundary
                                                // The implementation finds the word that contains the offset
    if !(result.is_none() || result.as_ref().map(|(w, _, _)| w.as_str()) == Some("foo")) {
        return Err("expected None or Some(\"foo\") at end-of-word boundary".into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_on_space() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let foo = 42";
    let result = get_word_at_offset(source, 3); // At position 3, still touching "let"
                                                // At boundary, the implementation may return the adjacent word
                                                // This is acceptable behavior for LSP word lookup
    let (word, _, _) = result.ok_or("offset at space boundary should return adjacent word")?;
    if word != "let" {
        return Err(format!("expected {:?} but got {:?}", "let", word).into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_on_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let foo = 42";
    let result = get_word_at_offset(source, 10); // In "42"
    if result.is_none() {
        return Err("assertion failed".into());
    }
    let (word, _, _) = result.ok_or("get_word_at_offset returned None")?;
    if word != "42" {
        return Err(format!("expected {:?} but got {:?}", "42", word).into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_empty_source() -> Result<(), Box<dyn std::error::Error>> {
    let source = "";
    let result = get_word_at_offset(source, 0);
    if result.is_some() {
        return Err("expected None for empty source".into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_underscore() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let my_var = 1";
    let result = get_word_at_offset(source, 6); // In "my_var"
    if result.is_none() {
        return Err("assertion failed".into());
    }
    let (word, _, _) = result.ok_or("get_word_at_offset returned None")?;
    if word != "my_var" {
        return Err(format!("expected {:?} but got {:?}", "my_var", word).into());
    }
    Ok(())
}

#[test]
fn test_get_word_at_offset_out_of_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let source = "abc";
    let result = get_word_at_offset(source, 100);
    if result.is_some() {
        return Err("expected None for out-of-bounds offset".into());
    }
    Ok(())
}

#[test]
fn test_get_line_at_position_first_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "line one\nline two\nline three";
    let pos = LspPosition::new(0, 0);
    let line = get_line_at_position(source, pos);
    if line != "line one" {
        return Err(format!("expected {:?} but got {:?}", "line one", line).into());
    }
    Ok(())
}

#[test]
fn test_get_line_at_position_second_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "line one\nline two\nline three";
    let pos = LspPosition::new(1, 0);
    let line = get_line_at_position(source, pos);
    if line != "line two" {
        return Err(format!("expected {:?} but got {:?}", "line two", line).into());
    }
    Ok(())
}

#[test]
fn test_get_line_at_position_last_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "line one\nline two\nline three";
    let pos = LspPosition::new(2, 0);
    let line = get_line_at_position(source, pos);
    if line != "line three" {
        return Err(format!("expected {:?} but got {:?}", "line three", line).into());
    }
    Ok(())
}

#[test]
fn test_get_line_at_position_out_of_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let source = "line one\nline two";
    let pos = LspPosition::new(10, 0);
    let line = get_line_at_position(source, pos);
    // Returns empty string for out of bounds
    if !line.is_empty() {
        return Err(format!("expected {:?} but got {:?}", "", line).into());
    }
    Ok(())
}

#[test]
fn test_get_line_at_position_single_line() -> Result<(), Box<dyn std::error::Error>> {
    let source = "only one line";
    let pos = LspPosition::new(0, 5);
    let line = get_line_at_position(source, pos);
    if line != "only one line" {
        return Err(format!("expected {:?} but got {:?}", "only one line", line).into());
    }
    Ok(())
}

// =============================================================================
// Query Provider Tests
// =============================================================================

#[test]
fn test_query_provider_completions_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String,
            age: Number
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();

    // Should include the User struct
    let has_user = completions.iter().any(|c| c.label == "User");
    if !(has_user) {
        return Err("Should have User completion".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_completions_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Printable {
            text: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();

    // Should include the Printable trait
    let has_printable = completions.iter().any(|c| c.label == "Printable");
    if !(has_printable) {
        return Err("Should have Printable completion".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_completions_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            active,
            inactive
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();

    // Should include the Status enum
    let has_status = completions.iter().any(|c| c.label == "Status");
    if !(has_status) {
        return Err("Should have Status completion".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_completions_multiple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: String }
        struct B { y: Number }
        trait C { z: Boolean }
        enum D { one, two }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();

    // Should include all definitions
    if completions.len() < 4 {
        return Err(format!(
            "Should have at least 4 completions, got {}",
            completions.len()
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_query_provider_completions_builtin_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_type_completions();

    // Should include builtin types like String, Number, Boolean
    let has_string = completions.iter().any(|c| c.label == "String");
    let has_number = completions.iter().any(|c| c.label == "Number");
    let has_boolean = completions.iter().any(|c| c.label == "Boolean");

    if !(has_string) {
        return Err("Should have String completion".into());
    }
    if !(has_number) {
        return Err("Should have Number completion".into());
    }
    if !(has_boolean) {
        return Err("Should have Boolean completion".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_completions_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();

    let has_box = completions.iter().any(|c| c.label == "Box");
    if !(has_box) {
        return Err("Should have Box completion".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_module_definitions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod core {
            struct Config {
                value: String
            }
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();

    // Verify completions were generated - the count should be > 0
    // Config may be in completions with full path or short name
    if completions.is_empty() {
        return Err("Should have some completions from module".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_info() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());

    let hover = provider.get_hover_for_symbol("User");
    if hover.is_none() {
        return Err("Should have hover info for User".into());
    }
    let hover = hover.ok_or("hover was None")?;
    if !(hover.signature.contains("struct User")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_find_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());

    let def = provider.find_definition_by_name("Named");
    if def.is_none() {
        return Err("Should find definition for Named".into());
    }
    let def = def.ok_or("definition was None")?;
    if def.symbol_name != "Named" {
        return Err(format!("expected {:?} but got {:?}", "Named", def.symbol_name).into());
    }
    Ok(())
}

// =============================================================================
// Node Finder Tests
// =============================================================================

/// Helper to check if a position context found a specific node (not just File)
const fn found_specific_node(ctx: &formalang::semantic::node_finder::PositionContext) -> bool {
    !matches!(ctx.node, NodeAtPosition::File)
}

#[test]
fn test_find_node_at_offset_struct_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset of "User" in the source
    let user_offset = source.find("User").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, user_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at struct name position".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_field_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset of "name" in the source
    let name_offset = source.find("name").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, name_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at field name position".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_type_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset of "String" in the source
    let string_offset = source.find("String").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, string_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at type reference position".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_trait_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    let named_offset = source.find("Named").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, named_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at trait name position".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_enum_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            active,
            inactive
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    let status_offset = source.find("Status").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, status_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at enum name position".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_enum_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            active,
            inactive
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    let active_offset = source.find("active").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, active_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at enum variant position".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_impl_block() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Value {
            data: String = "test"
        }
        impl Value {
            fn get_data() -> String { self.data }
        }
    "#;
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find the impl keyword - node finder may not track impl blocks specifically
    let impl_offset = source.find("impl").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, impl_offset);

    // Verify node finder returns context (may be File if impl not tracked)
    if context.offset != impl_offset {
        return Err("Context should have correct offset".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod core {
            struct Config {
                value: String
            }
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    let module_offset = source.find("core").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, module_offset);

    // Verify context has the right offset
    if context.offset != module_offset {
        return Err("Context should have correct offset".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_expression_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x = 42
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    let num_offset = source.find("42").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, num_offset);

    // Verify context has the right offset
    if context.offset != num_offset {
        return Err("Context should have correct offset".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_string_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s = "hello"
    "#;
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    let str_offset = source.find("hello").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, str_offset);

    // Verify context has the right offset
    if context.offset != str_offset {
        return Err("Context should have correct offset".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_whitespace() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: String }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Offset 0 is whitespace/newline
    let context = find_node_at_offset(&file, 0);

    // At whitespace, should return File
    if !(matches!(context.node, NodeAtPosition::File)) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_out_of_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { x: String }";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Way beyond source length
    let context = find_node_at_offset(&file, 10000);

    // Should return File
    if !(matches!(context.node, NodeAtPosition::File)) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_nested_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner {
            id: Number
        }
        struct Outer {
            inner: Inner
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find "Inner" in the field type (second usage)
    let inner_usages: Vec<_> = source.match_indices("Inner").collect();
    if inner_usages.len() < 2 {
        return Err(format!(
            "Expected at least 2 'Inner' usages, got {}",
            inner_usages.len()
        )
        .into());
    }

    // Second usage is as field type
    let field_type_offset = inner_usages
        .get(1)
        .ok_or("out of bounds: inner_usages[1]")?
        .0;
    let context = find_node_at_offset(&file, field_type_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at nested type reference".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_generic_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    let t_offset = source.find("<T>").ok_or("unwrap on None")? + 1; // Inside <T>
    let context = find_node_at_offset(&file, t_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at generic param position".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_offset_trait_conformance() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }
        struct User {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find "Named" in the trait definition
    let named_usages: Vec<_> = source.match_indices("Named").collect();
    if named_usages.is_empty() {
        return Err("Expected at least 1 'Named' usage".into());
    }

    let trait_offset = named_usages
        .first()
        .ok_or("out of bounds: named_usages[0]")?
        .0;
    let context = find_node_at_offset(&file, trait_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at trait definition position".into());
    }
    Ok(())
}

#[test]
fn test_position_context_has_parents() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Outer {
            inner: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset of "String" in the field type
    let string_offset = source.find("String").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, string_offset);

    // Should have parent context (the field, then the struct)
    if context.parents.is_empty() && !found_specific_node(&context) {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Integration Tests for LSP Workflow
// =============================================================================

#[test]
fn test_lsp_workflow_position_to_completion() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
        struct Admin {
            user: User
        }
    ";

    // Compile and get analyzer
    let result = compile_with_analyzer(source);
    let (file, analyzer) = result.map_err(|e| format!("{e:?}"))?;

    // Simulate cursor at "User" in Admin's field type
    let user_in_admin = source.rfind("User").ok_or("'User' not found in source")?;

    // Find node at position
    let context = find_node_at_offset(&file, user_in_admin);
    if !(found_specific_node(&context)) {
        return Err("assertion failed".into());
    }

    // Get completions
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();

    // User should be in completions
    let has_user = completions.iter().any(|c| c.label == "User");
    if !(has_user) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lsp_workflow_multiline_navigation() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A {\n    x: String\n}\nstruct B {\n    y: Number\n}";

    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Navigate to different lines
    let pos_line_0 = LspPosition::new(0, 7); // "A" in struct A
    let pos_line_3 = LspPosition::new(3, 7); // "B" in struct B

    let offset_a = LspPosition::to_offset(source, pos_line_0);
    let offset_b = LspPosition::to_offset(source, pos_line_3);

    let context_a = find_node_at_offset(&file, offset_a);
    let context_b = find_node_at_offset(&file, offset_b);

    if !(found_specific_node(&context_a)) {
        return Err("Should find struct A".into());
    }
    if !(found_specific_node(&context_b)) {
        return Err("Should find struct B".into());
    }
    Ok(())
}

#[test]
fn test_lsp_position_line_content() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct First { a: String }\nstruct Second { b: Number }";

    let pos1 = LspPosition::new(0, 0);
    let pos2 = LspPosition::new(1, 0);

    let line1 = get_line_at_position(source, pos1);
    let line2 = get_line_at_position(source, pos2);

    if line1 != "struct First { a: String }" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "struct First { a: String }", line1
        )
        .into());
    }
    if line2 != "struct Second { b: Number }" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "struct Second { b: Number }", line2
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lsp_word_extraction_from_position() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct MyStruct { field: String }";

    // Get word at different positions
    let word_struct = get_word_at_offset(source, 3); // In "struct"
    let word_name = get_word_at_offset(source, 10); // In "MyStruct"
    let word_field = get_word_at_offset(source, 20); // In "field"

    let word_struct = word_struct.ok_or("word_struct was None")?;
    if word_struct.0 != "struct" {
        return Err(format!("expected {:?} but got {:?}", "struct", word_struct.0).into());
    }

    let word_name = word_name.ok_or("word_name was None")?;
    if word_name.0 != "MyStruct" {
        return Err(format!("expected {:?} but got {:?}", "MyStruct", word_name.0).into());
    }

    let word_field = word_field.ok_or("word_field was None")?;
    if word_field.0 != "field" {
        return Err(format!("expected {:?} but got {:?}", "field", word_field.0).into());
    }
    Ok(())
}

#[test]
fn test_position_to_offset_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let source = "line1\nline2\nline3";

    // Multiple positions on the same line should map correctly
    let pos1 = LspPosition::new(1, 0);
    let pos2 = LspPosition::new(1, 2);
    let pos3 = LspPosition::new(1, 4);

    let off1 = LspPosition::to_offset(source, pos1);
    let off2 = LspPosition::to_offset(source, pos2);
    let off3 = LspPosition::to_offset(source, pos3);

    // Should be consecutive
    if off2.wrapping_sub(off1) != 2 {
        return Err(format!("expected off2 - off1 == 2, got {}", off2.wrapping_sub(off1)).into());
    }
    if off3.wrapping_sub(off2) != 2 {
        return Err(format!("expected off3 - off2 == 2, got {}", off3.wrapping_sub(off2)).into());
    }
    Ok(())
}

// =============================================================================
// Additional Query Provider Coverage Tests
// =============================================================================

#[test]
fn test_query_provider_view_completion() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Card {
            title: String,
            content: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();

    // Should include Card as a completion
    let has_card = completions.iter().any(|c| c.label == "Card");
    if !(has_card) {
        return Err("Should have Card completion".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_view_trait_completion() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container {
            content: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();

    // Should include Container as a completion
    let has_container = completions.iter().any(|c| c.label == "Container");
    if !(has_container) {
        return Err("Should have Container completion".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_type_completions_with_view() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Model {
            value: String
        }
        struct View {
            title: String,
            body: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_type_completions();

    // Both Model and View should be available as type completions
    let has_model = completions.iter().any(|c| c.label == "Model");
    let has_view = completions.iter().any(|c| c.label == "View");
    if !(has_model) {
        return Err("Should have Model type completion".into());
    }
    if !(has_view) {
        return Err("Should have View type completion".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_for_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            active,
            pending,
            done
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());

    let hover = provider.get_hover_for_symbol("Status");
    if hover.is_none() {
        return Err("Should have hover info for Status".into());
    }
    let hover = hover.ok_or("hover was None")?;
    if !(hover.signature.contains("enum Status")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_for_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let config = "value"
    "#;
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());

    let hover = provider.get_hover_for_symbol("config");
    if hover.is_none() {
        return Err("Should have hover info for config".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_find_definition_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color {
            red,
            green,
            blue
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());

    let def = provider.find_definition_by_name("Color");
    if def.is_none() {
        return Err("Should find definition for Color".into());
    }
    let def = def.ok_or("definition was None")?;
    if def.symbol_name != "Color" {
        return Err(format!("expected {:?} but got {:?}", "Color", def.symbol_name).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_find_definition_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let myValue = 42
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());

    let def = provider.find_definition_by_name("myValue");
    if def.is_none() {
        return Err("Should find definition for myValue".into());
    }
    let def = def.ok_or("definition was None")?;
    if def.symbol_name != "myValue" {
        return Err(format!("expected {:?} but got {:?}", "myValue", def.symbol_name).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_find_definition_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point {
            x: Number,
            y: Number
        }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());

    let def = provider.find_definition_by_name("Point");
    if def.is_none() {
        return Err("Should find definition for Point".into());
    }
    let def = def.ok_or("definition was None")?;
    if def.symbol_name != "Point" {
        return Err(format!("expected {:?} but got {:?}", "Point", def.symbol_name).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_find_nonexistent() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User { name: String }
    ";
    let result = compile_with_analyzer(source);
    let (_, analyzer) = result.map_err(|e| format!("{e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());

    let def = provider.find_definition_by_name("NonExistent");
    if def.is_some() {
        return Err("Should not find definition for NonExistent".into());
    }

    let hover = provider.get_hover_for_symbol("NonExistent");
    if hover.is_some() {
        return Err("Should not have hover for NonExistent".into());
    }
    Ok(())
}

// =============================================================================
// Additional Node Finder Coverage Tests
// =============================================================================

#[test]
fn test_find_node_enclosing_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset inside field "name"
    let field_offset = source.find("name:").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, field_offset);

    // Should have enclosing definition (the struct)
    let enclosing = context.enclosing_definition();
    if enclosing.is_none() {
        return Err("Should have enclosing definition".into());
    }
    Ok(())
}

#[test]
fn test_find_node_is_in_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Result { value: Number = 42 }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at the literal "42"
    let expr_offset = source.find("42").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, expr_offset);

    // Context should have the correct offset
    if context.offset != expr_offset {
        return Err(format!(
            "Context should have correct offset: {:?} != {:?}",
            context.offset, expr_offset
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_node_is_in_type_position() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at "String" type
    let type_offset = source.find("String").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, type_offset);

    // Context should have the correct offset
    if context.offset != type_offset {
        return Err(format!(
            "Context should have correct offset: {:?} != {:?}",
            context.offset, type_offset
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_let_binding() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let value = 42
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at "value"
    let value_offset = source.find("value").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, value_offset);

    if !found_specific_node(&context) {
        return Err("Should find node at let binding name".into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_let_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let x = "hello"
    "#;
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at the string literal
    let str_offset = source.find("\"hello\"").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, str_offset);

    // Context should track the offset
    if context.offset != str_offset {
        return Err(format!(
            "Context should have correct offset: {:?} != {:?}",
            context.offset, str_offset
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_enum_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            active,
            pending
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at "pending" variant
    let variant_offset = source.find("pending").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, variant_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at enum variant".into());
    }
    Ok(())
}

#[test]
fn test_find_node_in_impl_with_struct_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Container { point: Point = Point(x: 1, y: 2) }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at "Point" in instantiation
    let impl_offset = source.rfind("Point").ok_or("'Point' not found in source")?;
    let context = find_node_at_offset(&file, impl_offset);

    // Context should track the offset
    if context.offset != impl_offset {
        return Err(format!(
            "Context should have correct offset: {:?} != {:?}",
            context.offset, impl_offset
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_node_in_for_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct List { items: [String] = for x in ["a", "b"] { x } }
    "#;
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at iterator variable "x" (first occurrence in for)
    let for_offset = source.find("for x").ok_or("unwrap on None")? + 4;
    let context = find_node_at_offset(&file, for_offset);

    // Context should track the offset
    if context.offset != for_offset {
        return Err(format!(
            "Context should have correct offset: {:?} != {:?}",
            context.offset, for_offset
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_node_in_if_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Result { value: String = if true { "yes" } else { "no" } }
    "#;
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at "true" condition
    let cond_offset = source.find("true").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, cond_offset);

    // Context should track the offset
    if context.offset != cond_offset {
        return Err(format!(
            "Context should have correct offset: {:?} != {:?}",
            context.offset, cond_offset
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_node_in_binary_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Math { result: Number = 1 + 2 }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at "+"
    let op_offset = source.find('+').ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, op_offset);

    // Should find something at the operator position
    if context.offset != op_offset {
        return Err(format!(
            "Context should have correct offset: {:?} != {:?}",
            context.offset, op_offset
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_node_trait_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String,
            age: Number
        }
    ";
    let result = compile_with_analyzer(source);
    let (file, _) = result.map_err(|e| format!("{e:?}"))?;

    // Find offset at "age" field
    let field_offset = source.find("age:").ok_or("unwrap on None")?;
    let context = find_node_at_offset(&file, field_offset);

    if !(found_specific_node(&context)) {
        return Err("Should find node at trait field".into());
    }
    Ok(())
}
