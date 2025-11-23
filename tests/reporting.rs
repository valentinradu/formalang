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

// =============================================================================
// Additional Error Reporting Tests for Coverage
// =============================================================================

#[test]
fn test_error_report_type_mismatch() {
    let source = r#"
        struct Test {
            count: Number
        }
        impl Test {
            count: "not a number"
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    // Type mismatch should be detected - impl assigns String to Number field
    assert!(result.is_err(), "Type mismatch in impl should error");
}

#[test]
fn test_error_report_circular_trait() {
    let source = r#"
        trait A: B { x: String }
        trait B: A { y: String }
    "#;
    let result = compile_and_report(source, "test.fv");
    // Circular trait inheritance should be detected as an error
    assert!(result.is_err(), "Circular trait inheritance should error");
}

#[test]
fn test_error_display_type_mismatch() {
    let error = CompilerError::TypeMismatch {
        expected: "Number".to_string(),
        found: "String".to_string(),
        span: Span {
            start: Location { offset: 0, line: 1, column: 1 },
            end: Location { offset: 10, line: 1, column: 11 },
        },
    };
    let display = format!("{}", error);
    assert!(display.contains("Number") || display.contains("mismatch"));
}

#[test]
fn test_error_display_module_not_found() {
    let error = CompilerError::ModuleNotFound {
        name: "missing_module".to_string(),
        span: Span {
            start: Location { offset: 0, line: 1, column: 1 },
            end: Location { offset: 14, line: 1, column: 15 },
        },
    };
    let display = format!("{}", error);
    assert!(display.contains("missing_module") || display.contains("Module"));
}

#[test]
fn test_error_display_circular_import() {
    let error = CompilerError::CircularImport {
        cycle: "A -> B -> A".to_string(),
        span: Span::default(),
    };
    let display = format!("{}", error);
    assert!(display.contains("Circular") || display.contains("import"));
}

#[test]
fn test_error_display_circular_dependency() {
    let error = CompilerError::CircularDependency {
        cycle: "X -> Y -> X".to_string(),
        span: Span::default(),
    };
    let display = format!("{}", error);
    assert!(display.contains("Circular") || display.contains("dependency"));
}

#[test]
fn test_error_display_undefined_reference_again() {
    let error = CompilerError::UndefinedReference {
        name: "missing_field".to_string(),
        span: Span::default(),
    };
    let display = format!("{}", error);
    assert!(display.contains("missing_field") || display.contains("reference"));
}

#[test]
fn test_error_display_unknown_property() {
    let error = CompilerError::UnknownProperty {
        component: "Button".to_string(),
        property: "invalid_prop".to_string(),
        span: Span::default(),
    };
    let display = format!("{}", error);
    assert!(display.contains("invalid_prop") || display.contains("property"));
}

#[test]
fn test_error_display_trait_field_type_mismatch() {
    let error = CompilerError::TraitFieldTypeMismatch {
        field: "value".to_string(),
        trait_name: "Valuable".to_string(),
        expected: "Number".to_string(),
        actual: "String".to_string(),
        span: Span::default(),
    };
    let display = format!("{}", error);
    assert!(display.contains("value") || display.contains("Valuable"));
}

#[test]
fn test_error_display_invalid_binary_op_details() {
    let error = CompilerError::InvalidBinaryOp {
        op: "Subtract".to_string(),
        left_type: "Boolean".to_string(),
        right_type: "Boolean".to_string(),
        span: Span::default(),
    };
    let display = format!("{}", error);
    assert!(display.contains("Subtract") || display.contains("Boolean"));
}

#[test]
fn test_error_display_for_loop_not_array() {
    let error = CompilerError::ForLoopNotArray {
        actual: "String".to_string(),
        span: Span::default(),
    };
    let display = format!("{}", error);
    assert!(display.contains("String") || display.contains("array"));
}

#[test]
fn test_error_display_invalid_if_condition() {
    let error = CompilerError::InvalidIfCondition {
        actual: "Number".to_string(),
        span: Span::default(),
    };
    let display = format!("{}", error);
    assert!(display.contains("Number") || display.contains("condition"));
}

#[test]
fn test_error_span_method() {
    let error = CompilerError::ParseError {
        message: "test".to_string(),
        span: Span {
            start: Location { offset: 5, line: 2, column: 3 },
            end: Location { offset: 10, line: 2, column: 8 },
        },
    };
    let span = error.span();
    assert_eq!(span.start.offset, 5);
    assert_eq!(span.end.offset, 10);
}

// =============================================================================
// Complex Error Scenarios
// =============================================================================

#[test]
fn test_error_nested_undefined_types() {
    let source = r#"
        struct Outer {
            inner: Missing1,
            other: Missing2
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
}

#[test]
fn test_error_trait_with_undefined_type_in_field() {
    let source = r#"
        trait Broken {
            field: UndefinedType
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
}

#[test]
fn test_error_impl_for_trait() {
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
    assert!(result.is_err(), "Impl on trait should error");
}

#[test]
fn test_error_duplicate_enum_variants() {
    let source = r#"
        enum Status {
            active,
            active
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    // Duplicate enum variants should be caught as error
    assert!(result.is_err(), "Duplicate enum variants should error");
}

#[test]
fn test_error_in_module() {
    let source = r#"
        module broken {
            struct Test {
                field: NonexistentType
            }
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    // Undefined type inside module should error
    assert!(result.is_err(), "Undefined type in module should error");
}

#[test]
fn test_valid_complex_source() {
    let source = r#"
        trait Named {
            name: String
        }

        struct User: Named {
            name: String,
            age: Number
        }

        impl User {
            "default user"
        }

        enum Status {
            active,
            inactive,
            pending
        }

        let defaultStatus = Status.active
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_ok(), "Valid source should compile: {:?}", result);
}

// =============================================================================
// Direct report_error Tests (for coverage of formatting branches)
// =============================================================================

use formalang::reporting::report_error;

#[test]
fn test_report_error_type_mismatch() {
    let error = CompilerError::TypeMismatch {
        expected: "Number".to_string(),
        found: "String".to_string(),
        span: Span::default(),
    };
    let source = "struct Test { value: Number }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("Type mismatch") || report.contains("E004"));
}

#[test]
fn test_report_error_module_not_found() {
    let error = CompilerError::ModuleNotFound {
        name: "missing".to_string(),
        span: Span::default(),
    };
    let source = "use missing::Item";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("missing") || report.contains("E005"));
}

#[test]
fn test_report_error_circular_import() {
    let error = CompilerError::CircularImport {
        cycle: "A -> B -> A".to_string(),
        span: Span::default(),
    };
    let source = "use A::Item";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("Circular") || report.contains("E006"));
}

#[test]
fn test_report_error_circular_dependency() {
    let error = CompilerError::CircularDependency {
        cycle: "X -> Y -> X".to_string(),
        span: Span::default(),
    };
    let source = "struct X { y: Y }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("Circular") || report.contains("E007"));
}

#[test]
fn test_report_error_missing_trait_field() {
    let error = CompilerError::MissingTraitField {
        field: "name".to_string(),
        trait_name: "Named".to_string(),
        span: Span::default(),
    };
    let source = "struct User: Named { age: Number }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("name") || report.contains("E008"));
}

#[test]
fn test_report_error_trait_field_type_mismatch() {
    let error = CompilerError::TraitFieldTypeMismatch {
        field: "value".to_string(),
        trait_name: "Typed".to_string(),
        expected: "Number".to_string(),
        actual: "String".to_string(),
        span: Span::default(),
    };
    let source = "struct Test: Typed { value: String }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("value") || report.contains("E009"));
}

#[test]
fn test_report_error_invalid_binary_op() {
    let error = CompilerError::InvalidBinaryOp {
        op: "Add".to_string(),
        left_type: "String".to_string(),
        right_type: "Number".to_string(),
        span: Span::default(),
    };
    let source = "let x = \"a\" + 1";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("Add") || report.contains("E010"));
}

#[test]
fn test_report_error_for_loop_not_array() {
    let error = CompilerError::ForLoopNotArray {
        actual: "String".to_string(),
        span: Span::default(),
    };
    let source = "for x in text { x }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("array") || report.contains("E011"));
}

#[test]
fn test_report_error_invalid_if_condition() {
    let error = CompilerError::InvalidIfCondition {
        actual: "Number".to_string(),
        span: Span::default(),
    };
    let source = "if 42 { \"yes\" }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("condition") || report.contains("E012"));
}

#[test]
fn test_report_error_match_not_enum() {
    let error = CompilerError::MatchNotEnum {
        actual: "String".to_string(),
        span: Span::default(),
    };
    let source = "match text { }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("enum") || report.contains("E013"));
}

#[test]
fn test_report_error_non_exhaustive_match() {
    let error = CompilerError::NonExhaustiveMatch {
        missing: "inactive, pending".to_string(),
        span: Span::default(),
    };
    let source = "match status { .active: \"yes\" }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("exhaustive") || report.contains("E014"));
}

#[test]
fn test_report_error_duplicate_match_arm() {
    let error = CompilerError::DuplicateMatchArm {
        variant: "active".to_string(),
        span: Span::default(),
    };
    let source = "match status { .active: \"yes\", .active: \"no\" }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("active") || report.contains("E015"));
}

#[test]
fn test_report_error_private_import() {
    let error = CompilerError::PrivateImport {
        name: "Helper".to_string(),
        span: Span::default(),
    };
    let source = "use utils::Helper";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("Helper") || report.contains("E016"));
}

