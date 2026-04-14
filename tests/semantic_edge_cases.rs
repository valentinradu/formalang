//! Semantic analyzer edge case tests

use formalang::compile;

// =============================================================================
// Visibility Tests
// =============================================================================

#[test]
fn test_pub_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub trait Visible {
            value: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_pub_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub enum PublicStatus {
            active,
            inactive
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_pub_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod api {
            pub struct Endpoint {
                path: String
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
// Field Type Tests
// =============================================================================

#[test]
fn test_optional_field_with_default() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            name: String? = nil
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_mutable_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct State {
            mut current: String?
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_array_of_optional() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Items {
            values: [String?]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_optional_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Container {
            items: [String]?
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Complex Type Tests
// =============================================================================

#[test]
fn test_nested_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
        struct Container {
            nested: Box<Box<String>>
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_array_of_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Wrapper<T> {
            item: T
        }
        struct List {
            items: [Wrapper<String>]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_tuple_with_optionals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Data {
            point: (x: Number, y: Number?)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Expression Edge Cases
// =============================================================================

#[test]
fn test_nested_arrays() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let matrix = [[1, 2], [3, 4]]
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_empty_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let empty = []
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_grouped_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let grouped = (1 + 2) * 3
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_complex_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let result = 1 + 2 * 3 - 4 / 2
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_logical_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let result = true || false && true
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_comparison_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a = 1 < 2
        let b = 2 > 1
        let c = a && b
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Struct Instantiation Tests
// =============================================================================

#[test]
fn test_struct_instantiation_with_type_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Box<T> {
            value: T
        }
        struct Container {
            box: Box<String> = Box<String>(value: "hello")
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_struct_instantiation_shorthand() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point {
            x: Number,
            y: Number
        }
        struct Line {
            start: Point = Point(x: 0, y: 0),
            end: Point
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Impl Block Tests
// =============================================================================

#[test]
fn test_impl_with_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Item {
            name: String
        }
        struct List {
            items: [Item] = [Item(name: "a"), Item(name: "b")]
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_impl_with_if() -> Result<(), Box<dyn std::error::Error>> {
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
fn test_impl_with_for() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Item {
            id: Number
        }
        struct Collection {
            items: [Item] = for i in [1, 2, 3] { Item(id: i) }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Mount Field Tests
// =============================================================================

#[test]
fn test_mount_with_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Item {
            value: String
        }
        struct Container {
            mount items: [Item]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_mount_with_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Renderable {
            content: String
        }
        struct View {
            mount body: Renderable
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Dictionary Tests
// =============================================================================

#[test]
fn test_dictionary_field_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Cache {
            data: [String: Number] = ["a": 1, "b": 2]
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_nested_dictionary() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let nested = ["outer": ["inner": "value"]]
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Closure Type Tests
// =============================================================================

#[test]
fn test_closure_field_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            onEvent: String -> Boolean
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_array_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct EventBus {
            handlers: [() -> String]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Error Recovery Tests
// =============================================================================

#[test]
fn test_multiple_errors() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Unknown1 }
        struct B { y: Unknown2 }
        struct C { z: Unknown3 }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if errors.len() < 3 {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_partial_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Valid {
            name: String
        }
        struct Invalid {
            data: MissingType
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}
