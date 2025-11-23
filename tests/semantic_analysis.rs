//! Comprehensive semantic analysis tests
//!
//! Tests for exercising semantic analyzer validation paths

use formalang::compile;

// =============================================================================
// View vs Model Tests
// =============================================================================

#[test]
fn test_view_with_mount_field() {
    let source = r#"
        struct Card {
            @mount header: String,
            @mount content: String
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "View with mount should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_model_without_mount_field() {
    let source = r#"
        struct User {
            name: String,
            age: Number
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Model without mount should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_view_trait_with_mount() {
    let source = r#"
        trait Layout {
            @mount header: String,
            @mount footer: String
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "View trait with mount should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_model_trait_without_mount() {
    let source = r#"
        trait Identifiable {
            id: Number
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Model trait without mount should compile: {:?}",
        result.err()
    );
}

// =============================================================================
// Impl Block Expression Tests
// =============================================================================

#[test]
fn test_impl_with_string_literal() {
    let source = r#"
        struct Greeting {
            name: String
        }
        impl Greeting {
            name: "Hello!"
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Impl with string literal: {:?}",
        result.err()
    );
}

#[test]
fn test_impl_with_number_literal() {
    let source = r#"
        struct Counter {
            count: Number
        }
        impl Counter {
            count: 42
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Impl with number literal: {:?}",
        result.err()
    );
}

#[test]
fn test_impl_with_boolean_literal() {
    let source = r#"
        struct Flag {
            enabled: Boolean
        }
        impl Flag {
            enabled: true
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Impl with boolean literal: {:?}",
        result.err()
    );
}

#[test]
fn test_impl_with_array_literal() {
    let source = r#"
        struct List {
            items: [String]
        }
        impl List {
            items: ["a", "b", "c"]
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Impl with array literal: {:?}",
        result.err()
    );
}

#[test]
fn test_impl_with_struct_reference() {
    let source = r#"
        struct Person {
            name: String,
            age: Number
        }
        struct Container {
            person: Person
        }
        impl Container {
            person: Person(name: "John", age: 30)
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Impl with struct reference: {:?}",
        result.err()
    );
}

// =============================================================================
// Complex Expression Tests
// =============================================================================

#[test]
fn test_nested_if_expression() {
    let source = r#"
        struct Logic {
            a: Boolean
        }
        impl Logic {
            a: if true {
                if false { true } else { false }
            } else {
                true
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Nested if expression: {:?}", result.err());
}

#[test]
fn test_nested_for_expression() {
    let source = r#"
        struct Matrix {
            data: [[String]]
        }
        impl Matrix {
            data: for item in ["a", "b", "c"] {
                [item]
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Nested for expression: {:?}", result.err());
}

#[test]
fn test_let_with_if() {
    let source = r#"
        struct Conditional {
            flag: Boolean
        }
        impl Conditional {
            flag: (let x = if true { true } else { false }
            x)
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Let with if: {:?}", result.err());
}

#[test]
fn test_let_with_for() {
    let source = r#"
        struct Iterator {
            items: [String]
        }
        impl Iterator {
            items: (let result = for x in ["a", "b"] { x }
            result)
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Let with for: {:?}", result.err());
}

// =============================================================================
// Binary Operator Tests
// =============================================================================

#[test]
fn test_string_in_impl() {
    let source = r#"
        struct Text {
            value: String
        }
        impl Text {
            value: "hello world"
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "String in impl: {:?}", result.err());
}

#[test]
fn test_number_arithmetic() {
    let source = r#"
        struct Math {
            value: Number
        }
        impl Math {
            value: 1 + 2 * 3 - 4 / 2
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Number arithmetic: {:?}", result.err());
}

#[test]
fn test_boolean_logic() {
    let source = r#"
        struct Logic {
            value: Boolean
        }
        impl Logic {
            value: true && false || true
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Boolean logic: {:?}", result.err());
}

#[test]
fn test_comparison_operators() {
    let source = r#"
        struct Compare {
            result: Boolean
        }
        impl Compare {
            result: 1 < 2 && 3 > 2 && 4 >= 4 && 5 <= 5 && 1 == 1 && 1 != 2
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Comparison operators: {:?}", result.err());
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_generic_struct_single_param() {
    let source = r#"
        struct Box<T> {
            value: T
        }
        struct Usage {
            box: Box<String>
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Generic struct single param: {:?}",
        result.err()
    );
}

#[test]
fn test_generic_struct_multiple_params() {
    let source = r#"
        struct Pair<A, B> {
            first: A,
            second: B
        }
        struct Usage {
            pair: Pair<String, Number>
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Generic struct multiple params: {:?}",
        result.err()
    );
}

#[test]
fn test_generic_trait() {
    let source = r#"
        trait Container<T> {
            item: T
        }
        struct Box<T> {
            item: T
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Generic trait: {:?}", result.err());
}

#[test]
fn test_nested_generic_types() {
    let source = r#"
        struct Inner<T> {
            value: T
        }
        struct Outer<T> {
            inner: Inner<T>
        }
        struct Usage {
            data: Outer<String>
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Nested generic types: {:?}", result.err());
}

// =============================================================================
// Field Modifier Tests
// =============================================================================

#[test]
fn test_optional_field() {
    let source = r#"
        struct User {
            name: String,
            nickname: String?
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Optional field: {:?}", result.err());
}

#[test]
fn test_mutable_field() {
    let source = r#"
        struct Counter {
            mut count: Number
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Mutable field: {:?}", result.err());
}

#[test]
fn test_field_with_default() {
    let source = r#"
        struct Config {
            name: String = "default",
            count: Number = 0,
            enabled: Boolean = true
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Field with default: {:?}", result.err());
}

#[test]
fn test_array_field() {
    let source = r#"
        struct List {
            items: [String],
            numbers: [Number]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Array field: {:?}", result.err());
}

#[test]
fn test_dictionary_field() {
    let source = r#"
        struct Cache {
            data: [String: Number]
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Dictionary field: {:?}", result.err());
}

// =============================================================================
// Module Tests
// =============================================================================

#[test]
fn test_module_with_struct() {
    let source = r#"
        mod utils {
            struct Helper {
                value: String
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Module with struct: {:?}", result.err());
}

#[test]
fn test_module_with_trait() {
    let source = r#"
        mod traits {
            trait Named {
                name: String
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Module with trait: {:?}", result.err());
}

#[test]
fn test_module_with_enum() {
    let source = r#"
        mod enums {
            enum Status {
                active,
                inactive
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Module with enum: {:?}", result.err());
}

#[test]
fn test_nested_modules() {
    let source = r#"
        mod outer {
            mod inner {
                struct Deep {
                    value: String
                }
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Nested modules: {:?}", result.err());
}

#[test]
fn test_pub_struct_in_module() {
    let source = r#"
        mod api {
            pub struct Public {
                value: String
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Pub struct in module: {:?}", result.err());
}

// =============================================================================
// Enum Tests
// =============================================================================

#[test]
fn test_enum_with_variants() {
    let source = r#"
        enum Direction {
            north,
            south,
            east,
            west
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Enum with variants: {:?}", result.err());
}

#[test]
fn test_generic_enum() {
    let source = r#"
        enum Result<T> {
            success,
            failure
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Generic enum: {:?}", result.err());
}

#[test]
fn test_enum_usage_in_struct() {
    let source = r#"
        enum Status {
            active,
            inactive
        }
        struct User {
            status: Status
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Enum usage in struct: {:?}", result.err());
}

// =============================================================================
// Match Expression Tests
// =============================================================================

#[test]
fn test_match_basic() {
    let source = r#"
        enum Option {
            some,
            none
        }
        struct Handler {
            option: Option
        }
        impl Handler {
            option: match Option.some {
                .some: .some,
                .none: .none
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Match basic: {:?}", result.err());
}

// =============================================================================
// Error Detection Tests
// =============================================================================

#[test]
fn test_undefined_type_error() {
    let source = r#"
        struct User {
            data: NonexistentType
        }
    "#;
    let result = compile(source);
    assert!(result.is_err(), "Should detect undefined type");
}

#[test]
fn test_duplicate_definition_error() {
    let source = r#"
        struct Config {
            name: String
        }
        struct Config {
            value: Number
        }
    "#;
    let result = compile(source);
    assert!(result.is_err(), "Should detect duplicate definition");
}

#[test]
fn test_missing_trait_field_error() {
    let source = r#"
        trait Named {
            name: String
        }
        struct User: Named {
            age: Number
        }
    "#;
    let result = compile(source);
    assert!(result.is_err(), "Should detect missing trait field");
}

#[test]
fn test_invalid_type_in_binary_op() {
    let source = r#"
        struct Test {
            value: Boolean
        }
        impl Test {
            value: true + false
        }
    "#;
    let result = compile(source);
    assert!(result.is_err(), "Should detect invalid binary op");
}

// =============================================================================
// Primitive Type Tests
// =============================================================================

#[test]
fn test_all_primitive_types() {
    let source = r#"
        struct AllPrimitives {
            text: String,
            number: Number,
            flag: Boolean,
            file: Path,
            pattern: Regex
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "All primitive types: {:?}", result.err());
}

#[test]
fn test_never_type() {
    let source = r#"
        struct Result {
            error: Never
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Never type: {:?}", result.err());
}

// =============================================================================
// Closure Type Tests
// =============================================================================

#[test]
fn test_closure_type_single_param() {
    let source = r#"
        struct Handler {
            callback: String -> Number
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Closure type single param: {:?}",
        result.err()
    );
}

#[test]
fn test_closure_type_no_return() {
    let source = r#"
        struct Handler {
            action: String -> Never
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Closure type no return: {:?}", result.err());
}

// =============================================================================
// Let Binding Tests
// =============================================================================

#[test]
fn test_let_at_top_level() {
    let source = r#"
        let config = "default"
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Let at top level: {:?}", result.err());
}

#[test]
fn test_pub_let() {
    let source = r#"
        pub let VERSION = "1.0"
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Pub let: {:?}", result.err());
}

#[test]
fn test_let_simple() {
    let source = r#"
        let count = 42
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Let simple: {:?}", result.err());
}

// =============================================================================
// Trait Conformance Tests
// =============================================================================

#[test]
fn test_struct_conforming_to_trait() {
    let source = r#"
        trait Displayable {
            label: String
        }
        struct Item: Displayable {
            label: String
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Struct conforming to trait: {:?}",
        result.err()
    );
}

#[test]
fn test_trait_with_optional_field() {
    let source = r#"
        trait MaybeNamed {
            name: String?
        }
        struct Item: MaybeNamed {
            name: String?
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Trait with optional field: {:?}",
        result.err()
    );
}

#[test]
fn test_multiple_trait_conformance() {
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
    assert!(
        result.is_ok(),
        "Multiple trait conformance: {:?}",
        result.err()
    );
}

// =============================================================================
// Complex Integration Tests
// =============================================================================

#[test]
fn test_full_application_model() {
    let source = r#"
        trait Identifiable {
            id: Number
        }

        trait Named {
            name: String
        }

        enum UserRole {
            admin,
            user,
            guest
        }

        struct User: Identifiable + Named {
            id: Number,
            name: String,
            role: UserRole,
            email: String?
        }

        struct Team {
            name: String,
            members: [User]
        }

        mod utils {
            struct Config {
                apiUrl: String = "https://api.example.com",
                timeout: Number = 30
            }
        }

        impl User {
            name: "User Profile"
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Full application model: {:?}", result.err());
}

#[test]
fn test_view_component_model() {
    let source = r#"
        struct Button {
            label: String,
            disabled: Boolean = false,
            @mount onClick: String
        }

        struct Card {
            title: String,
            @mount content: String,
            @mount footer: String?
        }

        impl Button {
            onClick: label
        }

        impl Card {
            content: title
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "View component model: {:?}", result.err());
}
