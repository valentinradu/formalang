//! Lexer tests
//!
//! Tests for the Lexer and Token functionality

use formalang::lexer::{Lexer, Token};

// =============================================================================
// Basic Token Tests
// =============================================================================

#[test]
fn test_simple_tokens() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Test { }";
    let tokens = Lexer::tokenize_all(source);
    if tokens.is_empty() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_empty_source() -> Result<(), Box<dyn std::error::Error>> {
    let source = "";
    let tokens = Lexer::tokenize_all(source);
    if !tokens.is_empty() {
        return Err(format!("expected no tokens for empty source, got {tokens:?}").into());
    }
    Ok(())
}

#[test]
fn test_only_whitespace() -> Result<(), Box<dyn std::error::Error>> {
    let source = "    ";
    let tokens = Lexer::tokenize_all(source);
    // Whitespace should be skipped
    if !tokens.is_empty() {
        return Err(format!(
            "expected whitespace-only source to produce no tokens, got {tokens:?}"
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Keyword Token Tests
// =============================================================================

#[test]
fn test_struct_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct";
    let tokens = Lexer::tokenize_all(source);
    let has_struct = tokens.iter().any(|(t, _)| matches!(t, Token::Struct));
    if !has_struct {
        return Err("Expected Struct token".into());
    }
    Ok(())
}

#[test]
fn test_trait_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait";
    let tokens = Lexer::tokenize_all(source);
    let has_trait = tokens.iter().any(|(t, _)| matches!(t, Token::Trait));
    if !has_trait {
        return Err("Expected Trait token".into());
    }
    Ok(())
}

#[test]
fn test_enum_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum";
    let tokens = Lexer::tokenize_all(source);
    let has_enum = tokens.iter().any(|(t, _)| matches!(t, Token::Enum));
    if !has_enum {
        return Err("Expected Enum token".into());
    }
    Ok(())
}

#[test]
fn test_impl_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "impl";
    let tokens = Lexer::tokenize_all(source);
    let has_impl = tokens.iter().any(|(t, _)| matches!(t, Token::Impl));
    if !has_impl {
        return Err("Expected Impl token".into());
    }
    Ok(())
}

#[test]
fn test_module_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "mod";
    let tokens = Lexer::tokenize_all(source);
    let has_module = tokens.iter().any(|(t, _)| matches!(t, Token::Module));
    if !has_module {
        return Err("Expected Module token".into());
    }
    Ok(())
}

#[test]
fn test_use_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "use";
    let tokens = Lexer::tokenize_all(source);
    let has_use = tokens.iter().any(|(t, _)| matches!(t, Token::Use));
    if !has_use {
        return Err("Expected Use token".into());
    }
    Ok(())
}

#[test]
fn test_pub_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub";
    let tokens = Lexer::tokenize_all(source);
    let has_pub = tokens.iter().any(|(t, _)| matches!(t, Token::Pub));
    if !has_pub {
        return Err("Expected Pub token".into());
    }
    Ok(())
}

#[test]
fn test_let_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let";
    let tokens = Lexer::tokenize_all(source);
    let has_let = tokens.iter().any(|(t, _)| matches!(t, Token::Let));
    if !has_let {
        return Err("Expected Let token".into());
    }
    Ok(())
}

#[test]
fn test_mut_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "mut";
    let tokens = Lexer::tokenize_all(source);
    let has_mut = tokens.iter().any(|(t, _)| matches!(t, Token::Mut));
    if !has_mut {
        return Err("Expected Mut token".into());
    }
    Ok(())
}

#[test]
fn test_if_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "if";
    let tokens = Lexer::tokenize_all(source);
    let has_if = tokens.iter().any(|(t, _)| matches!(t, Token::If));
    if !has_if {
        return Err("Expected If token".into());
    }
    Ok(())
}

#[test]
fn test_else_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "else";
    let tokens = Lexer::tokenize_all(source);
    let has_else = tokens.iter().any(|(t, _)| matches!(t, Token::Else));
    if !has_else {
        return Err("Expected Else token".into());
    }
    Ok(())
}

#[test]
fn test_for_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "for";
    let tokens = Lexer::tokenize_all(source);
    let has_for = tokens.iter().any(|(t, _)| matches!(t, Token::For));
    if !has_for {
        return Err("Expected For token".into());
    }
    Ok(())
}

