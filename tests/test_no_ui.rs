//! Tests for UI-layer removal (#5)
//!
//! Verifies that all UI/mount concepts are gone from the language surface.

use formalang::{compile, parse_only};

// =============================================================================
// mount keyword is gone
// =============================================================================

#[test]
fn test_mount_keyword_rejected_in_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait View {
    mount body: View
}
";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error for 'mount' keyword in trait".into());
    }
    Ok(())
}

#[test]
fn test_mount_keyword_rejected_in_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Button {
    label: String,
    mount body: String
}
";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error for 'mount' keyword in struct".into());
    }
    Ok(())
}

#[test]
fn test_mount_keyword_rejected_as_field_name() -> Result<(), Box<dyn std::error::Error>> {
    // 'mount' is no longer a keyword; it can be used as a regular field name
    let source = r"
struct Foo {
    mount: String
}
";
    compile(source).map_err(|e| format!("unexpected error: {e:?}"))?;
    Ok(())
}

// =============================================================================
// No inline trait conformance on structs
// =============================================================================

#[test]
fn test_struct_colon_trait_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Printable {
    name: String
}
struct Doc: Printable {
    name: String
}
";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error: struct colon-trait conformance is removed".into());
    }
    Ok(())
}

#[test]
fn test_struct_multi_trait_conformance_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait A { x: Number }
trait B { y: Number }
struct Foo: A + B {
    x: Number,
    y: Number
}
";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error: struct colon-multi-trait is removed".into());
    }
    Ok(())
}

// =============================================================================
// Trait inheritance still works (trait A: B + C)
// =============================================================================

#[test]
fn test_trait_inheritance_still_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Base {
    x: Number
}
trait Extended: Base {
    y: Number
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// GPU types are gone
// =============================================================================

#[test]
fn test_f32_type_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // f32 is no longer a built-in type; using it causes a semantic UndefinedType error
    let source = r"
struct Shader {
    value: f32
}
";
    let result = compile(source);
    if result.is_ok() {
        return Err("expected error: f32 type is not a built-in".into());
    }
    Ok(())
}

#[test]
fn test_i32_type_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // i32 is no longer a built-in type; using it causes a semantic UndefinedType error
    let source = r"
struct Shader {
    value: i32
}
";
    let result = compile(source);
    if result.is_ok() {
        return Err("expected error: i32 type is not a built-in".into());
    }
    Ok(())
}

#[test]
fn test_u32_type_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // u32 is no longer a built-in type; using it causes a semantic UndefinedType error
    let source = r"
struct Shader {
    value: u32
}
";
    let result = compile(source);
    if result.is_ok() {
        return Err("expected error: u32 type is not a built-in".into());
    }
    Ok(())
}

#[test]
fn test_vec2_type_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // vec2 is no longer a built-in type; using it causes a semantic UndefinedType error
    let source = r"
struct Pos {
    value: vec2
}
";
    let result = compile(source);
    if result.is_ok() {
        return Err("expected error: vec2 type is not a built-in".into());
    }
    Ok(())
}

#[test]
fn test_vec3_type_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // vec3 is no longer a built-in type; using it causes a semantic UndefinedType error
    let source = r"
struct Color {
    rgb: vec3
}
";
    let result = compile(source);
    if result.is_ok() {
        return Err("expected error: vec3 type is not a built-in".into());
    }
    Ok(())
}

#[test]
fn test_mat4_type_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // mat4 is no longer a built-in type; using it causes a semantic UndefinedType error
    let source = r"
struct Transform {
    matrix: mat4
}
";
    let result = compile(source);
    if result.is_ok() {
        return Err("expected error: mat4 type is not a built-in".into());
    }
    Ok(())
}

#[test]
fn test_unsigned_int_literal_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
let x: Number = 42u
";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error: unsigned int literal suffix 'u' is removed".into());
    }
    Ok(())
}

#[test]
fn test_signed_int_literal_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
let x: Number = -3i
";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error: signed int literal suffix 'i' is removed".into());
    }
    Ok(())
}

// =============================================================================
// Struct with no mount fields or inline traits parses fine
// =============================================================================

#[test]
fn test_plain_struct_still_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
pub struct Point {
    x: Number,
    y: Number
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_plain_trait_still_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
pub trait Shape {
    area: Number
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}