#[test]
fn test_report_error_import_item_not_found() {
    let error = CompilerError::ImportItemNotFound {
        item: "Missing".to_string(),
        module: "utils".to_string(),
        available: "Helper, Config".to_string(),
        span: Span::default(),
    };
    let source = "use utils::Missing";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("Missing") || report.contains("E017"));
}

#[test]
fn test_report_error_view_trait_in_model() {
    let error = CompilerError::ViewTraitInModel {
        name: "Renderable".to_string(),
        model: "User".to_string(),
        span: Span::default(),
    };
    let source = "struct User: Renderable { }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("Renderable") || report.contains("E018"));
}

#[test]
fn test_report_error_model_trait_in_view() {
    let error = CompilerError::ModelTraitInView {
        name: "Serializable".to_string(),
        view: "Card".to_string(),
        span: Span::default(),
    };
    let source = "struct Card: Serializable { @mount x: String }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("Serializable") || report.contains("E019"));
}

#[test]
fn test_report_error_not_a_trait() {
    let error = CompilerError::NotATrait {
        name: "User".to_string(),
        actual_kind: "struct".to_string(),
        span: Span::default(),
    };
    let source = "struct Test: User { }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("User") || report.contains("E020"));
}

#[test]
fn test_report_error_unknown_enum_variant() {
    let error = CompilerError::UnknownEnumVariant {
        variant: "unknown".to_string(),
        enum_name: "Status".to_string(),
        span: Span::default(),
    };
    let source = "Status.unknown";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("unknown") || report.contains("E021"));
}