#[test]
fn test_in_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "in";
    let tokens = Lexer::tokenize_all(source);
    let has_in = tokens.iter().any(|(t, _)| matches!(t, Token::In));
    if !has_in {
        return Err("Expected In token".into());
    }
    Ok(())
}

#[test]
fn test_match_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "match";
    let tokens = Lexer::tokenize_all(source);
    let has_match = tokens.iter().any(|(t, _)| matches!(t, Token::Match));
    if !has_match {
        return Err("Expected Match token".into());
    }
    Ok(())
}

// =============================================================================
// Literal Token Tests
// =============================================================================

#[test]
fn test_string_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = "\"hello world\"";
    let tokens = Lexer::tokenize_all(source);
    let has_string = tokens.iter().any(|(t, _)| matches!(t, Token::String(_)));
    if !has_string {
        return Err("Expected String token".into());
    }
    Ok(())
}

#[test]
fn test_number_literal_integer() -> Result<(), Box<dyn std::error::Error>> {
    let source = "42";
    let tokens = Lexer::tokenize_all(source);
    let has_number = tokens.iter().any(|(t, _)| matches!(t, Token::Number(_)));
    if !has_number {
        return Err("Expected I32 token".into());
    }
    Ok(())
}

#[test]
fn test_number_literal_float() -> Result<(), Box<dyn std::error::Error>> {
    let source = "3.14";
    let tokens = Lexer::tokenize_all(source);
    let has_number = tokens.iter().any(|(t, _)| matches!(t, Token::Number(_)));
    if !has_number {
        return Err("Expected I32 token for float".into());
    }
    Ok(())
}

#[test]
fn test_true_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = "true";
    let tokens = Lexer::tokenize_all(source);
    let has_true = tokens.iter().any(|(t, _)| matches!(t, Token::True));
    if !has_true {
        return Err("Expected True token".into());
    }
    Ok(())
}

#[test]
fn test_false_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = "false";
    let tokens = Lexer::tokenize_all(source);
    let has_false = tokens.iter().any(|(t, _)| matches!(t, Token::False));
    if !has_false {
        return Err("Expected False token".into());
    }
    Ok(())
}

#[test]
fn test_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let source = "myVariable";
    let tokens = Lexer::tokenize_all(source);
    let has_ident = tokens.iter().any(|(t, _)| matches!(t, Token::Ident(_)));
    if !has_ident {
        return Err("Expected Ident token".into());
    }
    Ok(())
}

// =============================================================================
// Operator Token Tests
// =============================================================================

#[test]
fn test_plus_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "+";
    let tokens = Lexer::tokenize_all(source);
    let has_plus = tokens.iter().any(|(t, _)| matches!(t, Token::Plus));
    if !has_plus {
        return Err("Expected Plus token".into());
    }
    Ok(())
}

#[test]
fn test_minus_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "-";
    let tokens = Lexer::tokenize_all(source);
    let has_minus = tokens.iter().any(|(t, _)| matches!(t, Token::Minus));
    if !has_minus {
        return Err("Expected Minus token".into());
    }
    Ok(())
}

#[test]
fn test_star_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "*";
    let tokens = Lexer::tokenize_all(source);
    let has_star = tokens.iter().any(|(t, _)| matches!(t, Token::Star));
    if !has_star {
        return Err("Expected Star token".into());
    }
    Ok(())
}

#[test]
fn test_slash_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "/";
    let tokens = Lexer::tokenize_all(source);
    let has_slash = tokens.iter().any(|(t, _)| matches!(t, Token::Slash));
    if !has_slash {
        return Err("Expected Slash token".into());
    }
    Ok(())
}

#[test]
fn test_equals_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "=";
    let tokens = Lexer::tokenize_all(source);
    let has_equals = tokens.iter().any(|(t, _)| matches!(t, Token::Equals));
    if !has_equals {
        return Err("Expected Equals token".into());
    }
    Ok(())
}

#[test]
fn test_eqeq_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "==";
    let tokens = Lexer::tokenize_all(source);
    let has_eqeq = tokens.iter().any(|(t, _)| matches!(t, Token::EqEq));
    if !has_eqeq {
        return Err("Expected EqEq token".into());
    }
    Ok(())
}

