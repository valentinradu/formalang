//! A dictionary key needs a total equality. `F32` and `F64` do not
//! have one: `NaN != NaN`, so a key can never be found again, and
//! `0.0 == -0.0`, so two distinct-looking keys collide. Rust refuses
//! the same thing, because `f64` is not `Eq`.
//!
//! Every other key type is kept. A struct or enum key is a walk over
//! the key's bytes once the layout is known, which is mechanical.

#![expect(
    clippy::expect_used,
    reason = "tests assert compilation fails; expect_err() is the desired panic-on-failure shape"
)]

use formalang::compile_to_ir;
use formalang::error::CompilerError;

const TYPES: &str = "
pub enum Color { red, green }
pub struct Point { x: I32, y: I32 }
";

fn compile_with_key(key: &str) -> Result<(), Vec<CompilerError>> {
    compile_to_ir(&format!("{TYPES}pub let d: [{key}: String] = [:]")).map(|_| ())
}

fn assert_rejected(key: &str) {
    let errors = compile_with_key(key).expect_err("a float key must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CompilerError::FloatDictionaryKey { key_type, .. } if key_type == key
        )),
        "expected FloatDictionaryKey for {key}, got {errors:?}"
    );
}

fn assert_accepted(key: &str) {
    assert!(
        compile_with_key(key).is_ok(),
        "{key} must still work as a dictionary key"
    );
}

#[test]
fn rejects_f64_key() {
    assert_rejected("F64");
}

#[test]
fn rejects_f32_key() {
    assert_rejected("F32");
}

#[test]
fn rejects_a_float_key_nested_in_a_container() {
    let errors = compile_to_ir("pub let d: [[F64: String]] = []")
        .expect_err("a float key is rejected wherever the type appears");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CompilerError::FloatDictionaryKey { .. })),
        "expected FloatDictionaryKey, got {errors:?}"
    );
}

#[test]
fn rejects_a_float_key_in_a_struct_field() {
    let errors = compile_to_ir("pub struct Cache { rates: [F32: String] }")
        .expect_err("a float key is rejected in a field");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CompilerError::FloatDictionaryKey { .. })),
        "expected FloatDictionaryKey, got {errors:?}"
    );
}

#[test]
fn accepts_string_key() {
    assert_accepted("String");
}

#[test]
fn accepts_i32_key() {
    assert_accepted("I32");
}

#[test]
fn accepts_i64_key() {
    assert_accepted("I64");
}

#[test]
fn accepts_boolean_key() {
    assert_accepted("Boolean");
}

#[test]
fn accepts_enum_key() {
    assert_accepted("Color");
}

#[test]
fn accepts_struct_key() {
    assert_accepted("Point");
}

/// A float is fine as a dictionary *value*; only the key needs
/// equality.
#[test]
fn accepts_a_float_value() {
    assert!(
        compile_to_ir("pub let d: [String: F64] = [:]").is_ok(),
        "a float value must still work"
    );
}
