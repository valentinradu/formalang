// Parser for FormaLang using chumsky parser combinator library
//
// This module converts tokens into an Abstract Syntax Tree (AST).
// It uses recursive descent parsing with excellent error recovery.

mod defs;
mod exprs;
mod span;
mod types;

use chumsky::input::{Stream, ValueInput};
use chumsky::prelude::*;

use crate::ast::{
    BlockStatement, Expr, File, Ident, LetBinding, Literal, Statement, UseItems, UseStmt,
    Visibility,
};
use crate::lexer::Token;
use crate::location::Span as CustomSpan;

use defs::{binding_pattern_parser, definition_parser};
use exprs::expr_parser;
use span::fill_file_spans;
use types::type_parser;

/// Main entry point for parsing
/// Returns the parsed AST or a vector of (`error_message`, span) tuples
///
/// # Errors
///
/// Returns a vector of `(message, span)` pairs if the token stream contains parse errors.
pub fn parse_file(tokens: &[(Token, CustomSpan)]) -> Result<File, Vec<(String, CustomSpan)>> {
    parse_file_internal(tokens, None)
}

/// Parse with source text for better error positions
///
/// # Errors
///
/// Returns a vector of `(message, span)` pairs if the token stream contains parse errors.
pub fn parse_file_with_source(
    tokens: &[(Token, CustomSpan)],
    source: &str,
) -> Result<File, Vec<(String, CustomSpan)>> {
    parse_file_internal(tokens, Some(source))
}

/// Internal parsing implementation
fn parse_file_internal(
    tokens: &[(Token, CustomSpan)],
    source: Option<&str>,
) -> Result<File, Vec<(String, CustomSpan)>> {
    // Convert our custom spans to SimpleSpan for chumsky
    let token_iter = tokens.iter().map(|(tok, span)| {
        (
            tok.clone(),
            SimpleSpan::new(span.start.offset, span.end.offset),
        )
    });

    // Create a stream with end-of-input span
    let end_offset = tokens.last().map_or(0, |(_, s)| s.end.offset);
    let end_span: SimpleSpan = (end_offset..end_offset).into();
    let token_stream = Stream::from_iter(token_iter).map(end_span, |(t, s)| (t, s));

    // Parse
    let mut file = file_parser()
        .parse(token_stream)
        .into_result()
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|e| {
                    let simple_span = e.span();
                    let message = format_parse_error(&e);
                    let custom_span = source.map_or_else(
                        || {
                            // Use span from tokens if available
                            tokens
                                .iter()
                                .find(|(_, span)| {
                                    span.start.offset == simple_span.start
                                        && span.end.offset == simple_span.end
                                })
                                .map_or_else(|| span_from_simple(*simple_span), |(_, span)| *span)
                        },
                        |src| {
                            // Compute line/column from source text
                            CustomSpan::from_range_with_source(
                                simple_span.start,
                                simple_span.end,
                                src,
                            )
                        },
                    );
                    (message, custom_span)
                })
                .collect::<Vec<_>>()
        })?;

    // Post-process: Fill in line/column info for all spans if source is provided
    if let Some(src) = source {
        fill_file_spans(&mut file, src);
    }

    Ok(file)
}

/// Format a parse error with lowercase keywords and readable token names
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "RichPattern is defined in the chumsky library and cannot be exhaustively enumerated"
)]
fn format_parse_error(error: &Rich<'_, Token>) -> String {
    use chumsky::error::RichPattern;

    let found = error
        .found()
        .map_or_else(|| "end of input".to_string(), |t| format!("{t}"));

    let expected: Vec<String> = error
        .expected()
        .map(|exp| match exp {
            RichPattern::Token(tok) => {
                // tok is a Maybe<Token, &Token> which derefs to &Token
                format!("{}", &**tok)
            }
            RichPattern::Label(label) => label.to_string(),
            RichPattern::EndOfInput => "end of input".to_string(),
            _ => "<unknown>".to_string(),
        })
        .collect();

    let span = error.span();

    if expected.is_empty() {
        format!("found {} at {}..{}", found, span.start, span.end)
    } else if expected.len() == 1 {
        #[expect(
            clippy::indexing_slicing,
            reason = "bounds checked above: expected.len() == 1"
        )]
        let first = &expected[0];
        format!(
            "found {} at {}..{}, expected {}",
            found, span.start, span.end, first
        )
    } else {
        format!(
            "found {} at {}..{}, expected one of: {}",
            found,
            span.start,
            span.end,
            expected.join(", ")
        )
    }
}

/// Parse a complete file
fn file_parser<'tokens, I>(
) -> impl Parser<'tokens, I, File, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    statement_parser()
        .repeated()
        .collect::<Vec<_>>()
        .map_with(|statements, e| File {
            format_version: crate::ast::FORMAT_VERSION,
            statements,
            span: span_from_simple(e.span()),
        })
}

/// Parse a top-level statement
fn statement_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Statement, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    choice((
        use_stmt_parser().map(Statement::Use),
        let_binding_parser().map(|lb| Statement::Let(Box::new(lb))),
        definition_parser().map(|d| Statement::Definition(Box::new(d))),
    ))
}

/// Parse a use statement
fn use_stmt_parser<'tokens, I>(
) -> impl Parser<'tokens, I, UseStmt, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then_ignore(just(Token::Use))
        .then(
            ident_parser()
                .separated_by(just(Token::DoubleColon))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then(
            just(Token::DoubleColon)
                .ignore_then(use_items_parser())
                .or_not(),
        )
        .map_with(|((visibility, mut path), items), e| {
            let items = items.unwrap_or_else(|| {
                // If no items specified, last segment is the item
                path.pop().map_or_else(
                    || {
                        UseItems::Single(Ident {
                            name: String::new(),
                            span: CustomSpan::default(),
                        })
                    },
                    UseItems::Single,
                )
            });

            UseStmt {
                visibility,
                path,
                items,
                span: span_from_simple(e.span()),
            }
        })
}

