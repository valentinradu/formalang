//! Lexer tests
//!
//! Tests for the Lexer and Token functionality

use formalang::lexer::{Lexer, Token};

// =============================================================================
// Basic Token Tests
// =============================================================================

#[test]
fn test_simple_tokens() {
    let source = "struct Test { }";
    let tokens = Lexer::tokenize_all(source);
    assert!(!tokens.is_empty());
}

#[test]
fn test_empty_source() {
    let source = "";
    let tokens = Lexer::tokenize_all(source);
    assert!(tokens.is_empty() || tokens.iter().all(|(t, _)| matches!(t, Token::Eof)));
}

#[test]
fn test_only_whitespace() {
    let source = "    ";
    let tokens = Lexer::tokenize_all(source);
    // Whitespace should be skipped
    assert!(tokens.is_empty() || tokens.iter().all(|(t, _)| matches!(t, Token::Eof)));
}

// =============================================================================
// Keyword Token Tests
// =============================================================================

#[test]
fn test_struct_keyword() {
    let source = "struct";
    let tokens = Lexer::tokenize_all(source);
    let has_struct = tokens.iter().any(|(t, _)| matches!(t, Token::Struct));
    assert!(has_struct, "Expected Struct token");
}

#[test]
fn test_trait_keyword() {
    let source = "trait";
    let tokens = Lexer::tokenize_all(source);
    let has_trait = tokens.iter().any(|(t, _)| matches!(t, Token::Trait));
    assert!(has_trait, "Expected Trait token");
}

#[test]
fn test_enum_keyword() {
    let source = "enum";
    let tokens = Lexer::tokenize_all(source);
    let has_enum = tokens.iter().any(|(t, _)| matches!(t, Token::Enum));
    assert!(has_enum, "Expected Enum token");
}

#[test]
fn test_impl_keyword() {
    let source = "impl";
    let tokens = Lexer::tokenize_all(source);
    let has_impl = tokens.iter().any(|(t, _)| matches!(t, Token::Impl));
    assert!(has_impl, "Expected Impl token");
}

#[test]
fn test_module_keyword() {
    let source = "mod";
    let tokens = Lexer::tokenize_all(source);
    let has_module = tokens.iter().any(|(t, _)| matches!(t, Token::Module));
    assert!(has_module, "Expected Module token");
}

#[test]
fn test_use_keyword() {
    let source = "use";
    let tokens = Lexer::tokenize_all(source);
    let has_use = tokens.iter().any(|(t, _)| matches!(t, Token::Use));
    assert!(has_use, "Expected Use token");
}

#[test]
fn test_pub_keyword() {
    let source = "pub";
    let tokens = Lexer::tokenize_all(source);
    let has_pub = tokens.iter().any(|(t, _)| matches!(t, Token::Pub));
    assert!(has_pub, "Expected Pub token");
}

#[test]
fn test_let_keyword() {
    let source = "let";
    let tokens = Lexer::tokenize_all(source);
    let has_let = tokens.iter().any(|(t, _)| matches!(t, Token::Let));
    assert!(has_let, "Expected Let token");
}

#[test]
fn test_mut_keyword() {
    let source = "mut";
    let tokens = Lexer::tokenize_all(source);
    let has_mut = tokens.iter().any(|(t, _)| matches!(t, Token::Mut));
    assert!(has_mut, "Expected Mut token");
}

#[test]
fn test_if_keyword() {
    let source = "if";
    let tokens = Lexer::tokenize_all(source);
    let has_if = tokens.iter().any(|(t, _)| matches!(t, Token::If));
    assert!(has_if, "Expected If token");
}

#[test]
fn test_else_keyword() {
    let source = "else";
    let tokens = Lexer::tokenize_all(source);
    let has_else = tokens.iter().any(|(t, _)| matches!(t, Token::Else));
    assert!(has_else, "Expected Else token");
}

#[test]
fn test_for_keyword() {
    let source = "for";
    let tokens = Lexer::tokenize_all(source);
    let has_for = tokens.iter().any(|(t, _)| matches!(t, Token::For));
    assert!(has_for, "Expected For token");
}

#[test]
fn test_in_keyword() {
    let source = "in";
    let tokens = Lexer::tokenize_all(source);
    let has_in = tokens.iter().any(|(t, _)| matches!(t, Token::In));
    assert!(has_in, "Expected In token");
}

#[test]
fn test_match_keyword() {
    let source = "match";
    let tokens = Lexer::tokenize_all(source);
    let has_match = tokens.iter().any(|(t, _)| matches!(t, Token::Match));
    assert!(has_match, "Expected Match token");
}

#[test]
fn test_provides_keyword() {
    let source = "provides";
    let tokens = Lexer::tokenize_all(source);
    let has_provides = tokens.iter().any(|(t, _)| matches!(t, Token::Provides));
    assert!(has_provides, "Expected Provides token");
}

