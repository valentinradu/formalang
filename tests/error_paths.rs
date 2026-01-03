//! Error path tests
//!
//! Tests that exercise error detection and validation paths

use formalang::compile;

// =============================================================================
// Type Error Tests
// =============================================================================

#[test]
fn test_undefined_type_in_field() {
    let source = "struct A { x: Unknown }";
    assert!(compile(source).is_err());
}

#[test]
fn test_undefined_type_in_array() {
    let source = "struct A { items: [Unknown] }";
    assert!(compile(source).is_err());
}

#[test]
fn test_undefined_type_in_optional() {
    let source = "struct A { maybe: Unknown? }";
    assert!(compile(source).is_err());
}

#[test]
fn test_undefined_type_in_dictionary_key() {
    let source = "struct A { map: [Unknown: String] }";
    assert!(compile(source).is_err());
}

#[test]
fn test_undefined_type_in_dictionary_value() {
    let source = "struct A { map: [String: Unknown] }";
    assert!(compile(source).is_err());
}

#[test]
fn test_undefined_type_in_generic() {
    let source = r#"
        struct Box<T> { value: T }
        struct A { box: Box<Unknown> }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_undefined_trait() {
    let source = "struct A: Unknown { }";
    assert!(compile(source).is_err());
}

// =============================================================================
// Duplicate Definition Tests
// =============================================================================

#[test]
fn test_duplicate_struct() {
    let source = r#"
        struct A { x: String }
        struct A { y: Number }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_duplicate_trait() {
    let source = r#"
        trait A { x: String }
        trait A { y: Number }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_duplicate_enum() {
    let source = r#"
        enum A { one }
        enum A { two }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_duplicate_let() {
    let source = r#"
        let x = 1
        let x = 2
    "#;
    assert!(compile(source).is_err());
}

// =============================================================================
// Trait Conformance Error Tests
// =============================================================================

#[test]
fn test_missing_trait_field() {
    let source = r#"
        trait Named { name: String }
        struct User: Named { age: Number }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_wrong_trait_field_type() {
    let source = r#"
        trait Named { name: String }
        struct User: Named { name: Number }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_missing_multiple_trait_fields() {
    let source = r#"
        trait Full { a: String, b: Number }
        struct Empty: Full { }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_not_a_trait() {
    let source = r#"
        struct Helper { x: String }
        struct User: Helper { x: String }
    "#;
    assert!(compile(source).is_err());
}

// =============================================================================
// Binary Operation Error Tests
// =============================================================================

#[test]
fn test_add_boolean_boolean() {
    let source = r#"
        struct A { x: Boolean }
        impl A { true + false }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_subtract_string_string() {
    let source = r#"
        struct A { x: String }
        impl A { "a" - "b" }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_multiply_string_number() {
    let source = r#"
        struct A { x: String }
        impl A { "a" * 2 }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_divide_boolean_number() {
    let source = r#"
        struct A { x: Boolean }
        impl A { true / 2 }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_and_number_number() {
    let source = r#"
        struct A { x: Number }
        impl A { 1 && 2 }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_or_string_string() {
    let source = r#"
        struct A { x: String }
        impl A { "a" || "b" }
    "#;
    assert!(compile(source).is_err());
}

// =============================================================================
// Control Flow Error Tests
// =============================================================================

#[test]
fn test_if_condition_not_boolean() {
    let source = r#"
        struct A { x: String }
        impl A { if "string" { "yes" } else { "no" } }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_if_condition_number() {
    let source = r#"
        struct A { x: Number }
        impl A { if 42 { "yes" } else { "no" } }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_for_not_array() {
    let source = r#"
        struct A { x: String }
        impl A { for x in "not an array" { x } }
    "#;
    assert!(compile(source).is_err());
}

#[test]
fn test_for_on_number() {
    let source = r#"
        struct A { x: Number }
        impl A { for x in 42 { x } }
    "#;
    assert!(compile(source).is_err());
}

// =============================================================================
// Enum Error Tests
// =============================================================================

#[test]
fn test_unknown_enum_variant() {
    let source = r#"
        enum Status { active, inactive }
        struct A { status: Status }
        impl A { Status.unknown }
    "#;
    assert!(compile(source).is_err());
}

// =============================================================================
// Reference Error Tests
// =============================================================================

#[test]
fn test_undefined_variable_in_expression() {
    let source = r#"
        struct A { x: String }
        impl A { 1 + undefined_var }
    "#;
    // Undefined variable should produce an error
    assert!(compile(source).is_err(), "Undefined variable should error");
}

// =============================================================================
// Valid Edge Cases
// =============================================================================

#[test]
fn test_empty_struct() {
    let source = "struct Empty { }";
    assert!(
        compile(source).is_ok(),
        "Empty struct: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_empty_trait() {
    let source = "trait Empty { }";
    assert!(
        compile(source).is_ok(),
        "Empty trait: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_empty_module() {
    let source = "mod empty { }";
    assert!(
        compile(source).is_ok(),
        "Empty module: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_struct_conforming_empty_trait() {
    let source = r#"
        trait Empty { }
        struct A: Empty { }
    "#;
    assert!(
        compile(source).is_ok(),
        "Struct conforming empty trait: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_deeply_nested_types() {
    let source = r#"
        struct A { items: [[[[String]]]] }
    "#;
    assert!(
        compile(source).is_ok(),
        "Deeply nested types: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_optional_array() {
    let source = "struct A { items: [String]? }";
    assert!(
        compile(source).is_ok(),
        "Optional array: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_array_of_optionals() {
    let source = "struct A { items: [String?] }";
    assert!(
        compile(source).is_ok(),
        "Array of optionals: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_dictionary_of_arrays() {
    let source = "struct A { map: [String: [Number]] }";
    assert!(
        compile(source).is_ok(),
        "Dictionary of arrays: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_many_fields() {
    let source = r#"
        struct Big {
            a: String,
            b: Number,
            c: Boolean,
            d: String?,
            e: [String],
            f: [String: Number]
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Many fields: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_multiple_traits() {
    let source = r#"
        trait A { a: String }
        trait B { b: Number }
        trait C { c: Boolean }
        struct Full: A + B + C {
            a: String,
            b: Number,
            c: Boolean
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Multiple traits: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_closure_single_param() {
    let source = "struct A { callback: String -> Boolean }";
    assert!(
        compile(source).is_ok(),
        "Closure single param: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_closure_returning_closure() {
    let source = "struct A { callback: String -> (Number -> Boolean) }";
    assert!(
        compile(source).is_ok(),
        "Closure returning closure: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_generic_with_multiple_params() {
    let source = r#"
        struct Triple<A, B, C> {
            first: A,
            second: B,
            third: C
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Generic with multiple params: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_impl_complex_expression() {
    let source = r#"
        struct A { x: Number }
        impl A {
            x: if true {
                for i in [1, 2, 3] {
                    i
                }
            } else {
                0
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Impl complex expression: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_multiple_impls() {
    let source = r#"
        struct A { x: String }
        struct B { y: Number }
        impl A { x: "a value" }
        impl B { y: 42 }
    "#;
    assert!(
        compile(source).is_ok(),
        "Multiple impls: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_multiple_enums() {
    let source = r#"
        enum One { a }
        enum Two { b, c }
        enum Three { d, e, f }
    "#;
    assert!(
        compile(source).is_ok(),
        "Multiple enums: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_module_with_all_types() {
    let source = r#"
        mod full {
            trait T { x: String }
            struct S: T { x: String }
            enum E { a, b }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Module with all types: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_pub_in_module() {
    let source = r#"
        mod api {
            pub trait PublicTrait { x: String }
            pub struct PublicStruct { y: Number }
            pub enum PublicEnum { a }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Pub in module: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Complex Valid Scenarios
// =============================================================================

#[test]
fn test_realistic_data_model() {
    let source = r#"
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
    "#;
    assert!(
        compile(source).is_ok(),
        "Realistic data model: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_ui_component_model() {
    let source = r#"
        trait Renderable {
            @mount content: String
        }

        struct Text {
            value: String,
            color: String = "black",
            size: Number = 14,
            display: String
        }

        struct Button {
            label: String,
            disabled: Boolean = false,
            @mount onClick: String,
            display: String
        }

        struct Card: Renderable {
            title: String,
            @mount content: String,
            @mount footer: String?,
            display: String
        }

        impl Text { display: value }
        impl Button { display: label }
        impl Card { display: title }
    "#;
    assert!(
        compile(source).is_ok(),
        "UI component model: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_config_model() {
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
    assert!(
        compile(source).is_ok(),
        "Config model: {:?}",
        compile(source).err()
    );
}
