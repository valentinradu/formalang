//! A unary operator checks the type of its operand.
//!
//! `docs/user/expressions.md` gives `-` to numbers and `!` to
//! `Boolean`. The binary operators reject a wrong operand with
//! `InvalidBinaryOp`. The unary operators have no such check today:
//! `-"a"` compiles, and `!1` compiles with the type `Boolean`.

#![expect(
    clippy::panic,
    clippy::expect_used,
    reason = "a test reports its own failure by failing loudly"
)]

use formalang::{compile_to_ir, CompilerError};

/// Compile `source` and assert that the compiler rejects it with an
/// error that names the user's mistake, not with an internal error.
fn assert_rejected(source: &str) {
    match compile_to_ir(source) {
        Ok(_) => panic!("the compiler accepts a wrong unary operand:\n{source}"),
        Err(errors) => assert!(
            !errors
                .iter()
                .any(|e| matches!(e, CompilerError::InternalError { .. })),
            "the compiler rejects the program with an internal error:\n{source}\n{errors:?}"
        ),
    }
}

#[test]
fn negation_of_a_string_is_rejected() {
    assert_rejected("pub let x: String = -\"a\"");
}

#[test]
fn negation_of_a_boolean_is_rejected() {
    assert_rejected("pub let x: Boolean = -true");
}

#[test]
fn negation_of_an_array_is_rejected() {
    assert_rejected("pub let x = -[1]");
}

#[test]
fn negation_of_a_struct_is_rejected() {
    assert_rejected("pub struct P { x: I32 }\npub let x = -P(x: 1)");
}

#[test]
fn logical_not_of_an_integer_is_rejected() {
    assert_rejected("pub let x: Boolean = !1");
}

#[test]
fn logical_not_of_a_string_is_rejected() {
    assert_rejected("pub let x: Boolean = !\"a\"");
}

#[test]
fn logical_not_of_a_float_is_rejected() {
    assert_rejected("pub let x = !1.5");
}

#[test]
fn logical_not_of_a_parameter_of_the_wrong_type_is_rejected() {
    assert_rejected("pub fn f(n: I32) -> Boolean { !n }");
}

#[test]
fn negation_of_a_parameter_of_the_wrong_type_is_rejected() {
    assert_rejected("pub fn f(s: String) -> String { -s }");
}

/// The message of a signature mismatch must show the two types that
/// differ. Today it shows the trait's return type twice: "expected
/// I32, found I32", for a parameter that is a `String`.
#[test]
fn a_trait_signature_mismatch_names_the_types_that_differ() {
    let source = "pub trait T { fn f(self, a: I32) -> I32 }\n\
                  pub struct S { v: I32 }\n\
                  impl T for S { fn f(self, a: String) -> I32 { 1 } }\n";
    let errors = compile_to_ir(source).expect_err("the parameter types differ");
    let mismatch = errors
        .iter()
        .find(|e| matches!(e, CompilerError::TraitMethodSignatureMismatch { .. }))
        .unwrap_or_else(|| panic!("no TraitMethodSignatureMismatch: {errors:?}"));
    let message = mismatch.to_string();
    assert!(
        message.contains("String"),
        "the message does not name the parameter type that differs: {message}"
    );
}
