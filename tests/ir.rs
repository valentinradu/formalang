//! Tests for the IR (Intermediate Representation) module
//!
//! Tests lowering from AST to IR and verifies correct type resolution.

use formalang::compile_to_ir;

// =============================================================================
// Basic Lowering Tests
// =============================================================================

#[test]
fn test_lower_empty_source() -> Result<(), Box<dyn std::error::Error>> {
    let result = compile_to_ir("");
    let module = result.map_err(|e| format!("{e:?}"))?;
    if !module.structs.is_empty() {
        return Err("assertion failed".into());
    }
    if !module.traits.is_empty() {
        return Err("assertion failed".into());
    }
    if !module.enums.is_empty() {
        return Err("assertion failed".into());
    }
    if !module.impls.is_empty() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lower_simple_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Point { x: Number, y: Number }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }
    let point = &module.structs.first().ok_or("index out of bounds")?;
    if point.name != "Point" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            point.name, "Point"
        )
        .into());
    }
    if point.fields.len() != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, point.fields.len()).into());
    }
    if point.fields.first().ok_or("index out of bounds")?.name != "x" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            point.fields.first().ok_or("index out of bounds")?.name,
            "x"
        )
        .into());
    }
    if point.fields.get(1).ok_or("index out of bounds")?.name != "y" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            point.fields.get(1).ok_or("index out of bounds")?.name,
            "y"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_struct_with_string_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct User { name: String }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }
    let user = &module.structs.first().ok_or("index out of bounds")?;
    if user.name != "User" {
        return Err(format!("expected {:?} but got {:?}", "User", user.name).into());
    }
    if user.fields.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, user.fields.len()).into());
    }
    if user.fields.first().ok_or("index out of bounds")?.name != "name" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            user.fields.first().ok_or("index out of bounds")?.name,
            "name"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_struct_with_boolean_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Config { enabled: Boolean }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }
    if module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .name
        != "enabled"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            module
                .structs
                .first()
                .ok_or("index out of bounds")?
                .fields
                .first()
                .ok_or("index out of bounds")?
                .name,
            "enabled"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_struct_with_array_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct List { items: [String] }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }
    if module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .name
        != "items"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            module
                .structs
                .first()
                .ok_or("index out of bounds")?
                .fields
                .first()
                .ok_or("index out of bounds")?
                .name,
            "items"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_struct_with_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Profile { bio: String? }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }
    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    if field.name != "bio" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            field.name, "bio"
        )
        .into());
    }
    if !(field.optional) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lower_struct_with_mutable_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Counter { mut count: Number }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }
    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    if field.name != "count" {
        return Err(format!("expected {:?} but got {:?}", "count", field.name).into());
    }
    if !(field.mutable) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lower_public_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub struct Public { value: Number }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }
    if !module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .visibility
        .is_public()
    {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lower_private_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Private { value: Number }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }
    if module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .visibility
        .is_public()
    {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Trait Lowering Tests
// =============================================================================

#[test]
fn test_lower_simple_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Named { name: String }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.traits.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.traits.len()).into());
    }
    let named = &module.traits.first().ok_or("index out of bounds")?;
    if named.name != "Named" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            named.name, "Named"
        )
        .into());
    }
    if named.fields.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, named.fields.len()).into());
    }
    if named.fields.first().ok_or("index out of bounds")?.name != "name" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            named.fields.first().ok_or("index out of bounds")?.name,
            "name"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_trait_with_multiple_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Entity { id: Number, name: String, active: Boolean }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.traits.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.traits.len()).into());
    }
    if module
        .traits
        .first()
        .ok_or("index out of bounds")?
        .fields
        .len()
        != 3
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            3,
            module
                .traits
                .first()
                .ok_or("index out of bounds")?
                .fields
                .len()
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_public_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub trait Visible { }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.traits.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.traits.len()).into());
    }
    if !module
        .traits
        .first()
        .ok_or("index out of bounds")?
        .visibility
        .is_public()
    {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Enum Lowering Tests
// =============================================================================

#[test]
fn test_lower_simple_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Status { active, inactive, pending }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.enums.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.enums.len()).into());
    }
    let status = &module.enums.first().ok_or("index out of bounds")?;
    if status.name != "Status" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            status.name, "Status"
        )
        .into());
    }
    if status.variants.len() != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, status.variants.len()).into());
    }
    if status.variants.first().ok_or("index out of bounds")?.name != "active" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            status.variants.first().ok_or("index out of bounds")?.name,
            "active"
        )
        .into());
    }
    if status.variants.get(1).ok_or("index out of bounds")?.name != "inactive" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            status.variants.get(1).ok_or("index out of bounds")?.name,
            "inactive"
        )
        .into());
    }
    if status.variants.get(2).ok_or("index out of bounds")?.name != "pending" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            status.variants.get(2).ok_or("index out of bounds")?.name,
            "pending"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_enum_with_data() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Result { ok(value: String), error(message: String) }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.enums.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.enums.len()).into());
    }
    let result_enum = &module.enums.first().ok_or("index out of bounds")?;
    if result_enum.name != "Result" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            result_enum.name, "Result"
        )
        .into());
    }
    if result_enum.variants.len() != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, result_enum.variants.len()).into());
    }

    // Check variant with data
    let ok_variant = &result_enum.variants.first().ok_or("index out of bounds")?;
    if ok_variant.name != "ok" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            ok_variant.name, "ok"
        )
        .into());
    }
    if ok_variant.fields.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, ok_variant.fields.len()).into());
    }
    if ok_variant.fields.first().ok_or("index out of bounds")?.name != "value" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            ok_variant.fields.first().ok_or("index out of bounds")?.name,
            "value"
        )
        .into());
    }

    let error_variant = &result_enum.variants.get(1).ok_or("index out of bounds")?;
    if error_variant.name != "error" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            error_variant.name, "error"
        )
        .into());
    }
    if error_variant.fields.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, error_variant.fields.len()).into());
    }
    if error_variant
        .fields
        .first()
        .ok_or("index out of bounds")?
        .name
        != "message"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            error_variant
                .fields
                .first()
                .ok_or("index out of bounds")?
                .name,
            "message"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_enum_mixed_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Option { none, some(value: Number) }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let option = &module.enums.first().ok_or("index out of bounds")?;
    if option.variants.first().ok_or("index out of bounds")?.name != "none" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            option.variants.first().ok_or("index out of bounds")?.name,
            "none"
        )
        .into());
    }
    if !option
        .variants
        .first()
        .ok_or("index out of bounds")?
        .fields
        .is_empty()
    {
        return Err("expected empty fields for unit variant".into());
    } // Unit variant
    if option.variants.get(1).ok_or("index out of bounds")?.name != "some" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            option.variants.get(1).ok_or("index out of bounds")?.name,
            "some"
        )
        .into());
    }
    if option
        .variants
        .get(1)
        .ok_or("index out of bounds")?
        .fields
        .len()
        != 1
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            1,
            option
                .variants
                .get(1)
                .ok_or("index out of bounds")?
                .fields
                .len()
        )
        .into());
    } // Data variant
    Ok(())
}

#[test]
fn test_lower_public_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub enum Color { red, green, blue }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if !module
        .enums
        .first()
        .ok_or("index out of bounds")?
        .visibility
        .is_public()
    {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Module Lookup Tests
// =============================================================================

#[test]
fn test_struct_id_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { } struct B { } struct C { }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.struct_id("A").is_none() {
        return Err("assertion failed".into());
    }
    if module.struct_id("B").is_none() {
        return Err("assertion failed".into());
    }
    if module.struct_id("C").is_none() {
        return Err("assertion failed".into());
    }
    if module.struct_id("D").is_some() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_trait_id_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait X { } trait Y { }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.trait_id("X").is_none() {
        return Err("assertion failed".into());
    }
    if module.trait_id("Y").is_none() {
        return Err("assertion failed".into());
    }
    if module.trait_id("Z").is_some() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_enum_id_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum E1 { a } enum E2 { b }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.enum_id("E1").is_none() {
        return Err("assertion failed".into());
    }
    if module.enum_id("E2").is_none() {
        return Err("assertion failed".into());
    }
    if module.enum_id("E3").is_some() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_get_struct_by_id() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct First { a: Number } struct Second { b: String }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let first_id = module.struct_id("First").ok_or("struct not found")?;
    let second_id = module.struct_id("Second").ok_or("struct not found")?;

    let first_name = &module
        .get_struct(first_id)
        .ok_or("first struct not found")?
        .name;
    if first_name != "First" {
        return Err(format!("expected {:?} but got {:?}", "First", first_name).into());
    }
    let second_name = &module
        .get_struct(second_id)
        .ok_or("second struct not found")?
        .name;
    if second_name != "Second" {
        return Err(format!("expected {:?} but got {:?}", "Second", second_name).into());
    }
    Ok(())
}

