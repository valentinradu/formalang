//! Lexer and parser correctness tests for Phase 2 fixes.
//!
//! Covers:
//! - Number literals with underscores (`1_000_000`).
//! - Number literals with scientific notation (`1.5e-3`).
//! - Unterminated strings producing `UnterminatedString`.
//! - Invalid characters producing `InvalidCharacter`.
//! - Multiline strings with raw newlines.
//! - Valid and invalid escape sequence handling.
//! - Parser error labels on top-level definitions.

#![allow(clippy::wildcard_enum_match_arm)]

use formalang::lexer::{Lexer, Token};
use formalang::{CompilerError};

// ============================================================================
// Number literals
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
            Token::Number(n) => Some(*n),
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
            Token::Number(n) => Some(*n),
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
            Token::Number(n) => Some(*n),
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
            Token::Number(n) => Some(*n),
            _ => None,
        })
        .ok_or("expected Token::Number")?;
    if (got - 1000.5005_f64).abs() > f64::EPSILON {
        return Err(format!("expected 1000.5005, got {got}").into());
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