#[test]
fn test_ne_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "!=";
    let tokens = Lexer::tokenize_all(source);
    let has_ne = tokens.iter().any(|(t, _)| matches!(t, Token::Ne));
    if !has_ne {
        return Err("Expected Ne token".into());
    }
    Ok(())
}

#[test]
fn test_lt_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "<";
    let tokens = Lexer::tokenize_all(source);
    let has_lt = tokens.iter().any(|(t, _)| matches!(t, Token::Lt));
    if !has_lt {
        return Err("Expected Lt token".into());
    }
    Ok(())
}

#[test]
fn test_gt_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = ">";
    let tokens = Lexer::tokenize_all(source);
    let has_gt = tokens.iter().any(|(t, _)| matches!(t, Token::Gt));
    if !has_gt {
        return Err("Expected Gt token".into());
    }
    Ok(())
}

#[test]
fn test_le_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "<=";
    let tokens = Lexer::tokenize_all(source);
    let has_le = tokens.iter().any(|(t, _)| matches!(t, Token::Le));
    if !has_le {
        return Err("Expected Le token".into());
    }
    Ok(())
}

#[test]
fn test_ge_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = ">=";
    let tokens = Lexer::tokenize_all(source);
    let has_ge = tokens.iter().any(|(t, _)| matches!(t, Token::Ge));
    if !has_ge {
        return Err("Expected Ge token".into());
    }
    Ok(())
}

#[test]
fn test_and_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "&&";
    let tokens = Lexer::tokenize_all(source);
    let has_and = tokens.iter().any(|(t, _)| matches!(t, Token::And));
    if !has_and {
        return Err("Expected And token".into());
    }
    Ok(())
}

#[test]
fn test_or_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "||";
    let tokens = Lexer::tokenize_all(source);
    let has_or = tokens.iter().any(|(t, _)| matches!(t, Token::Or));
    if !has_or {
        return Err("Expected Or token".into());
    }
    Ok(())
}

#[test]
fn test_percent_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = "%";
    let tokens = Lexer::tokenize_all(source);
    let has_percent = tokens.iter().any(|(t, _)| matches!(t, Token::Percent));
    if !has_percent {
        return Err("Expected Percent token".into());
    }
    Ok(())
}

// =============================================================================
// Punctuation Token Tests
// =============================================================================

#[test]
fn test_lbrace() -> Result<(), Box<dyn std::error::Error>> {
    let source = "{";
    let tokens = Lexer::tokenize_all(source);
    let has_lbrace = tokens.iter().any(|(t, _)| matches!(t, Token::LBrace));
    if !has_lbrace {
        return Err("Expected LBrace token".into());
    }
    Ok(())
}

#[test]
fn test_rbrace() -> Result<(), Box<dyn std::error::Error>> {
    let source = "}";
    let tokens = Lexer::tokenize_all(source);
    let has_rbrace = tokens.iter().any(|(t, _)| matches!(t, Token::RBrace));
    if !has_rbrace {
        return Err("Expected RBrace token".into());
    }
    Ok(())
}

#[test]
fn test_lbracket() -> Result<(), Box<dyn std::error::Error>> {
    let source = "[";
    let tokens = Lexer::tokenize_all(source);
    let has_lbracket = tokens.iter().any(|(t, _)| matches!(t, Token::LBracket));
    if !has_lbracket {
        return Err("Expected LBracket token".into());
    }
    Ok(())
}

#[test]
fn test_rbracket() -> Result<(), Box<dyn std::error::Error>> {
    let source = "]";
    let tokens = Lexer::tokenize_all(source);
    let has_rbracket = tokens.iter().any(|(t, _)| matches!(t, Token::RBracket));
    if !has_rbracket {
        return Err("Expected RBracket token".into());
    }
    Ok(())
}

#[test]
fn test_lparen() -> Result<(), Box<dyn std::error::Error>> {
    let source = "(";
    let tokens = Lexer::tokenize_all(source);
    let has_lparen = tokens.iter().any(|(t, _)| matches!(t, Token::LParen));
    if !has_lparen {
        return Err("Expected LParen token".into());
    }
    Ok(())
}