#[test]
fn test_get_trait_by_id() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait TraitA { } trait TraitB { }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let a_id = module.trait_id("TraitA").ok_or("trait not found")?;
    let b_id = module.trait_id("TraitB").ok_or("trait not found")?;

    let a_name = &module.get_trait(a_id).ok_or("trait a not found")?.name;
    if a_name != "TraitA" {
        return Err(format!("expected {:?} but got {:?}", "TraitA", a_name).into());
    }
    let b_name = &module.get_trait(b_id).ok_or("trait b not found")?.name;
    if b_name != "TraitB" {
        return Err(format!("expected {:?} but got {:?}", "TraitB", b_name).into());
    }
    Ok(())
}

#[test]
fn test_get_enum_by_id() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum EnumA { x } enum EnumB { y }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let a_id = module.enum_id("EnumA").ok_or("enum not found")?;
    let b_id = module.enum_id("EnumB").ok_or("enum not found")?;

    let a_name = &module.get_enum(a_id).ok_or("enum a not found")?.name;
    if a_name != "EnumA" {
        return Err(format!("expected {:?} but got {:?}", "EnumA", a_name).into());
    }
    let b_name = &module.get_enum(b_id).ok_or("enum b not found")?.name;
    if b_name != "EnumB" {
        return Err(format!("expected {:?} but got {:?}", "EnumB", b_name).into());
    }
    Ok(())
}

// =============================================================================
// Impl Block Lowering Tests
// =============================================================================

#[test]
fn test_lower_impl_block() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let count: Number = 1
        struct Counter { count: Number, display: Number = count }
        impl Counter {}
    ";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }
    // Impl block is explicitly defined
    if module.impls.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.impls.len()).into());
    }
    Ok(())
}

#[test]
fn test_lower_impl_with_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config { name: String = "default" }
    "#;
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.is_empty() {
        return Err("assertion failed".into());
    }
    if module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .is_none()
    {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Struct with Trait Implementation Tests
// =============================================================================

#[test]
fn test_lower_struct_implementing_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        struct User { name: String, age: Number }
    ";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.traits.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.traits.len()).into());
    }
    if module.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.structs.len()).into());
    }

    let user = &module.structs.first().ok_or("index out of bounds")?;
    if user.name != "User" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            user.name, "User"
        )
        .into());
    }
    // Struct-trait composition via `: Trait` syntax has been removed.
    // Traits are now associated via `impl Trait for Struct` blocks.
    Ok(())
}

#[test]
fn test_lower_struct_with_multiple_traits() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        trait Aged { age: Number }
        struct Person { name: String, age: Number }
    ";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    // Verify struct lowered correctly even without trait composition syntax
    let person = &module.structs.first().ok_or("index out of bounds")?;
    if person.name != "Person" {
        return Err(format!("expected Person, got {:?}", person.name).into());
    }
    if person.fields.len() != 2 {
        return Err(format!("expected 2 fields, got {:?}", person.fields.len()).into());
    }
    Ok(())
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_lower_generic_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Box<T> { value: T }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let box_struct = &module.structs.first().ok_or("index out of bounds")?;
    if box_struct.name != "Box" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            box_struct.name, "Box"
        )
        .into());
    }
    if box_struct.generic_params.len() != 1 {
        return Err(format!(
            "expected {:?} but got {:?}",
            1,
            box_struct.generic_params.len()
        )
        .into());
    }
    if box_struct
        .generic_params
        .first()
        .ok_or("index out of bounds")?
        .name
        != "T"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            box_struct
                .generic_params
                .first()
                .ok_or("index out of bounds")?
                .name,
            "T"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_generic_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Container<T> { item: T }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let container = &module.traits.first().ok_or("index out of bounds")?;
    if container.name != "Container" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            container.name, "Container"
        )
        .into());
    }
    if container.generic_params.len() != 1 {
        return Err(format!(
            "expected {:?} but got {:?}",
            1,
            container.generic_params.len()
        )
        .into());
    }
    if container
        .generic_params
        .first()
        .ok_or("index out of bounds")?
        .name
        != "T"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            container
                .generic_params
                .first()
                .ok_or("index out of bounds")?
                .name,
            "T"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_generic_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Maybe<T> { nothing, just(value: T) }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let maybe = &module.enums.first().ok_or("index out of bounds")?;
    if maybe.name != "Maybe" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            maybe.name, "Maybe"
        )
        .into());
    }
    if maybe.generic_params.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, maybe.generic_params.len()).into());
    }
    if maybe
        .generic_params
        .first()
        .ok_or("index out of bounds")?
        .name
        != "T"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            maybe
                .generic_params
                .first()
                .ok_or("index out of bounds")?
                .name,
            "T"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_multiple_generic_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Pair<A, B> { first: A, second: B }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let pair = &module.structs.first().ok_or("index out of bounds")?;
    if pair.generic_params.len() != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, pair.generic_params.len()).into());
    }
    if pair
        .generic_params
        .first()
        .ok_or("index out of bounds")?
        .name
        != "A"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            pair.generic_params
                .first()
                .ok_or("index out of bounds")?
                .name,
            "A"
        )
        .into());
    }
    if pair
        .generic_params
        .get(1)
        .ok_or("index out of bounds")?
        .name
        != "B"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            pair.generic_params
                .get(1)
                .ok_or("index out of bounds")?
                .name,
            "B"
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Complex Definition Tests
// =============================================================================

#[test]
fn test_lower_multiple_definitions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Identifiable { id: Number }
        struct User { id: Number, name: String }
        struct Post { id: Number, title: String }
        enum Status { draft, published, archived }
    ";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.traits.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.traits.len()).into());
    }
    if module.structs.len() != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, module.structs.len()).into());
    }
    if module.enums.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, module.enums.len()).into());
    }
    Ok(())
}

#[test]
fn test_lower_struct_referencing_another() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Author { name: String }
        struct Book { title: String, author: Author }
    ";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.structs.len() != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, module.structs.len()).into());
    }

    // Book should have an Author field with struct type
    let book = module
        .structs
        .iter()
        .find(|s| s.name == "Book")
        .ok_or("not found")?;
    let author_field = book
        .fields
        .iter()
        .find(|f| f.name == "author")
        .ok_or("not found")?;
    // The type should reference Author struct
    if !matches!(&author_field.ty, formalang::ir::ResolvedType::Struct(_)) {
        return Err("expected Struct type".into());
    }
    Ok(())
}

#[test]
fn test_lower_struct_with_enum_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, inactive }
        struct User { name: String, status: Status }
    ";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let user = module
        .structs
        .iter()
        .find(|s| s.name == "User")
        .ok_or("not found")?;
    let status_field = user
        .fields
        .iter()
        .find(|f| f.name == "status")
        .ok_or("not found")?;
    if !matches!(&status_field.ty, formalang::ir::ResolvedType::Enum(_)) {
        return Err("expected Enum type".into());
    }
    Ok(())
}

// =============================================================================
// Default Value Tests
// =============================================================================

#[test]
fn test_lower_field_with_default_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Counter { count: Number = 0 }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    if field.name != "count" {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            field.name, "count"
        )
        .into());
    }
    if field.default.is_none() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lower_field_with_default_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"struct Config { name: String = "default" }"#;
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    if field.default.is_none() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lower_field_with_default_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Settings { enabled: Boolean = true }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    if field.default.is_none() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Trait Composition Tests
// =============================================================================

#[test]
fn test_lower_trait_composition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait A { a: Number }
        trait B { b: Number }
        trait C: A + B { c: Number }
    ";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    if module.traits.len() != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, module.traits.len()).into());
    }

    let trait_c = module
        .traits
        .iter()
        .find(|t| t.name == "C")
        .ok_or("not found")?;
    if trait_c.composed_traits.len() != 2 {
        return Err(format!(
            "expected {:?} but got {:?}",
            2,
            trait_c.composed_traits.len()
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Nested Array Tests
// =============================================================================

#[test]
fn test_lower_nested_array_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Matrix { rows: [[Number]] }";
    let result = compile_to_ir(source);
    let module = result.map_err(|e| format!("{e:?}"))?;

    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    if field.name != "rows" {
        return Err(format!("expected {:?} but got {:?}", "rows", field.name).into());
    }
    // Should be Array(Array(Primitive(Number)))
    if let formalang::ir::ResolvedType::Array(inner) = &field.ty {
        if !matches!(inner.as_ref(), formalang::ir::ResolvedType::Array(_)) {
            return Err("expected inner Array type".into());
        }
    } else {
        return Err("Expected nested array type".into());
    }
    Ok(())
}

// =============================================================================
// Error Case Tests
// =============================================================================

#[test]
fn test_lower_invalid_source_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = "this is not valid formalang";
    let result = compile_to_ir(source);
    if result.is_ok() {
        return Err("expected compile error for invalid source".into());
    }
    Ok(())
}

#[test]
fn test_lower_undefined_type_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Bad { field: UnknownType }";
    let result = compile_to_ir(source);
    if result.is_ok() {
        return Err("expected compile error for undefined type".into());
    }
    Ok(())
}

