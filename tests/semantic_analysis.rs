//! Comprehensive semantic analysis tests
//!
//! Tests for exercising semantic analyzer validation paths

use formalang::compile;

// =============================================================================
// View vs Model Tests
// =============================================================================

#[test]
fn test_view_with_mount_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Card {
            @mount header: String,
            @mount content: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("View with mount should compile: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_model_without_mount_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String,
            age: Number
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Model without mount should compile: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_view_trait_with_mount() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Layout {
            @mount header: String,
            @mount footer: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("View trait with mount should compile: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_model_trait_without_mount() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Identifiable {
            id: Number
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Model trait without mount should compile: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

// =============================================================================
// Impl Block Expression Tests
// =============================================================================

#[test]
fn test_impl_with_string_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Greeting {
            name: String = "Hello!"
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Impl with string literal: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_impl_with_number_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            count: Number = 42
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Impl with number literal: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_impl_with_boolean_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Flag {
            enabled: Boolean = true
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Impl with boolean literal: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_impl_with_array_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct List {
            items: [String] = ["a", "b", "c"]
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Impl with array literal: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_impl_with_struct_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Person {
            name: String,
            age: Number
        }
        struct Container {
            person: Person = Person(name: "John", age: 30)
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Impl with struct reference: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Complex Expression Tests
// =============================================================================

#[test]
fn test_nested_if_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Logic {
            a: Boolean = if true {
                if false { true } else { false }
            } else {
                true
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Nested if expression: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_nested_for_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Matrix {
            data: [[String]] = for item in ["a", "b", "c"] {
                [item]
            }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Nested for expression: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_with_if() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Conditional {
            flag: Boolean = (let x = if true { true } else { false }
            x)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let with if: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_with_for() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Iterator {
            items: [String] = (let result = for x in ["a", "b"] { x }
            result)
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let with for: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Binary Operator Tests
// =============================================================================

#[test]
fn test_string_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Text {
            value: String = "hello world"
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("String in impl: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_number_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Math {
            value: Number = 1 + 2 * 3 - 4 / 2
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Number arithmetic: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_boolean_logic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Logic {
            value: Boolean = true && false || true
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Boolean logic: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_comparison_operators() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Compare {
            result: Boolean = 1 < 2 && 3 > 2 && 4 >= 4 && 5 <= 5 && 1 == 1 && 1 != 2
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Comparison operators: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_generic_struct_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
        struct Usage {
            box: Box<String>
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Generic struct single param: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_generic_struct_multiple_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Pair<A, B> {
            first: A,
            second: B
        }
        struct Usage {
            pair: Pair<String, Number>
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Generic struct multiple params: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_generic_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container<T> {
            item: T
        }
        struct Box<T> {
            item: T
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Generic trait: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_nested_generic_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner<T> {
            value: T
        }
        struct Outer<T> {
            inner: Inner<T>
        }
        struct Usage {
            data: Outer<String>
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Nested generic types: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Field Modifier Tests
// =============================================================================

#[test]
fn test_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String,
            nickname: String?
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Optional field: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_mutable_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut count: Number
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Mutable field: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_field_with_default() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config {
            name: String = "default",
            count: Number = 0,
            enabled: Boolean = true
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Field with default: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_array_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct List {
            items: [String],
            numbers: [Number]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Array field: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_dictionary_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Cache {
            data: [String: Number]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Dictionary field: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Module Tests
// =============================================================================

#[test]
fn test_module_with_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod utils {
            struct Helper {
                value: String
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Module with struct: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_module_with_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod traits {
            trait Named {
                name: String
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Module with trait: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_module_with_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod enums {
            enum Status {
                active,
                inactive
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Module with enum: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_nested_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod outer {
            mod inner {
                struct Deep {
                    value: String
                }
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Nested modules: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_pub_struct_in_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod api {
            pub struct Public {
                value: String
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Pub struct in module: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Enum Tests
// =============================================================================

#[test]
fn test_enum_with_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Direction {
            north,
            south,
            east,
            west
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Enum with variants: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_generic_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Result<T> {
            success,
            failure
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Generic enum: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_enum_usage_in_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            active,
            inactive
        }
        struct User {
            status: Status
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Enum usage in struct: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Match Expression Tests
// =============================================================================

#[test]
fn test_match_basic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Option {
            some,
            none
        }
        struct Handler {
            option: Option = match Option.some {
                .some: .some,
                .none: .none
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Match basic: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Error Detection Tests
// =============================================================================

#[test]
fn test_undefined_type_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            data: NonexistentType
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Should detect undefined type".into());
    }
    Ok(())
}

#[test]
fn test_duplicate_definition_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            name: String
        }
        struct Config {
            value: Number
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Should detect duplicate definition".into());
    }
    Ok(())
}

#[test]
fn test_missing_trait_field_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }
        struct User: Named {
            age: Number
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Should detect missing trait field".into());
    }
    Ok(())
}

#[test]
fn test_invalid_type_in_binary_op() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Boolean = true + false
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Should detect invalid binary op".into());
    }
    Ok(())
}

// =============================================================================
// Primitive Type Tests
// =============================================================================

#[test]
fn test_all_primitive_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct AllPrimitives {
            text: String,
            number: Number,
            flag: Boolean,
            file: Path,
            pattern: Regex
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("All primitive types: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_never_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Result {
            error: Never
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Never type: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Closure Type Tests
// =============================================================================

#[test]
fn test_closure_type_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            callback: String -> Number
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Closure type single param: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_type_no_return() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            action: String -> Never
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Closure type no return: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Let Binding Tests
// =============================================================================

#[test]
fn test_let_at_top_level() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let config = "default"
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let at top level: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_pub_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        pub let VERSION = "1.0"
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Pub let: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let count = 42
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let simple: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Trait Conformance Tests
// =============================================================================

#[test]
fn test_struct_conforming_to_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Displayable {
            label: String
        }
        struct Item: Displayable {
            label: String
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Struct conforming to trait: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_trait_with_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait MaybeNamed {
            name: String?
        }
        struct Item: MaybeNamed {
            name: String?
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Trait with optional field: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_multiple_trait_conformance() -> Result<(), Box<dyn std::error::Error>> {
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
        return Err(format!("Multiple trait conformance: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Complex Integration Tests
// =============================================================================

#[test]
fn test_full_application_model() -> Result<(), Box<dyn std::error::Error>> {
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
            name: String = "User Profile",
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
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Full application model: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_view_component_model() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Button {
            label: String,
            disabled: Boolean = false,
            @mount onClick: String = "click"
        }

        struct Card {
            title: String,
            @mount content: String = "content",
            @mount footer: String?
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("View component model: {:?}", result.err()).into());
    }
    Ok(())
}