#[test]
fn test_rparen() -> Result<(), Box<dyn std::error::Error>> {
    let source = ")";
    let tokens = Lexer::tokenize_all(source);
    let has_rparen = tokens.iter().any(|(t, _)| matches!(t, Token::RParen));
    if !has_rparen {
        return Err("Expected RParen token".into());
    }
    Ok(())
}

#[test]
fn test_colon() -> Result<(), Box<dyn std::error::Error>> {
    let source = ":";
    let tokens = Lexer::tokenize_all(source);
    let has_colon = tokens.iter().any(|(t, _)| matches!(t, Token::Colon));
    if !has_colon {
        return Err("Expected Colon token".into());
    }
    Ok(())
}

#[test]
fn test_double_colon() -> Result<(), Box<dyn std::error::Error>> {
    let source = "::";
    let tokens = Lexer::tokenize_all(source);
    let has_double_colon = tokens.iter().any(|(t, _)| matches!(t, Token::DoubleColon));
    if !has_double_colon {
        return Err("Expected DoubleColon token".into());
    }
    Ok(())
}

#[test]
fn test_comma() -> Result<(), Box<dyn std::error::Error>> {
    let source = ",";
    let tokens = Lexer::tokenize_all(source);
    let has_comma = tokens.iter().any(|(t, _)| matches!(t, Token::Comma));
    if !has_comma {
        return Err("Expected Comma token".into());
    }
    Ok(())
}

#[test]
fn test_dot() -> Result<(), Box<dyn std::error::Error>> {
    let source = ".";
    let tokens = Lexer::tokenize_all(source);
    let has_dot = tokens.iter().any(|(t, _)| matches!(t, Token::Dot));
    if !has_dot {
        return Err("Expected Dot token".into());
    }
    Ok(())
}

#[test]
fn test_question() -> Result<(), Box<dyn std::error::Error>> {
    let source = "?";
    let tokens = Lexer::tokenize_all(source);
    let has_question = tokens.iter().any(|(t, _)| matches!(t, Token::Question));
    if !has_question {
        return Err("Expected Question token".into());
    }
    Ok(())
}

#[test]
fn test_arrow() -> Result<(), Box<dyn std::error::Error>> {
    let source = "->";
    let tokens = Lexer::tokenize_all(source);
    let has_arrow = tokens.iter().any(|(t, _)| matches!(t, Token::Arrow));
    if !has_arrow {
        return Err("Expected Arrow token".into());
    }
    Ok(())
}

#[test]
fn test_extern_keyword() -> Result<(), Box<dyn std::error::Error>> {
    let source = "extern";
    let tokens = Lexer::tokenize_all(source);
    let has_extern = tokens.iter().any(|(t, _)| matches!(t, Token::Extern));
    if !has_extern {
        return Err("Expected Extern token".into());
    }
    Ok(())
}

// =============================================================================
// Complex Source Tests
// =============================================================================

#[test]
fn test_struct_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct User { name: String, age: I32 }";
    let tokens = Lexer::tokenize_all(source);
    if tokens.len() < 8 {
        return Err(format!("Expected multiple tokens, got {}", tokens.len()).into());
    }
    Ok(())
}

#[test]
fn test_trait_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Named { name: String }";
    let tokens = Lexer::tokenize_all(source);
    if tokens.len() < 6 {
        return Err(format!("Expected multiple tokens, got {}", tokens.len()).into());
    }
    Ok(())
}

#[test]
fn test_enum_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Status { active, inactive, pending }";
    let tokens = Lexer::tokenize_all(source);
    if tokens.len() < 7 {
        return Err(format!("Expected multiple tokens, got {}", tokens.len()).into());
    }
    Ok(())
}

#[test]
fn test_impl_block() -> Result<(), Box<dyn std::error::Error>> {
    let source = "impl User { \"default\" }";
    let tokens = Lexer::tokenize_all(source);
    if tokens.len() < 4 {
        return Err(format!("Expected multiple tokens, got {}", tokens.len()).into());
    }
    Ok(())
}

#[test]
fn test_module_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = "mod utils { struct Helper { } }";
    let tokens = Lexer::tokenize_all(source);
    if tokens.len() < 6 {
        return Err(format!("Expected multiple tokens, got {}", tokens.len()).into());
    }
    Ok(())
}