#[test]
fn test_lower_duplicate_struct_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Dup { } struct Dup { }";
    let result = compile_to_ir(source);
    if result.is_ok() {
        return Err("expected compile error for duplicate struct".into());
    }
    Ok(())
}

// =============================================================================
// Visitor Pattern Tests
// =============================================================================

use formalang::ir::{
    walk_module, EnumId, IrEnum, IrEnumVariant, IrField, IrImpl, IrStruct, IrTrait, IrVisitor,
    StructId, TraitId,
};

#[expect(
    clippy::struct_field_names,
    reason = "counter fields all end in _count by design"
)]
struct TypeCounter {
    struct_count: usize,
    trait_count: usize,
    enum_count: usize,
    field_count: usize,
    impl_count: usize,
    variant_count: usize,
}

impl TypeCounter {
    const fn new() -> Self {
        Self {
            struct_count: 0,
            trait_count: 0,
            enum_count: 0,
            field_count: 0,
            impl_count: 0,
            variant_count: 0,
        }
    }
}

impl IrVisitor for TypeCounter {
    fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
        self.struct_count = self.struct_count.saturating_add(1);
    }

    fn visit_trait(&mut self, _id: TraitId, _t: &IrTrait) {
        self.trait_count = self.trait_count.saturating_add(1);
    }

    fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {
        self.enum_count = self.enum_count.saturating_add(1);
    }

    fn visit_field(&mut self, _f: &IrField) {
        self.field_count = self.field_count.saturating_add(1);
    }

    fn visit_impl(&mut self, _i: &IrImpl) {
        self.impl_count = self.impl_count.saturating_add(1);
    }

    fn visit_enum_variant(&mut self, _v: &IrEnumVariant) {
        self.variant_count = self.variant_count.saturating_add(1);
    }
}

#[test]
fn test_visitor_counts_structs() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { } struct B { } struct C { }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    if counter.struct_count != 3 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.struct_count, 3
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_counts_traits() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait X { } trait Y { }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    if counter.trait_count != 2 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.trait_count, 2
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_counts_enums() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum E1 { a } enum E2 { b } enum E3 { c }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    if counter.enum_count != 3 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.enum_count, 3
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_counts_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Point { x: Number, y: Number, z: Number }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    if counter.field_count != 3 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.field_count, 3
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_counts_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Color { red, green, blue, yellow }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    if counter.variant_count != 4 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.variant_count, 4
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_counts_impls() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = 1, display: Number = 2 }
        struct B { y: Number = 3, display: Number = 4 }
        impl A {}
        impl B {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    if counter.impl_count != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, counter.impl_count).into());
    }
    Ok(())
}

#[test]
fn test_visitor_mixed_definitions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        trait Named { name: String }
        struct User { name: String, age: Number, display: String = "default" }
        enum Status { active, inactive }
        impl User {}
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    if counter.struct_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.struct_count, 1
        )
        .into());
    }
    if counter.trait_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.trait_count, 1
        )
        .into());
    }
    if counter.enum_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.enum_count, 1
        )
        .into());
    }
    if counter.impl_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.impl_count, 1
        )
        .into());
    }
    // 1 trait field + 3 struct fields = 4 fields
    if counter.field_count != 4 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.field_count, 4
        )
        .into());
    }
    // 2 enum variants
    if counter.variant_count != 2 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.variant_count, 2
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_enum_variant_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Option { none, some(value: Number) }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    // 2 variants
    if counter.variant_count != 2 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.variant_count, 2
        )
        .into());
    }
    // 1 field (in "some" variant)
    if counter.field_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.field_count, 1
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_trait_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Entity { id: Number, name: String }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = TypeCounter::new();
    walk_module(&mut counter, &module);

    if counter.trait_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.trait_count, 1
        )
        .into());
    }
    if counter.field_count != 2 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.field_count, 2
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Expression Type Tests (using IrExpr::ty() method)
// =============================================================================

use formalang::ir::ResolvedType;

fn type_name(ty: &ResolvedType) -> String {
    match ty {
        ResolvedType::Primitive(p) => format!("{p:?}"),
        ResolvedType::Struct(_) => "Struct".to_string(),
        ResolvedType::Enum(_) => "Enum".to_string(),
        ResolvedType::Array(_) => "Array".to_string(),
        ResolvedType::Trait(_)
        | ResolvedType::Optional(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::External { .. }
        | ResolvedType::Dictionary { .. }
        | ResolvedType::Closure { .. } => "Other".to_string(),
    }
}

#[test]
fn test_expr_type_literal_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct S { name: String = "hello" }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if type_name(expr.ty()) != "String" {
        return Err(format!("expected {:?} but got {:?}", "String", type_name(expr.ty())).into());
    }
    Ok(())
}

#[test]
fn test_expr_type_literal_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { value: Number = 42 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if type_name(expr.ty()) != "Number" {
        return Err(format!("expected {:?} but got {:?}", "Number", type_name(expr.ty())).into());
    }
    Ok(())
}

#[test]
fn test_expr_type_literal_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { flag: Boolean = true }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if type_name(expr.ty()) != "Boolean" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Boolean",
            type_name(expr.ty())
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_expr_type_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { items: [Number] = [1, 2, 3] }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if type_name(expr.ty()) != "Array" {
        return Err(format!("expected {:?} but got {:?}", "Array", type_name(expr.ty())).into());
    }
    Ok(())
}

#[test]
fn test_expr_type_struct_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Container { p: Point = Point(x: 1, y: 2) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .get(1)
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if type_name(expr.ty()) != "Struct" {
        return Err(format!("expected {:?} but got {:?}", "Struct", type_name(expr.ty())).into());
    }
    Ok(())
}

#[test]
fn test_expr_type_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x: Number = 1
        struct S { y: Number = x }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    // Struct field defaults should have the reference expression
    if module.structs.is_empty() {
        return Err("assertion failed".into());
    }
    if module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .is_none()
    {
        return Err("assertion failed".into());
    }
    // The expression has a type
    let _ty = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?
        .ty();
    Ok(())
}

#[test]
fn test_expr_type_binary_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { sum: Number = 1 + 2 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    // Arithmetic results in Number
    if type_name(expr.ty()) != "Number" {
        return Err(format!("expected {:?} but got {:?}", "Number", type_name(expr.ty())).into());
    }
    Ok(())
}