#[test]
fn test_consumes_keyword() {
    let source = "consumes";
    let tokens = Lexer::tokenize_all(source);
    let has_consumes = tokens.iter().any(|(t, _)| matches!(t, Token::Consumes));
    assert!(has_consumes, "Expected Consumes token");
}

// =============================================================================
// Literal Token Tests
// =============================================================================

#[test]
fn test_string_literal() {
    let source = "\"hello world\"";
    let tokens = Lexer::tokenize_all(source);
    let has_string = tokens.iter().any(|(t, _)| matches!(t, Token::String(_)));
    assert!(has_string, "Expected String token");
}

#[test]
fn test_number_literal_integer() {
    let source = "42";
    let tokens = Lexer::tokenize_all(source);
    let has_number = tokens.iter().any(|(t, _)| matches!(t, Token::Number(_)));
    assert!(has_number, "Expected Number token");
}

#[test]
fn test_number_literal_float() {
    let source = "3.14";
    let tokens = Lexer::tokenize_all(source);
    let has_number = tokens.iter().any(|(t, _)| matches!(t, Token::Number(_)));
    assert!(has_number, "Expected Number token for float");
}

#[test]
fn test_true_literal() {
    let source = "true";
    let tokens = Lexer::tokenize_all(source);
    let has_true = tokens.iter().any(|(t, _)| matches!(t, Token::True));
    assert!(has_true, "Expected True token");
}

#[test]
fn test_false_literal() {
    let source = "false";
    let tokens = Lexer::tokenize_all(source);
    let has_false = tokens.iter().any(|(t, _)| matches!(t, Token::False));
    assert!(has_false, "Expected False token");
}

#[test]
fn test_identifier() {
    let source = "myVariable";
    let tokens = Lexer::tokenize_all(source);
    let has_ident = tokens.iter().any(|(t, _)| matches!(t, Token::Ident(_)));
    assert!(has_ident, "Expected Ident token");
}

// =============================================================================
// Operator Token Tests
// =============================================================================

#[test]
fn test_plus_operator() {
    let source = "+";
    let tokens = Lexer::tokenize_all(source);
    let has_plus = tokens.iter().any(|(t, _)| matches!(t, Token::Plus));
    assert!(has_plus, "Expected Plus token");
}

#[test]
fn test_minus_operator() {
    let source = "-";
    let tokens = Lexer::tokenize_all(source);
    let has_minus = tokens.iter().any(|(t, _)| matches!(t, Token::Minus));
    assert!(has_minus, "Expected Minus token");
}

#[test]
fn test_star_operator() {
    let source = "*";
    let tokens = Lexer::tokenize_all(source);
    let has_star = tokens.iter().any(|(t, _)| matches!(t, Token::Star));
    assert!(has_star, "Expected Star token");
}

#[test]
fn test_slash_operator() {
    let source = "/";
    let tokens = Lexer::tokenize_all(source);
    let has_slash = tokens.iter().any(|(t, _)| matches!(t, Token::Slash));
    assert!(has_slash, "Expected Slash token");
}

#[test]
fn test_equals_operator() {
    let source = "=";
    let tokens = Lexer::tokenize_all(source);
    let has_equals = tokens.iter().any(|(t, _)| matches!(t, Token::Equals));
    assert!(has_equals, "Expected Equals token");
}

#[test]
fn test_eqeq_operator() {
    let source = "==";
    let tokens = Lexer::tokenize_all(source);
    let has_eqeq = tokens.iter().any(|(t, _)| matches!(t, Token::EqEq));
    assert!(has_eqeq, "Expected EqEq token");
}

#[test]
fn test_ne_operator() {
    let source = "!=";
    let tokens = Lexer::tokenize_all(source);
    let has_ne = tokens.iter().any(|(t, _)| matches!(t, Token::Ne));
    assert!(has_ne, "Expected Ne token");
}

#[test]
fn test_lt_operator() {
    let source = "<";
    let tokens = Lexer::tokenize_all(source);
    let has_lt = tokens.iter().any(|(t, _)| matches!(t, Token::Lt));
    assert!(has_lt, "Expected Lt token");
}

#[test]
fn test_gt_operator() {
    let source = ">";
    let tokens = Lexer::tokenize_all(source);
    let has_gt = tokens.iter().any(|(t, _)| matches!(t, Token::Gt));
    assert!(has_gt, "Expected Gt token");
}

#[test]
fn test_le_operator() {
    let source = "<=";
    let tokens = Lexer::tokenize_all(source);
    let has_le = tokens.iter().any(|(t, _)| matches!(t, Token::Le));
    assert!(has_le, "Expected Le token");
}

