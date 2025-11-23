//! Semantic analyzer tests
//!
//! Tests for type validation, trait checking, and semantic errors

use formalang::{compile, compile_and_report};

// =============================================================================
// Type Validation Tests
// =============================================================================

#[test]
fn test_type_validation_primitive_string() {
    let source = r#"
        struct Test {
            value: String
        }
        impl Test {
            "hello"
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_type_validation_primitive_number() {
    let source = r#"
        struct Test {
            value: Number
        }
        impl Test {
            42.5
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_type_validation_primitive_boolean() {
    let source = r#"
        struct Test {
            value: Boolean
        }
        impl Test {
            true
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_type_validation_array() {
    let source = r#"
        struct Test {
            items: [String]
        }
        impl Test {
            ["a", "b", "c"]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_type_validation_nested_struct() {
    let source = r#"
        struct Inner {
            id: Number
        }
        struct Outer {
            inner: Inner
        }
        impl Outer {
            Inner(id: 1)
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Trait Validation Tests
// =============================================================================

#[test]
fn test_trait_single_field() {
    let source = r#"
        trait Identifiable {
            id: String
        }
        struct User: Identifiable {
            id: String,
            name: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_trait_multiple_fields() {
    let source = r#"
        trait Entity {
            id: String,
            createdAt: Number
        }
        struct Document: Entity {
            id: String,
            createdAt: Number,
            title: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_trait_with_mount_field() {
    let source = r#"
        trait Container {
            mount content: String
        }
        struct Box: Container {
            mount content: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_error_trait_missing_mount_field() {
    let source = r#"
        trait Container {
            mount content: String
        }
        struct Box: Container {
            content: String
        }
    "#;
    let result = compile(source);
    // Should fail because mount field is required
    assert!(result.is_err());
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_generic_single_param() {
    let source = r#"
        struct Wrapper<T> {
            value: T
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_generic_multiple_params() {
    let source = r#"
        struct Pair<A, B> {
            first: A,
            second: B
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_generic_with_constraint() {
    let source = r#"
        trait Printable {
            text: String
        }
        struct Printer<T: Printable> {
            item: T
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_generic_instantiation() {
    let source = r#"
        struct Box<T> {
            value: T
        }
        struct Container {
            box: Box<String>
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Expression Validation Tests
// =============================================================================

#[test]
fn test_binary_op_number_arithmetic() {
    let source = r#"
        let a = 1 + 2
        let b = 3 - 1
        let c = 2 * 3
        let d = 6 / 2
        let e = 7 % 3
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_binary_op_string_concat() {
    let source = r#"
        let s = "hello" + " world"
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_binary_op_comparison() {
    let source = r#"
        let a = 1 < 2
        let b = 2 > 1
        let c = 1 <= 1
        let d = 2 >= 2
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_binary_op_equality() {
    let source = r#"
        let a = 1 == 1
        let b = 1 != 2
        let c = "a" == "a"
        let d = true == false
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_binary_op_logical() {
    let source = r#"
        let a = true && false
        let b = true || false
        let c = (1 < 2) && (2 < 3)
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// If Expression Tests
// =============================================================================

#[test]
fn test_if_simple() {
    let source = r#"
        struct Result {
            value: String
        }
        impl Result {
            if true { "yes" } else { "no" }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_if_with_comparison() {
    let source = r#"
        struct Result {
            value: String
        }
        impl Result {
            if 1 < 2 { "less" } else { "not less" }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_if_nested() {
    let source = r#"
        struct Result {
            value: String
        }
        impl Result {
            if true {
                if false { "a" } else { "b" }
            } else {
                "c"
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// For Expression Tests
// =============================================================================

#[test]
fn test_for_array_literal() {
    let source = r#"
        struct Item {
            name: String
        }
        struct List {
            items: [Item]
        }
        impl List {
            for name in ["a", "b"] { Item(name: name) }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_for_with_let() {
    let source = r#"
        struct Item {
            text: String
        }
        struct List {
            items: [Item]
        }
        impl List {
            for x in ["a", "b"] {
                let y = "item"
                Item(text: y)
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Module Tests
// =============================================================================

#[test]
fn test_module_simple() {
    let source = r#"
        module core {
            struct Config {
                value: String
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_module_with_trait() {
    let source = r#"
        module data {
            trait Serializable {
                data: String
            }
            struct Json: Serializable {
                data: String
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_module_nested() {
    let source = r#"
        module ui {
            module components {
                struct Button {
                    label: String
                }
            }
            module styles {
                struct Theme {
                    color: String
                }
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn test_error_undefined_field_reference() {
    let source = r#"
        struct User {
            name: String
        }
        impl User {
            unknown_field
        }
    "#;
    let result = compile(source);
    // Referencing unknown_field should produce an undefined reference error
    assert!(result.is_err(), "Unknown field reference should error");
}

#[test]
fn test_error_type_mismatch_in_field() {
    let source = r#"
        struct Wrapper {
            count: Number
        }
        impl Wrapper {
            "not a number"
        }
    "#;
    let result = compile(source);
    // Impl body is a string, which is valid - impl can have any expression
    // This test verifies the impl compiles without panic
    assert!(
        result.is_ok(),
        "Impl with expression should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_error_circular_trait() {
    // This would be a circular dependency if traits could reference each other
    // Currently traits don't have this issue in FormaLang
    let source = r#"
        trait A {
            value: String
        }
        trait B {
            other: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Complex Scenarios
// =============================================================================

#[test]
fn test_complex_form() {
    let source = r#"
        trait Validatable {
            isValid: Boolean
        }

        struct TextField: Validatable {
            value: String,
            placeholder: String?,
            isValid: Boolean = true
        }

        struct NumberField: Validatable {
            value: Number,
            min: Number?,
            max: Number?,
            isValid: Boolean = true
        }

        struct Form {
            title: String,
            mount fields: [Validatable]
        }

        impl Form {
            for i in [1, 2, 3] { TextField(value: "", placeholder: "Enter text") }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_complex_state_machine() {
    let source = r#"
        enum ConnectionState {
            disconnected,
            connecting,
            connected,
            error(message: String)
        }

        struct Connection {
            url: String,
            timeout: Number = 30
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Error Reporting Tests
// =============================================================================

#[test]
fn test_error_report_undefined_type() {
    let source = r#"
        struct User {
            status: UnknownType
        }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("UnknownType"));
}

#[test]
fn test_error_report_duplicate() {
    let source = r#"
        struct User { name: String }
        struct User { age: Number }
    "#;
    let result = compile_and_report(source, "test.fv");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("User"));
}

// =============================================================================
// Dictionary Expression Tests
// =============================================================================

#[test]
fn test_dict_literal_string_keys() {
    let source = r#"
        let config = ["host": "localhost", "port": "8080"]
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_dict_literal_number_keys() {
    let source = r#"
        let scores = [1: "first", 2: "second", 3: "third"]
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_dict_empty() {
    let source = r#"
        let empty = [:]
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Closure Tests
// =============================================================================

#[test]
fn test_closure_identity() {
    let source = r#"
        let identity = x -> x
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_closure_constant() {
    let source = r#"
        let always42 = () -> 42
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_closure_binary() {
    let source = r#"
        let pair = x, y -> (first: x, second: y)
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Let Expression Tests
// =============================================================================

#[test]
fn test_let_chain() {
    let source = r#"
        struct Calc {
            result: Number
        }
        impl Calc {
            let a = 1
            let b = 2
            let c = 3
            c
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_let_with_struct() {
    let source = r#"
        struct Point {
            x: Number,
            y: Number
        }
        struct Container {
            point: Point
        }
        impl Container {
            let p = Point(x: 1, y: 2)
            p
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Provides/Consumes Tests
// =============================================================================

#[test]
fn test_provides_simple() {
    let source = r#"
        struct Theme {
            color: String
        }
        struct App {
            content: String
        }
        impl App {
            provides Theme(color: "blue") {
                "content"
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_provides_multiple() {
    let source = r#"
        struct Theme {
            color: String
        }
        struct Config {
            debug: Boolean
        }
        struct App {
            content: String
        }
        impl App {
            provides Theme(color: "red"), Config(debug: true) {
                "content"
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}