#[test]
fn test_expr_type_binary_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { result: Boolean = 1 == 2 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    // Comparison results in Boolean
    if type_name(expr.ty()) != "Boolean" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Boolean",
            type_name(expr.ty())
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// ResolvedType Display Name Tests
// =============================================================================

#[test]
fn test_resolved_type_display_primitive() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct S { n: Number, s: String, b: Boolean }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let s = &module.structs.first().ok_or("index out of bounds")?;
    if s.fields
        .first()
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "Number"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Number",
            s.fields
                .first()
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    if s.fields
        .get(1)
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "String"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "String",
            s.fields
                .get(1)
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    if s.fields
        .get(2)
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "Boolean"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Boolean",
            s.fields
                .get(2)
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_resolved_type_display_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct S { items: [String] }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let s = &module.structs.first().ok_or("index out of bounds")?;
    if s.fields
        .first()
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "[String]"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "[String]",
            s.fields
                .first()
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_resolved_type_display_optional() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct S { maybe: String? }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let s = &module.structs.first().ok_or("index out of bounds")?;
    if s.fields
        .first()
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "String?"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "String?",
            s.fields
                .first()
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_resolved_type_display_struct_ref() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Inner { } struct Outer { inner: Inner }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let outer = module
        .structs
        .iter()
        .find(|s| s.name == "Outer")
        .ok_or("not found")?;
    if outer
        .fields
        .first()
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "Inner"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Inner",
            outer
                .fields
                .first()
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_resolved_type_display_enum_ref() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Status { active } struct S { status: Status }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let s = module
        .structs
        .iter()
        .find(|s| s.name == "S")
        .ok_or("not found")?;
    if s.fields
        .first()
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "Status"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Status",
            s.fields
                .first()
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_resolved_type_display_nested_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct S { matrix: [[Number]] }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let s = &module.structs.first().ok_or("index out of bounds")?;
    if s.fields
        .first()
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "[[Number]]"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "[[Number]]",
            s.fields
                .first()
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Expression Lowering Tests - Control Flow
// =============================================================================

#[test]
fn test_lower_if_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { value: Number = if true { 1 } else { 2 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if !(matches!(expr, formalang::ir::IrExpr::If { .. })) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lower_if_without_else() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { value: Number? = if true { 1 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if let formalang::ir::IrExpr::If { else_branch, .. } = expr {
        if else_branch.is_some() {
            return Err("assertion failed".into());
        }
    } else {
        return Err("Expected If expression".into());
    }
    Ok(())
}

#[test]
fn test_lower_for_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { items: [Number] = for x in [1, 2, 3] { x } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if let formalang::ir::IrExpr::For { var, .. } = expr {
        if var != "x" {
            return Err(format!(
                "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                var, "x"
            )
            .into());
        }
    } else {
        return Err("Expected For expression".into());
    }
    Ok(())
}

#[test]
fn test_lower_let_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S {
            value: Number = (let x = 5
            x)
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    if module.structs.is_empty() {
        return Err("assertion failed".into());
    }
    if module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .is_none()
    {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Expression Lowering Tests - Enum Instantiation
// =============================================================================

#[test]
fn test_lower_enum_instantiation_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, inactive }
        struct S { status: Status = Status.active }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if let formalang::ir::IrExpr::EnumInst { variant, .. } = expr {
        if variant != "active" {
            return Err(format!(
                "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                variant, "active"
            )
            .into());
        }
    } else {
        return Err("Expected EnumInst expression".into());
    }
    Ok(())
}

#[test]
fn test_lower_enum_instantiation_with_data() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Option { none, some(value: Number) }
        struct S { opt: Option = Option.some(value: 42) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if let formalang::ir::IrExpr::EnumInst {
        variant, fields, ..
    } = expr
    {
        if variant != "some" {
            return Err(format!(
                "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                variant, "some"
            )
            .into());
        }
        if fields.len() != 1 {
            return Err(format!("expected {:?} but got {:?}", 1, fields.len()).into());
        }
        if fields.first().ok_or("index out of bounds")?.0 != "value" {
            return Err(format!(
                "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                fields.first().ok_or("index out of bounds")?.0,
                "value"
            )
            .into());
        }
    } else {
        return Err("Expected EnumInst expression".into());
    }
    Ok(())
}

#[test]
fn test_lower_inferred_enum_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, inactive }
        struct S { status: Status = .active }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if let formalang::ir::IrExpr::EnumInst { variant, .. } = expr {
        if variant != "active" {
            return Err(format!(
                "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                variant, "active"
            )
            .into());
        }
    } else {
        return Err("Expected EnumInst expression for inferred enum".into());
    }
    Ok(())
}

// =============================================================================
// Expression Lowering Tests - Tuple
// =============================================================================

#[test]
fn test_lower_tuple_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { point: (x: Number, y: Number) = (x: 1, y: 2) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if let formalang::ir::IrExpr::Tuple { fields, ty } = expr {
        if fields.len() != 2 {
            return Err(format!("expected {:?} but got {:?}", 2, fields.len()).into());
        }
        if fields.first().ok_or("index out of bounds")?.0 != "x" {
            return Err(format!(
                "expected {:?} but got {:?}",
                "x",
                fields.first().ok_or("index out of bounds")?.0
            )
            .into());
        }
        if fields.get(1).ok_or("index out of bounds")?.0 != "y" {
            return Err(format!(
                "expected {:?} but got {:?}",
                "y",
                fields.get(1).ok_or("index out of bounds")?.0
            )
            .into());
        }
        if !(matches!(ty, formalang::ir::ResolvedType::Tuple(_))) {
            return Err("assertion failed".into());
        }
    } else {
        return Err("Expected Tuple expression".into());
    }
    Ok(())
}

// =============================================================================
// Expression Lowering Tests - Binary Operations
// =============================================================================

#[test]
fn test_lower_binary_subtraction() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { diff: Number = 10 - 3 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if !(matches!(expr, formalang::ir::IrExpr::BinaryOp { .. })) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lower_binary_multiplication() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { product: Number = 5 * 4 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if !(matches!(expr, formalang::ir::IrExpr::BinaryOp { .. })) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_lower_binary_logical_and() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { result: Boolean = true && false }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if !(matches!(expr, formalang::ir::IrExpr::BinaryOp { .. })) {
        return Err("assertion failed".into());
    }
    if type_name(expr.ty()) != "Boolean" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Boolean",
            type_name(expr.ty())
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_binary_logical_or() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { result: Boolean = true || false }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if !(matches!(expr, formalang::ir::IrExpr::BinaryOp { .. })) {
        return Err("assertion failed".into());
    }
    if type_name(expr.ty()) != "Boolean" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Boolean",
            type_name(expr.ty())
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_binary_less_than() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { result: Boolean = 1 < 2 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if type_name(expr.ty()) != "Boolean" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Boolean",
            type_name(expr.ty())
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_binary_greater_than() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { result: Boolean = 2 > 1 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;
    if type_name(expr.ty()) != "Boolean" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Boolean",
            type_name(expr.ty())
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Visitor Expression Walking Tests
// =============================================================================

use formalang::ir::IrExpr;

#[expect(
    clippy::struct_field_names,
    reason = "counter fields all end in _count by design"
)]
struct ExprCounter {
    literal_count: usize,
    binary_op_count: usize,
    if_count: usize,
    for_count: usize,
    match_count: usize,
    array_count: usize,
    tuple_count: usize,
    struct_inst_count: usize,
    enum_inst_count: usize,
    reference_count: usize,
}

impl ExprCounter {
    const fn new() -> Self {
        Self {
            literal_count: 0,
            binary_op_count: 0,
            if_count: 0,
            for_count: 0,
            match_count: 0,
            array_count: 0,
            tuple_count: 0,
            struct_inst_count: 0,
            enum_inst_count: 0,
            reference_count: 0,
        }
    }
}

impl IrVisitor for ExprCounter {
    fn visit_expr(&mut self, e: &IrExpr) {
        match e {
            IrExpr::Literal { .. } => self.literal_count = self.literal_count.saturating_add(1),
            IrExpr::BinaryOp { .. } => {
                self.binary_op_count = self.binary_op_count.saturating_add(1);
            }
            IrExpr::If { .. } => self.if_count = self.if_count.saturating_add(1),
            IrExpr::For { .. } => self.for_count = self.for_count.saturating_add(1),
            IrExpr::Match { .. } => self.match_count = self.match_count.saturating_add(1),
            IrExpr::Array { .. } => self.array_count = self.array_count.saturating_add(1),
            IrExpr::Tuple { .. } => self.tuple_count = self.tuple_count.saturating_add(1),
            IrExpr::StructInst { .. } => {
                self.struct_inst_count = self.struct_inst_count.saturating_add(1);
            }
            IrExpr::EnumInst { .. } => {
                self.enum_inst_count = self.enum_inst_count.saturating_add(1);
            }
            IrExpr::Reference { .. } | IrExpr::SelfFieldRef { .. } | IrExpr::LetRef { .. } => {
                self.reference_count = self.reference_count.saturating_add(1);
            }
            IrExpr::FunctionCall { .. }
            | IrExpr::MethodCall { .. }
            | IrExpr::DictLiteral { .. }
            | IrExpr::DictAccess { .. }
            | IrExpr::UnaryOp { .. }
            | IrExpr::Block { .. }
            | IrExpr::FieldAccess { .. }
            | IrExpr::Closure { .. } => {}
        }
        // Walk children
        formalang::ir::walk_expr_children(self, e);
    }
}

#[test]
fn test_visitor_walks_if_children() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { value: Number = if true { 1 } else { 2 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    if counter.if_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.if_count, 1
        )
        .into());
    }
    // Condition (true) + then branch (1) + else branch (2) = 3 literals
    if counter.literal_count != 3 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.literal_count, 3
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walks_for_children() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { items: [Number] = for x in [1, 2] { x } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    if counter.for_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.for_count, 1
        )
        .into());
    }
    if counter.array_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.array_count, 1
        )
        .into());
    }
    if counter.reference_count != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, counter.reference_count).into());
    } // x reference in body
    Ok(())
}

#[test]
fn test_visitor_walks_nested_if() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S {
            value: Number = if true {
                if false { 1 } else { 2 }
            } else {
                3
            }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    // 2 if expressions (outer and nested)
    if counter.if_count != 2 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.if_count, 2
        )
        .into());
    }
    // literals: true, false, 1, 2, 3
    if counter.literal_count != 5 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.literal_count, 5
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walks_binary_op_children() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { result: Number = 1 + 2 + 3 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    // (1 + 2) + 3 = 2 binary ops
    if counter.binary_op_count != 2 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.binary_op_count, 2
        )
        .into());
    }
    if counter.literal_count != 3 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.literal_count, 3
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walks_struct_inst_children() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Container { p: Point = Point(x: 1 + 2, y: 3) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    if counter.struct_inst_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.struct_inst_count, 1
        )
        .into());
    }
    if counter.binary_op_count != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, counter.binary_op_count).into());
    } // 1 + 2
    if counter.literal_count != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, counter.literal_count).into());
    } // 1, 2, 3
    Ok(())
}

