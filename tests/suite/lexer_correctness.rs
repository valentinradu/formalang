//! Lexer and parser correctness tests for Phase 2 fixes.
//!
//! Covers:
//! - I32 literals with underscores (`1_000_000`).
//! - I32 literals with scientific notation (`1.5e-3`).
//! - Unterminated strings producing `UnterminatedString`.
//! - Invalid characters producing `InvalidCharacter`.
//! - Multiline strings with raw newlines.
//! - Valid and invalid escape sequence handling.
//! - Parser error labels on top-level definitions.

#![allow(clippy::wildcard_enum_match_arm)]

use formalang::lexer::{Lexer, Token};
use formalang::CompilerError;

// ============================================================================
// I32 literals
// ============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn tokenize_number_with_underscores() -> Result<(), Box<dyn std::error::Error>> {
    let (tokens, errors) = Lexer::tokenize_all_with_errors("1_000_000");
    if !errors.is_empty() {
        return Err(format!("expected no errors, got {errors:?}").into());
    }
    let got = tokens
        .iter()
        .find_map(|(t, _)| match t {
            Token::Number(n) => Some(n.value.as_f64()),
            _ => None,
        })
        .ok_or("expected Token::Number")?;
    if (got - 1_000_000.0_f64).abs() > f64::EPSILON {
        return Err(format!("expected 1000000.0, got {got}").into());
    }
    Ok(())
}

#[test]
fn tokenize_number_scientific_notation() -> Result<(), Box<dyn std::error::Error>> {
    let (tokens, errors) = Lexer::tokenize_all_with_errors("1.5e-3");
    if !errors.is_empty() {
        return Err(format!("expected no errors, got {errors:?}").into());
    }
    let got = tokens
        .iter()
        .find_map(|(t, _)| match t {
            Token::Number(n) => Some(n.value.as_f64()),
            _ => None,
        })
        .ok_or("expected Token::Number")?;
    if (got - 0.0015_f64).abs() > 1e-9 {
        return Err(format!("expected 0.0015, got {got}").into());
    }
    Ok(())
}

#[test]
fn tokenize_number_scientific_notation_positive_exponent() -> Result<(), Box<dyn std::error::Error>>
{
    let (tokens, errors) = Lexer::tokenize_all_with_errors("2E+10");
    if !errors.is_empty() {
        return Err(format!("expected no errors, got {errors:?}").into());
    }
    let got = tokens
        .iter()
        .find_map(|(t, _)| match t {
            Token::Number(n) => Some(n.value.as_f64()),
            _ => None,
        })
        .ok_or("expected Token::Number")?;
    if (got - 2e10_f64).abs() > 1e-3 {
        return Err(format!("expected 2e10, got {got}").into());
    }
    Ok(())
}

#[test]
fn tokenize_number_mixed_underscore_and_decimal() -> Result<(), Box<dyn std::error::Error>> {
    let (tokens, errors) = Lexer::tokenize_all_with_errors("1_000.500_5");
    if !errors.is_empty() {
        return Err(format!("expected no errors, got {errors:?}").into());
    }
    let got = tokens
        .iter()
        .find_map(|(t, _)| match t {
            Token::Number(n) => Some(n.value.as_f64()),
            _ => None,
        })
        .ok_or("expected Token::Number")?;
    if (got - 1000.5005_f64).abs() > f64::EPSILON {
        return Err(format!("expected 1000.5005, got {got}").into());
    }
    Ok(())
}

#[test]
fn tokenize_number_with_width_tag_suffix() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::NumericSuffix;

    let cases = [
        ("42I32", 42.0, NumericSuffix::I32),
        (
            "9_223_372_036_854_775I64",
            9_223_372_036_854_775.0,
            NumericSuffix::I64,
        ),
        ("2.5F32", 2.5, NumericSuffix::F32),
        ("1.5e-3F64", 0.0015, NumericSuffix::F64),
    ];
    for (source, expected_value, expected_suffix) in cases {
        let (tokens, errors) = Lexer::tokenize_all_with_errors(source);
        if !errors.is_empty() {
            return Err(format!("{source}: expected no errors, got {errors:?}").into());
        }
        let lit = tokens
            .iter()
            .find_map(|(t, _)| match t {
                Token::Number(n) => Some(*n),
                _ => None,
            })
            .ok_or_else(|| format!("{source}: expected Token::Number"))?;
        if (lit.value.as_f64() - expected_value).abs() > 1e-9 {
            return Err(format!(
                "{source}: expected value {expected_value}, got {}",
                lit.value.as_f64()
            )
            .into());
        }
        if lit.suffix != Some(expected_suffix) {
            return Err(format!(
                "{source}: expected suffix {expected_suffix:?}, got {:?}",
                lit.suffix
            )
            .into());
        }
    }
    Ok(())
}

