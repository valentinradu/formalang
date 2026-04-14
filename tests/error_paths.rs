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
    compile(source).map_err(|e| format!("Empty struct: {e:?}"))?;
    Ok(())
}

#[test]
fn test_empty_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Empty { }";
    compile(source).map_err(|e| format!("Empty trait: {e:?}"))?;
    Ok(())
}

#[test]
fn test_empty_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = "mod empty { }";
    compile(source).map_err(|e| format!("Empty module: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_conforming_empty_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Empty { }
        struct A: Empty { }
    ";
    compile(source).map_err(|e| format!("Struct conforming empty trait: {e:?}"))?;
    Ok(())
}

#[test]
fn test_deeply_nested_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { items: [[[[String]]]] }
    ";
    compile(source).map_err(|e| format!("Deeply nested types: {e:?}"))?;
    Ok(())
}

#[test]
fn test_optional_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { items: [String]? }";
    compile(source).map_err(|e| format!("Optional array: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_of_optionals() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { items: [String?] }";
    compile(source).map_err(|e| format!("Array of optionals: {e:?}"))?;
    Ok(())
}

#[test]
fn test_dictionary_of_arrays() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { map: [String: [Number]] }";
    compile(source).map_err(|e| format!("Dictionary of arrays: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Many fields: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Multiple traits: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { callback: String -> Boolean }";
    compile(source).map_err(|e| format!("Closure single param: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_returning_closure() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { callback: String -> (Number -> Boolean) }";
    compile(source).map_err(|e| format!("Closure returning closure: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Generic with multiple params: {e:?}"))?;
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
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_multiple_structs_with_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String = "a value" }
        struct B { y: Number = 42 }
    "#;
    compile(source).map_err(|e| format!("Multiple structs with defaults: {e:?}"))?;
    Ok(())
}

#[test]
fn test_multiple_enums() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum One { a }
        enum Two { b, c }
        enum Three { d, e, f }
    ";
    compile(source).map_err(|e| format!("Multiple enums: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Module with all types: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Pub in module: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Realistic data model: {e:?}"))?;
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
    compile(source).map_err(|e| format!("UI component model: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Config model: {e:?}"))?;
    Ok(())
}