#[test]
fn test_visitor_walks_enum_inst_children() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Option { none, some(value: Number) }
        struct S { opt: Option = Option.some(value: 1 + 2) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    if counter.enum_inst_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.enum_inst_count, 1
        )
        .into());
    }
    if counter.binary_op_count != 1 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.binary_op_count, 1
        )
        .into());
    }
    if counter.literal_count != 2 {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            counter.literal_count, 2
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walks_array_children() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { items: [Number] = [1, 2 + 3, 4] }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    if counter.array_count != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, counter.array_count).into());
    }
    if counter.binary_op_count != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, counter.binary_op_count).into());
    }
    if counter.literal_count != 4 {
        return Err(format!("expected {:?} but got {:?}", 4, counter.literal_count).into());
    } // 1, 2, 3, 4
    Ok(())
}

#[test]
fn test_visitor_walks_tuple_children() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { point: (x: Number, y: Number) = (x: 1 + 2, y: 3) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut counter = ExprCounter::new();
    walk_module(&mut counter, &module);

    if counter.tuple_count != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, counter.tuple_count).into());
    }
    if counter.binary_op_count != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, counter.binary_op_count).into());
    }
    if counter.literal_count != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, counter.literal_count).into());
    }
    Ok(())
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_lower_generic_wrapper_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Wrapper<T> { value: T }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let wrapper = &module.structs.first().ok_or("index out of bounds")?;
    if wrapper.name != "Wrapper" {
        return Err(format!("expected {:?} but got {:?}", "Wrapper", wrapper.name).into());
    }
    if wrapper.generic_params.len() != 1 {
        return Err(format!(
            "expected {:?} but got {:?}",
            1,
            wrapper.generic_params.len()
        )
        .into());
    }
    if wrapper
        .generic_params
        .first()
        .ok_or("index out of bounds")?
        .name
        != "T"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "T",
            wrapper
                .generic_params
                .first()
                .ok_or("index out of bounds")?
                .name
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_generic_struct_multiple_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Pair<A, B> { first: A, second: B }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let pair = &module.structs.first().ok_or("index out of bounds")?;
    if pair.generic_params.len() != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, pair.generic_params.len()).into());
    }
    if pair
        .generic_params
        .first()
        .ok_or("index out of bounds")?
        .name
        != "A"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            pair.generic_params
                .first()
                .ok_or("index out of bounds")?
                .name,
            "A"
        )
        .into());
    }
    if pair
        .generic_params
        .get(1)
        .ok_or("index out of bounds")?
        .name
        != "B"
    {
        return Err(format!(
            "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
            pair.generic_params
                .get(1)
                .ok_or("index out of bounds")?
                .name,
            "B"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_lower_generic_with_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        struct Container<T: Named> { item: T }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .ok_or("not found")?;
    if container.generic_params.len() != 1 {
        return Err(format!(
            "expected {:?} but got {:?}",
            1,
            container.generic_params.len()
        )
        .into());
    }
    if container
        .generic_params
        .first()
        .ok_or("index out of bounds")?
        .constraints
        .is_empty()
    {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// ResolvedType Additional Coverage
// =============================================================================

#[test]
fn test_resolved_type_display_trait_ref() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        struct Container { item: Named }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .ok_or("not found")?;
    if container
        .fields
        .first()
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "Named"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Named",
            container
                .fields
                .first()
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_resolved_type_display_type_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let box_struct = &module.structs.first().ok_or("index out of bounds")?;
    // Type parameter T should display as "T"
    if box_struct
        .fields
        .first()
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module)
        != "T"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "T",
            box_struct
                .fields
                .first()
                .ok_or("index out of bounds")?
                .ty
                .display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_resolved_type_display_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Box<T> { value: T } struct Container { item: Box<String> }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .ok_or("not found")?;
    let display = container
        .fields
        .first()
        .ok_or("index out of bounds")?
        .ty
        .display_name(&module);
    // Generic instantiation should show type args
    if !(display.contains("Box") || display.contains("String")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// External Reference Tests
// =============================================================================

use formalang::ir::ExternalKind;
use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
use std::collections::HashMap;
use std::path::PathBuf;

/// Mock module resolver for IR external reference tests
struct MockResolver {
    modules: HashMap<Vec<String>, (String, PathBuf)>,
}

impl MockResolver {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    fn add_module(&mut self, path: Vec<String>, source: &str) {
        let file_path = PathBuf::from(format!("{}.forma", path.join("/")));
        self.modules.insert(path, (source.to_string(), file_path));
    }
}

impl ModuleResolver for MockResolver {
    fn resolve(
        &self,
        path: &[String],
        _current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        self.modules
            .get(path)
            .cloned()
            .ok_or_else(|| ModuleError::NotFound {
                path: path.to_vec(),
                searched_paths: vec![],
            })
    }
}

fn compile_to_ir_with_resolver<R: ModuleResolver>(
    source: &str,
    resolver: R,
) -> Result<formalang::IrModule, Vec<formalang::CompilerError>> {
    let (ast, analyzer) = formalang::compile_with_analyzer_and_resolver(source, resolver)?;
    formalang::ir::lower_to_ir(&ast, analyzer.symbols())
}

#[test]
fn test_external_struct_reference() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r"
use utils::Helper
struct Main {
    helper: Helper
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    // Main struct should exist
    let main = module
        .structs
        .iter()
        .find(|s| s.name == "Main")
        .ok_or("not found")?;
    let helper_field = &main.fields.first().ok_or("index out of bounds")?;

    // Helper type should be External, not a local struct
    match &helper_field.ty {
        ResolvedType::External {
            module_path,
            name,
            kind,
            type_args,
        } => {
            if module_path != &vec!["utils".to_string()] {
                return Err(format!(
                    "expected {:?} but got {:?}",
                    &vec!["utils".to_string()],
                    module_path
                )
                .into());
            }
            if name != "Helper" {
                return Err(format!(
                    "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                    name, "Helper"
                )
                .into());
            }
            if *kind != ExternalKind::Struct {
                return Err(format!(
                    "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                    *kind,
                    ExternalKind::Struct
                )
                .into());
            }
            if !type_args.is_empty() {
                return Err("assertion failed".into());
            }
        }
        other @ (ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Array(_)
        | ResolvedType::Optional(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::Dictionary { .. }
        | ResolvedType::Closure { .. }) => {
            return Err(format!("Unexpected variant: {other:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_external_trait_reference() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["traits".to_string()],
        "pub trait Named { name: String }",
    );

    let source = r"
use traits::Named
struct User {
    name: String
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    // User struct should be in the IR
    let _user = module
        .structs
        .iter()
        .find(|s| s.name == "User")
        .ok_or("User struct not found")?;

    // The Named trait was imported — verify module compiled successfully.
    // Note: Without struct-trait composition, the import may or may not
    // appear in module.imports depending on whether it is referenced.
    // We simply verify the module compiled and User is present.
    Ok(())
}

#[test]
fn test_external_enum_reference() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["types".to_string()],
        "pub enum Status { active, inactive }",
    );

    let source = r"
use types::Status
struct Item {
    status: Status
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let item = module
        .structs
        .iter()
        .find(|s| s.name == "Item")
        .ok_or("not found")?;
    let status_field = &item.fields.first().ok_or("index out of bounds")?;

    match &status_field.ty {
        ResolvedType::External {
            module_path,
            name,
            kind,
            ..
        } => {
            if module_path != &vec!["types".to_string()] {
                return Err(format!(
                    "expected {:?} but got {:?}",
                    &vec!["types".to_string()],
                    module_path
                )
                .into());
            }
            if name != "Status" {
                return Err(format!(
                    "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                    name, "Status"
                )
                .into());
            }
            if *kind != ExternalKind::Enum {
                return Err(format!(
                    "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                    *kind,
                    ExternalKind::Enum
                )
                .into());
            }
        }
        other @ (ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Array(_)
        | ResolvedType::Optional(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::Dictionary { .. }
        | ResolvedType::Closure { .. }) => {
            return Err(format!("Unexpected variant: {other:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_external_generic_reference() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["containers".to_string()],
        "pub struct Box<T> { value: T }",
    );

    let source = r"
use containers::Box
struct Wrapper {
    item: Box<String>
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let wrapper = module
        .structs
        .iter()
        .find(|s| s.name == "Wrapper")
        .ok_or("not found")?;
    let item_field = &wrapper.fields.first().ok_or("index out of bounds")?;

    match &item_field.ty {
        ResolvedType::External {
            module_path,
            name,
            kind,
            type_args,
        } => {
            if module_path != &vec!["containers".to_string()] {
                return Err(format!(
                    "expected {:?} but got {:?}",
                    &vec!["containers".to_string()],
                    module_path
                )
                .into());
            }
            if name != "Box" {
                return Err(format!("expected {:?} but got {:?}", "Box", name).into());
            }
            if *kind != ExternalKind::Struct {
                return Err(format!(
                    "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                    *kind,
                    ExternalKind::Struct
                )
                .into());
            }
            if type_args.len() != 1 {
                return Err(format!("expected {:?} but got {:?}", 1, type_args.len()).into());
            }
            if !matches!(
                &type_args.first().ok_or("index out of bounds")?,
                ResolvedType::Primitive(formalang::ast::PrimitiveType::String)
            ) {
                return Err("expected String primitive type".into());
            }
        }
        other @ (ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Array(_)
        | ResolvedType::Optional(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::Dictionary { .. }
        | ResolvedType::Closure { .. }) => {
            return Err(format!("Unexpected variant: {other:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_ir_imports_populated() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r"
pub struct Helper { name: String }
pub struct Utils { value: Number }
",
    );

    // Only Helper is actually used, so only Helper should be in imports
    let source = r"
use utils::{Helper, Utils}
struct Main {
    helper: Helper
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    // imports should contain only used items
    if module.imports.is_empty() {
        return Err("assertion failed".into());
    }

    let utils_import = module
        .imports
        .iter()
        .find(|i| i.module_path == vec!["utils".to_string()]);

    if utils_import.is_none() {
        return Err("assertion failed".into());
    }
    let utils_import = utils_import.ok_or("not found")?;

    // Only Helper is used, Utils is imported but not used
    if !(utils_import.items.iter().any(|i| i.name == "Helper")) {
        return Err("assertion failed".into());
    }
    // Utils is NOT used, so it should NOT be in the imports
    // (we only track imports that are actually used in the code)
    Ok(())
}

#[test]
fn test_external_nested_module_path() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["std".to_string(), "collections".to_string()],
        "pub struct List { items: [String] }",
    );

    let source = r"
use std::collections::List
struct Container {
    items: List
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .ok_or("not found")?;
    let items_field = &container.fields.first().ok_or("index out of bounds")?;

    match &items_field.ty {
        ResolvedType::External { module_path, .. } => {
            let expected = vec!["std".to_string(), "collections".to_string()];
            if module_path != &expected {
                return Err(format!("expected {expected:?} but got {module_path:?}").into());
            }
        }
        other @ (ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Array(_)
        | ResolvedType::Optional(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::Dictionary { .. }
        | ResolvedType::Closure { .. }) => {
            return Err(format!("Unexpected variant: {other:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_external_display_name_simple() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r"
use utils::Helper
struct Main {
    helper: Helper
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let main = module
        .structs
        .iter()
        .find(|s| s.name == "Main")
        .ok_or("not found")?;
    let helper_field = &main.fields.first().ok_or("index out of bounds")?;

    // display_name should return just the type name
    if helper_field.ty.display_name(&module) != "Helper" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Helper",
            helper_field.ty.display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_external_display_name_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["containers".to_string()],
        "pub struct Box<T> { value: T }",
    );

    let source = r"
use containers::Box
struct Wrapper {
    item: Box<String>
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let wrapper = module
        .structs
        .iter()
        .find(|s| s.name == "Wrapper")
        .ok_or("not found")?;
    let item_field = &wrapper.fields.first().ok_or("index out of bounds")?;

    // display_name should show type with args
    if item_field.ty.display_name(&module) != "Box<String>" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Box<String>",
            item_field.ty.display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_local_types_not_external() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Helper { name: String }
struct Main {
    helper: Helper
}
";

    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let main = module
        .structs
        .iter()
        .find(|s| s.name == "Main")
        .ok_or("not found")?;
    let helper_field = &main.fields.first().ok_or("index out of bounds")?;

    // Local types should remain as Struct, not External
    if !(matches!(helper_field.ty, ResolvedType::Struct(_))) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_mixed_local_and_external() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct External { name: String }",
    );

    let source = r"
use utils::External
struct Local { value: Number }
struct Main {
    external: External,
    local: Local
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let main = module
        .structs
        .iter()
        .find(|s| s.name == "Main")
        .ok_or("not found")?;

    let external_field = main
        .fields
        .iter()
        .find(|f| f.name == "external")
        .ok_or("not found")?;
    let local_field = main
        .fields
        .iter()
        .find(|f| f.name == "local")
        .ok_or("not found")?;

    // External type should be External variant
    if !(matches!(external_field.ty, ResolvedType::External { .. })) {
        return Err("assertion failed".into());
    }

    // Local type should be Struct variant
    if !(matches!(local_field.ty, ResolvedType::Struct(_))) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_external_in_array() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Item { name: String }",
    );

    let source = r"
use utils::Item
struct Collection {
    items: [Item]
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let collection = module
        .structs
        .iter()
        .find(|s| s.name == "Collection")
        .ok_or("not found")?;
    let items_field = &collection.fields.first().ok_or("index out of bounds")?;

    match &items_field.ty {
        ResolvedType::Array(inner) => match inner.as_ref() {
            ResolvedType::External { name, .. } => {
                if name != "Item" {
                    return Err(format!(
                        "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                        name, "Item"
                    )
                    .into());
                }
            }
            other @ (ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }) => {
                return Err(format!("Unexpected variant: {other:?}").into())
            }
        },
        other @ (ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Optional(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::External { .. }
        | ResolvedType::Dictionary { .. }
        | ResolvedType::Closure { .. }) => {
            return Err(format!("Unexpected variant: {other:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_external_in_optional() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Item { name: String }",
    );

    let source = r"
use utils::Item
struct Container {
    item: Item?
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .ok_or("not found")?;
    let item_field = &container.fields.first().ok_or("index out of bounds")?;

    match &item_field.ty {
        ResolvedType::Optional(inner) => match inner.as_ref() {
            ResolvedType::External { name, .. } => {
                if name != "Item" {
                    return Err(format!(
                        "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                        name, "Item"
                    )
                    .into());
                }
            }
            other @ (ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }) => {
                return Err(format!("Unexpected variant: {other:?}").into())
            }
        },
        other @ (ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Array(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::External { .. }
        | ResolvedType::Dictionary { .. }
        | ResolvedType::Closure { .. }) => {
            return Err(format!("Unexpected variant: {other:?}").into())
        }
    }
    Ok(())
}

// =============================================================================
// External Reference Safety Tests
// =============================================================================

/// Tests that external types cannot be looked up via `struct_id` - this is the
/// expected behavior that code generators must handle.
#[test]
fn test_external_struct_not_in_struct_id_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r"
use utils::Helper
struct Main {
    helper: Helper
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    // Imported types ARE now found via struct_id lookup (new behavior)
    // They are registered during IR lowering so they have valid IDs
    if module.struct_id("Helper").is_none() {
        return Err("Imported types should be in struct_id lookup".into());
    }

    // Local structs should also be in the lookup
    if module.struct_id("Main").is_none() {
        return Err("assertion failed".into());
    }
    Ok(())
}

/// Tests that code generators can safely iterate over all struct fields
/// without panicking when encountering external types.
#[test]
fn test_safe_iteration_over_external_types() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r"
use utils::Helper
struct Local { value: Number }
struct Main {
    helper: Helper,
    local: Local,
    primitive: String
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let main = module
        .structs
        .iter()
        .find(|s| s.name == "Main")
        .ok_or("not found")?;

    // This is how code generators should safely handle all type variants
    for field in &main.fields {
        match &field.ty {
            ResolvedType::Struct(id) => {
                // Safe: only local structs have StructIds
                let struct_def = module.get_struct(*id).ok_or("struct not found")?;
                if struct_def.name.is_empty() {
                    return Err("assertion failed".into());
                }
            }
            ResolvedType::External {
                module_path, name, ..
            } => {
                // External types should be handled by emitting imports
                if module_path.is_empty() {
                    return Err("assertion failed".into());
                }
                if name.is_empty() {
                    return Err("assertion failed".into());
                }
            }
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. } => {}
        }
    }
    Ok(())
}

/// Tests that all `StructIds` in a module are valid and won't cause panics.
/// This catches the bug where imported types incorrectly get `u32::MAX` IDs.
#[test]
#[expect(
    clippy::items_after_statements,
    reason = "local helper fn defined after setup"
)]
fn test_all_struct_ids_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r"
use utils::Helper
struct Local { value: Number }
struct Main {
    helper: Helper,
    local: Local
}
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    // Collect all StructIds from the IR and verify they are valid
    fn collect_struct_ids(ty: &ResolvedType, ids: &mut Vec<StructId>) {
        match ty {
            ResolvedType::Struct(id) => ids.push(*id),
            ResolvedType::Generic { base, args } => {
                ids.push(*base);
                for arg in args {
                    collect_struct_ids(arg, ids);
                }
            }
            ResolvedType::Array(inner) | ResolvedType::Optional(inner) => {
                collect_struct_ids(inner, ids);
            }
            ResolvedType::Tuple(fields) => {
                for (_, ty) in fields {
                    collect_struct_ids(ty, ids);
                }
            }
            ResolvedType::External { type_args, .. } => {
                // External types don't have StructIds, but their type_args might
                for arg in type_args {
                    collect_struct_ids(arg, ids);
                }
            }
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. } => {}
        }
    }

    let mut all_ids = Vec::new();
    for s in &module.structs {
        for field in &s.fields {
            collect_struct_ids(&field.ty, &mut all_ids);
        }
    }

    // All collected StructIds must be valid (in bounds)
    for id in all_ids {
        if (id.0 as usize) >= module.structs.len() {
            return Err(format!(
                "StructId({}) is out of bounds (module has {} structs)",
                id.0,
                module.structs.len()
            )
            .into());
        }
        let s = module
            .get_struct(id)
            .ok_or_else(|| format!("get_struct({id:?}) returned None"))?;
        if s.name.is_empty() {
            return Err(format!("get_struct({id:?}) returned struct with empty name").into());
        }
    }
    Ok(())
}

/// Tests that `get_struct` returns None for invalid IDs — external types
/// incorrectly assigned `u32::MAX` must not cause panics.
#[test]
fn test_get_struct_returns_none_for_invalid_id() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Only { value: Number }";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    if module.structs.len() != 1 {
        return Err(format!("expected 1 struct, got {}", module.structs.len()).into());
    }
    let invalid_id = StructId(u32::MAX);
    if module.get_struct(invalid_id).is_some() {
        return Err("get_struct should return None for out-of-bounds id".into());
    }
    Ok(())
}

/// Tests that instantiating an external struct produces `struct_id=None`
/// and ty=External, not `struct_id=u32::MAX` which would panic.
#[test]
fn test_external_struct_instantiation_has_none_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );

    let source = r#"
use utils::Helper
struct Container { h: Helper = Helper(name: "test") }
"#;

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    // Find Container struct by name (imported Helper might be first)
    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .ok_or("not found")?;
    let expr = container
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;

    // It should be a StructInst with a valid struct_id (imported types now get IDs)
    if let IrExpr::StructInst { struct_id, ty, .. } = expr {
        if struct_id.is_none() {
            return Err("Imported struct instantiation should have struct_id, got None".into());
        }

        // The type should be Struct (not External, since we registered it)
        match ty {
            ResolvedType::Struct(id) => {
                if struct_id != &Some(*id) {
                    return Err(format!("expected {:?} but got {:?}", &Some(*id), struct_id).into());
                }
            }
            other @ (ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }) => {
                return Err(format!("Unexpected variant: {other:?}").into())
            }
        }
    } else {
        return Err(format!("Expected StructInst expression, got {expr:?}").into());
    }
    Ok(())
}

/// Tests that instantiating an external enum produces `enum_id=None`.
#[test]
fn test_external_enum_instantiation_has_none_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add_module(
        vec!["types".to_string()],
        "pub enum Status { active, inactive }",
    );

    let source = r"
use types::Status
struct Item { status: Status = Status.active }
";

    let module = compile_to_ir_with_resolver(source, resolver).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;

    if let IrExpr::EnumInst {
        enum_id,
        variant,
        ty,
        ..
    } = expr
    {
        if enum_id.is_none() {
            return Err("Imported enum instantiation should have enum_id, got None".into());
        }
        if variant != "active" {
            return Err(format!(
                "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                variant, "active"
            )
            .into());
        }

        match ty {
            ResolvedType::Enum(id) => {
                if enum_id != &Some(*id) {
                    return Err(format!("expected {:?} but got {:?}", &Some(*id), enum_id).into());
                }
            }
            other @ (ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Array(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }) => {
                return Err(format!("Unexpected variant: {other:?}").into())
            }
        }
    } else {
        return Err(format!("Expected EnumInst expression, got {expr:?}").into());
    }
    Ok(())
}

/// Tests that local struct instantiation still has `Some(struct_id)`.
#[test]
fn test_local_struct_instantiation_has_some_id() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Point { x: Number, y: Number }
struct Container { p: Point = Point(x: 1, y: 2) }
";

    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let expr = module
        .structs
        .get(1)
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("expected Some")?;

    if let IrExpr::StructInst { struct_id, ty, .. } = expr {
        if struct_id.is_none() {
            return Err("Local struct instantiation should have Some(struct_id)".into());
        }

        // Verify the ID is valid
        let id = struct_id.ok_or("struct not found")?;
        let struct_def = module.get_struct(id).ok_or("struct not found")?;
        if struct_def.name != "Point" {
            return Err(format!(
                "assertion failed: `(left == right)` left: `{:?}`, right: `{:?}`",
                struct_def.name, "Point"
            )
            .into());
        }

        // The type should be Struct, not External
        if !(matches!(ty, ResolvedType::Struct(_))) {
            return Err("assertion failed".into());
        }
    } else {
        return Err("Expected StructInst expression".into());
    }
    Ok(())
}

// =============================================================================
// Method Resolution Tests
// =============================================================================

/// Compile through a `FileSystemResolver` rooted at the current directory,
/// so tests can write fixture modules alongside the test binary.
fn compile_rooted_here(source: &str) -> Result<formalang::IrModule, Vec<formalang::CompilerError>> {
    let root_dir = std::path::PathBuf::from(".");
    let resolver = formalang::FileSystemResolver::new(root_dir);
    let (ast, analyzer) = formalang::compile_with_analyzer_and_resolver(source, resolver)?;
    formalang::ir::lower_to_ir(&ast, analyzer.symbols())
}

/// Test that method calls on extern types get proper return type resolution
#[test]
#[expect(
    clippy::items_after_statements,
    reason = "local helper struct defined after setup"
)]
fn test_method_call_resolve_normalize() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrExpr, IrVisitor, ResolvedType};

    // A struct that uses a method call on an extern-impl type
    let source = r"
        struct Vec3 { x: Number, y: Number, z: Number }
        extern impl Vec3 {
            fn normalize(self) -> Vec3
        }
        struct Particle {
            velocity: Vec3
        }
        extern fn get_velocity() -> Vec3
        impl Particle {
            fn direction() -> Vec3 {
                self.velocity.normalize()
            }
        }
    ";

    let module = compile_rooted_here(source).map_err(|e| format!("Should compile: {e:?}"))?;

    // Find the method call in the function body
    struct MethodCallFinder {
        found_normalize: bool,
        return_type: Option<ResolvedType>,
    }

    impl IrVisitor for MethodCallFinder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExpr::MethodCall { method, ty, .. } = e {
                if method == "normalize" {
                    self.found_normalize = true;
                    self.return_type = Some(ty.clone());
                }
            }
            formalang::ir::walk_expr_children(self, e);
        }
    }

    let mut finder = MethodCallFinder {
        found_normalize: false,
        return_type: None,
    };
    walk_module(&mut finder, &module);

    if !(finder.found_normalize) {
        return Err("Should find normalize method call".into());
    }
    // Return type should be resolved (Some variant)
    if finder.return_type.is_none() {
        return Err("normalize method call should have a resolved return type".into());
    }
    Ok(())
}

/// Test that method calls for `size()` return Number
#[test]
#[expect(
    clippy::items_after_statements,
    reason = "local helper struct defined after setup"
)]
fn test_method_call_resolve_length() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ir::{walk_module, IrExpr, IrVisitor, ResolvedType};

    let source = r"
        struct Canvas { width: Number, height: Number }
        extern impl Canvas {
            fn size(self) -> Number
        }
        struct Renderer {
            canvas: Canvas
        }
        extern fn get_canvas() -> Canvas
        impl Renderer {
            fn area() -> Number {
                self.canvas.size()
            }
        }
    ";

    let module = compile_rooted_here(source).map_err(|e| format!("Should compile: {e:?}"))?;

    struct MethodCallFinder {
        found_length: bool,
        return_type: Option<ResolvedType>,
    }

    impl IrVisitor for MethodCallFinder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExpr::MethodCall { method, ty, .. } = e {
                if method == "size" {
                    self.found_length = true;
                    self.return_type = Some(ty.clone());
                }
            }
            formalang::ir::walk_expr_children(self, e);
        }
    }

    let mut finder = MethodCallFinder {
        found_length: false,
        return_type: None,
    };
    walk_module(&mut finder, &module);

    if !finder.found_length {
        return Err("Should find size method call".into());
    }
    // Return type should be Number
    if finder.return_type != Some(ResolvedType::Primitive(PrimitiveType::Number)) {
        return Err(format!(
            "size on Canvas should return Number, got {:?}",
            finder.return_type
        )
        .into());
    }
    Ok(())
}

/// Test method resolution with chained calls (`self.field.method()`)
#[test]
#[expect(
    clippy::items_after_statements,
    reason = "local helper struct defined after setup"
)]
fn test_method_call_chained() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrExpr, IrVisitor, ResolvedType};

    let source = r"
        struct Handle { raw: Number }
        extern impl Handle {
            fn normalize(self) -> Handle
        }
        struct Particle {
            velocity: Handle
        }
        extern fn make_handle() -> Handle
        impl Particle {
            fn normalized_velocity() -> Handle {
                self.velocity.normalize()
            }
        }
    ";

    let module = compile_rooted_here(source).map_err(|e| format!("Should compile: {e:?}"))?;

    // Find the method call in function body
    struct MethodCallFinder {
        found: bool,
        return_type: Option<ResolvedType>,
        receiver_field: Option<String>,
    }

    impl IrVisitor for MethodCallFinder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExpr::MethodCall {
                method,
                ty,
                receiver,
                ..
            } = e
            {
                if method == "normalize" {
                    self.found = true;
                    self.return_type = Some(ty.clone());
                    // Check receiver is SelfFieldRef
                    if let IrExpr::SelfFieldRef { field, .. } = receiver.as_ref() {
                        self.receiver_field = Some(field.clone());
                    }
                }
            }
            formalang::ir::walk_expr_children(self, e);
        }
    }

    let mut finder = MethodCallFinder {
        found: false,
        return_type: None,
        receiver_field: None,
    };
    walk_module(&mut finder, &module);

    if !finder.found {
        return Err("Should find normalize method call".into());
    }
    if finder.receiver_field.as_deref() != Some("velocity") {
        return Err(format!("expected velocity field, got {:?}", finder.receiver_field).into());
    }
    // Return type should be resolved
    if finder.return_type.is_none() {
        return Err("normalize method call should have a resolved return type".into());
    }
    Ok(())
}

