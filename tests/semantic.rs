//! Semantic analyzer tests
//!
//! Tests for type validation, trait checking, and semantic errors

use formalang::compile_and_report;

// =============================================================================
// Type Validation Tests
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_type_validation_primitive_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Test {
            value: String = "hello"
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_type_validation_primitive_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: F64 = 42.5
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_type_validation_primitive_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Boolean = true
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_type_validation_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Test {
            items: [String] = ["a", "b", "c"]
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_type_validation_nested_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner {
            id: I32
        }
        struct Outer {
            inner: Inner = Inner(id: 1)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
        struct User {
            id: String,
            name: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_multiple_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Entity {
            id: String,
            createdAt: I32
        }
        struct Document {
            id: String,
            createdAt: I32,
            title: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_with_mount_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container {
            content: String
        }
        struct Box {
            content: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_error_trait_missing_mount_field() -> Result<(), Box<dyn std::error::Error>> {
    // Verify that a trait referencing an undefined type in a field causes an error
    let source = r"
        trait Container {
            content: NonexistentType
        }
    ";
    let result = compile(source);
    // Should fail because NonexistentType is undefined
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_op_string_concat() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s = "hello" + " world"
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_op_logical() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a = true && false
        let b = true || false
        let c = (1 < 2) && (2 < 3)
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_if_with_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Result {
            value: String = if 1 < 2 { "less" } else { "not less" }
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_module_with_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod data {
            trait Serializable {
                data: String
            }
            struct Json {
                data: String
            }
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
fn test_field_default_type_must_match_annotation() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #52: the previous body matched its name's negative intent
    // ("test_error_type_mismatch_in_field") with a passing case that
    // asserted nothing. Split into two real assertions:
    //   1. matching default → compiles cleanly
    //   2. mismatched default (`Number = "hi"`) → TypeMismatch
    let happy = r"
        struct Wrapper {
            count: I32 = 42
        }
    ";
    compile(happy).map_err(|e| format!("matching default should compile: {e:?}"))?;

    let mismatched = r#"
        struct Wrapper {
            count: I32 = "hi"
        }
    "#;
    let errors = compile(mismatched)
        .err()
        .ok_or("expected mismatched default to error")?;
    let saw_mismatch = errors
        .iter()
        .any(|e| matches!(e, formalang::CompilerError::TypeMismatch { .. }));
    if !saw_mismatch {
        return Err(format!("expected TypeMismatch for `Number = \"hi\"`, got: {errors:?}").into());
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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

        struct TextField {
            value: String,
            placeholder: String?,
            isValid: Boolean = true
        }

        struct NumberField {
            value: I32,
            min: I32?,
            max: I32?,
            isValid: Boolean = true
        }

        struct Form {
            title: String,
            fields: [TextField] = for i in [1, 2, 3] { TextField(value: "", placeholder: "Enter text") }
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
            timeout: I32 = 30
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
        struct User { age: I32 }
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_dict_literal_number_keys() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let scores = [1: "first", 2: "second", 3: "third"]
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_dict_empty() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let empty = [:]
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_constant() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let always42 = () -> 42
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_binary() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let pair = x, y -> (first: x, second: y)
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Let Expression Tests
// =============================================================================

#[test]
fn test_let_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calc {
            result: I32 = (let a = 1
            in let b = 2
            in let c = 3
            in c)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_with_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point {
            x: I32,
            y: I32
        }
        struct Container {
            point: Point = (let p = Point(x: 1, y: 2)
            in p)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}
