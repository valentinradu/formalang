//! Error reporting tests
//!
//! Tests for error message formatting and display

use formalang::{compile_and_report, CompilerError, Location, Span};

// =============================================================================
// Error Report Formatting Tests
// =============================================================================

#[test]
fn test_error_report_parse_error() {
    let source = r#"
        struct Invalid {
            name String
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
    let report = result.unwrap_err();
    // Should contain error information
    assert!(!report.is_empty());
}

#[test]
fn test_error_report_undefined_type() {
    let source = r#"
        struct User {
            data: UnknownType
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(report.contains("UnknownType"));
}

#[test]
fn test_error_report_duplicate_definition() {
    let source = r#"
        struct Config { name: String }
        struct Config { value: Number }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(report.contains("Config"));
}

#[test]
fn test_error_report_missing_trait_field() {
    let source = r#"
        trait Named {
            name: String
        }
        struct User: Named {
            age: Number
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
}

#[test]
fn test_error_report_undefined_trait() {
    let source = r#"
        struct User: NonexistentTrait {
            name: String
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(report.contains("NonexistentTrait"));
}

#[test]
fn test_error_report_impl_undefined() {
    let source = r#"
        impl UnknownStruct {
            "value"
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
}

// =============================================================================
// CompilerError Display Tests
// =============================================================================

#[test]
fn test_compiler_error_display_parse() {
    let error = CompilerError::ParseError {
        message: "unexpected token".to_string(),
        span: Span {
            start: Location { offset: 0, line: 1, column: 1 },
            end: Location { offset: 10, line: 1, column: 11 },
        },
    };
    let display = format!("{}", error);
    assert!(display.contains("Parse error"));
}

#[test]
fn test_compiler_error_display_undefined_type() {
    let error = CompilerError::UndefinedType {
        name: "FooBar".to_string(),
        span: Span {
            start: Location { offset: 0, line: 1, column: 1 },
            end: Location { offset: 6, line: 1, column: 7 },
        },
    };
    let display = format!("{}", error);
    assert!(display.contains("FooBar"));
}

#[test]
fn test_compiler_error_display_duplicate() {
    let error = CompilerError::DuplicateDefinition {
        name: "Widget".to_string(),
        span: Span {
            start: Location { offset: 0, line: 1, column: 1 },
            end: Location { offset: 10, line: 1, column: 11 },
        },
    };
    let display = format!("{}", error);
    assert!(display.contains("Widget"));
}

#[test]
fn test_compiler_error_display_missing_trait_field() {
    let error = CompilerError::MissingTraitField {
        field: "name".to_string(),
        trait_name: "Named".to_string(),
        span: Span {
            start: Location { offset: 0, line: 1, column: 1 },
            end: Location { offset: 10, line: 1, column: 11 },
        },
    };
    let display = format!("{}", error);
    assert!(display.contains("name"));
    assert!(display.contains("Named"));
}

#[test]
fn test_compiler_error_display_undefined_reference() {
    let error = CompilerError::UndefinedReference {
        name: "unknownVar".to_string(),
        span: Span {
            start: Location { offset: 0, line: 1, column: 1 },
            end: Location { offset: 10, line: 1, column: 11 },
        },
    };
    let display = format!("{}", error);
    assert!(display.contains("unknownVar"));
}

#[test]
fn test_compiler_error_display_undefined_trait() {
    let error = CompilerError::UndefinedTrait {
        name: "NonexistentTrait".to_string(),
        span: Span {
            start: Location { offset: 0, line: 1, column: 1 },
            end: Location { offset: 16, line: 1, column: 17 },
        },
    };
    let display = format!("{}", error);
    assert!(display.contains("NonexistentTrait"));
}

#[test]
fn test_compiler_error_display_invalid_binary_op() {
    let error = CompilerError::InvalidBinaryOp {
        op: "Add".to_string(),
        left_type: "String".to_string(),
        right_type: "Number".to_string(),
        span: Span {
            start: Location { offset: 0, line: 1, column: 1 },
            end: Location { offset: 10, line: 1, column: 11 },
        },
    };
    let display = format!("{}", error);
    assert!(display.contains("Add") || display.contains("binary"));
}

// =============================================================================
// Multiple Error Tests
// =============================================================================

#[test]
fn test_multiple_errors_reported() {
    let source = r#"
        struct A { x: Unknown1 }
        struct B { y: Unknown2 }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
    let report = result.unwrap_err();
    // Should contain both error types
    assert!(report.contains("Unknown1") || report.contains("Unknown2"));
}

// =============================================================================
// Location and Span Tests
// =============================================================================

#[test]
fn test_location_default() {
    let loc = Location::default();
    assert_eq!(loc.offset, 0);
    assert_eq!(loc.line, 1);
    assert_eq!(loc.column, 1);
}

#[test]
fn test_span_default() {
    let span = Span::default();
    assert_eq!(span.start.line, 1);
    assert_eq!(span.end.line, 1);
}

#[test]
fn test_span_creation() {
    let span = Span {
        start: Location { offset: 10, line: 2, column: 5 },
        end: Location { offset: 20, line: 2, column: 15 },
    };
    assert_eq!(span.start.offset, 10);
    assert_eq!(span.end.offset, 20);
}