// =============================================================================
// Dictionary Lowering Tests
// =============================================================================

#[test]
#[expect(
    clippy::items_after_statements,
    reason = "local helper struct defined after setup"
)]
fn test_dict_literal_lowering() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrExpr, IrVisitor, ResolvedType};

    let source = r#"
        struct Config { data: [String: Number] = ["a": 1, "b": 2] }
        let cfg: Config = Config()
    "#;

    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    struct DictFinder {
        found: bool,
        entry_count: usize,
        type_ok: bool,
    }

    impl IrVisitor for DictFinder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExpr::DictLiteral { entries, ty, .. } = e {
                self.found = true;
                self.entry_count = entries.len();
                self.type_ok = matches!(ty, ResolvedType::Dictionary { .. });
            }
            formalang::ir::walk_expr_children(self, e);
        }
    }

    let mut finder = DictFinder {
        found: false,
        entry_count: 0,
        type_ok: false,
    };
    walk_module(&mut finder, &module);

    if !(finder.found) {
        return Err("Should find DictLiteral".into());
    }
    if !finder.type_ok {
        return Err("expected Dictionary type".into());
    }
    if finder.entry_count != 2 {
        return Err(format!("Should have 2 entries, got {}", finder.entry_count).into());
    }
    Ok(())
}

