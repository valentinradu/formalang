//! Tests for Token type methods and helper functions
//!
//! Targets: is_keyword, is_type_keyword, as_str, Display

use formalang::lexer::{Lexer, Token};

// =============================================================================
// Token::is_keyword() Tests
// =============================================================================

#[test]
fn test_token_is_keyword_true() {
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
        Token::Mount,
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
        assert!(kw.is_keyword(), "Expected {:?} to be a keyword", kw);
    }
}

#[test]
fn test_token_is_keyword_false() {
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
        assert!(!tok.is_keyword(), "Expected {:?} to not be a keyword", tok);
    }
}

// =============================================================================
// Token::is_type_keyword() Tests
// =============================================================================

#[test]
fn test_token_is_type_keyword_true() {
    let type_keywords = [
        Token::StringType,
        Token::NumberType,
        Token::BooleanType,
        Token::PathType,
        Token::RegexType,
        Token::NeverType,
    ];

    for tk in type_keywords {
        assert!(
            tk.is_type_keyword(),
            "Expected {:?} to be a type keyword",
            tk
        );
    }
}

#[test]
fn test_token_is_type_keyword_false() {
    let non_type_keywords = [
        Token::Struct,
        Token::Trait,
        Token::Let,
        Token::String("test".to_string()),
        Token::Number(42.0),
        Token::Ident("name".to_string()),
    ];

    for tok in non_type_keywords {
        assert!(
            !tok.is_type_keyword(),
            "Expected {:?} to not be a type keyword",
            tok
        );
    }
}

// =============================================================================
// Token::as_str() Tests
// =============================================================================

#[test]
fn test_token_as_str_keywords() {
    assert_eq!(Token::Trait.as_str(), "trait");
    assert_eq!(Token::Struct.as_str(), "struct");
    assert_eq!(Token::Impl.as_str(), "impl");
    assert_eq!(Token::Enum.as_str(), "enum");
    assert_eq!(Token::Module.as_str(), "mod");
    assert_eq!(Token::Use.as_str(), "use");
    assert_eq!(Token::Pub.as_str(), "pub");
    assert_eq!(Token::Let.as_str(), "let");
    assert_eq!(Token::Mut.as_str(), "mut");
    assert_eq!(Token::Mount.as_str(), "mount");
    assert_eq!(Token::Match.as_str(), "match");
    assert_eq!(Token::For.as_str(), "for");
    assert_eq!(Token::In.as_str(), "in");
    assert_eq!(Token::If.as_str(), "if");
    assert_eq!(Token::Else.as_str(), "else");
    assert_eq!(Token::True.as_str(), "true");
    assert_eq!(Token::False.as_str(), "false");
    assert_eq!(Token::Nil.as_str(), "nil");
    assert_eq!(Token::As.as_str(), "as");
}

#[test]
fn test_token_as_str_type_keywords() {
    assert_eq!(Token::StringType.as_str(), "String");
    assert_eq!(Token::NumberType.as_str(), "Number");
    assert_eq!(Token::BooleanType.as_str(), "Boolean");
    assert_eq!(Token::PathType.as_str(), "Path");
    assert_eq!(Token::RegexType.as_str(), "Regex");
    assert_eq!(Token::NeverType.as_str(), "Never");
}

#[test]
fn test_token_as_str_operators() {
    assert_eq!(Token::Dot.as_str(), ".");
    assert_eq!(Token::Colon.as_str(), ":");
    assert_eq!(Token::DoubleColon.as_str(), "::");
    assert_eq!(Token::Comma.as_str(), ",");
    assert_eq!(Token::Equals.as_str(), "=");
    assert_eq!(Token::Plus.as_str(), "+");
    assert_eq!(Token::Minus.as_str(), "-");
    assert_eq!(Token::Star.as_str(), "*");
    assert_eq!(Token::Slash.as_str(), "/");
    assert_eq!(Token::Percent.as_str(), "%");
    assert_eq!(Token::EqEq.as_str(), "==");
    assert_eq!(Token::Ne.as_str(), "!=");
    assert_eq!(Token::Lt.as_str(), "<");
    assert_eq!(Token::Gt.as_str(), ">");
    assert_eq!(Token::Le.as_str(), "<=");
    assert_eq!(Token::Ge.as_str(), ">=");
    assert_eq!(Token::And.as_str(), "&&");
    assert_eq!(Token::Or.as_str(), "||");
    assert_eq!(Token::Question.as_str(), "?");
    assert_eq!(Token::Arrow.as_str(), "->");
}

#[test]
fn test_token_as_str_delimiters() {
    assert_eq!(Token::LParen.as_str(), "(");
    assert_eq!(Token::RParen.as_str(), ")");
    assert_eq!(Token::LBrace.as_str(), "{");
    assert_eq!(Token::RBrace.as_str(), "}");
    assert_eq!(Token::LBracket.as_str(), "[");
    assert_eq!(Token::RBracket.as_str(), "]");
    assert_eq!(Token::Eof.as_str(), "<eof>");
}

