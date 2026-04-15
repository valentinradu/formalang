//! Tests for Token type methods and helper functions
//!
//! Targets: `is_keyword`, `is_type_keyword`, `as_str`, Display

use formalang::lexer::{parse_regex, Lexer, Token};

// =============================================================================
// Token::is_keyword() Tests
// =============================================================================

#[test]
fn test_token_is_keyword_true() -> Result<(), Box<dyn std::error::Error>> {
    let keywords = [
        Token::Trait,
        Token::Struct,
        Token::Impl,
        Token::Enum,
        Token::Module,
        Token::Use,
        Token::Pub,
        Token::Let,
        Token::Mut,
        Token::Extern,
        Token::Match,
        Token::For,
        Token::In,
        Token::If,
        Token::Else,
        Token::True,
        Token::False,
        Token::Nil,
        Token::As,
    ];

    for kw in keywords {
        if !kw.is_keyword() {
            return Err(format!("Expected {kw:?} to be a keyword").into());
        }
    }
    Ok(())
}

#[test]
fn test_token_is_keyword_false() -> Result<(), Box<dyn std::error::Error>> {
    let non_keywords = [
        Token::Dot,
        Token::Colon,
        Token::Comma,
        Token::Plus,
        Token::Minus,
        Token::Star,
        Token::Slash,
        Token::LParen,
        Token::RParen,
        Token::LBrace,
        Token::RBrace,
        Token::String("test".to_string()),
        Token::Number(42.0),
        Token::Ident("name".to_string()),
    ];

    for tok in non_keywords {
        if tok.is_keyword() {
            return Err(format!("Expected {tok:?} to not be a keyword").into());
        }
    }
    Ok(())
}

// =============================================================================
// Token::is_type_keyword() Tests
// =============================================================================

#[test]
fn test_token_is_type_keyword_true() -> Result<(), Box<dyn std::error::Error>> {
    let type_keywords = [
        Token::StringType,
        Token::NumberType,
        Token::BooleanType,
        Token::PathType,
        Token::RegexType,
        Token::NeverType,
    ];

    for tk in type_keywords {
        if !tk.is_type_keyword() {
            return Err(format!("Expected {tk:?} to be a type keyword").into());
        }
    }
    Ok(())
}

#[test]
fn test_token_is_type_keyword_false() -> Result<(), Box<dyn std::error::Error>> {
    let non_type_keywords = [
        Token::Struct,
        Token::Trait,
        Token::Let,
        Token::String("test".to_string()),
        Token::Number(42.0),
        Token::Ident("name".to_string()),
    ];

    for tok in non_type_keywords {
        if tok.is_type_keyword() {
            return Err(format!("Expected {tok:?} to not be a type keyword").into());
        }
    }
    Ok(())
}

// =============================================================================
// Token::as_str() Tests
// =============================================================================

#[test]
fn test_token_as_str_keywords() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (Token::Trait, "trait"),
        (Token::Struct, "struct"),
        (Token::Impl, "impl"),
        (Token::Enum, "enum"),
        (Token::Module, "mod"),
        (Token::Use, "use"),
        (Token::Pub, "pub"),
        (Token::Let, "let"),
        (Token::Mut, "mut"),
        (Token::Extern, "extern"),
        (Token::Match, "match"),
        (Token::For, "for"),
        (Token::In, "in"),
        (Token::If, "if"),
        (Token::Else, "else"),
        (Token::True, "true"),
        (Token::False, "false"),
        (Token::Nil, "nil"),
        (Token::As, "as"),
    ];
    for (tok, expected) in cases {
        let got = tok.as_str();
        if got != expected {
            return Err(format!("expected {expected:?} but got {got:?}").into());
        }
    }
    Ok(())
}

#[test]
fn test_token_as_str_type_keywords() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (Token::StringType, "String"),
        (Token::NumberType, "Number"),
        (Token::BooleanType, "Boolean"),
        (Token::PathType, "Path"),
        (Token::RegexType, "Regex"),
        (Token::NeverType, "Never"),
    ];
    for (tok, expected) in cases {
        let got = tok.as_str();
        if got != expected {
            return Err(format!("expected {expected:?} but got {got:?}").into());
        }
    }
    Ok(())
}

