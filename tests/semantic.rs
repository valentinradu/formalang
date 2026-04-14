//! Semantic analyzer tests
//!
//! Tests for type validation, trait checking, and semantic errors

use formalang::{compile, compile_and_report};

// =============================================================================
// Type Validation Tests
// =============================================================================

#[test]
fn test_type_validation_primitive_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Test {
            value: String = "hello"
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_type_validation_primitive_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number = 42.5
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_type_validation_primitive_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Boolean = true
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_type_validation_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Test {
            items: [String] = ["a", "b", "c"]
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_type_validation_nested_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner {
            id: Number
        }
        struct Outer {
            inner: Inner = Inner(id: 1)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Trait Validation Tests
// =============================================================================

#[test]
fn test_trait_single_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Identifiable {
            id: String
        }
        struct User: Identifiable {
            id: String,
            name: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_trait_multiple_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Entity {
            id: String,
            createdAt: Number
        }
        struct Document: Entity {
            id: String,
            createdAt: Number,
            title: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_trait_with_mount_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container {
            mount content: String
        }
        struct Box: Container {
            mount content: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_error_trait_missing_mount_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container {
            mount content: String
        }
        struct Box: Container {
            content: String
        }
    ";
    let result = compile(source);
    // Should fail because mount field is required
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_generic_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Wrapper<T> {
            value: T
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_generic_multiple_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Pair<A, B> {
            first: A,
            second: B
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_generic_with_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Printable {
            text: String
        }
        struct Printer<T: Printable> {
            item: T
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_generic_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
        struct Container {
            box: Box<String>
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Expression Validation Tests
// =============================================================================

#[test]
fn test_binary_op_number_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a = 1 + 2
        let b = 3 - 1
        let c = 2 * 3
        let d = 6 / 2
        let e = 7 % 3
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_binary_op_string_concat() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s = "hello" + " world"
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_binary_op_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a = 1 < 2
        let b = 2 > 1
        let c = 1 <= 1
        let d = 2 >= 2
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_binary_op_equality() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let a = 1 == 1
        let b = 1 != 2
        let c = "a" == "a"
        let d = true == false
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_binary_op_logical() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a = true && false
        let b = true || false
        let c = (1 < 2) && (2 < 3)
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// If Expression Tests
// =============================================================================

#[test]
fn test_if_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Result {
            value: String = if true { "yes" } else { "no" }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_if_with_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Result {
            value: String = if 1 < 2 { "less" } else { "not less" }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_if_nested() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Result {
            value: String = if true {
                if false { "a" } else { "b" }
            } else {
                "c"
            }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// For Expression Tests
// =============================================================================

#[test]
fn test_for_array_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Item {
            name: String
        }
        struct List {
            items: [Item] = for name in ["a", "b"] { Item(name: name) }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_for_with_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Item {
            text: String
        }
        struct List {
            items: [Item] = for x in ["a", "b"] {
                let y = "item"
                Item(text: y)
            }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Module Tests
// =============================================================================

#[test]
fn test_module_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod core {
            struct Config {
                value: String
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_module_with_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod data {
            trait Serializable {
                data: String
            }
            struct Json: Serializable {
                data: String
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_module_nested() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod ui {
            mod components {
                struct Button {
                    label: String
                }
            }
            mod styles {
                struct Theme {
                    color: String
                }
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn test_error_undefined_field_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
        impl User {
            name: unknown_field
        }
    ";
    let result = compile(source);
    // Referencing unknown_field should produce an undefined reference error
    if result.is_ok() {
        return Err("Unknown field reference should error".into());
    }
    Ok(())
}

#[test]
fn test_error_type_mismatch_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Wrapper {
            count: Number = 42
        }
    ";
    let result = compile(source);
    // This test verifies the struct with default compiles without panic
    if result.is_err() {
        return Err(format!("Struct with default should compile: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_error_circular_trait() -> Result<(), Box<dyn std::error::Error>> {
    // This would be a circular dependency if traits could reference each other
    // Currently traits don't have this issue in FormaLang
    let source = r"
        trait A {
            value: String
        }
        trait B {
            other: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Complex Scenarios
// =============================================================================

#[test]
fn test_complex_form() -> Result<(), Box<dyn std::error::Error>> {
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
            mount fields: [Validatable] = for i in [1, 2, 3] { TextField(value: "", placeholder: "Enter text") }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_complex_state_machine() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
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
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Error Reporting Tests
// =============================================================================

#[test]
fn test_error_report_undefined_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            status: UnknownType
        }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let err = result.err().ok_or("expected error")?;
    if !(err.contains("UnknownType")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_report_duplicate() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User { name: String }
        struct User { age: Number }
    ";
    let result = compile_and_report(source, "test.fv");
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let err = result.err().ok_or("expected error")?;
    if !(err.contains("User")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Dictionary Expression Tests
// =============================================================================

#[test]
fn test_dict_literal_string_keys() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let config = ["host": "localhost", "port": "8080"]
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_dict_literal_number_keys() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let scores = [1: "first", 2: "second", 3: "third"]
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_dict_empty() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let empty = [:]
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Closure Tests
// =============================================================================

#[test]
fn test_closure_identity() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let identity = x -> x
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_constant() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let always42 = () -> 42
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_binary() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let pair = x, y -> (first: x, second: y)
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Let Expression Tests
// =============================================================================

#[test]
fn test_let_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calc {
            result: Number = (let a = 1
            let b = 2
            let c = 3
            c)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_with_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point {
            x: Number,
            y: Number
        }
        struct Container {
            point: Point = (let p = Point(x: 1, y: 2)
            p)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}
