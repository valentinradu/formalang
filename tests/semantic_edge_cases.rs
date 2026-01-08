//! Semantic analyzer edge case tests

use formalang::compile;

// =============================================================================
// Visibility Tests
// =============================================================================

#[test]
fn test_pub_trait() {
    let source = r#"
        pub trait Visible {
            value: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_pub_enum() {
    let source = r#"
        pub enum PublicStatus {
            active,
            inactive
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_pub_module() {
    let source = r#"
        pub mod api {
            pub struct Endpoint {
                path: String
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Field Type Tests
// =============================================================================

#[test]
fn test_optional_field_with_default() {
    let source = r#"
        struct Config {
            name: String? = nil
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_mutable_optional_field() {
    let source = r#"
        struct State {
            mut current: String?
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_array_of_optional() {
    let source = r#"
        struct Items {
            values: [String?]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_optional_array() {
    let source = r#"
        struct Container {
            items: [String]?
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Complex Type Tests
// =============================================================================

#[test]
fn test_nested_generic() {
    let source = r#"
        struct Box<T> {
            value: T
        }
        struct Container {
            nested: Box<Box<String>>
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_array_of_generic() {
    let source = r#"
        struct Wrapper<T> {
            item: T
        }
        struct List {
            items: [Wrapper<String>]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_tuple_with_optionals() {
    let source = r#"
        struct Data {
            point: (x: Number, y: Number?)
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Expression Edge Cases
// =============================================================================

#[test]
fn test_nested_arrays() {
    let source = r#"
        let matrix = [[1, 2], [3, 4]]
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_empty_array() {
    let source = r#"
        let empty = []
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_grouped_expression() {
    let source = r#"
        let grouped = (1 + 2) * 3
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_complex_precedence() {
    let source = r#"
        let result = 1 + 2 * 3 - 4 / 2
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_logical_precedence() {
    let source = r#"
        let result = true || false && true
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_comparison_chain() {
    let source = r#"
        let a = 1 < 2
        let b = 2 > 1
        let c = a && b
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Struct Instantiation Tests
// =============================================================================

#[test]
fn test_struct_instantiation_with_type_args() {
    let source = r#"
        struct Box<T> {
            value: T
        }
        struct Container {
            box: Box<String> = Box<String>(value: "hello")
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_struct_instantiation_shorthand() {
    let source = r#"
        struct Point {
            x: Number,
            y: Number
        }
        struct Line {
            start: Point = Point(x: 0, y: 0),
            end: Point
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Impl Block Tests
// =============================================================================

#[test]
fn test_impl_with_array() {
    let source = r#"
        struct Item {
            name: String
        }
        struct List {
            items: [Item] = [Item(name: "a"), Item(name: "b")]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_impl_with_if() {
    let source = r#"
        struct Result {
            value: String = if true { "yes" } else { "no" }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_impl_with_for() {
    let source = r#"
        struct Item {
            id: Number
        }
        struct Collection {
            items: [Item] = for i in [1, 2, 3] { Item(id: i) }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Mount Field Tests
// =============================================================================

#[test]
fn test_mount_with_array() {
    let source = r#"
        struct Item {
            value: String
        }
        struct Container {
            mount items: [Item]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_mount_with_trait() {
    let source = r#"
        trait Renderable {
            content: String
        }
        struct View {
            mount body: Renderable
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Dictionary Tests
// =============================================================================

#[test]
fn test_dictionary_field_type() {
    let source = r#"
        struct Cache {
            data: [String: Number] = ["a": 1, "b": 2]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_nested_dictionary() {
    let source = r#"
        let nested = ["outer": ["inner": "value"]]
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Closure Type Tests
// =============================================================================

#[test]
fn test_closure_field_type() {
    let source = r#"
        struct Handler {
            onEvent: String -> Boolean
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_closure_array_field() {
    let source = r#"
        struct EventBus {
            handlers: [() -> String]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Error Recovery Tests
// =============================================================================

#[test]
fn test_multiple_errors() {
    let source = r#"
        struct A { x: Unknown1 }
        struct B { y: Unknown2 }
        struct C { z: Unknown3 }
    "#;
    let result = compile(source);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.len() >= 3);
}

#[test]
fn test_partial_valid() {
    let source = r#"
        struct Valid {
            name: String
        }
        struct Invalid {
            data: MissingType
        }
    "#;
    let result = compile(source);
    assert!(result.is_err());
}
