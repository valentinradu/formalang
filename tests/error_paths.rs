//! Error path tests
//!
//! Tests that exercise error detection and validation paths

use formalang::compile;

// =============================================================================
// Type Error Tests
// =============================================================================

#[test]
fn test_undefined_type_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { x: Unknown }";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_undefined_type_in_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { items: [Unknown] }";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_undefined_type_in_optional() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { maybe: Unknown? }";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_undefined_type_in_dictionary_key() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { map: [Unknown: String] }";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_undefined_type_in_dictionary_value() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { map: [String: Unknown] }";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_undefined_type_in_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
        struct A { box: Box<Unknown> }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_undefined_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A: Unknown { }";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Duplicate Definition Tests
// =============================================================================

#[test]
fn test_duplicate_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: String }
        struct A { y: Number }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_duplicate_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait A { x: String }
        trait A { y: Number }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_duplicate_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum A { one }
        enum A { two }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_duplicate_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x = 1
        let x = 2
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Trait Conformance Error Tests
// =============================================================================

#[test]
fn test_missing_trait_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        struct User: Named { age: Number }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_wrong_trait_field_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        struct User: Named { name: Number }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_missing_multiple_trait_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Full { a: String, b: Number }
        struct Empty: Full { }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_not_a_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Helper { x: String }
        struct User: Helper { x: String }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Binary Operation Error Tests
// =============================================================================

#[test]
fn test_add_boolean_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean }
        impl A { true + false }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_subtract_string_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String }
        impl A { "a" - "b" }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_multiply_string_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String }
        impl A { "a" * 2 }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_divide_boolean_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean }
        impl A { true / 2 }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_and_number_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number }
        impl A { 1 && 2 }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_or_string_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String }
        impl A { "a" || "b" }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Control Flow Error Tests
// =============================================================================

#[test]
fn test_if_condition_not_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String }
        impl A { if "string" { "yes" } else { "no" } }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_if_condition_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: Number }
        impl A { if 42 { "yes" } else { "no" } }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_for_not_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String }
        impl A { for x in "not an array" { x } }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_for_on_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number }
        impl A { for x in 42 { x } }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Enum Error Tests
// =============================================================================

#[test]
fn test_unknown_enum_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, inactive }
        struct A { status: Status }
        impl A { Status.unknown }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Reference Error Tests
// =============================================================================

#[test]
fn test_undefined_variable_in_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: String }
        impl A { 1 + undefined_var }
    ";
    // Undefined variable should produce an error
    if compile(source).is_ok() {
        return Err("Undefined variable should error".into());
    }
    Ok(())
}

// =============================================================================
// Valid Edge Cases
// =============================================================================

#[test]
fn test_empty_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Empty { }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Empty struct: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_empty_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Empty { }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Empty trait: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_empty_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = "mod empty { }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Empty module: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_struct_conforming_empty_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Empty { }
        struct A: Empty { }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Struct conforming empty trait: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_deeply_nested_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { items: [[[[String]]]] }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Deeply nested types: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_optional_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { items: [String]? }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Optional array: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_array_of_optionals() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { items: [String?] }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Array of optionals: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_dictionary_of_arrays() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { map: [String: [Number]] }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Dictionary of arrays: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_many_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Big {
            a: String,
            b: Number,
            c: Boolean,
            d: String?,
            e: [String],
            f: [String: Number]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Many fields: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_multiple_traits() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait A { a: String }
        trait B { b: Number }
        trait C { c: Boolean }
        struct Full: A + B + C {
            a: String,
            b: Number,
            c: Boolean
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Multiple traits: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { callback: String -> Boolean }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Closure single param: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_returning_closure() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { callback: String -> (Number -> Boolean) }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Closure returning closure: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_generic_with_multiple_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Triple<A, B, C> {
            first: A,
            second: B,
            third: C
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Generic with multiple params: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_impl_complex_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A {
            x: Number = if true {
                for i in [1, 2, 3] {
                    i
                }
            } else {
                0
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!("Complex expression in struct field default: {:?}", result.err()).into(),
        );
    }
    Ok(())
}

#[test]
fn test_multiple_structs_with_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String = "a value" }
        struct B { y: Number = 42 }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Multiple structs with defaults: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_multiple_enums() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum One { a }
        enum Two { b, c }
        enum Three { d, e, f }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Multiple enums: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_module_with_all_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod full {
            trait T { x: String }
            struct S: T { x: String }
            enum E { a, b }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Module with all types: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_pub_in_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod api {
            pub trait PublicTrait { x: String }
            pub struct PublicStruct { y: Number }
            pub enum PublicEnum { a }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Pub in module: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Complex Valid Scenarios
// =============================================================================

#[test]
fn test_realistic_data_model() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Identifiable { id: Number }
        trait Timestamped {
            createdAt: Number,
            updatedAt: Number
        }

        enum OrderStatus {
            pending,
            processing,
            shipped,
            delivered,
            cancelled
        }

        struct Product: Identifiable {
            id: Number,
            name: String,
            price: Number,
            description: String?,
            tags: [String]
        }

        struct OrderItem {
            product: Product,
            quantity: Number
        }

        struct Order: Identifiable + Timestamped {
            id: Number,
            createdAt: Number,
            updatedAt: Number,
            items: [OrderItem],
            status: OrderStatus,
            total: Number
        }

        struct Customer: Identifiable {
            id: Number,
            name: String,
            email: String,
            orders: [Order]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Realistic data model: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_ui_component_model() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        trait Renderable {
            @mount content: String
        }

        struct Text {
            value: String,
            color: String = "black",
            size: Number = 14
        }

        struct Button {
            label: String,
            disabled: Boolean = false,
            @mount onClick: String
        }

        struct Card: Renderable {
            title: String,
            @mount content: String,
            @mount footer: String?
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("UI component model: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_config_model() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        mod config {
            pub struct DatabaseConfig {
                host: String = "localhost",
                port: Number = 5432,
                database: String,
                username: String,
                password: String?
            }

            pub struct ServerConfig {
                port: Number = 8080,
                host: String = "0.0.0.0",
                debug: Boolean = false
            }

            pub struct AppConfig {
                database: DatabaseConfig,
                server: ServerConfig,
                features: [String]
            }
        }

        let defaultPort = 3000
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Config model: {:?}", result.err()).into());
    }
    Ok(())
}