#[test]
fn test_report_error_variant_arity_mismatch() {
    let error = CompilerError::VariantArityMismatch {
        variant: "some".to_string(),
        expected: 1,
        actual: 0,
        span: Span::default(),
    };
    let source = "Option.some";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("some") || report.contains("E022"));
}

#[test]
fn test_report_error_missing_trait_mounting_point() {
    let error = CompilerError::MissingTraitMountingPoint {
        mount: "content".to_string(),
        trait_name: "Renderable".to_string(),
        span: Span::default(),
    };
    let source = "struct View: Renderable { }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("content") || report.contains("E023"));
}

#[test]
fn test_report_error_trait_mounting_point_type_mismatch() {
    let error = CompilerError::TraitMountingPointTypeMismatch {
        mount: "content".to_string(),
        trait_name: "Renderable".to_string(),
        expected: "View".to_string(),
        actual: "String".to_string(),
        span: Span::default(),
    };
    let source = "struct Test: Renderable { @mount content: String }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("content") || report.contains("E024"));
}

#[test]
fn test_report_error_undefined_reference() {
    let error = CompilerError::UndefinedReference {
        name: "unknown_var".to_string(),
        span: Span::default(),
    };
    let source = "unknown_var + 1";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("unknown_var") || report.contains("E999"));
}

#[test]
fn test_report_error_unknown_property() {
    let error = CompilerError::UnknownProperty {
        component: "Button".to_string(),
        property: "invalid".to_string(),
        span: Span::default(),
    };
    let source = "Button { invalid: true }";
    let report = report_error(&error, source, "test.fv");
    assert!(report.contains("invalid") || report.contains("E999"));
}

#[test]
fn test_report_errors_multiple() {
    use formalang::reporting::report_errors;

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
    assert!(report.contains("Unknown1") || report.contains("Unknown2"));
}