/// Parse use items (single, multiple, or glob)
fn use_items_parser<'tokens, I>(
) -> impl Parser<'tokens, I, UseItems, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    choice((
        // Glob import: *
        just(Token::Star).to(UseItems::Glob),
        // Multiple items: { A, B, C }
        ident_parser()
            .separated_by(just(Token::Comma))
            .at_least(1)
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(UseItems::Multiple),
        // Single item
        ident_parser().map(UseItems::Single),
    ))
}

/// Parse a let binding
fn let_binding_parser<'tokens, I>(
) -> impl Parser<'tokens, I, LetBinding, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then_ignore(just(Token::Let))
        .then(mutability_parser())
        .then(binding_pattern_parser())
        .then(just(Token::Colon).ignore_then(type_parser()).or_not()) // Optional type annotation
        .then_ignore(just(Token::Equals))
        .then(expr_parser())
        .map_with(
            |((((visibility, mutable), pattern), type_annotation), value), e| LetBinding {
                visibility,
                mutable,
                pattern,
                type_annotation,
                value,
                span: span_from_simple(e.span()),
            },
        )
}

/// Parse visibility modifier (pub or private)
fn visibility_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Visibility, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    just(Token::Pub)
        .to(Visibility::Public)
        .or_not()
        .map(|v| v.unwrap_or(Visibility::Private))
}

/// Parse mutability modifier (mut or immutable)
fn mutability_parser<'tokens, I>(
) -> impl Parser<'tokens, I, bool, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    just(Token::Mut).or_not().map(|m| m.is_some())
}

/// Parse an identifier
fn ident_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Ident, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    select! {
        Token::Ident(name) = e => Ident::new(name, span_from_simple(e.span())),
        Token::SelfKeyword = e => Ident::new("self".to_string(), span_from_simple(e.span()))
    }
    .labelled("identifier")
}

/// Parse an identifier (excluding 'self' keyword)
/// Used in type and enum contexts where 'self' is not valid
fn ident_no_self_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Ident, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    select! {
        Token::Ident(name) = e => Ident::new(name, span_from_simple(e.span()))
    }
    .labelled("identifier")
}

/// Parse an invocation target: identifier or self
fn invocation_target_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Ident, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    select! {
        Token::Ident(name) = e => Ident::new(name, span_from_simple(e.span())),
        Token::SelfKeyword = e => Ident::new("self".to_string(), span_from_simple(e.span())),
    }
    .labelled("identifier")
}

/// Convert a list of block statements to an Expr (Block or single expression).
///
/// This is shared logic used by both function bodies and block expressions.
fn block_statements_to_expr(mut statements: Vec<BlockStatement>, span: crate::Span) -> Expr {
    // Empty body -> Nil
    if statements.is_empty() {
        return Expr::Literal(Literal::Nil);
    }

    // Last item becomes the result expression
    let Some(last) = statements.pop() else {
        return Expr::Literal(Literal::Nil);
    };
    let result = match last {
        BlockStatement::Expr(expr) => expr,
        // If last is a statement (not expr), push it back and use Nil as result
        stmt @ (BlockStatement::Let { .. } | BlockStatement::Assign { .. }) => {
            statements.push(stmt);
            Expr::Literal(Literal::Nil)
        }
    };

    // Single expression with no statements -> return it directly
    if statements.is_empty() {
        return result;
    }

    Expr::Block {
        statements,
        result: Box::new(result),
        span,
    }
}