#[test]
fn test_use_statement() -> Result<(), Box<dyn std::error::Error>> {
    let source = "use utils::Helper";
    let tokens = Lexer::tokenize_all(source);
    let has_use = tokens.iter().any(|(t, _)| matches!(t, Token::Use));
    let has_double_colon = tokens.iter().any(|(t, _)| matches!(t, Token::DoubleColon));
    if !has_use {
        return Err("Expected Use token".into());
    }
    if !has_double_colon {
        return Err("Expected DoubleColon token".into());
    }
    Ok(())
}

#[test]
fn test_let_binding() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 42";
    let tokens = Lexer::tokenize_all(source);
    let has_let = tokens.iter().any(|(t, _)| matches!(t, Token::Let));
    let has_equals = tokens.iter().any(|(t, _)| matches!(t, Token::Equals));
    if !has_let {
        return Err("Expected Let token".into());
    }
    if !has_equals {
        return Err("Expected Equals token".into());
    }
    Ok(())
}

// =============================================================================
// Span Tests
// =============================================================================

#[test]
fn test_token_spans() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Test";
    let tokens = Lexer::tokenize_all(source);
    if tokens.len() < 2 {
        return Err(format!("Expected at least 2 tokens, got {}", tokens.len()).into());
    }
    // First token should start at offset 0 (guaranteed non-empty by check above)
    let (_, span) = tokens.first().ok_or("tokens is non-empty")?;
    if span.start.offset != 0 {
        return Err(format!(
            "First token should start at offset 0, got {}",
            span.start.offset
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_span_positions() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 42";
    let tokens = Lexer::tokenize_all(source);
    // All spans should have valid positions
    for (_, span) in &tokens {
        if span.start.offset > span.end.offset {
            return Err("Invalid span".into());
        }
    }
    Ok(())
}

// =============================================================================
// Lexer Direct Usage Tests
// =============================================================================

#[test]
fn test_lexer_new() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Test { }";
    let mut lexer = Lexer::new(source);
    // Verify lexer was created by checking we can get a token
    let first = lexer.next_token();
    if first.is_none() {
        return Err("Lexer should produce tokens".into());
    }
    Ok(())
}

#[test]
fn test_lexer_next_token() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Test { }";
    let mut lexer = Lexer::new(source);

    let mut count = 0_u32;
    while let Some((_token, _span)) = lexer.next_token() {
        count = count.saturating_add(1);
        if count > 100 {
            return Err("Possible infinite loop".into());
        }
    }
    if count == 0 {
        return Err("Should have tokens".into());
    }
    Ok(())
}

#[test]
fn test_lexer_span() -> Result<(), Box<dyn std::error::Error>> {
    let source = "test";
    let mut lexer = Lexer::new(source);
    lexer.next_token();
    let span = lexer.span();
    if span.end.offset < span.start.offset {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Multiline Tests
// =============================================================================

#[test]
fn test_multiline_source() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A {\n    field: String\n}";
    let tokens = Lexer::tokenize_all(source);
    if tokens.is_empty() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_multiline_with_newlines() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { }\n\nstruct B { }";
    let tokens = Lexer::tokenize_all(source);
    // Count how many struct tokens
    let struct_count = tokens
        .iter()
        .filter(|(t, _)| matches!(t, Token::Struct))
        .count();
    if struct_count != 2 {
        return Err(format!("expected 2 but got {struct_count:?}").into());
    }
    Ok(())
}

// =============================================================================
// Regex Token Tests
// =============================================================================

#[test]
fn test_regex_literal() -> Result<(), Box<dyn std::error::Error>> {
    // Regex syntax is r/pattern/flags
    let source = "r/hello.*/i";
    let tokens = Lexer::tokenize_all(source);
    let has_regex = tokens.iter().any(|(t, _)| matches!(t, Token::Regex(_)));
    if !has_regex {
        return Err("Should tokenize regex literal".into());
    }
    Ok(())
}

// =============================================================================
// Path Token Tests
// =============================================================================

#[test]
fn test_path_literal() -> Result<(), Box<dyn std::error::Error>> {
    // Path syntax is /path/to/file (starts with /, no quotes)
    let source = "/path/to/file";
    let tokens = Lexer::tokenize_all(source);
    let has_path = tokens.iter().any(|(t, _)| matches!(t, Token::Path(_)));
    if !has_path {
        return Err("Should tokenize path literal".into());
    }
    Ok(())
}
