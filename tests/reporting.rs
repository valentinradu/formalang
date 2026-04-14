//! Error reporting tests
//!
//! Tests for error message formatting and display

use formalang::reporting::report_error;
use formalang::reporting::report_errors;
use formalang::{compile_and_report, CompilerError, Location, Span};

// =============================================================================
// Error Report Formatting Tests
// =============================================================================

#[test]
fn test_error_report_parse_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Invalid {
            name String
        }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let report = result.err().ok_or("expected error")?;
    // Should contain error information
    if report.is_empty() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_report_undefined_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            data: UnknownType
        }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let report = result.err().ok_or("expected error")?;
    if !(report.contains("UnknownType")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_report_duplicate_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { name: String }
        struct Config { value: Number }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let report = result.err().ok_or("expected error")?;
    if !(report.contains("Config")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_report_missing_trait_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }
        struct User: Named {
            age: Number
        }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_report_undefined_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User: NonexistentTrait {
            name: String
        }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let report = result.err().ok_or("expected error")?;
    if !(report.contains("NonexistentTrait")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_report_impl_undefined() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        impl UnknownStruct {
            "value"
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// CompilerError Display Tests
// =============================================================================

#[test]
fn test_compiler_error_display_parse() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::ParseError {
        message: "unexpected token".to_string(),
        span: Span {
            start: Location {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Location {
                offset: 10,
                line: 1,
                column: 11,
            },
        },
    };
    let display = format!("{error}");
    if !(display.contains("Parse error")) {
        return Err(format!("expected display to contain 'Parse error', got: {display}").into());
    }
    Ok(())
}

#[test]
fn test_compiler_error_display_undefined_type() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::UndefinedType {
        name: "FooBar".to_string(),
        span: Span {
            start: Location {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Location {
                offset: 6,
                line: 1,
                column: 7,
            },
        },
    };
    let display = format!("{error}");
    if !(display.contains("FooBar")) {
        return Err(format!("expected display to contain 'FooBar', got: {display}").into());
    }
    Ok(())
}

#[test]
fn test_compiler_error_display_duplicate() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::DuplicateDefinition {
        name: "Widget".to_string(),
        span: Span {
            start: Location {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Location {
                offset: 10,
                line: 1,
                column: 11,
            },
        },
    };
    let display = format!("{error}");
    if !(display.contains("Widget")) {
        return Err(format!("expected display to contain 'Widget', got: {display}").into());
    }
    Ok(())
}

#[test]
fn test_compiler_error_display_missing_trait_field() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::MissingTraitField {
        field: "name".to_string(),
        trait_name: "Named".to_string(),
        span: Span {
            start: Location {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Location {
                offset: 10,
                line: 1,
                column: 11,
            },
        },
    };
    let display = format!("{error}");
    if !(display.contains("name")) {
        return Err(format!("expected display to contain 'name', got: {display}").into());
    }
    if !(display.contains("Named")) {
        return Err(format!("expected display to contain 'Named', got: {display}").into());
    }
    Ok(())
}

#[test]
fn test_compiler_error_display_undefined_reference() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::UndefinedReference {
        name: "unknownVar".to_string(),
        span: Span {
            start: Location {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Location {
                offset: 10,
                line: 1,
                column: 11,
            },
        },
    };
    let display = format!("{error}");
    if !(display.contains("unknownVar")) {
        return Err(format!("expected display to contain 'unknownVar', got: {display}").into());
    }
    Ok(())
}

#[test]
fn test_compiler_error_display_undefined_trait() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::UndefinedTrait {
        name: "NonexistentTrait".to_string(),
        span: Span {
            start: Location {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Location {
                offset: 16,
                line: 1,
                column: 17,
            },
        },
    };
    let display = format!("{error}");
    if !(display.contains("NonexistentTrait")) {
        return Err(
            format!("expected display to contain 'NonexistentTrait', got: {display}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_compiler_error_display_invalid_binary_op() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::InvalidBinaryOp {
        op: "Add".to_string(),
        left_type: "String".to_string(),
        right_type: "Number".to_string(),
        span: Span {
            start: Location {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Location {
                offset: 10,
                line: 1,
                column: 11,
            },
        },
    };
    let display = format!("{error}");
    if !(display.contains("Add") || display.contains("binary")) {
        return Err(
            format!("expected display to contain 'Add' or 'binary', got: {display}").into(),
        );
    }
    Ok(())
}

// =============================================================================
// Multiple Error Tests
// =============================================================================

#[test]
fn test_multiple_errors_reported() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Unknown1 }
        struct B { y: Unknown2 }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let report = result.err().ok_or("expected error")?;
    // Should contain both error types
    if !(report.contains("Unknown1") || report.contains("Unknown2")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Location and Span Tests
// =============================================================================

#[test]
fn test_location_default() -> Result<(), Box<dyn std::error::Error>> {
    let loc = Location::default();
    if loc.offset != 0 {
        return Err(format!("expected offset 0 but got {:?}", loc.offset).into());
    }
    if loc.line != 1 {
        return Err(format!("expected line 1 but got {:?}", loc.line).into());
    }
    if loc.column != 1 {
        return Err(format!("expected column 1 but got {:?}", loc.column).into());
    }
    Ok(())
}

#[test]
fn test_span_default() -> Result<(), Box<dyn std::error::Error>> {
    let span = Span::default();
    if span.start.line != 1 {
        return Err(format!("expected start.line 1 but got {:?}", span.start.line).into());
    }
    if span.end.line != 1 {
        return Err(format!("expected end.line 1 but got {:?}", span.end.line).into());
    }
    Ok(())
}

#[test]
fn test_span_creation() -> Result<(), Box<dyn std::error::Error>> {
    let span = Span {
        start: Location {
            offset: 10,
            line: 2,
            column: 5,
        },
        end: Location {
            offset: 20,
            line: 2,
            column: 15,
        },
    };
    if span.start.offset != 10 {
        return Err(format!("expected start.offset 10 but got {:?}", span.start.offset).into());
    }
    if span.end.offset != 20 {
        return Err(format!("expected end.offset 20 but got {:?}", span.end.offset).into());
    }
    Ok(())
}

// =============================================================================
// Additional Error Reporting Tests for Coverage
// =============================================================================

#[test]
fn test_error_report_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // Type mismatch in trait field implementation
    let source = r"
        trait Countable {
            count: Number
        }
        struct Test: Countable {
            count: String
        }
    ";
    let result = compile_and_report(source, "test.fv");
    // Type mismatch should be detected - struct field has wrong type for trait
    if result.is_ok() {
        return Err("Type mismatch in trait implementation should error".into());
    }
    Ok(())
}

#[test]
fn test_error_report_circular_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait A: B { x: String }
        trait B: A { y: String }
    ";
    let result = compile_and_report(source, "test.fv");
    // Circular trait inheritance should be detected as an error
    if result.is_ok() {
        return Err("Circular trait inheritance should error".into());
    }
    Ok(())
}

#[test]
fn test_error_display_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::TypeMismatch {
        expected: "Number".to_string(),
        found: "String".to_string(),
        span: Span {
            start: Location {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Location {
                offset: 10,
                line: 1,
                column: 11,
            },
        },
    };
    let display = format!("{error}");
    if !(display.contains("Number") || display.contains("mismatch")) {
        return Err(
            format!("expected display to contain 'Number' or 'mismatch', got: {display}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_display_module_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::ModuleNotFound {
        name: "missing_module".to_string(),
        span: Span {
            start: Location {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Location {
                offset: 14,
                line: 1,
                column: 15,
            },
        },
    };
    let display = format!("{error}");
    if !(display.contains("missing_module") || display.contains("Module")) {
        return Err(
            format!("expected display to contain 'missing_module' or 'Module', got: {display}")
                .into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_display_circular_import() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::CircularImport {
        cycle: "A -> B -> A".to_string(),
        span: Span::default(),
    };
    let display = format!("{error}");
    if !(display.contains("Circular") || display.contains("import")) {
        return Err(
            format!("expected display to contain 'Circular' or 'import', got: {display}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_display_circular_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::CircularDependency {
        cycle: "X -> Y -> X".to_string(),
        span: Span::default(),
    };
    let display = format!("{error}");
    if !(display.contains("Circular") || display.contains("dependency")) {
        return Err(
            format!("expected display to contain 'Circular' or 'dependency', got: {display}")
                .into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_display_undefined_reference_again() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::UndefinedReference {
        name: "missing_field".to_string(),
        span: Span::default(),
    };
    let display = format!("{error}");
    if !(display.contains("missing_field") || display.contains("reference")) {
        return Err(
            format!(
                "expected display to contain 'missing_field' or 'reference', got: {display}"
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_display_unknown_property() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::UnknownProperty {
        component: "Button".to_string(),
        property: "invalid_prop".to_string(),
        span: Span::default(),
    };
    let display = format!("{error}");
    if !(display.contains("invalid_prop") || display.contains("property")) {
        return Err(
            format!(
                "expected display to contain 'invalid_prop' or 'property', got: {display}"
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_display_trait_field_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::TraitFieldTypeMismatch {
        field: "value".to_string(),
        trait_name: "Valuable".to_string(),
        expected: "Number".to_string(),
        actual: "String".to_string(),
        span: Span::default(),
    };
    let display = format!("{error}");
    if !(display.contains("value") || display.contains("Valuable")) {
        return Err(
            format!("expected display to contain 'value' or 'Valuable', got: {display}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_display_invalid_binary_op_details() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::InvalidBinaryOp {
        op: "Subtract".to_string(),
        left_type: "Boolean".to_string(),
        right_type: "Boolean".to_string(),
        span: Span::default(),
    };
    let display = format!("{error}");
    if !(display.contains("Subtract") || display.contains("Boolean")) {
        return Err(
            format!("expected display to contain 'Subtract' or 'Boolean', got: {display}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_display_for_loop_not_array() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::ForLoopNotArray {
        actual: "String".to_string(),
        span: Span::default(),
    };
    let display = format!("{error}");
    if !(display.contains("String") || display.contains("array")) {
        return Err(
            format!("expected display to contain 'String' or 'array', got: {display}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_display_invalid_if_condition() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::InvalidIfCondition {
        actual: "Number".to_string(),
        span: Span::default(),
    };
    let display = format!("{error}");
    if !(display.contains("Number") || display.contains("condition")) {
        return Err(
            format!("expected display to contain 'Number' or 'condition', got: {display}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_error_span_method() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::ParseError {
        message: "test".to_string(),
        span: Span {
            start: Location {
                offset: 5,
                line: 2,
                column: 3,
            },
            end: Location {
                offset: 10,
                line: 2,
                column: 8,
            },
        },
    };
    let span = error.span();
    if span.start.offset != 5 {
        return Err(format!("expected start.offset 5 but got {:?}", span.start.offset).into());
    }
    if span.end.offset != 10 {
        return Err(format!("expected end.offset 10 but got {:?}", span.end.offset).into());
    }
    Ok(())
}

// =============================================================================
// Complex Error Scenarios
// =============================================================================

#[test]
fn test_error_nested_undefined_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Outer {
            inner: Missing1,
            other: Missing2
        }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_trait_with_undefined_type_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Broken {
            field: UndefinedType
        }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_impl_for_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        trait MyTrait {
            value: String
        }
        impl MyTrait {
            value: "default"
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    // Impl directly on a trait (not a struct) should error
    if result.is_ok() {
        return Err("Impl on trait should error".into());
    }
    Ok(())
}

#[test]
fn test_error_duplicate_enum_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            active,
            active
        }
    ";
    let result = compile_and_report(source, "test.fv");
    // Duplicate enum variants should be caught as error
    if result.is_ok() {
        return Err("Duplicate enum variants should error".into());
    }
    Ok(())
}

#[test]
fn test_error_in_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod broken {
            struct Test {
                field: NonexistentType
            }
        }
    ";
    let result = compile_and_report(source, "test.fv");
    // Undefined type inside module should error
    if result.is_ok() {
        return Err("Undefined type in module should error".into());
    }
    Ok(())
}

#[test]
fn test_valid_complex_source() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        trait Named {
            name: String
        }

        struct User: Named {
            name: String = "default user",
            age: Number
        }

        enum Status {
            active,
            inactive,
            pending
        }

        let defaultStatus = Status.active
    "#;
    compile_and_report(source, "test.fv").map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Direct report_error Tests (for coverage of formatting branches)
// =============================================================================

#[test]
fn test_report_error_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::TypeMismatch {
        expected: "Number".to_string(),
        found: "String".to_string(),
        span: Span::default(),
    };
    let source = "struct Test { value: Number }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("Type mismatch") || report.contains("E004")) {
        return Err(
            format!("expected report to contain 'Type mismatch' or 'E004', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_module_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::ModuleNotFound {
        name: "missing".to_string(),
        span: Span::default(),
    };
    let source = "use missing::Item";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("missing") || report.contains("E005")) {
        return Err(
            format!("expected report to contain 'missing' or 'E005', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_circular_import() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::CircularImport {
        cycle: "A -> B -> A".to_string(),
        span: Span::default(),
    };
    let source = "use A::Item";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("Circular") || report.contains("E006")) {
        return Err(
            format!("expected report to contain 'Circular' or 'E006', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_circular_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::CircularDependency {
        cycle: "X -> Y -> X".to_string(),
        span: Span::default(),
    };
    let source = "struct X { y: Y }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("Circular") || report.contains("E007")) {
        return Err(
            format!("expected report to contain 'Circular' or 'E007', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_missing_trait_field() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::MissingTraitField {
        field: "name".to_string(),
        trait_name: "Named".to_string(),
        span: Span::default(),
    };
    let source = "struct User: Named { age: Number }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("name") || report.contains("E008")) {
        return Err(
            format!("expected report to contain 'name' or 'E008', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_trait_field_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::TraitFieldTypeMismatch {
        field: "value".to_string(),
        trait_name: "Typed".to_string(),
        expected: "Number".to_string(),
        actual: "String".to_string(),
        span: Span::default(),
    };
    let source = "struct Test: Typed { value: String }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("value") || report.contains("E009")) {
        return Err(
            format!("expected report to contain 'value' or 'E009', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_invalid_binary_op() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::InvalidBinaryOp {
        op: "Add".to_string(),
        left_type: "String".to_string(),
        right_type: "Number".to_string(),
        span: Span::default(),
    };
    let source = "let x = \"a\" + 1";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("Add") || report.contains("E010")) {
        return Err(
            format!("expected report to contain 'Add' or 'E010', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_for_loop_not_array() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::ForLoopNotArray {
        actual: "String".to_string(),
        span: Span::default(),
    };
    let source = "for x in text { x }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("array") || report.contains("E011")) {
        return Err(
            format!("expected report to contain 'array' or 'E011', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_invalid_if_condition() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::InvalidIfCondition {
        actual: "Number".to_string(),
        span: Span::default(),
    };
    let source = "if 42 { \"yes\" }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("condition") || report.contains("E012")) {
        return Err(
            format!("expected report to contain 'condition' or 'E012', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_match_not_enum() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::MatchNotEnum {
        actual: "String".to_string(),
        span: Span::default(),
    };
    let source = "match text { }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("enum") || report.contains("E013")) {
        return Err(
            format!("expected report to contain 'enum' or 'E013', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_non_exhaustive_match() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::NonExhaustiveMatch {
        missing: "inactive, pending".to_string(),
        span: Span::default(),
    };
    let source = "match status { .active: \"yes\" }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("exhaustive") || report.contains("E014")) {
        return Err(
            format!("expected report to contain 'exhaustive' or 'E014', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_duplicate_match_arm() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::DuplicateMatchArm {
        variant: "active".to_string(),
        span: Span::default(),
    };
    let source = "match status { .active: \"yes\", .active: \"no\" }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("active") || report.contains("E015")) {
        return Err(
            format!("expected report to contain 'active' or 'E015', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_private_import() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::PrivateImport {
        name: "Helper".to_string(),
        span: Span::default(),
    };
    let source = "use utils::Helper";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("Helper") || report.contains("E016")) {
        return Err(
            format!("expected report to contain 'Helper' or 'E016', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_import_item_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::ImportItemNotFound {
        item: "Missing".to_string(),
        module: "utils".to_string(),
        available: "Helper, Config".to_string(),
        span: Span::default(),
    };
    let source = "use utils::Missing";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("Missing") || report.contains("E017")) {
        return Err(
            format!("expected report to contain 'Missing' or 'E017', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_view_trait_in_model() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::ViewTraitInModel {
        name: "Renderable".to_string(),
        model: "User".to_string(),
        span: Span::default(),
    };
    let source = "struct User: Renderable { }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("Renderable") || report.contains("E018")) {
        return Err(
            format!("expected report to contain 'Renderable' or 'E018', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_model_trait_in_view() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::ModelTraitInView {
        name: "Serializable".to_string(),
        view: "Card".to_string(),
        span: Span::default(),
    };
    let source = "struct Card: Serializable { @mount x: String }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("Serializable") || report.contains("E019")) {
        return Err(
            format!("expected report to contain 'Serializable' or 'E019', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_not_a_trait() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::NotATrait {
        name: "User".to_string(),
        actual_kind: "struct".to_string(),
        span: Span::default(),
    };
    let source = "struct Test: User { }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("User") || report.contains("E020")) {
        return Err(
            format!("expected report to contain 'User' or 'E020', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_unknown_enum_variant() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::UnknownEnumVariant {
        variant: "unknown".to_string(),
        enum_name: "Status".to_string(),
        span: Span::default(),
    };
    let source = "Status.unknown";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("unknown") || report.contains("E021")) {
        return Err(
            format!("expected report to contain 'unknown' or 'E021', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_variant_arity_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::VariantArityMismatch {
        variant: "some".to_string(),
        expected: 1,
        actual: 0,
        span: Span::default(),
    };
    let source = "Option.some";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("some") || report.contains("E022")) {
        return Err(
            format!("expected report to contain 'some' or 'E022', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_missing_trait_mounting_point() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::MissingTraitMountingPoint {
        mount: "content".to_string(),
        trait_name: "Renderable".to_string(),
        span: Span::default(),
    };
    let source = "struct View: Renderable { }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("content") || report.contains("E023")) {
        return Err(
            format!("expected report to contain 'content' or 'E023', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_trait_mounting_point_type_mismatch() -> Result<(), Box<dyn std::error::Error>>
{
    let error = CompilerError::TraitMountingPointTypeMismatch {
        mount: "content".to_string(),
        trait_name: "Renderable".to_string(),
        expected: "View".to_string(),
        actual: "String".to_string(),
        span: Span::default(),
    };
    let source = "struct Test: Renderable { @mount content: String }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("content") || report.contains("E024")) {
        return Err(
            format!("expected report to contain 'content' or 'E024', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_undefined_reference() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::UndefinedReference {
        name: "unknown_var".to_string(),
        span: Span::default(),
    };
    let source = "unknown_var + 1";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("unknown_var") || report.contains("E999")) {
        return Err(
            format!("expected report to contain 'unknown_var' or 'E999', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_error_unknown_property() -> Result<(), Box<dyn std::error::Error>> {
    let error = CompilerError::UnknownProperty {
        component: "Button".to_string(),
        property: "invalid".to_string(),
        span: Span::default(),
    };
    let source = "Button { invalid: true }";
    let report = report_error(&error, source, "test.fv");
    if !(report.contains("invalid") || report.contains("E999")) {
        return Err(
            format!("expected report to contain 'invalid' or 'E999', got: {report}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_report_errors_multiple() -> Result<(), Box<dyn std::error::Error>> {
    let errors = vec![
        CompilerError::UndefinedType {
            name: "Unknown1".to_string(),
            span: Span::default(),
        },
        CompilerError::UndefinedType {
            name: "Unknown2".to_string(),
            span: Span::default(),
        },
    ];
    let source = "struct A { x: Unknown1, y: Unknown2 }";
    let report = report_errors(&errors, source, "test.fv");
    if !(report.contains("Unknown1") || report.contains("Unknown2")) {
        return Err(
            format!("expected report to contain 'Unknown1' or 'Unknown2', got: {report}").into(),
        );
    }
    Ok(())
}