#[test]
fn tokenize_unsuffixed_number_has_no_suffix() -> Result<(), Box<dyn std::error::Error>> {
    let (tokens, errors) = Lexer::tokenize_all_with_errors("42");
    if !errors.is_empty() {
        return Err(format!("expected no errors, got {errors:?}").into());
    }
    let lit = tokens
        .iter()
        .find_map(|(t, _)| match t {
            Token::Number(n) => Some(*n),
            _ => None,
        })
        .ok_or("expected Token::Number")?;
    if lit.suffix.is_some() {
        return Err(format!("expected no suffix, got {:?}", lit.suffix).into());
    }
    Ok(())
}

// ============================================================================
// Unterminated string / invalid character
// ============================================================================

#[test]
fn unterminated_string_produces_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"let s = "hello"#;
    let (_, errors) = Lexer::tokenize_all_with_errors(source);
    let has_unterminated = errors
        .iter()
        .any(|e| matches!(e, CompilerError::UnterminatedString { .. }));
    if !has_unterminated {
        return Err(format!("expected UnterminatedString error, got {errors:?}").into());
    }
    Ok(())
}

#[test]
fn invalid_character_produces_error() -> Result<(), Box<dyn std::error::Error>> {
    // `@` is not a valid FormaLang token.
    let source = "let x = @";
    let (_, errors) = Lexer::tokenize_all_with_errors(source);
    let has_invalid = errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidCharacter { character: '@', .. }));
    if !has_invalid {
        return Err(format!("expected InvalidCharacter '@', got {errors:?}").into());
    }
    Ok(())
}

#[test]
fn compile_surfaces_lexer_errors() -> Result<(), Box<dyn std::error::Error>> {
    // Previously the lexer would silently drop `@`; now `compile` must surface it.
    let source = "let x = @";
    let Err(errors) = compile(source) else {
        return Err("expected compile to fail".into());
    };
    let has_invalid = errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidCharacter { character: '@', .. }));
    if !has_invalid {
        return Err(format!("expected InvalidCharacter, got {errors:?}").into());
    }
    Ok(())
}

// ============================================================================
// Multi-line strings with raw newlines
// ============================================================================

#[test]
fn multiline_string_allows_raw_newlines() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = \"\"\"\nfirst line\nsecond line\n\"\"\"";
    let (tokens, errors) = Lexer::tokenize_all_with_errors(source);
    if !errors.is_empty() {
        return Err(format!("expected no lexer errors, got {errors:?}").into());
    }
    let found = tokens.iter().any(|(t, _)| match t {
        Token::String(s) => s.contains("first line") && s.contains("second line"),
        _ => false,
    });
    if !found {
        return Err("expected multiline String token containing both lines".into());
    }
    Ok(())
}

// ============================================================================
// Escape sequences (valid / invalid)
// ============================================================================

#[test]
fn valid_escape_sequences_produce_string_token() -> Result<(), Box<dyn std::error::Error>> {
    // Spec-valid escapes: \" \\ \n \t \r \uXXXX
    let source = "\"a\\\"b\\\\c\\nd\\te\\rf\\u0041\"";
    let (tokens, errors) = Lexer::tokenize_all_with_errors(source);
    if !errors.is_empty() {
        return Err(format!("expected no errors, got {errors:?}").into());
    }
    let s = tokens
        .iter()
        .find_map(|(t, _)| match t {
            Token::String(s) => Some(s.clone()),
            _ => None,
        })
        .ok_or("expected Token::String")?;
    // A is 'A'
    let expected = "a\"b\\c\nd\te\rfA";
    if s != expected {
        return Err(format!("expected {expected:?}, got {s:?}").into());
    }
    Ok(())
}

#[test]
fn invalid_escape_rejects_string() -> Result<(), Box<dyn std::error::Error>> {
    // `\q` is not a valid escape in FormaLang. The regex should not match the
    // whole string literal, so the lexer emits an error rather than producing
    // a String token.
    let source = "\"bad\\qescape\"";
    let (tokens, errors) = Lexer::tokenize_all_with_errors(source);
    let has_string = tokens.iter().any(|(t, _)| matches!(t, Token::String(_)));
    if has_string && errors.is_empty() {
        return Err("expected invalid-escape string to be rejected".into());
    }
    Ok(())
}

// ============================================================================
// Parser error labels
// ============================================================================