#[test]
fn test_token_as_str_operators() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (Token::Dot, "."),
        (Token::Colon, ":"),
        (Token::DoubleColon, "::"),
        (Token::Comma, ","),
        (Token::Equals, "="),
        (Token::Plus, "+"),
        (Token::Minus, "-"),
        (Token::Star, "*"),
        (Token::Slash, "/"),
        (Token::Percent, "%"),
        (Token::EqEq, "=="),
        (Token::Ne, "!="),
        (Token::Lt, "<"),
        (Token::Gt, ">"),
        (Token::Le, "<="),
        (Token::Ge, ">="),
        (Token::And, "&&"),
        (Token::Or, "||"),
        (Token::Question, "?"),
        (Token::Arrow, "->"),
    ];
    for (tok, expected) in cases {
        let got = tok.as_str();
        if got != expected {
            return Err(format!("expected {expected:?} but got {got:?}").into());
        }
    }
    Ok(())
}

#[test]
fn test_token_as_str_delimiters() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (Token::LParen, "("),
        (Token::RParen, ")"),
        (Token::LBrace, "{"),
        (Token::RBrace, "}"),
        (Token::LBracket, "["),
        (Token::RBracket, "]"),
        (Token::Eof, "<eof>"),
    ];
    for (tok, expected) in cases {
        let got = tok.as_str();
        if got != expected {
            return Err(format!("expected {expected:?} but got {got:?}").into());
        }
    }
    Ok(())
}

#[test]
fn test_token_as_str_complex() -> Result<(), Box<dyn std::error::Error>> {
    // Complex tokens return "<complex token>"
    let cases = [
        Token::String("test".to_string()),
        Token::Number(42.0),
        Token::Ident("name".to_string()),
        Token::Regex("r/test/".to_string()),
        Token::Path("usr/bin".to_string()),
    ];
    for tok in cases {
        let got = tok.as_str();
        if got != "<complex token>" {
            return Err(format!("expected \"<complex token>\" but got {got:?}").into());
        }
    }
    Ok(())
}

// =============================================================================
// Token Display Tests
// =============================================================================

#[test]
fn test_token_display_literals() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (format!("{}", Token::String("test".to_string())), "string"),
        (format!("{}", Token::Number(42.0)), "number"),
        (format!("{}", Token::Regex("r/test/".to_string())), "regex"),
        (format!("{}", Token::Path("usr/bin".to_string())), "path"),
        (
            format!("{}", Token::Ident("name".to_string())),
            "identifier",
        ),
    ];
    for (got, expected) in cases {
        if got != expected {
            return Err(format!("expected {expected:?} but got {got:?}").into());
        }
    }
    Ok(())
}

#[test]
fn test_token_display_keywords() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (format!("{}", Token::Struct), "'struct'"),
        (format!("{}", Token::Trait), "'trait'"),
        (format!("{}", Token::Impl), "'impl'"),
        (format!("{}", Token::Let), "'let'"),
    ];
    for (got, expected) in cases {
        if got != expected {
            return Err(format!("expected {expected:?} but got {got:?}").into());
        }
    }
    Ok(())
}

#[test]
fn test_token_display_operators() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (format!("{}", Token::Plus), "'+'"),
        (format!("{}", Token::Minus), "'-'"),
        (format!("{}", Token::Arrow), "'->'"),
    ];
    for (got, expected) in cases {
        if got != expected {
            return Err(format!("expected {expected:?} but got {got:?}").into());
        }
    }
    Ok(())
}

// =============================================================================
// Tokenizer Tests for Escape Sequences
// =============================================================================

#[test]
fn test_tokenize_string_with_valid_escapes() -> Result<(), Box<dyn std::error::Error>> {
    // Test strings with valid escape sequences
    let tokens = Lexer::tokenize_all("\"line1\\nline2\\ttab\"");
    // Should have the string token with processed escapes
    let has_match = tokens
        .iter()
        .any(|(t, _)| matches!(t, Token::String(s) if s.contains('\n') && s.contains('\t')));
    if !has_match {
        return Err("expected a String token containing newline and tab escapes".into());
    }
    Ok(())
}