/// Helper to convert `SimpleSpan` to our custom Span
const fn span_from_simple(s: SimpleSpan) -> CustomSpan {
    CustomSpan::from_range(s.start, s.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BindingPattern, Definition, Expr, Literal, PrimitiveType, Type};
    use crate::lexer::Lexer;

    fn parse_type_str(input: &str) -> Result<Type, Vec<(String, CustomSpan)>> {
        // Parse the type as a struct field and extract it
        let wrapper = format!("struct Test {{ field: {input} }}");
        let tokens = Lexer::tokenize_all(&wrapper);
        let result = parse_file(&tokens)?;

        // Extract the type from the parsed struct
        if let Some(Statement::Definition(def)) = result.statements.first() {
            if let Definition::Struct(s) = &**def {
                if let Some(field) = s.fields.first() {
                    return Ok(field.ty.clone());
                }
            }
        }
        Err(vec![(
            "Could not extract type".to_string(),
            CustomSpan::default(),
        )])
    }

    #[test]
    fn test_never_type_parsing() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("Never");
        if result.is_err() {
            return Err(format!("Failed to parse Never type: {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        if ty != Type::Primitive(PrimitiveType::Never) {
            return Err(format!("{:?} != {:?}", ty, Type::Primitive(PrimitiveType::Never)).into());
        }
        Ok(())
    }

    #[test]
    fn test_never_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
        let input = r"
            pub struct Empty {
                body: Never
            }
        ";
        let tokens = Lexer::tokenize_all(input);
        let result = parse_file(&tokens);
        if result.is_err() {
            return Err(format!("Failed to parse struct with Never field: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_optional_never_type() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("Never?");
        if result.is_err() {
            return Err(format!("Failed to parse Never? type: {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Optional(inner) => {
                if *inner != Type::Primitive(PrimitiveType::Never) {
                    return Err(format!(
                        "{:?} != {:?}",
                        *inner,
                        Type::Primitive(PrimitiveType::Never)
                    )
                    .into());
                }
            }
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::Closure { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Optional type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_array_of_never_type() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("[Never]");
        if result.is_err() {
            return Err(format!("Failed to parse [Never] type: {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Array(inner) => {
                if *inner != Type::Primitive(PrimitiveType::Never) {
                    return Err(format!(
                        "{:?} != {:?}",
                        *inner,
                        Type::Primitive(PrimitiveType::Never)
                    )
                    .into());
                }
            }
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::Closure { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Array type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_dictionary_type_parsing() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("[String: Number]");
        if result.is_err() {
            return Err(format!("Failed to parse [String: Number] type: : {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Dictionary { key, value } => {
                if *key != Type::Primitive(PrimitiveType::String) {
                    return Err(format!(
                        "{:?} != {:?}",
                        *key,
                        Type::Primitive(PrimitiveType::String)
                    )
                    .into());
                }
                if *value != Type::Primitive(PrimitiveType::Number) {
                    return Err(format!(
                        "{:?} != {:?}",
                        *value,
                        Type::Primitive(PrimitiveType::Number)
                    )
                    .into());
                }
            }
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Closure { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Dictionary type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_dictionary_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
        let input = r"
            pub struct Config {
                settings: [String: String]
            }
        ";
        let tokens = Lexer::tokenize_all(input);
        let result = parse_file(&tokens);
        if result.is_err() {
            return Err(
                format!("Failed to parse struct with Dictionary field: : {result:?}").into(),
            );
        }

        let file = result.map_err(|e| format!("{e:?}"))?;
        if let Some(Statement::Definition(def)) = file.statements.first() {
            if let Definition::Struct(s) = &**def {
                if let Some(field) = s.fields.first() {
                    match &field.ty {
                        Type::Dictionary { key, value } => {
                            if **key != Type::Primitive(PrimitiveType::String) {
                                return Err(format!(
                                    "{:?} != {:?}",
                                    **key,
                                    Type::Primitive(PrimitiveType::String)
                                )
                                .into());
                            }
                            if **value != Type::Primitive(PrimitiveType::String) {
                                return Err(format!(
                                    "{:?} != {:?}",
                                    **value,
                                    Type::Primitive(PrimitiveType::String)
                                )
                                .into());
                            }
                        }
                        Type::Primitive(_)
                        | Type::Ident(_)
                        | Type::Generic { .. }
                        | Type::Array(_)
                        | Type::Optional(_)
                        | Type::Tuple(_)
                        | Type::Closure { .. }
                        | Type::TypeParameter(_) => {
                            return Err(
                                format!("Expected Dictionary type, got {:?}", field.ty).into()
                            )
                        }
                    }
                } else {
                    return Err("No fields found".into());
                }
            } else {
                return Err("No struct found".into());
            }
        } else {
            return Err("No definition found".into());
        }
        Ok(())
    }

    #[test]
    fn test_nested_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("[String: [Number: Boolean]]");
        if result.is_err() {
            return Err(format!("Failed to parse nested dictionary type: : {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Dictionary { key, value } => {
                if *key != Type::Primitive(PrimitiveType::String) {
                    return Err(format!(
                        "{:?} != {:?}",
                        *key,
                        Type::Primitive(PrimitiveType::String)
                    )
                    .into());
                }
                match *value {
                    Type::Dictionary {
                        key: inner_key,
                        value: inner_value,
                    } => {
                        if *inner_key != Type::Primitive(PrimitiveType::Number) {
                            return Err(format!(
                                "{:?} != {:?}",
                                *inner_key,
                                Type::Primitive(PrimitiveType::Number)
                            )
                            .into());
                        }
                        if *inner_value != Type::Primitive(PrimitiveType::Boolean) {
                            return Err(format!(
                                "{:?} != {:?}",
                                *inner_value,
                                Type::Primitive(PrimitiveType::Boolean)
                            )
                            .into());
                        }
                    }
                    Type::Primitive(_)
                    | Type::Ident(_)
                    | Type::Generic { .. }
                    | Type::Array(_)
                    | Type::Optional(_)
                    | Type::Tuple(_)
                    | Type::Closure { .. }
                    | Type::TypeParameter(_) => {
                        return Err(format!("Expected inner Dictionary type, got {value:?}").into())
                    }
                }
            }
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Closure { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Dictionary type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_optional_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("[String: Number]?");
        if result.is_err() {
            return Err(format!("Failed to parse optional dictionary type: : {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Optional(inner) => match *inner {
                Type::Dictionary { key, value } => {
                    if *key != Type::Primitive(PrimitiveType::String) {
                        return Err(format!(
                            "{:?} != {:?}",
                            *key,
                            Type::Primitive(PrimitiveType::String)
                        )
                        .into());
                    }
                    if *value != Type::Primitive(PrimitiveType::Number) {
                        return Err(format!(
                            "{:?} != {:?}",
                            *value,
                            Type::Primitive(PrimitiveType::Number)
                        )
                        .into());
                    }
                }
                Type::Primitive(_)
                | Type::Ident(_)
                | Type::Generic { .. }
                | Type::Array(_)
                | Type::Optional(_)
                | Type::Tuple(_)
                | Type::Closure { .. }
                | Type::TypeParameter(_) => {
                    return Err(
                        format!("Expected Dictionary type inside Optional, got {inner:?}").into(),
                    )
                }
            },
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::Closure { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Optional type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_dictionary_with_custom_types() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("[UserId: UserData]");
        if result.is_err() {
            return Err(
                format!("Failed to parse dictionary with custom types: : {result:?}").into(),
            );
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Dictionary { key, value } => match (*key, *value) {
                (Type::Ident(k), Type::Ident(v)) => {
                    if k.name != "UserId" {
                        return Err(format!("expected {:?} == {:?}", k.name, "UserId").into());
                    }
                    if v.name != "UserData" {
                        return Err(format!("expected {:?} == {:?}", v.name, "UserData").into());
                    }
                }
                _ => return Err("Expected Ident types".into()),
            },
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Closure { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Dictionary type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    // Helper to parse an expression from let binding
    fn parse_expr_from_let(input: &str) -> Result<Expr, Vec<(String, CustomSpan)>> {
        let wrapper = format!("let x = {input}");
        let tokens = Lexer::tokenize_all(&wrapper);
        let result = parse_file(&tokens)?;

        if let Some(Statement::Let(binding)) = result.statements.first() {
            return Ok(binding.value.clone());
        }
        Err(vec![(
            "Could not extract expression".to_string(),
            CustomSpan::default(),
        )])
    }

    #[test]
    fn test_dictionary_literal_parsing() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("[\"key\": 42, \"name\": 100]");
        if result.is_err() {
            return Err(format!("Failed to parse dictionary literal: : {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::DictLiteral { entries, .. } => {
                if entries.len() != 2 {
                    return Err(format!("expected {:?} == {:?}", entries.len(), 2).into());
                }
                // Check first entry
                #[expect(
                    clippy::indexing_slicing,
                    reason = "bounds checked above: entries.len() == 2"
                )]
                let (first_key, first_val) = (&entries[0].0, &entries[0].1);
                match (first_key, first_val) {
                    (Expr::Literal(Literal::String(k)), Expr::Literal(Literal::Number(v))) => {
                        if k != "key" {
                            return Err(format!("expected {:?} == {:?}", k, "key").into());
                        }
                        if (*v - 42.0_f64).abs() > f64::EPSILON {
                            return Err(format!("expected {:?} == {:?}", *v, 42.0).into());
                        }
                    }
                    _ => return Err("Expected string key and number value".into()),
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => {
                return Err(format!("Expected DictLiteral, got {expr:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_empty_dictionary_literal() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("[:]");
        if result.is_err() {
            return Err(format!("Failed to parse empty dictionary: : {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::DictLiteral { entries, .. } => {
                if !entries.is_empty() {
                    return Err("Expected empty entries".into());
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => {
                return Err(format!("Expected DictLiteral, got {expr:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_dictionary_access_parsing() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("data[\"key\"]");
        if result.is_err() {
            return Err(format!("Failed to parse dictionary access: : {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::DictAccess { dict, key, .. } => match (*dict, *key) {
                (Expr::Reference { path, .. }, Expr::Literal(Literal::String(k))) => {
                    let first = path.first().ok_or("expected at least one path segment")?;
                    if first.name != "data" {
                        return Err(format!("expected {:?} == {:?}", first.name, "data").into());
                    }
                    if k != "key" {
                        return Err(format!("expected {:?} == {:?}", k, "key").into());
                    }
                }
                _ => return Err("Expected reference and string key".into()),
            },
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => return Err(format!("Expected DictAccess, got {expr:?}").into()),
        }
        Ok(())
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "match expression over all Expr variants — exhaustive arms cannot be extracted without losing context"
    )]
    fn test_chained_dictionary_access() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("data[\"outer\"][\"inner\"]");
        if result.is_err() {
            return Err(format!("Failed to parse chained dict access: : {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::DictAccess { dict, key, .. } => {
                // Outer access: dict is another DictAccess, key is "inner"
                match (*key,) {
                    (Expr::Literal(Literal::String(k)),) => {
                        if k != "inner" {
                            return Err(format!("expected {:?} == {:?}", k, "inner").into());
                        }
                    }
                    _ => return Err("Expected string key 'inner'".into()),
                }
                match *dict {
                    Expr::DictAccess {
                        dict: inner_dict,
                        key: inner_key,
                        ..
                    } => {
                        match (*inner_key,) {
                            (Expr::Literal(Literal::String(k)),) => {
                                if k != "outer" {
                                    return Err(format!("expected {:?} == {:?}", k, "outer").into());
                                }
                            }
                            _ => return Err("Expected string key 'outer'".into()),
                        }
                        match *inner_dict {
                            Expr::Reference { path, .. } => {
                                let first =
                                    path.first().ok_or("expected at least one path segment")?;
                                if first.name != "data" {
                                    return Err(format!(
                                        "expected {:?} == {:?}",
                                        first.name, "data"
                                    )
                                    .into());
                                }
                            }
                            Expr::Literal(_)
                            | Expr::Invocation { .. }
                            | Expr::EnumInstantiation { .. }
                            | Expr::InferredEnumInstantiation { .. }
                            | Expr::Array { .. }
                            | Expr::Tuple { .. }
                            | Expr::BinaryOp { .. }
                            | Expr::UnaryOp { .. }
                            | Expr::ForExpr { .. }
                            | Expr::IfExpr { .. }
                            | Expr::MatchExpr { .. }
                            | Expr::Group { .. }
                            | Expr::DictLiteral { .. }
                            | Expr::DictAccess { .. }
                            | Expr::FieldAccess { .. }
                            | Expr::ClosureExpr { .. }
                            | Expr::LetExpr { .. }
                            | Expr::MethodCall { .. }
                            | Expr::Block { .. } => return Err("Expected reference 'data'".into()),
                        }
                    }
                    Expr::Literal(_)
                    | Expr::Invocation { .. }
                    | Expr::EnumInstantiation { .. }
                    | Expr::InferredEnumInstantiation { .. }
                    | Expr::Array { .. }
                    | Expr::Tuple { .. }
                    | Expr::Reference { .. }
                    | Expr::BinaryOp { .. }
                    | Expr::UnaryOp { .. }
                    | Expr::ForExpr { .. }
                    | Expr::IfExpr { .. }
                    | Expr::MatchExpr { .. }
                    | Expr::Group { .. }
                    | Expr::DictLiteral { .. }
                    | Expr::FieldAccess { .. }
                    | Expr::ClosureExpr { .. }
                    | Expr::LetExpr { .. }
                    | Expr::MethodCall { .. }
                    | Expr::Block { .. } => return Err("Expected inner DictAccess".into()),
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => return Err(format!("Expected DictAccess, got {expr:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn test_dictionary_with_expression_key() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("data[index]");
        if result.is_err() {
            return Err(format!("Failed to parse dict access with expr key: : {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::DictAccess { dict, key, .. } => match (*dict, *key) {
                (Expr::Reference { path: d, .. }, Expr::Reference { path: k, .. }) => {
                    let d0 = d
                        .first()
                        .ok_or("expected at least one segment in dict path")?;
                    if d0.name != "data" {
                        return Err(format!("expected {:?} == {:?}", d0.name, "data").into());
                    }
                    let k0 = k
                        .first()
                        .ok_or("expected at least one segment in key path")?;
                    if k0.name != "index" {
                        return Err(format!("expected {:?} == {:?}", k0.name, "index").into());
                    }
                }
                _ => return Err("Expected two references".into()),
            },
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => return Err(format!("Expected DictAccess, got {expr:?}").into()),
        }
        Ok(())
    }

    // Closure type tests
    #[test]
    fn test_closure_type_no_params() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("() -> Event");
        if result.is_err() {
            return Err(format!("Failed to parse () -> Event: {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Closure { params, ret } => {
                if !params.is_empty() {
                    return Err("Expected empty params".into());
                }
                match *ret {
                    Type::Ident(ident) => {
                        if ident.name != "Event" {
                            return Err(format!("{:?} != {:?}", ident.name, "Event").into());
                        }
                    }
                    Type::Primitive(_)
                    | Type::Generic { .. }
                    | Type::Array(_)
                    | Type::Optional(_)
                    | Type::Tuple(_)
                    | Type::Dictionary { .. }
                    | Type::Closure { .. }
                    | Type::TypeParameter(_) => return Err("Expected Ident return type".into()),
                }
            }
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Closure type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_closure_type_single_param() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("String -> Event");
        if result.is_err() {
            return Err(format!("Failed to parse String -> Event: : {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Closure { params, ret } => {
                if params.len() != 1 {
                    return Err(format!("expected {:?} == {:?}", params.len(), 1).into());
                }
                let (_, p0) = params.first().ok_or("expected at least one param")?;
                if *p0 != Type::Primitive(PrimitiveType::String) {
                    return Err(format!(
                        "{:?} != {:?}",
                        p0,
                        Type::Primitive(PrimitiveType::String)
                    )
                    .into());
                }
                match *ret {
                    Type::Ident(ident) => {
                        if ident.name != "Event" {
                            return Err(format!("{:?} != {:?}", ident.name, "Event").into());
                        }
                    }
                    Type::Primitive(_)
                    | Type::Generic { .. }
                    | Type::Array(_)
                    | Type::Optional(_)
                    | Type::Tuple(_)
                    | Type::Dictionary { .. }
                    | Type::Closure { .. }
                    | Type::TypeParameter(_) => return Err("Expected Ident return type".into()),
                }
            }
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Closure type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_closure_type_multi_params() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("Number, Number -> Point");
        if result.is_err() {
            return Err(format!("Failed to parse Number, Number -> Point: : {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Closure { params, ret } => {
                if params.len() != 2 {
                    return Err(format!("expected {:?} == {:?}", params.len(), 2).into());
                }
                let (_, p0) = params.first().ok_or("expected at least 1 param")?;
                if *p0 != Type::Primitive(PrimitiveType::Number) {
                    return Err(format!(
                        "{:?} != {:?}",
                        p0,
                        Type::Primitive(PrimitiveType::Number)
                    )
                    .into());
                }
                let (_, p1) = params.get(1).ok_or("expected at least 2 params")?;
                if *p1 != Type::Primitive(PrimitiveType::Number) {
                    return Err(format!(
                        "{:?} != {:?}",
                        p1,
                        Type::Primitive(PrimitiveType::Number)
                    )
                    .into());
                }
                match *ret {
                    Type::Ident(ident) => {
                        if ident.name != "Point" {
                            return Err(format!("{:?} != {:?}", ident.name, "Point").into());
                        }
                    }
                    Type::Primitive(_)
                    | Type::Generic { .. }
                    | Type::Array(_)
                    | Type::Optional(_)
                    | Type::Tuple(_)
                    | Type::Dictionary { .. }
                    | Type::Closure { .. }
                    | Type::TypeParameter(_) => return Err("Expected Ident return type".into()),
                }
            }
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Closure type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_optional_closure_type() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_type_str("(String -> Event)?");
        if result.is_err() {
            return Err(format!("Failed to parse (String -> Event)?: : {result:?}").into());
        }
        let ty = result.map_err(|e| format!("{e:?}"))?;
        match ty {
            Type::Optional(inner) => match *inner {
                Type::Closure { params, .. } => {
                    if params.len() != 1 {
                        return Err(format!("expected {:?} == {:?}", params.len(), 1).into());
                    }
                }
                Type::Primitive(_)
                | Type::Ident(_)
                | Type::Generic { .. }
                | Type::Array(_)
                | Type::Optional(_)
                | Type::Tuple(_)
                | Type::Dictionary { .. }
                | Type::TypeParameter(_) => return Err("Expected Closure inside Optional".into()),
            },
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::Closure { .. }
            | Type::TypeParameter(_) => {
                return Err(format!("Expected Optional type, got {ty:?}").into())
            }
        }
        Ok(())
    }

    // Closure expression tests
    #[test]
    fn test_closure_expr_no_params() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("() -> .submit");
        if result.is_err() {
            return Err(format!("Failed to parse () -> .submit: : {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::ClosureExpr { params, body, .. } => {
                if !params.is_empty() {
                    return Err(
                        format!("params should be empty, has {} items", params.len()).into(),
                    );
                }
                match *body {
                    Expr::InferredEnumInstantiation { variant, .. } => {
                        if variant.name != "submit" {
                            return Err(
                                format!("expected {:?} == {:?}", variant.name, "submit").into()
                            );
                        }
                    }
                    Expr::Literal(_)
                    | Expr::Invocation { .. }
                    | Expr::EnumInstantiation { .. }
                    | Expr::Array { .. }
                    | Expr::Tuple { .. }
                    | Expr::Reference { .. }
                    | Expr::BinaryOp { .. }
                    | Expr::UnaryOp { .. }
                    | Expr::ForExpr { .. }
                    | Expr::IfExpr { .. }
                    | Expr::MatchExpr { .. }
                    | Expr::Group { .. }
                    | Expr::DictLiteral { .. }
                    | Expr::DictAccess { .. }
                    | Expr::FieldAccess { .. }
                    | Expr::ClosureExpr { .. }
                    | Expr::LetExpr { .. }
                    | Expr::MethodCall { .. }
                    | Expr::Block { .. } => {
                        return Err(
                            format!("Expected InferredEnumInstantiation, got {body:?}").into()
                        )
                    }
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => {
                return Err(format!("Expected ClosureExpr, got {expr:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_closure_expr_single_param() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("x -> .changed(value: x)");
        if result.is_err() {
            return Err(format!("Failed to parse x -> .changed(...): : {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::ClosureExpr { params, .. } => {
                if params.len() != 1 {
                    return Err(format!("expected {:?} == {:?}", params.len(), 1).into());
                }
                let p0 = params.first().ok_or("expected at least one param")?;
                if p0.name.name != "x" {
                    return Err(format!("expected {:?} == {:?}", p0.name.name, "x").into());
                }
                if p0.ty.is_some() {
                    return Err("params[0].ty should be None".into());
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => {
                return Err(format!("Expected ClosureExpr, got {expr:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_closure_expr_multi_params() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("w, h -> .resized(width: w, height: h)");
        if result.is_err() {
            return Err(format!("Failed to parse w, h -> ...: {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::ClosureExpr { params, .. } => {
                if params.len() != 2 {
                    return Err(format!("expected {:?} == {:?}", params.len(), 2).into());
                }
                let p0 = params.first().ok_or("expected at least 1 param")?;
                if p0.name.name != "w" {
                    return Err(format!("expected {:?} == {:?}", p0.name.name, "w").into());
                }
                let p1 = params.get(1).ok_or("expected at least 2 params")?;
                if p1.name.name != "h" {
                    return Err(format!("expected {:?} == {:?}", p1.name.name, "h").into());
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => {
                return Err(format!("Expected ClosureExpr, got {expr:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_closure_expr_with_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("x: String -> .textChanged(value: x)");
        if result.is_err() {
            return Err(format!("Failed to parse x: String -> ...: : {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::ClosureExpr { params, .. } => {
                if params.len() != 1 {
                    return Err(format!("expected {:?} == {:?}", params.len(), 1).into());
                }
                let p0 = params.first().ok_or("expected at least one param")?;
                if p0.name.name != "x" {
                    return Err(format!("expected {:?} == {:?}", p0.name.name, "x").into());
                }
                match &p0.ty {
                    Some(Type::Primitive(PrimitiveType::String)) => {}
                    _ => return Err("Expected String type annotation".into()),
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => {
                return Err(format!("Expected ClosureExpr, got {expr:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_closure_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
        let input = r"
            pub struct Button<E> {
                action: () -> E
            }
        ";
        let tokens = Lexer::tokenize_all(input);
        let result = parse_file(&tokens);
        if result.is_err() {
            return Err(format!("Failed to parse struct with closure field: : {result:?}").into());
        }
        Ok(())
    }

    // Let expression tests
    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "match expression over all Expr variants — exhaustive arms cannot be extracted without losing context"
    )]
    fn test_let_expr_basic() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("let x = 42 x");
        if result.is_err() {
            return Err(format!("Failed to parse let x = 42 x: {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::LetExpr {
                mutable,
                pattern,
                ty,
                value,
                body,
                ..
            } => {
                if mutable {
                    return Err("expected !mutable, but was true".into());
                }
                match pattern {
                    BindingPattern::Simple(ident) => {
                        if ident.name != "x" {
                            return Err(format!("{:?} != {:?}", ident.name, "x").into());
                        }
                    }
                    BindingPattern::Array { .. }
                    | BindingPattern::Struct { .. }
                    | BindingPattern::Tuple { .. } => return Err("Expected simple pattern".into()),
                }
                if ty.is_some() {
                    return Err(format!("ty should be None but got {ty:?}").into());
                }
                match *value {
                    Expr::Literal(Literal::Number(n)) => {
                        if (n - 42.0_f64).abs() > f64::EPSILON {
                            return Err(format!("{:?} != {:?}", n, 42.0).into());
                        }
                    }
                    Expr::Literal(_)
                    | Expr::Invocation { .. }
                    | Expr::EnumInstantiation { .. }
                    | Expr::InferredEnumInstantiation { .. }
                    | Expr::Array { .. }
                    | Expr::Tuple { .. }
                    | Expr::Reference { .. }
                    | Expr::BinaryOp { .. }
                    | Expr::UnaryOp { .. }
                    | Expr::ForExpr { .. }
                    | Expr::IfExpr { .. }
                    | Expr::MatchExpr { .. }
                    | Expr::Group { .. }
                    | Expr::DictLiteral { .. }
                    | Expr::DictAccess { .. }
                    | Expr::FieldAccess { .. }
                    | Expr::ClosureExpr { .. }
                    | Expr::LetExpr { .. }
                    | Expr::MethodCall { .. }
                    | Expr::Block { .. } => return Err("Expected number literal".into()),
                }
                match *body {
                    Expr::Reference { path, .. } => {
                        let first = path.first().ok_or("expected at least one path segment")?;
                        if first.name != "x" {
                            return Err(format!("{:?} != {:?}", first.name, "x").into());
                        }
                    }
                    Expr::Literal(_)
                    | Expr::Invocation { .. }
                    | Expr::EnumInstantiation { .. }
                    | Expr::InferredEnumInstantiation { .. }
                    | Expr::Array { .. }
                    | Expr::Tuple { .. }
                    | Expr::BinaryOp { .. }
                    | Expr::UnaryOp { .. }
                    | Expr::ForExpr { .. }
                    | Expr::IfExpr { .. }
                    | Expr::MatchExpr { .. }
                    | Expr::Group { .. }
                    | Expr::DictLiteral { .. }
                    | Expr::DictAccess { .. }
                    | Expr::FieldAccess { .. }
                    | Expr::ClosureExpr { .. }
                    | Expr::LetExpr { .. }
                    | Expr::MethodCall { .. }
                    | Expr::Block { .. } => return Err("Expected reference in body".into()),
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => return Err(format!("Expected LetExpr, got {expr:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn test_let_expr_with_type() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("let count: Number = 100 count");
        if result.is_err() {
            return Err(format!("Failed to parse let with type: : {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::LetExpr { pattern, ty, .. } => {
                match pattern {
                    BindingPattern::Simple(ident) => {
                        if ident.name != "count" {
                            return Err(format!("{:?} != {:?}", ident.name, "count").into());
                        }
                    }
                    BindingPattern::Array { .. }
                    | BindingPattern::Struct { .. }
                    | BindingPattern::Tuple { .. } => return Err("Expected simple pattern".into()),
                }
                match ty {
                    Some(Type::Primitive(PrimitiveType::Number)) => {}
                    _ => return Err("Expected Number type annotation".into()),
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => return Err(format!("Expected LetExpr, got {expr:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn test_let_expr_mutable() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("let mut counter = 0 counter");
        if result.is_err() {
            return Err(format!("Failed to parse let mut: {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::LetExpr {
                mutable, pattern, ..
            } => {
                if !mutable {
                    return Err("expected mutable to be true".into());
                }
                match pattern {
                    BindingPattern::Simple(ident) => {
                        if ident.name != "counter" {
                            return Err(format!("{:?} != {:?}", ident.name, "counter").into());
                        }
                    }
                    BindingPattern::Array { .. }
                    | BindingPattern::Struct { .. }
                    | BindingPattern::Tuple { .. } => return Err("Expected simple pattern".into()),
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => return Err(format!("Expected LetExpr, got {expr:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn test_let_expr_in_for() -> Result<(), Box<dyn std::error::Error>> {
        let input = r"
            struct App {
                content: [String] = for item in items {
                    let formatted = item
                    Label(text: formatted)
                }
            }
        ";
        let tokens = Lexer::tokenize_all(input);
        let result = parse_file(&tokens);
        if result.is_err() {
            return Err(format!("Failed to parse let in for block: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_nested_let_exprs() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("let x = 1 let y = 2 x");
        if result.is_err() {
            return Err(format!("Failed to parse nested let: {result:?}").into());
        }
        let expr = result.map_err(|e| format!("{e:?}"))?;
        match expr {
            Expr::LetExpr { pattern, body, .. } => {
                match pattern {
                    BindingPattern::Simple(ident) => {
                        if ident.name != "x" {
                            return Err(format!("{:?} != {:?}", ident.name, "x").into());
                        }
                    }
                    BindingPattern::Array { .. }
                    | BindingPattern::Struct { .. }
                    | BindingPattern::Tuple { .. } => return Err("Expected simple pattern".into()),
                }
                match *body {
                    Expr::LetExpr {
                        pattern: inner_pattern,
                        ..
                    } => match inner_pattern {
                        BindingPattern::Simple(ident) => {
                            if ident.name != "y" {
                                return Err(format!("{:?} != {:?}", ident.name, "y").into());
                            }
                        }
                        BindingPattern::Array { .. }
                        | BindingPattern::Struct { .. }
                        | BindingPattern::Tuple { .. } => {
                            return Err("Expected simple pattern".into())
                        }
                    },
                    Expr::Literal(_)
                    | Expr::Invocation { .. }
                    | Expr::EnumInstantiation { .. }
                    | Expr::InferredEnumInstantiation { .. }
                    | Expr::Array { .. }
                    | Expr::Tuple { .. }
                    | Expr::Reference { .. }
                    | Expr::BinaryOp { .. }
                    | Expr::UnaryOp { .. }
                    | Expr::ForExpr { .. }
                    | Expr::IfExpr { .. }
                    | Expr::MatchExpr { .. }
                    | Expr::Group { .. }
                    | Expr::DictLiteral { .. }
                    | Expr::DictAccess { .. }
                    | Expr::FieldAccess { .. }
                    | Expr::ClosureExpr { .. }
                    | Expr::MethodCall { .. }
                    | Expr::Block { .. } => return Err("Expected nested LetExpr".into()),
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => return Err(format!("Expected LetExpr, got {expr:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn test_block_expr_simple() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("{ let x = 1 x }");
        if result.is_err() {
            return Err(format!("Failed to parse block: {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_block_expr_with_call() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_expr_from_let("{ let v = foo.bar(1) Result(value: v) }");
        if result.is_err() {
            return Err(format!("Failed to parse block with call: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_block_expr_no_let() -> Result<(), Box<dyn std::error::Error>> {
        // Block with just a result expression (no let statements)
        let result = parse_expr_from_let("{ Result(value: 1) }");
        if result.is_err() {
            return Err(format!("Failed to parse block no let: {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_block_expr_let_simple_then_call() -> Result<(), Box<dyn std::error::Error>> {
        // Block with let binding a literal, then a call
        let result = parse_expr_from_let("{ let v = 1 Result(value: v) }");
        if result.is_err() {
            return Err(format!("Failed to parse block let simple then call: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_block_expr_let_field_access() -> Result<(), Box<dyn std::error::Error>> {
        // Block with let binding field access, then a reference
        let result = parse_expr_from_let("{ let v = foo.bar v }");
        if result.is_err() {
            return Err(format!("Failed to parse block let field access: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_block_expr_let_call_then_ref() -> Result<(), Box<dyn std::error::Error>> {
        // Block with let binding a call, then a reference
        let result = parse_expr_from_let("{ let v = foo(1) v }");
        if result.is_err() {
            return Err(format!("Failed to parse block let call then ref: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_block_expr_let_method_call_then_ref() -> Result<(), Box<dyn std::error::Error>> {
        // Block with let binding a method call, then a reference
        let result = parse_expr_from_let("{ let v = foo.bar(1) v }");
        if result.is_err() {
            return Err(
                format!("Failed to parse block let method call then ref: : {result:?}").into(),
            );
        }
        Ok(())
    }

    #[test]
    fn test_let_expr_method_call_then_ref() -> Result<(), Box<dyn std::error::Error>> {
        // Let expression with method call value, then reference body
        // This uses the let EXPRESSION, not block statement
        let result = parse_expr_from_let("let v = foo.bar(1) v");
        if result.is_err() {
            return Err(
                format!("Failed to parse let expr method call then ref: : {result:?}").into(),
            );
        }
        Ok(())
    }

    #[test]
    fn test_let_expr_fn_call_then_ref() -> Result<(), Box<dyn std::error::Error>> {
        // Let expression with function call value, then reference body
        let result = parse_expr_from_let("let v = foo(1) v");
        if result.is_err() {
            return Err(format!("Failed to parse let expr fn call then ref: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_let_expr_field_access_then_ref() -> Result<(), Box<dyn std::error::Error>> {
        // Let expression with field access value, then reference body
        let result = parse_expr_from_let("let v = foo.bar v");
        if result.is_err() {
            return Err(
                format!("Failed to parse let expr field access then ref: : {result:?}").into(),
            );
        }
        Ok(())
    }

    #[test]
    fn test_method_call_standalone() -> Result<(), Box<dyn std::error::Error>> {
        // Just a method call, no following expression
        let result = parse_expr_from_let("foo.bar(1)");
        if result.is_err() {
            return Err(format!("Failed to parse standalone method call: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_method_call_no_args() -> Result<(), Box<dyn std::error::Error>> {
        // Method call with no args
        let result = parse_expr_from_let("foo.bar()");
        if result.is_err() {
            return Err(format!("Failed to parse method call no args: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_field_access_standalone() -> Result<(), Box<dyn std::error::Error>> {
        // Field access (no parens)
        let result = parse_expr_from_let("foo.bar");
        if result.is_err() {
            return Err(format!("Failed to parse field access: {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_reference_standalone() -> Result<(), Box<dyn std::error::Error>> {
        // Just a reference
        let result = parse_expr_from_let("foo");
        if result.is_err() {
            return Err(format!("Failed to parse reference: {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_method_call_on_self() -> Result<(), Box<dyn std::error::Error>> {
        // Method call on self
        let result = parse_expr_from_let("self.bar(1)");
        if result.is_err() {
            return Err(format!("Failed to parse method call on self: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_method_call_on_this() -> Result<(), Box<dyn std::error::Error>> {
        // Method call on 'this' (not a keyword, just an identifier)
        let result = parse_expr_from_let("this.bar(1)");
        if result.is_err() {
            return Err(format!("Failed to parse method call on this: : {result:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_invocation_simple() -> Result<(), Box<dyn std::error::Error>> {
        // Simple invocation (should work)
        let result = parse_expr_from_let("foo(1)");
        if result.is_err() {
            return Err(format!("Failed to parse invocation: {result:?}").into());
        }
        Ok(())
    }
}