#[test]
fn test_ge_operator() {
    let source = ">=";
    let tokens = Lexer::tokenize_all(source);
    let has_ge = tokens.iter().any(|(t, _)| matches!(t, Token::Ge));
    assert!(has_ge, "Expected Ge token");
}

#[test]
fn test_and_operator() {
    let source = "&&";
    let tokens = Lexer::tokenize_all(source);
    let has_and = tokens.iter().any(|(t, _)| matches!(t, Token::And));
    assert!(has_and, "Expected And token");
}

#[test]
fn test_or_operator() {
    let source = "||";
    let tokens = Lexer::tokenize_all(source);
    let has_or = tokens.iter().any(|(t, _)| matches!(t, Token::Or));
    assert!(has_or, "Expected Or token");
}

#[test]
fn test_percent_operator() {
    let source = "%";
    let tokens = Lexer::tokenize_all(source);
    let has_percent = tokens.iter().any(|(t, _)| matches!(t, Token::Percent));
    assert!(has_percent, "Expected Percent token");
}

// =============================================================================
// Punctuation Token Tests
// =============================================================================

#[test]
fn test_lbrace() {
    let source = "{";
    let tokens = Lexer::tokenize_all(source);
    let has_lbrace = tokens.iter().any(|(t, _)| matches!(t, Token::LBrace));
    assert!(has_lbrace, "Expected LBrace token");
}

#[test]
fn test_rbrace() {
    let source = "}";
    let tokens = Lexer::tokenize_all(source);
    let has_rbrace = tokens.iter().any(|(t, _)| matches!(t, Token::RBrace));
    assert!(has_rbrace, "Expected RBrace token");
}

#[test]
fn test_lbracket() {
    let source = "[";
    let tokens = Lexer::tokenize_all(source);
    let has_lbracket = tokens.iter().any(|(t, _)| matches!(t, Token::LBracket));
    assert!(has_lbracket, "Expected LBracket token");
}

#[test]
fn test_rbracket() {
    let source = "]";
    let tokens = Lexer::tokenize_all(source);
    let has_rbracket = tokens.iter().any(|(t, _)| matches!(t, Token::RBracket));
    assert!(has_rbracket, "Expected RBracket token");
}

#[test]
fn test_lparen() {
    let source = "(";
    let tokens = Lexer::tokenize_all(source);
    let has_lparen = tokens.iter().any(|(t, _)| matches!(t, Token::LParen));
    assert!(has_lparen, "Expected LParen token");
}

#[test]
fn test_rparen() {
    let source = ")";
    let tokens = Lexer::tokenize_all(source);
    let has_rparen = tokens.iter().any(|(t, _)| matches!(t, Token::RParen));
    assert!(has_rparen, "Expected RParen token");
}

#[test]
fn test_colon() {
    let source = ":";
    let tokens = Lexer::tokenize_all(source);
    let has_colon = tokens.iter().any(|(t, _)| matches!(t, Token::Colon));
    assert!(has_colon, "Expected Colon token");
}

#[test]
fn test_double_colon() {
    let source = "::";
    let tokens = Lexer::tokenize_all(source);
    let has_double_colon = tokens.iter().any(|(t, _)| matches!(t, Token::DoubleColon));
    assert!(has_double_colon, "Expected DoubleColon token");
}

#[test]
fn test_comma() {
    let source = ",";
    let tokens = Lexer::tokenize_all(source);
    let has_comma = tokens.iter().any(|(t, _)| matches!(t, Token::Comma));
    assert!(has_comma, "Expected Comma token");
}

#[test]
fn test_dot() {
    let source = ".";
    let tokens = Lexer::tokenize_all(source);
    let has_dot = tokens.iter().any(|(t, _)| matches!(t, Token::Dot));
    assert!(has_dot, "Expected Dot token");
}

#[test]
fn test_question() {
    let source = "?";
    let tokens = Lexer::tokenize_all(source);
    let has_question = tokens.iter().any(|(t, _)| matches!(t, Token::Question));
    assert!(has_question, "Expected Question token");
}

#[test]
fn test_arrow() {
    let source = "->";
    let tokens = Lexer::tokenize_all(source);
    let has_arrow = tokens.iter().any(|(t, _)| matches!(t, Token::Arrow));
    assert!(has_arrow, "Expected Arrow token");
}

#[test]
fn test_mount_keyword() {
    let source = "mount";
    let tokens = Lexer::tokenize_all(source);
    let has_mount = tokens.iter().any(|(t, _)| matches!(t, Token::Mount));
    assert!(has_mount, "Expected Mount token");
}

// =============================================================================
// Complex Source Tests
// =============================================================================

#[test]
fn test_struct_definition() {
    let source = "struct User { name: String, age: Number }";
    let tokens = Lexer::tokenize_all(source);
    assert!(tokens.len() >= 8, "Expected multiple tokens");
}