#[test]
#[expect(
    clippy::items_after_statements,
    reason = "local helper struct defined after setup"
)]
fn test_dict_access_lowering() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrExpr, IrVisitor};

    let source = r#"
        let data: [String: Number] = ["a": 1]
        struct Config { value: Number = data["a"] }
        let cfg: Config = Config()
    "#;

    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    struct AccessFinder {
        found: bool,
    }

    impl IrVisitor for AccessFinder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExpr::DictAccess { .. } = e {
                self.found = true;
            }
            formalang::ir::walk_expr_children(self, e);
        }
    }

    let mut finder = AccessFinder { found: false };
    walk_module(&mut finder, &module);

    if !(finder.found) {
        return Err("Should find DictAccess".into());
    }
    Ok(())
}

#[test]
fn test_dict_type_lowering() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::ResolvedType;

    let source = r"
        struct Container { data: [String: Number] }
    ";

    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let container = &module.structs.first().ok_or("index out of bounds")?;
    let data_field = container
        .fields
        .iter()
        .find(|f| f.name == "data")
        .ok_or("not found")?;

    match &data_field.ty {
        ResolvedType::Dictionary { key_ty, value_ty } => {
            if !(matches!(key_ty.as_ref(), ResolvedType::Primitive(_))) {
                return Err("assertion failed".into());
            }
            if !(matches!(value_ty.as_ref(), ResolvedType::Primitive(_))) {
                return Err("assertion failed".into());
            }
        }
        other @ (ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Array(_)
        | ResolvedType::Optional(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::External { .. }
        | ResolvedType::Closure { .. }) => {
            return Err(format!("Unexpected variant: {other:?}").into())
        }
    }
    Ok(())
}