#[test]
fn test_token_as_str_complex() {
    // Complex tokens return "<complex token>"
    assert_eq!(
        Token::String("test".to_string()).as_str(),
        "<complex token>"
    );
    assert_eq!(Token::Number(42.0).as_str(), "<complex token>");
    assert_eq!(Token::Ident("name".to_string()).as_str(), "<complex token>");
    assert_eq!(
        Token::Regex("r/test/".to_string()).as_str(),
        "<complex token>"
    );
    assert_eq!(
        Token::Path("usr/bin".to_string()).as_str(),
        "<complex token>"
    );
}

// =============================================================================
// Token Display Tests
// =============================================================================

#[test]
fn test_token_display_literals() {
    assert_eq!(format!("{}", Token::String("test".to_string())), "string");
    assert_eq!(format!("{}", Token::Number(42.0)), "number");
    assert_eq!(format!("{}", Token::Regex("r/test/".to_string())), "regex");
    assert_eq!(format!("{}", Token::Path("usr/bin".to_string())), "path");
    assert_eq!(
        format!("{}", Token::Ident("name".to_string())),
        "identifier"
    );
}

#[test]
fn test_token_display_keywords() {
    assert_eq!(format!("{}", Token::Struct), "'struct'");
    assert_eq!(format!("{}", Token::Trait), "'trait'");
    assert_eq!(format!("{}", Token::Impl), "'impl'");
    assert_eq!(format!("{}", Token::Let), "'let'");
}

#[test]
fn test_token_display_operators() {
    assert_eq!(format!("{}", Token::Plus), "'+'");
    assert_eq!(format!("{}", Token::Minus), "'-'");
    assert_eq!(format!("{}", Token::Arrow), "'->'");
}

// =============================================================================
// Tokenizer Tests for Escape Sequences
// =============================================================================

#[test]
fn test_tokenize_string_with_valid_escapes() {
    // Test strings with valid escape sequences
    let tokens = Lexer::tokenize_all("\"line1\\nline2\\ttab\"");
    // Should have the string token with processed escapes
    assert!(tokens
        .iter()
        .any(|(t, _)| matches!(t, Token::String(s) if s.contains('\n') && s.contains('\t'))));
}

#[test]
fn test_tokenize_string_trailing_backslash() {
    // Trailing backslash at end of string
    let tokens = Lexer::tokenize_all("\"test\\\\\"");
    assert!(tokens
        .iter()
        .any(|(t, _)| matches!(t, Token::String(s) if s == "test\\")));
}

// =============================================================================
// parse_regex Tests
// =============================================================================

#[test]
fn test_parse_regex_valid() {
    use formalang::lexer::parse_regex;

    let result = parse_regex("r/hello/gi");
    assert!(result.is_some());
    let (pattern, flags) = result.unwrap();
    assert_eq!(pattern, "hello");
    assert_eq!(flags, "gi");
}

#[test]
fn test_parse_regex_no_flags() {
    use formalang::lexer::parse_regex;

    let result = parse_regex("r/test/");
    assert!(result.is_some());
    let (pattern, flags) = result.unwrap();
    assert_eq!(pattern, "test");
    assert_eq!(flags, "");
}

#[test]
fn test_parse_regex_invalid_prefix() {
    use formalang::lexer::parse_regex;

    let result = parse_regex("/test/");
    assert!(result.is_none());
}

#[test]
fn test_parse_regex_complex_pattern() {
    use formalang::lexer::parse_regex;

    let result = parse_regex("r/[a-z0-9]+@[a-z]+\\.[a-z]{2,}/i");
    assert!(result.is_some());
    let (pattern, flags) = result.unwrap();
    assert!(pattern.contains("[a-z0-9]"));
    assert_eq!(flags, "i");
}

// =============================================================================
// Keyword Regression Tests
// =============================================================================
// These tests ensure keywords match the documentation specification.
// See docs/user/formalang.md for the canonical keyword list.

#[test]
fn test_mod_keyword_not_module() {
    // The module keyword is `mod` (not `module`) per docs/user/formalang.md
    // This test prevents regression to the incorrect `module` keyword.

    // `mod` should tokenize as the Module keyword
    let mod_tokens = Lexer::tokenize_all("mod");
    assert!(
        mod_tokens.iter().any(|(t, _)| matches!(t, Token::Module)),
        "Expected 'mod' to be recognized as Module keyword"
    );

    // `module` should NOT be a keyword - it should be an identifier
    let module_tokens = Lexer::tokenize_all("module");
    assert!(
        !module_tokens
            .iter()
            .any(|(t, _)| matches!(t, Token::Module)),
        "Expected 'module' to NOT be recognized as Module keyword"
    );
    assert!(
        module_tokens
            .iter()
            .any(|(t, _)| matches!(t, Token::Ident(s) if s == "module")),
        "Expected 'module' to be an identifier"
    );
}

#[test]
fn test_mod_keyword_in_context() {
    // Verify `mod` works correctly in module definition context
    let source = "mod utils { struct Helper { } }";
    let tokens = Lexer::tokenize_all(source);

    // Should have Module keyword followed by identifier
    let token_types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();

    assert!(
        matches!(&token_types[0], Token::Module),
        "First token should be Module keyword, got {:?}",
        token_types[0]
    );
    assert!(
        matches!(&token_types[1], Token::Ident(s) if s == "utils"),
        "Second token should be identifier 'utils', got {:?}",
        token_types[1]
    );
}
