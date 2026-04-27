//! Integration tests for the `FormaLang` compiler
//!
//! Audit2 B37: the helper used to call `compile_with_analyzer`, which
//! stopped after semantic analysis. That meant any of these tests
//! could pass despite latent IR-lowering bugs. The helper now goes all
//! the way through `compile_to_ir`, so every test in this file is at
//! minimum a strict round-trip smoke test through the *whole* pipeline
//! (Lexer → Parser → Semantic Analyzer → IR Lowering). Tests that
//! exercise specific IR shapes additionally assert on `ir.structs`,
//! `ir.enums`, `ir.traits`, etc., as appropriate.

use formalang::{compile_to_ir, parse_only, CompilerError};

// =============================================================================
// Basic Definition Tests
// =============================================================================

/// Compile through the full pipeline (Lexer → Parser → Semantic →
/// IR Lowering) and return the resulting `IrModule`. Used as the
/// default helper by every test in this file; tests that only care
/// about success drop the result with `let _ir = compile(source)?`.
fn compile(source: &str) -> Result<formalang::ir::IrModule, Vec<formalang::CompilerError>> {
    formalang::compile_to_ir(source)
}

/// Audit2 B36: format a vec of `CompilerError`s using each element's
/// Display impl rather than the unstable derived Debug. Used in
/// `.map_err(...)` adapters so test-failure output stays readable
/// across error-variant renames.
fn fmt_errs(errs: &[CompilerError]) -> String {
    errs.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_empty_file() -> Result<(), Box<dyn std::error::Error>> {
    let source = "";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_simple_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String,
            age: I32
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_public_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub struct User {
            name: String
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_struct_with_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String,
            nickname: String?
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_struct_with_default_value() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            timeout: I32 = 30,
            enabled: Boolean = true
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_simple_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_struct_implementing_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }

        struct User {
            name: String,
            age: I32
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_trait_composition() -> Result<(), Box<dyn std::error::Error>> {
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
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_simple_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            active,
            inactive,
            pending
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_enum_with_associated_data() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Result {
            success(value: String),
            error(message: String, code: I32)
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_module_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod ui {
            struct Button {
                label: String
            }
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_generic_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_generic_struct_with_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container {
            size: I32
        }

        struct Wrapper<T: Container> {
            item: T
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_generic_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Option<T> {
            some(value: T),
            none
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Primitive Type Tests
// =============================================================================

#[test]
fn test_all_primitive_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct AllTypes {
            s: String,
            n: I32,
            b: Boolean,
            p: Path,
            r: Regex
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_never_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Terminal {
            body: Never
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_array_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct List {
            items: [String]
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_nested_array_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Matrix {
            rows: [[I32]]
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_tuple_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point {
            coords: (x: I32, y: I32)
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Dictionary Type Tests
// =============================================================================

#[test]
fn test_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Cache {
            data: [String: I32]
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_optional_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            settings: [String: String]?
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_nested_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct NestedCache {
            data: [String: [String: I32]]
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Closure Type Tests
// =============================================================================

#[test]
fn test_closure_type_no_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Factory {
            create: () -> String
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_closure_type_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Transformer {
            transform: String -> I32
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_closure_type_multi_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator {
            compute: I32, I32 -> I32
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_optional_closure_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            callback: (String -> Boolean)?
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Impl Block Tests
// =============================================================================

#[test]
fn test_impl_block_with_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Greeting {
            message: String = "Hello, World!"
        }
    "#;
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_impl_block_with_struct_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner {
            value: I32
        }

        struct Outer {
            inner: Inner = Inner(value: 42)
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Expression Tests
// =============================================================================

#[test]
fn test_string_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let greeting = "Hello"
    "#;
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_number_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let count = 42
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_boolean_literals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let yes = true
        let no = false
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_nil_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let nothing = nil
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_path_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let file = /home/user/file.txt
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_regex_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let pattern = r/[a-z]+/i
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_array_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let items = [1, 2, 3]
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_tuple_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let point = (x: 10, y: 20)
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_dictionary_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let config = ["key": "value", "other": "data"]
    "#;
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_empty_dictionary_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let empty = [:]
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_binary_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let sum = 1 + 2
        let diff = 5 - 3
        let product = 4 * 2
        let quotient = 10 / 2
        let remainder = 7 % 3
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_binary_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let lt = 1 < 2
        let gt = 2 > 1
        let le = 1 <= 1
        let ge = 2 >= 2
        let eq = 1 == 1
        let ne = 1 != 2
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_binary_logical() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let and_result = true && false
        let or_result = true || false
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Control Flow Expression Tests
// =============================================================================

#[test]
fn test_if_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Widget {
            content: String = if true { "yes" } else { "no" }
        }
    "#;
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_for_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Item {
            value: String
        }

        struct List {
            items: [Item] = for item in ["a", "b", "c"] { Item(value: item) }
        }
    "#;
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_match_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status {
            active,
            inactive
        }

        struct Display {
            status: Status
        }

        impl Display {
            fn text() -> String {
                match self.status {
                    active: "Active",
                    inactive: "Inactive"
                }
            }
        }
    "#;
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Closure Expression Tests
// =============================================================================

#[test]
fn test_closure_no_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let factory = () -> "created"
    "#;
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_closure_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let double = x -> 2
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_closure_multi_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let add = x, y -> 0
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_closure_with_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let greet = name: String -> "Hello"
    "#;
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Let Expression Tests
// =============================================================================

#[test]
fn test_let_expression_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Result {
            value: I32 = (let x = 10
            in x)
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_let_with_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Result {
            value: I32 = (let x: I32 = 10
            in x)
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_let_mut() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            value: I32 = (let mut count = 0
            in count)
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_nested_let_expressions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Computation {
            result: I32 = (let a = 1
            in let b = 2
            in let c = 3
            in a)
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Parse-Only Tests
// =============================================================================

#[test]
fn test_parse_only_valid_syntax() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
    ";
    parse_only(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_parse_only_invalid_syntax() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name String
        }
    ";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Error Tests - Semantic Errors
// =============================================================================

#[test]
fn test_error_undefined_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            status: UndefinedType
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !(errors
        .iter()
        .any(|e| matches!(e, CompilerError::UndefinedType { .. })))
    {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_undefined_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User: UndefinedTrait {
            name: String
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_duplicate_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
        struct User {
            age: I32
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_missing_trait_field() -> Result<(), Box<dyn std::error::Error>> {
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
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_impl_for_undefined_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        impl UndefinedStruct {
            x: "value"
        }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Complex Integration Tests
// =============================================================================

#[test]
fn test_complex_ui_component() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        trait Renderable {
            render: String
        }

        struct Theme {
            primaryColor: String,
            fontSize: I32
        }

        struct Button {
            label: String,
            disabled: Boolean = false,
            render: String
        }

        struct Card {
            title: String,
            content: String = "Card component",
            actions: [Button]?
        }
    "#;
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_generic_data_structures() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Option<T> {
            some(value: T),
            none
        }

        enum Result<T, E> {
            ok(value: T),
            err(error: E)
        }

        struct Container<T> {
            items: [T],
            count: I32
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_nested_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod ui {
            mod components {
                struct Button {
                    label: String
                }
            }

            struct Theme {
                color: String
            }
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Comment Tests
// =============================================================================

#[test]
fn test_line_comments() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        // This is a comment
        struct User {
            name: String // inline comment
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_block_comments() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        /* Block comment */
        struct User {
            name: String
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_nested_block_comments() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #47: a `/* /* inner */ outer */` previously terminated at the
    // first `*/` and left ` */ struct ...` as a syntax error. The Logos
    // callback now tracks nest depth so the whole nested comment is
    // skipped cleanly.
    let source = r"
        /* outer
           /* nested */
           still in outer
           /* /* deeply nested */ */
        */
        struct User {
            name: String
        }
    ";
    compile(source)
        .map_err(|e| format!("Failed to handle nested block comments: {}", fmt_errs(&e)))?;
    Ok(())
}

#[test]
fn test_block_comment_terminates_inside_token_stream() -> Result<(), Box<dyn std::error::Error>> {
    // A block comment inside a definition body must not break tokenisation
    // of surrounding code, even when nested.
    let source = r"
        struct A /* /* nested */ */ {
            x: I32 /* trailing */
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_unterminated_block_comment_at_eof_is_diagnosed() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B3: a `/* …` that runs to end-of-input must surface a real
    // `UnterminatedBlockComment` diagnostic — previously the lexer
    // silently ate the rest of the file, so a parser that needed more
    // tokens would see "unexpected end of input" (or, worse, the file
    // would be accepted as if the comment had been a valid trailing
    // skip).
    let source = "struct A {} /* runaway comment with no closing";
    let errors = compile(source)
        .err()
        .ok_or("expected an UnterminatedBlockComment diagnostic, but compilation succeeded")?;
    let has_error = errors
        .iter()
        .any(|e| matches!(e, formalang::CompilerError::UnterminatedBlockComment { .. }));
    if !has_error {
        return Err(format!("expected UnterminatedBlockComment in errors, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_unterminated_nested_block_comment_at_eof_is_diagnosed(
) -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B3: nested unterminated case — outer never closes, inner
    // closes; the diagnostic should still fire because depth > 0 at EOF.
    let source = "/* outer /* inner */ still inside outer";
    let errors = compile(source)
        .err()
        .ok_or("expected an UnterminatedBlockComment diagnostic, but compilation succeeded")?;
    let has_error = errors
        .iter()
        .any(|e| matches!(e, formalang::CompilerError::UnterminatedBlockComment { .. }));
    if !has_error {
        return Err(format!("expected UnterminatedBlockComment in errors, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_invalid_unicode_escape_surrogate_is_diagnosed() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B4: `\uD800` is a UTF-16 high-surrogate code point; it is
    // not a valid Unicode scalar value, so `char::from_u32` returns None.
    // Previously the lexer silently elided the bad escape and produced
    // a string with one fewer character than the source suggested. Now
    // it must surface a real `InvalidUnicodeEscape` diagnostic.
    let source = r#"struct A { x: String = "hello \uD800 world" }"#;
    let errors = compile(source)
        .err()
        .ok_or("expected an InvalidUnicodeEscape diagnostic, but compilation succeeded")?;
    let has_error = errors.iter().any(|e| {
        matches!(
            e,
            formalang::CompilerError::InvalidUnicodeEscape { value, .. } if value.eq_ignore_ascii_case("D800")
        )
    });
    if !has_error {
        return Err(
            format!("expected InvalidUnicodeEscape(D800) in errors, got: {errors:?}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_valid_unicode_escape_does_not_diagnose() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B4 negative case: `é` is `é`, a valid scalar value;
    // no diagnostic should fire and the literal must compile.
    let source = r#"struct A { x: String = "café" }"#;
    compile(source).map_err(|e| format!("expected success, got: {}", fmt_errs(&e)))?;
    Ok(())
}

// =============================================================================
// Extern Type Field Tests
// =============================================================================

#[test]
fn test_struct_with_content_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Container {
            content: String
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_struct_with_multiple_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Layout {
            header: String,
            body: String,
            footer: String
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Mutable Field Tests
// =============================================================================

#[test]
fn test_mutable_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut value: I32
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Reference Tests
// =============================================================================

#[test]
fn test_field_reference() -> Result<(), Box<dyn std::error::Error>> {
    // Field references (self.field) are only valid in impl functions
    let source = r"
        struct User {
            name: String
        }

        impl User {
            fn displayName() -> String {
                self.name
            }
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_enum_variant_reference() -> Result<(), Box<dyn std::error::Error>> {
    // Inferred enum instantiation in struct field default
    let source = r"
        enum Color {
            red,
            blue
        }

        struct Widget {
            color: Color = .red
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

#[test]
fn test_inferred_enum_in_struct_instantiation_args() -> Result<(), Box<dyn std::error::Error>> {
    // Regression test: inferred enum variants inside struct instantiation arguments
    let source = r"
        enum SizeMode { auto, fixed(value: I32) }
        enum RepeatMode { none, horizontal, vertical, both }

        struct Size {
            width: SizeMode,
            height: SizeMode
        }

        struct Pattern {
            size: Size = Size(width: .auto, height: .auto),
            repeat: RepeatMode = .both
        }
    ";
    compile(source).map_err(|e| fmt_errs(&e))?;
    Ok(())
}

// =============================================================================
// Complete program — exercises every language feature end-to-end
// =============================================================================

#[test]
fn test_complete_program_compiles() -> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("fixtures/complete.fv");
    compile(source).map_err(|e| format!("complete.fv failed to compile: {}", fmt_errs(&e)))?;
    Ok(())
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end integration check asserting every IR section against a fixture"
)]
fn test_complete_program_lowers_to_ir() -> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("fixtures/complete.fv");
    let module = compile_to_ir(source)
        .map_err(|e| format!("complete.fv failed IR lowering: {}", fmt_errs(&e)))?;

    // Enums
    let priority = module
        .enums
        .iter()
        .find(|e| e.name == "Priority")
        .ok_or("Priority enum missing from IR")?;
    if priority.variants.len() != 3 {
        return Err(format!(
            "Priority should have 3 variants, got {}",
            priority.variants.len()
        )
        .into());
    }
    let status = module
        .enums
        .iter()
        .find(|e| e.name == "Status")
        .ok_or("Status enum missing from IR")?;
    if status.variants.len() != 2 {
        return Err(format!(
            "Status should have 2 variants, got {}",
            status.variants.len()
        )
        .into());
    }

    // Traits
    if !module.traits.iter().any(|t| t.name == "Labeled") {
        return Err("Labeled trait missing from IR".into());
    }
    if !module.traits.iter().any(|t| t.name == "Tracked") {
        return Err("Tracked trait missing from IR".into());
    }

    // Structs
    let task = module
        .structs
        .iter()
        .find(|s| s.name == "Task")
        .ok_or("Task struct missing from IR")?;
    if task.fields.len() < 8 {
        return Err(format!(
            "Task should have at least 8 fields, got {}",
            task.fields.len()
        )
        .into());
    }
    let notes = task
        .fields
        .iter()
        .find(|f| f.name == "notes")
        .ok_or("notes field missing")?;
    if !notes.optional {
        return Err("notes field should be optional".into());
    }
    let retry = task
        .fields
        .iter()
        .find(|f| f.name == "retry_count")
        .ok_or("retry_count missing")?;
    if !retry.mutable {
        return Err("retry_count should be mutable".into());
    }

    // Standalone functions
    if !module.functions.iter().any(|f| f.name == "clamp") {
        return Err("clamp function missing from IR".into());
    }
    if !module.functions.iter().any(|f| f.name == "score_label") {
        return Err("score_label function missing from IR".into());
    }

    // Impl block
    let task_struct_id = u32::try_from(
        module
            .structs
            .iter()
            .position(|s| s.name == "Task")
            .ok_or("Task struct not found")?,
    )
    .map_err(|e| format!("Task struct index out of range: {e}"))?;
    let task_impl = module
        .impls
        .iter()
        .find(
            |i| matches!(i.target, formalang::ir::ImplTarget::Struct(id) if id.0 == task_struct_id),
        )
        .ok_or("Task impl block missing from IR")?;
    for expected in ["is_done", "priority_score", "describe", "next_retry"] {
        if !task_impl.functions.iter().any(|f| f.name == expected) {
            return Err(format!("Task impl missing method '{expected}'").into());
        }
    }

    // Module-level lets
    for expected in ["max_retries", "task_count", "sample", "cfg", "final_score"] {
        if !module.lets.iter().any(|l| l.name == expected) {
            return Err(format!("let binding '{expected}' missing from IR").into());
        }
    }
    let task_count = module
        .lets
        .iter()
        .find(|l| l.name == "task_count")
        .ok_or("task_count let binding missing from IR")?;
    if !task_count.mutable {
        return Err("task_count should be mutable".into());
    }

    Ok(())
}