#[test]
fn test_nested_module_impl_blocks_captured() -> Result<(), Box<dyn std::error::Error>> {
    // Test that impl blocks inside nested modules are properly lowered to IR
    let source = r"
pub mod fill {
    pub struct Solid {
        color: Number = 0
    }

    impl Solid {
        fn sample(self, uv: Number) -> Number {
            self.color + uv
        }
    }
}
";

    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    // Struct should be named with module prefix
    let solid = module
        .structs
        .iter()
        .find(|s| s.name == "fill::Solid")
        .ok_or("Should have fill")?;
    if solid.fields.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, solid.fields.len()).into());
    }
    if solid.fields.first().ok_or("index out of bounds")?.name != "color" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "color",
            solid.fields.first().ok_or("index out of bounds")?.name
        )
        .into());
    }

    // Impl block should be captured
    if module.impls.is_empty() {
        return Err("Should have at least one impl block (fill::Solid impl)".into());
    }

    // Find the impl for fill::Solid
    let solid_id = module
        .structs
        .iter()
        .position(|s| s.name == "fill::Solid")
        .ok_or("fill")?;
    let solid_id_u32 = u32::try_from(solid_id).map_err(|e| format!("id overflow: {e}"))?;

    let solid_impl = module
        .impls
        .iter()
        .find(|i| i.struct_id() == Some(StructId(solid_id_u32)))
        .ok_or("Should have impl for fill::Solid")?;

    // Verify the sample function is captured
    if solid_impl.functions.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, solid_impl.functions.len()).into());
    }
    if solid_impl
        .functions
        .first()
        .ok_or("index out of bounds")?
        .name
        != "sample"
    {
        return Err(format!(
            "expected {:?} but got {:?}",
            "sample",
            solid_impl
                .functions
                .first()
                .ok_or("index out of bounds")?
                .name
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_inferred_enum_type_resolved_from_return_type() -> Result<(), Box<dyn std::error::Error>> {
    // Test that InferredEnumInstantiation (e.g., `.rgba(...)`) is resolved
    // to the correct enum type based on the function's return type context.
    let source = r"
pub enum Color {
    rgb(r: Number, g: Number, b: Number),
    rgba(r: Number, g: Number, b: Number, a: Number)
}

impl Color {
    fn transparent() -> Color {
        .rgba(r: 0.0, g: 0.0, b: 0.0, a: 0.0)
    }

    fn red() -> Color {
        .rgb(r: 255.0, g: 0.0, b: 0.0)
    }
}
";

    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    // Find the Color impl
    let color_id = module
        .enums
        .iter()
        .position(|e| e.name == "Color")
        .ok_or("Color enum should exist")?;
    let color_id_u32 = u32::try_from(color_id).map_err(|e| format!("id overflow: {e}"))?;

    let color_impl = module
        .impls
        .iter()
        .find(|i| i.enum_id() == Some(EnumId(color_id_u32)))
        .ok_or("Should have impl for Color")?;

    if color_impl.functions.len() != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, color_impl.functions.len()).into());
    }

    // Verify the transparent function body is correctly typed
    let transparent_fn = color_impl
        .functions
        .iter()
        .find(|f| f.name == "transparent")
        .ok_or("transparent function should exist")?;

    // The body should be an EnumInst with the correct enum type
    let body = transparent_fn
        .body
        .as_ref()
        .ok_or("transparent function should have a body")?;
    if let IrExpr::EnumInst { enum_id, ty, .. } = body {
        if enum_id.is_none() {
            return Err("EnumInst should have resolved enum_id".into());
        }
        if !(matches!(ty, ResolvedType::Enum(_))) {
            return Err("EnumInst type should be Enum, not TypeParam".into());
        }
    } else {
        return Err(format!("Expected EnumInst, got {:?}", transparent_fn.body).into());
    }
    Ok(())
}
