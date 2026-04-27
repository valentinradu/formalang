//! Comprehensive semantic analysis tests
//!
//! Tests for exercising semantic analyzer validation paths

// =============================================================================
// Struct and Trait Tests
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_struct_with_header_content_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Card {
            header: String,
            content: String
        }
    ";
    compile(source).map_err(|e| format!("View with mount should compile: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_with_name_age_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String,
            age: I32
        }
    ";
    compile(source).map_err(|e| format!("Model without mount should compile: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_with_header_footer_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Layout {
            header: String,
            footer: String
        }
    ";
    compile(source).map_err(|e| format!("View trait with mount should compile: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_with_id_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Identifiable {
            id: I32
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
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
    compile(source).map_err(|e| format!("Impl with string literal: {e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_with_number_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            count: I32 = 42
        }
    ";
    compile(source).map_err(|e| format!("Impl with number literal: {e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_with_boolean_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Flag {
            enabled: Boolean = true
        }
    ";
    compile(source).map_err(|e| format!("Impl with boolean literal: {e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_with_array_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct List {
            items: [String] = ["a", "b", "c"]
        }
    "#;
    compile(source).map_err(|e| format!("Impl with array literal: {e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_with_struct_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Person {
            name: String,
            age: I32
        }
        struct Container {
            person: Person = Person(name: "John", age: 30)
        }
    "#;
    compile(source).map_err(|e| format!("Impl with struct reference: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Nested if expression: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Nested for expression: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_with_if() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Conditional {
            flag: Boolean = (let x = if true { true } else { false }
            in x)
        }
    ";
    compile(source).map_err(|e| format!("Let with if: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_with_for() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Iterator {
            items: [String] = (let result = for x in ["a", "b"] { x }
            in result)
        }
    "#;
    compile(source).map_err(|e| format!("Let with for: {e:?}"))?;
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
    compile(source).map_err(|e| format!("String in impl: {e:?}"))?;
    Ok(())
}

#[test]
fn test_number_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Math {
            value: I32 = 1 + 2 * 3 - 4 / 2
        }
    ";
    compile(source).map_err(|e| format!("Number arithmetic: {e:?}"))?;
    Ok(())
}

#[test]
fn test_boolean_logic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Logic {
            value: Boolean = true && false || true
        }
    ";
    compile(source).map_err(|e| format!("Boolean logic: {e:?}"))?;
    Ok(())
}

#[test]
fn test_comparison_operators() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Compare {
            result: Boolean = 1 < 2 && 3 > 2 && 4 >= 4 && 5 <= 5 && 1 == 1 && 1 != 2
        }
    ";
    compile(source).map_err(|e| format!("Comparison operators: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Generic struct single param: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Generic struct multiple params: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Generic trait: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Nested generic types: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Optional field: {e:?}"))?;
    Ok(())
}

#[test]
fn test_mutable_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut count: I32
        }
    ";
    compile(source).map_err(|e| format!("Mutable field: {e:?}"))?;
    Ok(())
}

#[test]
fn test_field_with_default() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config {
            name: String = "default",
            count: I32 = 0,
            enabled: Boolean = true
        }
    "#;
    compile(source).map_err(|e| format!("Field with default: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct List {
            items: [String],
            numbers: [I32]
        }
    ";
    compile(source).map_err(|e| format!("Array field: {e:?}"))?;
    Ok(())
}

#[test]
fn test_dictionary_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Cache {
            data: [String: I32]
        }
    ";
    compile(source).map_err(|e| format!("Dictionary field: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Module with struct: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Module with trait: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Module with enum: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Nested modules: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Pub struct in module: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Enum with variants: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Generic enum: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Enum usage in struct: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Match basic: {e:?}"))?;
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
            value: I32
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
            age: I32
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
            number: I32,
            flag: Boolean,
            file: Path,
            pattern: Regex
        }
    ";
    compile(source).map_err(|e| format!("All primitive types: {e:?}"))?;
    Ok(())
}

#[test]
fn test_never_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Result {
            error: Never
        }
    ";
    compile(source).map_err(|e| format!("Never type: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Closure Type Tests
// =============================================================================

#[test]
fn test_closure_type_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            callback: String -> I32
        }
    ";
    compile(source).map_err(|e| format!("Closure type single param: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_type_no_return() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            action: String -> Never
        }
    ";
    compile(source).map_err(|e| format!("Closure type no return: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Let at top level: {e:?}"))?;
    Ok(())
}

#[test]
fn test_pub_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        pub let VERSION = "1.0"
    "#;
    compile(source).map_err(|e| format!("Pub let: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let count = 42
    ";
    compile(source).map_err(|e| format!("Let simple: {e:?}"))?;
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
        struct Item {
            label: String
        }
    ";
    compile(source).map_err(|e| format!("Struct conforming to trait: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_with_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait MaybeNamed {
            name: String?
        }
        struct Item {
            name: String?
        }
    ";
    compile(source).map_err(|e| format!("Trait with optional field: {e:?}"))?;
    Ok(())
}

#[test]
fn test_multiple_trait_conformance() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }
        trait Aged {
            age: I32
        }
        struct Person {
            name: String,
            age: I32
        }
    ";
    compile(source).map_err(|e| format!("Multiple trait conformance: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Complex Integration Tests
// =============================================================================

#[test]
fn test_full_application_model() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        trait Identifiable {
            id: I32
        }

        trait Named {
            name: String
        }

        enum UserRole {
            admin,
            user,
            guest
        }

        struct User {
            id: I32,
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
                timeout: I32 = 30
            }
        }
    "#;
    compile(source).map_err(|e| format!("Full application model: {e:?}"))?;
    Ok(())
}

#[test]
fn test_view_component_model() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Button {
            label: String,
            disabled: Boolean = false,
            onClick: String = "click"
        }

        struct Card {
            title: String,
            content: String = "content",
            footer: String?
        }
    "#;
    compile(source).map_err(|e| format!("View component model: {e:?}"))?;
    Ok(())
}
