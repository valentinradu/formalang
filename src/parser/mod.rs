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
    BlockStatement, Definition, Expr, File, Ident, LetBinding, Literal, Statement, UseItems,
    UseStmt, Visibility,
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

/// Parse a complete file.
///
/// Each statement carries a recovery strategy that, on parse failure,
/// skips input tokens until the parser finds a token that can legally
/// start the next top-level statement (or end of input). This lets the
/// parser surface multiple independent syntax errors in one pass
/// instead of bailing on the first bad token.
fn file_parser<'tokens, I>(
) -> impl Parser<'tokens, I, File, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let statement_start = one_of([
        Token::Use,
        Token::Let,
        Token::Pub,
        Token::Struct,
        Token::Enum,
        Token::Trait,
        Token::Impl,
        Token::Fn,
        Token::Extern,
        Token::Module,
    ])
    .ignored();

    statement_parser()
        .recover_with(skip_then_retry_until(
            any().ignored(),
            statement_start.rewind().ignored().or(end()),
        ))
        .repeated()
        .collect::<Vec<_>>()
        .map_with(|statements, e| File {
            format_version: crate::ast::FORMAT_VERSION,
            statements,
            span: span_from_simple(e.span()),
        })
}

/// Parse a top-level statement, optionally preceded by `///` doc-comment
/// lines. The collected doc text is attached to the resulting definition
/// or let binding.
fn statement_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Statement, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    doc_comments_parser()
        .then(choice((
            use_stmt_parser().map(Statement::Use),
            let_binding_parser().map(|lb| Statement::Let(Box::new(lb))),
            definition_parser().map(|d| Statement::Definition(Box::new(d))),
        )))
        .map(|(doc, stmt)| attach_doc_to_statement(doc, stmt))
        .labelled("statement (use, let, or definition: struct, enum, trait, impl, fn, extern, mod)")
}

/// Consume zero or more leading `///` doc-comment lines and join them
/// with newlines. Returns `None` when no doc comments precede the next
/// item. Inner `//!` comments are skipped at this level — they belong
/// to the enclosing scope and are handled by the file-level parser.
pub(super) fn doc_comments_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Option<String>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    select! { Token::DocComment(s) => s }
        .repeated()
        .collect::<Vec<_>>()
        .map(|lines| {
            if lines.is_empty() {
                None
            } else {
                Some(lines.join("\n"))
            }
        })
}

/// Attach a captured doc-comment string to whichever AST node the
/// statement carries. `Use` statements don't currently support docs and
/// silently drop the captured text.
fn attach_doc_to_statement(doc: Option<String>, stmt: Statement) -> Statement {
    let Some(doc) = doc else {
        return stmt;
    };
    match stmt {
        Statement::Let(mut lb) => {
            lb.doc = Some(doc);
            Statement::Let(lb)
        }
        Statement::Definition(def) => {
            Statement::Definition(Box::new(attach_doc_to_definition(doc, *def)))
        }
        Statement::Use(_) => stmt,
    }
}

fn attach_doc_to_definition(doc: String, def: Definition) -> Definition {
    match def {
        Definition::Function(mut f) => {
            f.doc = Some(doc);
            Definition::Function(f)
        }
        Definition::Struct(mut s) => {
            s.doc = Some(doc);
            Definition::Struct(s)
        }
        Definition::Trait(mut t) => {
            t.doc = Some(doc);
            Definition::Trait(t)
        }
        Definition::Enum(mut e) => {
            e.doc = Some(doc);
            Definition::Enum(e)
        }
        Definition::Impl(mut i) => {
            i.doc = Some(doc);
            Definition::Impl(i)
        }
        Definition::Module(mut m) => {
            m.doc = Some(doc);
            Definition::Module(m)
        }
    }
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
            // When no trailing `::<items>` block is present, treat the last
            // path segment as the imported item. The path parser above is
            // `at_least(1)`, so `path.pop()` always yields a segment — the
            // fallback is only reachable if that invariant is ever broken,
            // in which case we surface an empty-name ident that downstream
            // symbol lookup will report as `ItemNotFound`.
            let items = items.unwrap_or_else(|| {
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
                doc: None,
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
        return Expr::Literal {
            value: Literal::Nil,
            span,
        };
    }

    // Last item becomes the result expression
    let Some(last) = statements.pop() else {
        return Expr::Literal {
            value: Literal::Nil,
            span,
        };
    };
    let result = match last {
        BlockStatement::Expr(expr) => expr,
        // If last is a statement (not expr), push it back and use Nil as result
        stmt @ (BlockStatement::Let { .. } | BlockStatement::Assign { .. }) => {
            statements.push(stmt);
            Expr::Literal {
                value: Literal::Nil,
                span,
            }
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
mod tests;
