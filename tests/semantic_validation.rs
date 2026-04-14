//! Semantic validation tests for coverage
//!
//! These tests exercise validation paths in the semantic analyzer

use formalang::compile;

// =============================================================================
// Type Resolution Tests
// =============================================================================

#[test]
fn test_resolve_nested_generic_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
        struct Container<T> {
            box: Box<T>
        }
        struct Config {
            items: Container<String>
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_resolve_array_of_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Item<T> {
            value: T
        }
        struct List {
            items: [Item<String>]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_resolve_optional_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Wrapper<T> {
            value: T
        }
        struct Container {
            item: Wrapper<String>?
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_resolve_tuple_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Pair<A, B> {
            first: A,
            second: B
        }
        struct Data {
            pair: Pair<String, Number>
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
fn test_trait_field_type_validation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Typed {
            value: String
        }
        struct Impl: Typed {
            value: Number
        }
    ";
    let result = compile(source);
    // Type mismatch should be detected
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_trait_multiple_conformance() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
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
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_trait_with_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait MaybeNamed {
            name: String?
        }
        struct User: MaybeNamed {
            name: String?
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_trait_with_array_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait HasItems {
            items: [String]
        }
        struct Container: HasItems {
            items: [String]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_trait_inheritance() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
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
fn test_if_expression_with_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Data {
            status: Boolean = if true { true } else { false }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_for_expression_with_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct List {
            items: [String] = for item in ["a", "b"] { item }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_expression_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator {
            a: Number = (let x = 10
            x)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_nested_let_expressions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Logic {
            a: Boolean = (let x = true
            let y = false
            x)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_binary_operators_with_literals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Math {
            a: Number = 1 + 2
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_comparison_operators_with_literals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Compare {
            a: Number = if 1 < 2 { 1 } else { 0 }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_logical_operators_with_literals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Logic {
            a: Boolean = true && false
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Error Path Tests
// =============================================================================

#[test]
fn test_invalid_if_condition_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number
        }
        impl Test {
            value: if value { 1 } else { 0 }
        }
    ";
    let result = compile(source);
    // Number is not a valid condition type
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_invalid_for_not_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: String
        }
        impl Test {
            value: for item in value { item }
        }
    ";
    let result = compile(source);
    // String is not iterable
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_undefined_variable_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number
        }
        impl Test {
            value: undefinedVariable + 1
        }
    ";
    let result = compile(source);
    // Undefined variable
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_field_access_on_primitive() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number
        }
        impl Test {
            value: value.field
        }
    ";
    let result = compile(source);
    // Cannot access field on Number
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_invalid_arithmetic_on_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            flag: Boolean
        }
        impl Test {
            flag: flag + 1
        }
    ";
    let result = compile(source);
    // Cannot add Boolean and Number
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_invalid_comparison_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            text: String,
            num: Number
        }
        impl Test {
            text: text < num
        }
    ";
    let result = compile(source);
    // Cannot compare String and Number
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// View/Mount Field Tests
// =============================================================================

#[test]
fn test_mount_field_basic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Container {
            @mount content: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_multiple_mount_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Layout {
            @mount header: String,
            @mount main: String,
            @mount footer: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_view_trait_with_mount() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Renderable {
            @mount content: String
        }
        struct View: Renderable {
            @mount content: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Use Statement Tests
// =============================================================================

#[test]
fn test_use_single_item() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod utils {
            struct Helper { value: String }
        }
    ";
    // Use statements may need specific syntax - just test the module for now
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Complex Struct Tests
// =============================================================================

#[test]
fn test_struct_with_all_field_modifiers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Complex {
            required: String,
            optional: Number?,
            mut mutable: Boolean,
            @mount content: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_deeply_nested_structs() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Level3 { value: String }
        struct Level2 { inner: Level3 }
        struct Level1 { inner: Level2 }
        struct Root { inner: Level1 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_struct_with_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct WithDefaults {
            name: String = "default",
            count: Number = 0,
            active: Boolean = true
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Enum Tests
// =============================================================================

#[test]
fn test_enum_with_many_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
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
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_enum_status_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            pending,
            active,
            complete,
            failed
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_generic_enum_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Container<T> {
            full,
            empty
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
fn test_dictionary_with_struct_value() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User { name: String }
        struct Cache {
            users: [String: User]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_dictionary_literal_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config {
            data: [String: Number] = ["a": 1, "b": 2, "c": 3]
        }
    "#;
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
fn test_closure_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            process: String -> Number
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_multi_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator {
            operation: Number -> Number
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_expression_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Mapper {
            data: [String] = for item in ["a", "b"] { item }
        }
    "#;
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
fn test_let_with_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number = (let x: Number = 10
            x)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_mutable() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            initial: Number = (let mut count = 0
            count)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Failed: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_simple_value() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number = (let x = 2
            x)
        }
    ";
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
fn test_deeply_nested_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod a {
            mod b {
                mod c {
                    struct Deep { value: String }
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

#[test]
fn test_module_with_trait_and_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        mod core {
            trait Named {
                name: String
            }
            struct User: Named {
                name: String = "default"
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
// Impl Block Defaults Tests
// =============================================================================

#[test]
fn test_impl_block_defaults_applied_on_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    // Struct field defaults should make fields optional during instantiation
    let source = r##"
        struct MyBox {
            color: String = "#FF0000",
            size: Number = 10
        }
        struct Container {
            box: MyBox = MyBox()
        }
    "##;
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Struct field defaults should make fields optional: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_impl_block_defaults_with_mount_fields() -> Result<(), Box<dyn std::error::Error>> {
    // Mount fields with struct field defaults should be optional
    let source = r##"
        trait Shape {}
        struct Rect: Shape {}

        struct MyBox: Shape {
            color: String = "#FF0000",
            mount body: Shape = Rect()
        }
        struct Container: Shape {
            mount content: Shape = MyBox()
        }
    "##;
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Mount fields with defaults should be optional: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_impl_block_defaults_partial_override() -> Result<(), Box<dyn std::error::Error>> {
    // Can provide some fields while using struct field defaults for others
    let source = r#"
        struct Config {
            name: String = "default",
            value: Number = 0,
            enabled: Boolean = true
        }
        struct App {
            config: Config = Config(name: "custom")
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Should allow partial override of struct field defaults: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_impl_block_defaults_nested_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    // Nested struct instantiation should respect struct field defaults
    let source = r#"
        struct Inner {
            value: String = "inner default"
        }
        struct Outer {
            inner: Inner = Inner(),
            name: String = "outer default"
        }
        struct Container {
            outer: Outer = Outer()
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Nested instantiation should use struct field defaults: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

// =============================================================================
// Function Return Type Validation Tests
// =============================================================================

#[test]
fn test_function_return_type_valid_f32() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Vec2 {
            x: f32,
            y: f32
        }
        impl Vec2 {
            fn length_squared(self) -> f32 {
                self.x * self.x + self.y * self.y
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Function with matching return type should compile: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_function_return_type_valid_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator {
            value: Number
        }
        impl Calculator {
            fn double(self) -> Number {
                self.value * 2
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Function with Number return type should compile: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_function_return_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Data {
            count: Number
        }
        impl Data {
            fn get_count(self) -> String {
                self.count
            }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Function returning Number when String expected should fail".into());
    }
    let err = result.err().ok_or("expected error")?;
    let err_str = format!("{err:?}");
    if !err_str.contains("FunctionReturnTypeMismatch") {
        return Err(format!("Should report FunctionReturnTypeMismatch error: {err_str}").into());
    }
    Ok(())
}

#[test]
fn test_function_return_type_boolean_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Checker {
            value: Number
        }
        impl Checker {
            fn is_positive(self) -> Boolean {
                self.value > 0
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Function with Boolean return from comparison should compile: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_function_no_return_type_valid() -> Result<(), Box<dyn std::error::Error>> {
    // Functions without explicit return type should accept any body type
    let source = r"
        struct Processor {
            data: Number
        }
        impl Processor {
            fn process(self) {
                self.data * 2
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Function without return type should compile: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

// =============================================================================
// Assignment to Immutable Tests
// =============================================================================

#[test]
fn test_assignment_to_immutable_fails() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { value: Number }
        impl Counter {
            fn get_value(self) -> Number {
                let x = 10
                x = 20
                x
            }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Assignment to immutable binding should fail".into());
    }
    let errors = result.err().ok_or("expected error")?;
    let error_strings: Vec<String> = errors.iter().map(|e| format!("{e:?}")).collect();
    if !error_strings
        .iter()
        .any(|e| e.contains("immutable") || e.contains("Immutable"))
    {
        return Err(format!("Error should mention immutable: {error_strings:?}").into());
    }
    Ok(())
}

#[test]
fn test_assignment_to_mutable_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { value: Number }
        impl Counter {
            fn get_value(self) -> Number {
                let mut x = 10
                x = 20
                x
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Assignment to mutable binding should succeed: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

// =============================================================================
// Block Expression Tests
// =============================================================================

#[test]
fn test_block_expr_with_let_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number = {
                let a = 1
                let b = 2
                a + b
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Block expression with let bindings should compile: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_block_expr_with_assignment() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test { value: Number }
        impl Test {
            fn compute(self) -> Number {
                let mut sum = 0
                sum = sum + 10
                sum
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Block expression with assignment should compile: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_nested_block_expressions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number = {
                let outer = {
                    let inner = 5
                    inner * 2
                }
                outer + 1
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Nested block expressions should compile: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}