#[test]
fn parser_mislabeled_keyword_produces_labelled_error() -> Result<(), Box<dyn std::error::Error>> {
    // `strct` is a typo — parser should reject with a label mentioning the
    // definition keywords it expected.
    let source = "pub strct Foo {}";
    let Err(errors) = compile(source) else {
        return Err("expected compilation to fail".into());
    };
    let message = errors
        .iter()
        .find_map(|e| match e {
            CompilerError::ParseError { message, .. } => Some(message.clone()),
            _ => None,
        })
        .ok_or_else(|| format!("expected ParseError, got {errors:?}"))?;
    if !message.contains("expected") {
        return Err(format!("expected message to contain 'expected', got {message:?}").into());
    }
    // The label-aware parser enumerates the keywords that could have appeared
    // instead of 'strct' — make sure at least one definition keyword is named.
    if !(message.contains("struct") || message.contains("definition")) {
        return Err(format!(
            "expected message to mention 'struct' or 'definition', got {message:?}"
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Byte-order mark
// =============================================================================

/// A leading UTF-8 byte-order mark is not part of the program.
///
/// Several editors write one, and the user cannot see it. Reporting it
/// as an invalid character points them at a character that is not on
/// their screen, so the lexer skips it.
#[test]
fn a_leading_byte_order_mark_is_skipped() -> Result<(), Box<dyn std::error::Error>> {
    let source = "\u{feff}pub struct A { a: I32 }";
    formalang::compile_to_ir(source).map_err(|e| format!("a BOM broke the compile: {e:?}"))?;
    Ok(())
}

/// Skipping the mark must not move any span: a diagnostic snippet has
/// to line up with the original bytes.
#[test]
fn a_byte_order_mark_does_not_shift_spans() -> Result<(), Box<dyn std::error::Error>> {
    let plain = "pub struct A { a: I32 }";
    let marked = format!("\u{feff}{plain}");

    let (plain_tokens, _) = Lexer::tokenize_all_with_errors(plain);
    let (marked_tokens, _) = Lexer::tokenize_all_with_errors(&marked);

    if plain_tokens.len() != marked_tokens.len() {
        return Err("the mark changed the token count".into());
    }
    let bom_len = '\u{feff}'.len_utf8();
    for (i, ((_, plain_span), (_, marked_span))) in
        plain_tokens.iter().zip(marked_tokens.iter()).enumerate()
    {
        if marked_span.start.offset != plain_span.start.offset + bom_len {
            return Err(format!(
                "token {i}: expected the mark to shift the offset by {bom_len}, \
                 got {} against {}",
                marked_span.start.offset, plain_span.start.offset
            )
            .into());
        }
        if marked_span.start.line != plain_span.start.line {
            return Err(format!("token {i}: the mark changed the line number").into());
        }
    }
    Ok(())
}

/// A mark anywhere but the start stays an error: there it is a stray
/// zero-width character, not an encoding marker.
#[test]
fn a_byte_order_mark_inside_the_source_is_an_error() {
    let source = "pub struct A { a\u{feff}: I32 }";
    let (_, errors) = Lexer::tokenize_all_with_errors(source);
    assert!(
        !errors.is_empty(),
        "a stray zero-width character in the middle of a file must be reported"
    );
}

// =============================================================================
// Numeric literal range
// =============================================================================

/// A float literal that overflows `F64` must be rejected.
///
/// `f64::from_str` reports overflow as an infinity rather than an
/// error, so `1e400` used to compile. An infinity has no JSON form,
/// which made the serialised `IrModule` impossible to decode.
#[test]
fn a_float_literal_that_overflows_is_rejected() {
    for source in [
        "pub fn f() -> F64 { 1e400 }",
        "pub fn f() -> F64 { -1e400 }",
        "pub fn f() -> F64 { 1.7976931348623159e308 }",
    ] {
        assert!(
            formalang::compile_to_ir(source).is_err(),
            "the compiler accepted a float literal that does not fit in F64: {source}"
        );
    }
}

/// A float literal that underflows keeps IEEE 754 behaviour and
/// becomes zero. Every other language does the same, so rejecting it
/// would surprise.
#[test]
fn a_float_literal_that_underflows_becomes_zero() -> Result<(), Box<dyn std::error::Error>> {
    formalang::compile_to_ir("pub fn f() -> F64 { 1e-400 }")
        .map_err(|e| format!("an underflowing literal must still compile: {e:?}"))?;
    Ok(())
}

/// Whatever the compiler accepts must survive the IR JSON round trip.
/// That format is the contract with external consumers.
#[test]
fn every_accepted_float_literal_round_trips_through_json() -> Result<(), Box<dyn std::error::Error>>
{
    for source in [
        "pub fn f() -> F64 { 1e-400 }",
        "pub fn f() -> F64 { 0.1 }",
        "pub fn f() -> F64 { 1.7976931348623157e308 }",
        "pub fn f() -> F64 { -1.7976931348623157e308 }",
    ] {
        let Ok(module) = formalang::compile_to_ir(source) else {
            continue;
        };
        let json = serde_json::to_string(&module)
            .map_err(|e| format!("{source}: the IR must serialise: {e}"))?;
        serde_json::from_str::<formalang::IrModule>(&json)
            .map_err(|e| format!("{source}: the IR must decode again: {e}"))?;
    }
    Ok(())
}
