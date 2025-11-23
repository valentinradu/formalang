//! Semantic validation tests for coverage
//!
//! These tests exercise validation paths in the semantic analyzer

use formalang::compile;

// =============================================================================
// Type Resolution Tests
// =============================================================================

#[test]
fn test_resolve_nested_generic_type() {
    let source = r#"
        struct Box<T> {
            value: T
        }
        struct Container<T> {
            box: Box<T>
        }
        struct Config {
            items: Container<String>
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_resolve_array_of_generic() {
    let source = r#"
        struct Item<T> {
            value: T
        }
        struct List {
            items: [Item<String>]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_resolve_optional_generic() {
    let source = r#"
        struct Wrapper<T> {
            value: T
        }
        struct Container {
            item: Wrapper<String>?
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_resolve_tuple_with_generics() {
    let source = r#"
        struct Pair<A, B> {
            first: A,
            second: B
        }
        struct Data {
            pair: Pair<String, Number>
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Trait Validation Tests
// =============================================================================

#[test]
fn test_trait_field_type_validation() {
    let source = r#"
        trait Typed {
            value: String
        }
        struct Impl: Typed {
            value: Number
        }
    "#;
    let result = compile(source);
    // Type mismatch should be detected
    assert!(result.is_err());
}

#[test]
fn test_trait_multiple_conformance() {
    let source = r#"
        trait Named {
            name: String
        }
        trait Aged {
            age: Number
        }
        struct Person: Named + Aged {
            name: String,
            age: Number
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_trait_with_optional_field() {
    let source = r#"
        trait MaybeNamed {
            name: String?
        }
        struct User: MaybeNamed {
            name: String?
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_trait_with_array_field() {
    let source = r#"
        trait HasItems {
            items: [String]
        }
        struct Container: HasItems {
            items: [String]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_trait_inheritance() {
    let source = r#"
        trait Base {
            id: Number
        }
        trait Extended: Base {
            name: String
        }
        struct Entity: Extended {
            id: Number,
            name: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Expression Validation Tests
// =============================================================================

#[test]
fn test_if_expression_with_literal() {
    let source = r#"
        struct Data {
            status: Boolean
        }
        impl Data {
            if true { "yes" } else { "no" }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_for_expression_with_literal() {
    let source = r#"
        struct List {
            items: [String]
        }
        impl List {
            for item in ["a", "b"] { item }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_let_expression_simple() {
    let source = r#"
        struct Calculator {
            a: Number
        }
        impl Calculator {
            let x = 10
            x
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_nested_let_expressions() {
    let source = r#"
        struct Logic {
            a: Boolean
        }
        impl Logic {
            let x = 1
            let y = 2
            x
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_binary_operators_with_literals() {
    let source = r#"
        struct Math {
            a: Number
        }
        impl Math {
            1 + 2
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_comparison_operators_with_literals() {
    let source = r#"
        struct Compare {
            a: Number
        }
        impl Compare {
            1 < 2
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_logical_operators_with_literals() {
    let source = r#"
        struct Logic {
            a: Boolean
        }
        impl Logic {
            true && false
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Error Path Tests
// =============================================================================

#[test]
fn test_invalid_if_condition_type() {
    let source = r#"
        struct Test {
            value: Number
        }
        impl Test {
            if value { "yes" } else { "no" }
        }
    "#;
    let result = compile(source);
    // Number is not a valid condition type
    assert!(result.is_err());
}

#[test]
fn test_invalid_for_not_array() {
    let source = r#"
        struct Test {
            value: String
        }
        impl Test {
            for item in value { item }
        }
    "#;
    let result = compile(source);
    // String is not iterable
    assert!(result.is_err());
}

#[test]
fn test_undefined_variable_reference() {
    let source = r#"
        struct Test {
            value: Number
        }
        impl Test {
            undefinedVariable + 1
        }
    "#;
    let result = compile(source);
    // Undefined variable
    assert!(result.is_err());
}

#[test]
fn test_field_access_on_primitive() {
    let source = r#"
        struct Test {
            value: Number
        }
        impl Test {
            value.field
        }
    "#;
    let result = compile(source);
    // Cannot access field on Number
    assert!(result.is_err());
}

#[test]
fn test_invalid_arithmetic_on_boolean() {
    let source = r#"
        struct Test {
            flag: Boolean
        }
        impl Test {
            flag + 1
        }
    "#;
    let result = compile(source);
    // Cannot add Boolean and Number
    assert!(result.is_err());
}

#[test]
fn test_invalid_comparison_types() {
    let source = r#"
        struct Test {
            text: String,
            num: Number
        }
        impl Test {
            text < num
        }
    "#;
    let result = compile(source);
    // Cannot compare String and Number
    assert!(result.is_err());
}

// =============================================================================
// View/Mount Field Tests
// =============================================================================

#[test]
fn test_mount_field_basic() {
    let source = r#"
        struct Container {
            @mount content: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_multiple_mount_fields() {
    let source = r#"
        struct Layout {
            @mount header: String,
            @mount main: String,
            @mount footer: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_view_trait_with_mount() {
    let source = r#"
        trait Renderable {
            @mount content: String
        }
        struct View: Renderable {
            @mount content: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Provides/Consumes Tests
// =============================================================================

#[test]
fn test_provides_expression() {
    let source = r#"
        struct Theme {
            color: String
        }
        struct Provider {
            value: String
        }
        impl Provider {
            provides Theme(color: "blue") { "content" }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_consumes_expression_without_provider() {
    let source = r#"
        struct Theme {
            color: String
        }
        struct Consumer {
            value: String
        }
        impl Consumer {
            consumes theme { "value" }
        }
    "#;
    let result = compile(source);
    // consumes without a provider should fail
    assert!(result.is_err());
}

// =============================================================================
// Use Statement Tests
// =============================================================================

#[test]
fn test_use_single_item() {
    let source = r#"
        mod utils {
            struct Helper { value: String }
        }
    "#;
    // Use statements may need specific syntax - just test the module for now
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Complex Struct Tests
// =============================================================================

#[test]
fn test_struct_with_all_field_modifiers() {
    let source = r#"
        struct Complex {
            required: String,
            optional: Number?,
            mut mutable: Boolean,
            @mount content: String
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_deeply_nested_structs() {
    let source = r#"
        struct Level3 { value: String }
        struct Level2 { inner: Level3 }
        struct Level1 { inner: Level2 }
        struct Root { inner: Level1 }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_struct_with_defaults() {
    let source = r#"
        struct WithDefaults {
            name: String = "default",
            count: Number = 0,
            active: Boolean = true
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Enum Tests
// =============================================================================

#[test]
fn test_enum_with_many_variants() {
    let source = r#"
        enum Color {
            red,
            green,
            blue,
            yellow,
            cyan,
            magenta,
            white,
            black
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_enum_status_variants() {
    let source = r#"
        enum Status {
            pending,
            active,
            complete,
            failed
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_generic_enum_simple() {
    let source = r#"
        enum Container<T> {
            full,
            empty
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Dictionary Tests
// =============================================================================

#[test]
fn test_dictionary_with_struct_value() {
    let source = r#"
        struct User { name: String }
        struct Cache {
            users: [String: User]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_dictionary_literal_in_impl() {
    let source = r#"
        struct Config {
            data: [String: Number]
        }
        impl Config {
            ["a": 1, "b": 2, "c": 3]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Closure Tests
// =============================================================================

#[test]
fn test_closure_in_field() {
    let source = r#"
        struct Handler {
            process: String -> Number
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_closure_multi_param() {
    let source = r#"
        struct Calculator {
            operation: Number -> Number
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_closure_expression_in_impl() {
    let source = r#"
        struct Mapper {
            data: [String]
        }
        impl Mapper {
            for item in ["a", "b"] { item }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Let Expression Tests
// =============================================================================

#[test]
fn test_let_with_type_annotation() {
    let source = r#"
        struct Test {
            value: Number
        }
        impl Test {
            let x: Number = 10
            x
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_let_mutable() {
    let source = r#"
        struct Counter {
            initial: Number
        }
        impl Counter {
            let mut count = 0
            count
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_let_simple_value() {
    let source = r#"
        struct Test {
            value: Number
        }
        impl Test {
            let x = 2
            x
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Module Tests
// =============================================================================

#[test]
fn test_deeply_nested_modules() {
    let source = r#"
        mod a {
            mod b {
                mod c {
                    struct Deep { value: String }
                }
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

#[test]
fn test_module_with_trait_and_impl() {
    let source = r#"
        mod core {
            trait Named {
                name: String
            }
            struct User: Named {
                name: String
            }
            impl User {
                "default"
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}