#[test]
fn test_trait_definition() {
    let source = "trait Named { name: String }";
    let tokens = Lexer::tokenize_all(source);
    assert!(tokens.len() >= 6, "Expected multiple tokens");
}

#[test]
fn test_enum_definition() {
    let source = "enum Status { active, inactive, pending }";
    let tokens = Lexer::tokenize_all(source);
    assert!(tokens.len() >= 7, "Expected multiple tokens");
}

#[test]
fn test_impl_block() {
    let source = "impl User { \"default\" }";
    let tokens = Lexer::tokenize_all(source);
    assert!(tokens.len() >= 4, "Expected multiple tokens");
}

#[test]
fn test_module_definition() {
    let source = "mod utils { struct Helper { } }";
    let tokens = Lexer::tokenize_all(source);
    assert!(tokens.len() >= 6, "Expected multiple tokens");
}

#[test]
fn test_use_statement() {
    let source = "use utils::Helper";
    let tokens = Lexer::tokenize_all(source);
    let has_use = tokens.iter().any(|(t, _)| matches!(t, Token::Use));
    let has_double_colon = tokens.iter().any(|(t, _)| matches!(t, Token::DoubleColon));
    assert!(has_use, "Expected Use token");
    assert!(has_double_colon, "Expected DoubleColon token");
}

#[test]
fn test_let_binding() {
    let source = "let x = 42";
    let tokens = Lexer::tokenize_all(source);
    let has_let = tokens.iter().any(|(t, _)| matches!(t, Token::Let));
    let has_equals = tokens.iter().any(|(t, _)| matches!(t, Token::Equals));
    assert!(has_let, "Expected Let token");
    assert!(has_equals, "Expected Equals token");
}

// =============================================================================
// Span Tests
// =============================================================================

#[test]
fn test_token_spans() {
    let source = "struct Test";
    let tokens = Lexer::tokenize_all(source);
    assert!(tokens.len() >= 2, "Expected at least 2 tokens");
    // First token should start at offset 0
    if let Some((_, span)) = tokens.first() {
        assert_eq!(span.start.offset, 0, "First token should start at offset 0");
    }
}

#[test]
fn test_span_positions() {
    let source = "let x = 42";
    let tokens = Lexer::tokenize_all(source);
    // All spans should have valid positions
    for (_, span) in &tokens {
        assert!(span.start.offset <= span.end.offset, "Invalid span");
    }
}

// =============================================================================
// Lexer Direct Usage Tests
// =============================================================================

#[test]
fn test_lexer_new() {
    let source = "struct Test { }";
    let mut lexer = Lexer::new(source);
    // Verify lexer was created by checking we can get a token
    let first = lexer.next_token();
    assert!(first.is_some(), "Lexer should produce tokens");
}

#[test]
fn test_lexer_next_token() {
    let source = "struct Test { }";
    let mut lexer = Lexer::new(source);

    let mut count = 0;
    while let Some((token, _span)) = lexer.next_token() {
        if matches!(token, Token::Eof) {
            break;
        }
        count += 1;
        if count > 100 {
            panic!("Possible infinite loop");
        }
    }
    assert!(count > 0, "Should have tokens");
}

#[test]
fn test_lexer_span() {
    let source = "test";
    let mut lexer = Lexer::new(source);
    lexer.next_token();
    let span = lexer.span();
    assert!(span.end.offset >= span.start.offset);
}

// =============================================================================
// Multiline Tests
// =============================================================================

#[test]
fn test_multiline_source() {
    let source = "struct A {\n    field: String\n}";
    let tokens = Lexer::tokenize_all(source);
    assert!(!tokens.is_empty());
}

#[test]
fn test_multiline_with_newlines() {
    let source = "struct A { }\n\nstruct B { }";
    let tokens = Lexer::tokenize_all(source);
    // Count how many struct tokens
    let struct_count = tokens
        .iter()
        .filter(|(t, _)| matches!(t, Token::Struct))
        .count();
    assert_eq!(struct_count, 2, "Expected 2 struct tokens");
}

// =============================================================================
// Regex Token Tests
// =============================================================================

#[test]
fn test_regex_literal() {
    // Regex syntax is r/pattern/flags
    let source = "r/hello.*/i";
    let tokens = Lexer::tokenize_all(source);
    let has_regex = tokens.iter().any(|(t, _)| matches!(t, Token::Regex(_)));
    assert!(has_regex, "Should tokenize regex literal");
}

// =============================================================================
// Path Token Tests
// =============================================================================

#[test]
fn test_path_literal() {
    // Path syntax is /path/to/file (starts with /, no quotes)
    let source = "/path/to/file";
    let tokens = Lexer::tokenize_all(source);
    let has_path = tokens.iter().any(|(t, _)| matches!(t, Token::Path(_)));
    assert!(has_path, "Should tokenize path literal");
}
