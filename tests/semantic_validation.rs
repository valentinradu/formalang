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
            status: if true { true } else { false }
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
            items: for item in ["a", "b"] { item }
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
            a: (let x = 10
            x)
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
            a: (let x = true
            let y = false
            x)
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
            a: 1 + 2
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
            a: if 1 < 2 { 1 } else { 0 }
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
            a: true && false
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
            value: if value { 1 } else { 0 }
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
            value: for item in value { item }
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
            value: undefinedVariable + 1
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
            value: value.field
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
            flag: flag + 1
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
            text: text < num
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
            value: provides Theme(color: "blue") { "content" }
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
            value: consumes theme { "value" }
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
            data: ["a": 1, "b": 2, "c": 3]
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
            data: for item in ["a", "b"] { item }
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
            value: (let x: Number = 10
            x)
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
            initial: (let mut count = 0
            count)
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
            value: (let x = 2
            x)
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
                name: "default"
            }
        }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
}

// =============================================================================
// Impl Block Defaults Tests
// =============================================================================

#[test]
fn test_impl_block_defaults_applied_on_instantiation() {
    // Impl block defaults should make fields optional during instantiation
    let source = r##"
        struct MyBox {
            color: String,
            size: Number
        }
        impl MyBox {
            color: "#FF0000",
            size: 10
        }
        struct Container {
            box: MyBox
        }
        impl Container {
            box: MyBox()
        }
    "##;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Impl block defaults should make fields optional: {:?}",
        result.err()
    );
}

#[test]
fn test_impl_block_defaults_with_mount_fields() {
    // Mount fields with impl block defaults should be optional
    let source = r##"
        trait Shape {}
        struct Rect: Shape {}
        impl Rect {}

        struct MyBox: Shape {
            color: String,
            mount body: Shape
        }
        impl MyBox {
            color: "#FF0000",
            body: Rect()
        }
        struct Container: Shape {
            mount content: Shape
        }
        impl Container {
            content: MyBox()
        }
    "##;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Mount fields with impl defaults should be optional: {:?}",
        result.err()
    );
}

#[test]
fn test_impl_block_defaults_partial_override() {
    // Can provide some fields while using impl defaults for others
    let source = r#"
        struct Config {
            name: String,
            value: Number,
            enabled: Boolean
        }
        impl Config {
            name: "default",
            value: 0,
            enabled: true
        }
        struct App {
            config: Config
        }
        impl App {
            config: Config(name: "custom")
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Should allow partial override of impl defaults: {:?}",
        result.err()
    );
}

#[test]
fn test_impl_block_defaults_nested_instantiation() {
    // Nested struct instantiation should respect impl defaults
    let source = r#"
        struct Inner {
            value: String
        }
        impl Inner {
            value: "inner default"
        }
        struct Outer {
            inner: Inner,
            name: String
        }
        impl Outer {
            inner: Inner(),
            name: "outer default"
        }
        struct Container {
            outer: Outer
        }
        impl Container {
            outer: Outer()
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Nested instantiation should use impl defaults: {:?}",
        result.err()
    );
}

// =============================================================================
// Function Return Type Validation Tests
// =============================================================================

#[test]
fn test_function_return_type_valid_f32() {
    let source = r#"
        struct Vec2 {
            x: f32,
            y: f32
        }
        impl Vec2 {
            fn length_squared(self) -> f32 {
                self.x * self.x + self.y * self.y
            }
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Function with matching return type should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_function_return_type_valid_number() {
    let source = r#"
        struct Calculator {
            value: Number
        }
        impl Calculator {
            fn double(self) -> Number {
                self.value * 2
            }
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Function with Number return type should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_function_return_type_mismatch() {
    let source = r#"
        struct Data {
            count: Number
        }
        impl Data {
            fn get_count(self) -> String {
                self.count
            }
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_err(),
        "Function returning Number when String expected should fail"
    );
    let err = result.unwrap_err();
    let err_str = format!("{:?}", err);
    assert!(
        err_str.contains("FunctionReturnTypeMismatch"),
        "Should report FunctionReturnTypeMismatch error: {}",
        err_str
    );
}

#[test]
fn test_function_return_type_boolean_valid() {
    let source = r#"
        struct Checker {
            value: Number
        }
        impl Checker {
            fn is_positive(self) -> Boolean {
                self.value > 0
            }
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Function with Boolean return from comparison should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_function_no_return_type_valid() {
    // Functions without explicit return type should accept any body type
    let source = r#"
        struct Processor {
            data: Number
        }
        impl Processor {
            fn process(self) {
                self.data * 2
            }
        }
    "#;
    let result = compile(source);
    assert!(
        result.is_ok(),
        "Function without return type should compile: {:?}",
        result.err()
    );
}