#[test]
fn test_tokenize_string_trailing_backslash() -> Result<(), Box<dyn std::error::Error>> {
    // Trailing backslash at end of string
    let tokens = Lexer::tokenize_all("\"test\\\\\"");
    let has_match = tokens
        .iter()
        .any(|(t, _)| matches!(t, Token::String(s) if s == "test\\"));
    if !has_match {
        return Err("expected a String token equal to \"test\\\\\"".into());
    }
    Ok(())
}

// =============================================================================
// parse_regex Tests
// =============================================================================

#[test]
fn test_parse_regex_valid() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_regex("r/hello/gi");
    if result.is_none() {
        return Err("assertion failed".into());
    }
    let (pattern, flags) = result.ok_or("expected Some")?;
    if pattern != "hello" {
        return Err(format!("expected {:?} but got {:?}", "hello", pattern).into());
    }
    if flags != "gi" {
        return Err(format!("expected {:?} but got {:?}", "gi", flags).into());
    }
    Ok(())
}

#[test]
fn test_parse_regex_no_flags() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_regex("r/test/");
    if result.is_none() {
        return Err("assertion failed".into());
    }
    let (pattern, flags) = result.ok_or("expected Some")?;
    if pattern != "test" {
        return Err(format!("expected {:?} but got {:?}", "test", pattern).into());
    }
    if !flags.is_empty() {
        return Err(format!("expected {:?} but got {:?}", "", flags).into());
    }
    Ok(())
}

#[test]
fn test_parse_regex_invalid_prefix() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_regex("/test/");
    if result.is_some() {
        return Err("expected None for invalid prefix".into());
    }
    Ok(())
}

#[test]
fn test_parse_regex_complex_pattern() -> Result<(), Box<dyn std::error::Error>> {
    let result = parse_regex("r/[a-z0-9]+@[a-z]+\\.[a-z]{2,}/i");
    if result.is_none() {
        return Err("assertion failed".into());
    }
    let (pattern, flags) = result.ok_or("expected Some")?;
    if !(pattern.contains("[a-z0-9]")) {
        return Err("assertion failed".into());
    }
    if flags != "i" {
        return Err(format!("expected {:?} but got {:?}", "i", flags).into());
    }
    Ok(())
}

// =============================================================================
// Keyword Regression Tests
// =============================================================================
// These tests ensure keywords match the documentation specification.
// See docs/user/formalang.md for the canonical keyword list.

#[test]
fn test_mod_keyword_not_module() -> Result<(), Box<dyn std::error::Error>> {
    // The module keyword is `mod` (not `module`) per docs/user/formalang.md
    // This test prevents regression to the incorrect `module` keyword.

    // `mod` should tokenize as the Module keyword
    let mod_tokens = Lexer::tokenize_all("mod");
    if !mod_tokens.iter().any(|(t, _)| matches!(t, Token::Module)) {
        return Err("Expected 'mod' to be recognized as Module keyword".into());
    }

    // `module` should NOT be a keyword - it should be an identifier
    let module_tokens = Lexer::tokenize_all("module");
    if module_tokens
        .iter()
        .any(|(t, _)| matches!(t, Token::Module))
    {
        return Err("Expected 'module' to NOT be recognized as Module keyword".into());
    }
    if !module_tokens
        .iter()
        .any(|(t, _)| matches!(t, Token::Ident(s) if s == "module"))
    {
        return Err("Expected 'module' to be an identifier".into());
    }
    Ok(())
}

#[test]
fn test_mod_keyword_in_context() -> Result<(), Box<dyn std::error::Error>> {
    // Verify `mod` works correctly in module definition context
    let source = "mod utils { struct Helper { } }";
    let tokens = Lexer::tokenize_all(source);

    // Should have Module keyword followed by identifier
    let token_types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();

    let first = token_types.first().ok_or("token list is empty")?;
    if !matches!(first, Token::Module) {
        return Err(format!("First token should be Module keyword, got {first:?}").into());
    }

    let second = token_types
        .get(1)
        .ok_or("token list has fewer than 2 elements")?;
    if !matches!(second, Token::Ident(s) if s == "utils") {
        return Err(format!("Second token should be identifier 'utils', got {second:?}").into());
    }
    Ok(())
}
