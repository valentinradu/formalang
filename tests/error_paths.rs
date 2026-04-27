//! Error path tests
//!
//! Tests that exercise error detection and validation paths

use formalang::CompilerError;

// =============================================================================
// Type Error Tests
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

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
    let source = r"
        struct A { x: String }
        impl Unknown for A { }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    let found = errors.iter().any(|e| {
        matches!(e, CompilerError::UndefinedTrait { name, .. } if name == "Unknown")
            || matches!(e, CompilerError::UndefinedType { name, .. } if name == "Unknown")
    });
    if !found {
        return Err(
            format!("Expected UndefinedTrait or UndefinedType for 'Unknown': {errors:?}").into(),
        );
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
        struct A { y: I32 }
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
        trait A { y: I32 }
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
        struct User { age: I32 }
        impl Named for User { }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::MissingTraitField { field, .. } if field == "name"))
    {
        return Err(format!("Expected MissingTraitField for 'name': {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_wrong_trait_field_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        struct User { name: I32 }
        impl Named for User { }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors.iter().any(
        |e| matches!(e, CompilerError::TraitFieldTypeMismatch { field, .. } if field == "name"),
    ) {
        return Err(format!("Expected TraitFieldTypeMismatch for 'name': {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_missing_multiple_trait_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Full { a: String, b: I32 }
        struct Empty { }
        impl Full for Empty { }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::MissingTraitField { .. }))
    {
        return Err(format!("Expected MissingTraitField: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_not_a_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Helper { x: String }
        struct User { x: String }
        impl Helper for User { }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    let found = errors.iter().any(|e| {
        matches!(e, CompilerError::NotATrait { name, .. } if name == "Helper")
            || matches!(e, CompilerError::UndefinedType { name, .. } if name == "Helper")
    });
    if !found {
        return Err(format!("Expected NotATrait or UndefinedType for 'Helper': {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Binary Operation Error Tests
// =============================================================================

#[test]
fn test_add_boolean_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: I32 = true + false }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidBinaryOp { .. }))
    {
        return Err(format!("Expected InvalidBinaryOp: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_subtract_string_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: I32 = "a" - "b" }
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidBinaryOp { .. }))
    {
        return Err(format!("Expected InvalidBinaryOp: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_multiply_string_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: I32 = "a" * 2 }
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidBinaryOp { .. }))
    {
        return Err(format!("Expected InvalidBinaryOp: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_divide_boolean_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: I32 = true / 2 }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidBinaryOp { .. }))
    {
        return Err(format!("Expected InvalidBinaryOp: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_and_number_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean = 1 && 2 }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidBinaryOp { .. }))
    {
        return Err(format!("Expected InvalidBinaryOp: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_or_string_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: Boolean = "a" || "b" }
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidBinaryOp { .. }))
    {
        return Err(format!("Expected InvalidBinaryOp: {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Control Flow Error Tests
// =============================================================================

#[test]
fn test_if_condition_not_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A {
            x: I32 = if "string" { 1 } else { 0 }
        }
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidIfCondition { .. }))
    {
        return Err(format!("Expected InvalidIfCondition: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_if_condition_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A {
            x: I32 = if 42 { 1 } else { 0 }
        }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidIfCondition { .. }))
    {
        return Err(format!("Expected InvalidIfCondition: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_for_not_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A {
            x: String = for x in "not an array" { x }
        }
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ForLoopNotArray { .. }))
    {
        return Err(format!("Expected ForLoopNotArray: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_for_on_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A {
            x: I32 = for x in 42 { x }
        }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ForLoopNotArray { .. }))
    {
        return Err(format!("Expected ForLoopNotArray: {errors:?}").into());
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
        struct A { status: Status = Status.unknown }
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors.iter().any(
        |e| matches!(e, CompilerError::UnknownEnumVariant { variant, .. } if variant == "unknown"),
    ) {
        return Err(format!("Expected UnknownEnumVariant for 'unknown': {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Reference Error Tests
// =============================================================================

#[test]
fn test_undefined_variable_in_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: I32 = 1 + undefined_var }
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
        struct A { }
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
    let source = "struct A { map: [String: [I32]] }";
    compile(source).map_err(|e| format!("Dictionary of arrays: {e:?}"))?;
    Ok(())
}

#[test]
fn test_many_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Big {
            a: String,
            b: I32,
            c: Boolean,
            d: String?,
            e: [String],
            f: [String: I32]
        }
    ";
    compile(source).map_err(|e| format!("Many fields: {e:?}"))?;
    Ok(())
}

#[test]
fn test_multiple_traits() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait A { a: String }
        trait B { b: I32 }
        trait C { c: Boolean }
        struct Full {
            a: String,
            b: I32,
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
    let source = "struct A { callback: String -> (I32 -> Boolean) }";
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
            x: I32 = if true {
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
        struct B { y: I32 = 42 }
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
            struct S { x: String }
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
            pub struct PublicStruct { y: I32 }
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
        trait Identifiable { id: I32 }
        trait Timestamped {
            createdAt: I32,
            updatedAt: I32
        }

        enum OrderStatus {
            pending,
            processing,
            shipped,
            delivered,
            cancelled
        }

        struct Product {
            id: I32,
            name: String,
            price: I32,
            description: String?,
            tags: [String]
        }

        struct OrderItem {
            product: Product,
            quantity: I32
        }

        struct Order {
            id: I32,
            createdAt: I32,
            updatedAt: I32,
            items: [OrderItem],
            status: OrderStatus,
            total: I32
        }

        struct Customer {
            id: I32,
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
            content: String
        }

        struct Text {
            value: String,
            color: String = "black",
            size: I32 = 14
        }

        struct Button {
            label: String,
            disabled: Boolean = false,
            onClick: String
        }

        struct Card {
            title: String,
            content: String,
            footer: String?
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
                port: I32 = 5432,
                database: String,
                username: String,
                password: String?
            }

            pub struct ServerConfig {
                port: I32 = 8080,
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

// =============================================================================
// Type-mismatch negative tests (assert errors for bad programs)
// =============================================================================

#[test]
fn test_nil_to_non_optional_string_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
let s: String = nil
";
    let errors = compile(source).err().ok_or("expected error")?;
    let has_nil_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::NilAssignedToNonOptional { .. }));
    if !has_nil_error {
        return Err(format!("expected NilAssignedToNonOptional, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_field_access_via_optional_ref_on_fieldaccess_node() -> Result<(), Box<dyn std::error::Error>>
{
    // A parenthesised expression keeps the dot-chain as FieldAccess
    // rather than collapsing into a multi-segment Reference, which is
    // the path that currently runs the optional-unwrap check.
    let source = r"
struct User { name: String }
let u: User? = nil
let n = (u).name
";
    let errors = compile(source).err().ok_or("expected error")?;
    let has_opt_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::OptionalUsedAsNonOptional { .. }));
    if !has_opt_error {
        return Err(format!("expected OptionalUsedAsNonOptional, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_unknown_field_on_fieldaccess_node() -> Result<(), Box<dyn std::error::Error>> {
    // Parentheses around the receiver produce a FieldAccess node.
    let source = r"
struct Point { x: I32, y: I32 }
let p = Point(x: 1, y: 2)
let z = (p).q
";
    let errors = compile(source).err().ok_or("expected error")?;
    let has_field_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::UnknownField { field, .. } if field == "q"));
    if !has_field_error {
        return Err(format!("expected UnknownField for 'q', got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_unknown_field_on_multisegment_reference() -> Result<(), Box<dyn std::error::Error>> {
    // Plain `p.z` parses as a multi-segment Reference; the validator
    // walks the chain starting from `p`'s inferred type and reports
    // UnknownField at the first broken link.
    let source = r"
struct Point { x: I32, y: I32 }
let p = Point(x: 1, y: 2)
let z = p.z
";
    let errors = compile(source).err().ok_or("expected error")?;
    let has_field_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::UnknownField { field, .. } if field == "z"));
    if !has_field_error {
        return Err(format!("expected UnknownField for 'z', got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_unknown_field_mid_reference_chain() -> Result<(), Box<dyn std::error::Error>> {
    // The second segment resolves (`a.inner`), the third does not.
    let source = r"
struct Inner { value: I32 }
struct Outer { inner: Inner }
let a = Outer(inner: Inner(value: 1))
let bad = a.inner.missing
";
    let errors = compile(source).err().ok_or("expected error")?;
    let has_field_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::UnknownField { field, .. } if field == "missing"));
    if !has_field_error {
        return Err(format!("expected UnknownField for 'missing', got: {errors:?}").into());
    }
    Ok(())
}
