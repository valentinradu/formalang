// Parser for FormaLang using chumsky parser combinator library
//
// This module converts tokens into an Abstract Syntax Tree (AST).
// It uses recursive descent parsing with excellent error recovery.

use chumsky::input::{Stream, ValueInput};
use chumsky::pratt::*;
use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;
use crate::location::Span as CustomSpan;

/// Main entry point for parsing
/// Returns the parsed AST or a vector of (error_message, span) tuples
pub fn parse_file(tokens: &[(Token, CustomSpan)]) -> Result<File, Vec<(String, CustomSpan)>> {
    parse_file_internal(tokens, None)
}

/// Parse with source text for better error positions
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
    let end_offset = tokens.last().map(|(_, s)| s.end.offset).unwrap_or(0);
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
                    let custom_span = if let Some(src) = source {
                        // Compute line/column from source text
                        CustomSpan::from_range_with_source(simple_span.start, simple_span.end, src)
                    } else {
                        // Use span from tokens if available
                        tokens
                            .iter()
                            .find(|(_, span)| {
                                span.start.offset == simple_span.start
                                    && span.end.offset == simple_span.end
                            })
                            .map(|(_, span)| *span)
                            .unwrap_or_else(|| span_from_simple(*simple_span))
                    };
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

/// Fill in line/column information for all spans in the AST using source text
fn fill_file_spans(file: &mut File, source: &str) {
    for stmt in &mut file.statements {
        fill_statement_span(stmt, source);
    }
}

/// Helper to fill a span's line/column info from source
fn fill_span(span: &mut CustomSpan, source: &str) {
    if span.start.line == 0 && span.end.line == 0 {
        *span = CustomSpan::from_range_with_source(span.start.offset, span.end.offset, source);
    }
}

/// Fill spans in a statement
fn fill_statement_span(stmt: &mut Statement, source: &str) {
    match stmt {
        Statement::Use(use_stmt) => {
            for ident in &mut use_stmt.path {
                fill_span(&mut ident.span, source);
            }
            match &mut use_stmt.items {
                UseItems::Single(ident) => fill_span(&mut ident.span, source),
                UseItems::Multiple(idents) => {
                    for ident in idents {
                        fill_span(&mut ident.span, source);
                    }
                }
                UseItems::Glob => {} // No spans to fill for glob
            }
            fill_span(&mut use_stmt.span, source);
        }
        Statement::Let(let_stmt) => {
            fill_binding_pattern_span(&mut let_stmt.pattern, source);
            fill_expr_span(&mut let_stmt.value, source);
            fill_span(&mut let_stmt.span, source);
        }
        Statement::Definition(def) => fill_definition_span(def, source),
    }
}

/// Fill spans in a definition
fn fill_definition_span(def: &mut Definition, source: &str) {
    match def {
        Definition::Module(m) => {
            fill_span(&mut m.name.span, source);
            for def in &mut m.definitions {
                fill_definition_span(def, source);
            }
            fill_span(&mut m.span, source);
        }
        Definition::Trait(t) => {
            fill_span(&mut t.name.span, source);
            for base in &mut t.traits {
                fill_span(&mut base.span, source);
            }
            for param in &mut t.generics {
                fill_span(&mut param.name.span, source);
                for constraint in &mut param.constraints {
                    match constraint {
                        GenericConstraint::Trait(ident) => fill_span(&mut ident.span, source),
                    }
                }
                fill_span(&mut param.span, source);
            }
            for field in &mut t.fields {
                fill_span(&mut field.name.span, source);
                fill_type_span(&mut field.ty, source);
                fill_span(&mut field.span, source);
            }
            for mp in &mut t.mount_fields {
                fill_span(&mut mp.name.span, source);
                fill_type_span(&mut mp.ty, source);
                fill_span(&mut mp.span, source);
            }
            fill_span(&mut t.span, source);
        }
        Definition::Struct(s) => {
            fill_span(&mut s.name.span, source);
            for base in &mut s.traits {
                fill_span(&mut base.span, source);
            }
            for param in &mut s.generics {
                fill_span(&mut param.name.span, source);
                for constraint in &mut param.constraints {
                    match constraint {
                        GenericConstraint::Trait(ident) => fill_span(&mut ident.span, source),
                    }
                }
                fill_span(&mut param.span, source);
            }
            for field in &mut s.fields {
                fill_span(&mut field.name.span, source);
                fill_type_span(&mut field.ty, source);
                if let Some(default) = &mut field.default {
                    fill_expr_span(default, source);
                }
                fill_span(&mut field.span, source);
            }
            for mp in &mut s.mount_fields {
                fill_span(&mut mp.name.span, source);
                fill_type_span(&mut mp.ty, source);
                if let Some(default) = &mut mp.default {
                    fill_expr_span(default, source);
                }
                fill_span(&mut mp.span, source);
            }
            fill_span(&mut s.span, source);
        }
        Definition::Impl(i) => {
            fill_span(&mut i.name.span, source);
            for param in &mut i.generics {
                fill_span(&mut param.name.span, source);
                for constraint in &mut param.constraints {
                    match constraint {
                        GenericConstraint::Trait(ident) => fill_span(&mut ident.span, source),
                    }
                }
                fill_span(&mut param.span, source);
            }
            for (_field_name, default_expr) in &mut i.defaults {
                fill_expr_span(default_expr, source);
            }
            fill_span(&mut i.span, source);
        }
        Definition::Enum(e) => {
            fill_span(&mut e.name.span, source);
            for param in &mut e.generics {
                fill_span(&mut param.name.span, source);
                for constraint in &mut param.constraints {
                    match constraint {
                        GenericConstraint::Trait(ident) => fill_span(&mut ident.span, source),
                    }
                }
                fill_span(&mut param.span, source);
            }
            for variant in &mut e.variants {
                fill_span(&mut variant.name.span, source);
                for field in &mut variant.fields {
                    fill_span(&mut field.name.span, source);
                    fill_type_span(&mut field.ty, source);
                    fill_span(&mut field.span, source);
                }
                fill_span(&mut variant.span, source);
            }
            fill_span(&mut e.span, source);
        }
    }
}

/// Fill spans in a type
fn fill_type_span(ty: &mut Type, source: &str) {
    match ty {
        Type::Primitive(_) => {}
        Type::Ident(ident) => fill_span(&mut ident.span, source),
        Type::Array(inner) => fill_type_span(inner, source),
        Type::Optional(inner) => fill_type_span(inner, source),
        Type::Tuple(fields) => {
            for field in fields {
                fill_span(&mut field.name.span, source);
                fill_type_span(&mut field.ty, source);
                fill_span(&mut field.span, source);
            }
        }
        Type::Generic { name, args, span } => {
            fill_span(&mut name.span, source);
            for arg in args {
                fill_type_span(arg, source);
            }
            fill_span(span, source);
        }
        Type::TypeParameter(ident) => fill_span(&mut ident.span, source),
        Type::Dictionary { key, value } => {
            fill_type_span(key, source);
            fill_type_span(value, source);
        }
        Type::Closure { params, ret } => {
            for param in params {
                fill_type_span(param, source);
            }
            fill_type_span(ret, source);
        }
    }
}

/// Fill spans in an expression
fn fill_expr_span(expr: &mut Expr, source: &str) {
    match expr {
        Expr::Literal(_) => {} // Literals don't have mutable spans
        Expr::StructInstantiation {
            name,
            type_args,
            args,
            mounts,
            span,
        } => {
            fill_span(&mut name.span, source);
            for ty_arg in type_args {
                fill_type_span(ty_arg, source);
            }
            for (arg_name, arg_expr) in args {
                fill_span(&mut arg_name.span, source);
                fill_expr_span(arg_expr, source);
            }
            for (mount_name, mount_expr) in mounts {
                fill_span(&mut mount_name.span, source);
                fill_expr_span(mount_expr, source);
            }
            fill_span(span, source);
        }
        Expr::EnumInstantiation {
            enum_name,
            variant,
            data,
            span,
        } => {
            fill_span(&mut enum_name.span, source);
            fill_span(&mut variant.span, source);
            for (field_name, expr) in data {
                fill_span(&mut field_name.span, source);
                fill_expr_span(expr, source);
            }
            fill_span(span, source);
        }
        Expr::InferredEnumInstantiation {
            variant,
            data,
            span,
        } => {
            fill_span(&mut variant.span, source);
            for (field_name, expr) in data {
                fill_span(&mut field_name.span, source);
                fill_expr_span(expr, source);
            }
            fill_span(span, source);
        }
        Expr::Array { elements, span } => {
            for elem in elements {
                fill_expr_span(elem, source);
            }
            fill_span(span, source);
        }
        Expr::Tuple { fields, span } => {
            for (field_name, field_expr) in fields {
                fill_span(&mut field_name.span, source);
                fill_expr_span(field_expr, source);
            }
            fill_span(span, source);
        }
        Expr::Reference { path, span } => {
            for ident in path {
                fill_span(&mut ident.span, source);
            }
            fill_span(span, source);
        }
        Expr::BinaryOp {
            left, right, span, ..
        } => {
            fill_expr_span(left, source);
            fill_expr_span(right, source);
            fill_span(span, source);
        }
        Expr::ForExpr {
            var,
            collection,
            body,
            span,
        } => {
            fill_span(&mut var.span, source);
            fill_expr_span(collection, source);
            fill_expr_span(body, source);
            fill_span(span, source);
        }
        Expr::IfExpr {
            condition,
            then_branch,
            else_branch,
            span,
        } => {
            fill_expr_span(condition, source);
            fill_expr_span(then_branch, source);
            if let Some(else_br) = else_branch {
                fill_expr_span(else_br, source);
            }
            fill_span(span, source);
        }
        Expr::MatchExpr {
            scrutinee,
            arms,
            span,
        } => {
            fill_expr_span(scrutinee, source);
            for arm in arms {
                fill_pattern_span(&mut arm.pattern, source);
                fill_expr_span(&mut arm.body, source);
                fill_span(&mut arm.span, source);
            }
            fill_span(span, source);
        }
        Expr::Group { expr, span } => {
            fill_expr_span(expr, source);
            fill_span(span, source);
        }
        Expr::ProvidesExpr { items, body, span } => {
            for item in items {
                fill_expr_span(&mut item.expr, source);
                if let Some(alias) = &mut item.alias {
                    fill_span(&mut alias.span, source);
                }
                fill_span(&mut item.span, source);
            }
            fill_expr_span(body, source);
            fill_span(span, source);
        }
        Expr::ConsumesExpr { names, body, span } => {
            for name in names {
                fill_span(&mut name.span, source);
            }
            fill_expr_span(body, source);
            fill_span(span, source);
        }
        Expr::DictLiteral { entries, span } => {
            for (key, value) in entries {
                fill_expr_span(key, source);
                fill_expr_span(value, source);
            }
            fill_span(span, source);
        }
        Expr::DictAccess { dict, key, span } => {
            fill_expr_span(dict, source);
            fill_expr_span(key, source);
            fill_span(span, source);
        }
        Expr::ClosureExpr { params, body, span } => {
            for param in params {
                fill_span(&mut param.name.span, source);
                if let Some(ty) = &mut param.ty {
                    fill_type_span(ty, source);
                }
                fill_span(&mut param.span, source);
            }
            fill_expr_span(body, source);
            fill_span(span, source);
        }
        Expr::LetExpr {
            pattern,
            ty,
            value,
            body,
            span,
            ..
        } => {
            fill_binding_pattern_span(pattern, source);
            if let Some(type_ann) = ty {
                fill_type_span(type_ann, source);
            }
            fill_expr_span(value, source);
            fill_expr_span(body, source);
            fill_span(span, source);
        }
    }
}

/// Fill spans in a pattern
fn fill_pattern_span(pattern: &mut Pattern, source: &str) {
    match pattern {
        Pattern::Variant { name, bindings } => {
            fill_span(&mut name.span, source);
            for binding in bindings {
                fill_span(&mut binding.span, source);
            }
        }
    }
}

/// Fill spans in a binding pattern (for let destructuring)
fn fill_binding_pattern_span(pattern: &mut BindingPattern, source: &str) {
    match pattern {
        BindingPattern::Simple(ident) => {
            fill_span(&mut ident.span, source);
        }
        BindingPattern::Array { elements, span } => {
            for elem in elements {
                match elem {
                    ArrayPatternElement::Binding(p) => fill_binding_pattern_span(p, source),
                    ArrayPatternElement::Rest(Some(ident)) => fill_span(&mut ident.span, source),
                    ArrayPatternElement::Rest(None) | ArrayPatternElement::Wildcard => {}
                }
            }
            fill_span(span, source);
        }
        BindingPattern::Struct { fields, span } => {
            for field in fields {
                fill_span(&mut field.name.span, source);
                if let Some(alias) = &mut field.alias {
                    fill_span(&mut alias.span, source);
                }
            }
            fill_span(span, source);
        }
        BindingPattern::Tuple { elements, span } => {
            for elem in elements {
                fill_binding_pattern_span(elem, source);
            }
            fill_span(span, source);
        }
    }
}

/// Format a parse error with lowercase keywords and readable token names
fn format_parse_error(error: &Rich<'_, Token>) -> String {
    use chumsky::error::RichPattern;

    let found = error
        .found()
        .map(|t| format!("{}", t))
        .unwrap_or_else(|| "end of input".to_string());

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
        format!(
            "found {} at {}..{}, expected {}",
            found, span.start, span.end, expected[0]
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
        let_binding_parser().map(Statement::Let),
        definition_parser().map(Statement::Definition),
    ))
}

/// Parse a use statement
fn use_stmt_parser<'tokens, I>(
) -> impl Parser<'tokens, I, UseStmt, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    just(Token::Use)
        .ignore_then(
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
        .map_with(|(mut path, items), e| {
            let items = items.unwrap_or_else(|| {
                // If no items specified, last segment is the item
                let last = path.pop().expect("path must have at least 1 element");
                UseItems::Single(last)
            });

            UseStmt {
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

/// Parse a definition (trait, struct, impl, enum, or module)
fn definition_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Definition, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|def| {
        choice((
            trait_def_parser().map(Definition::Trait),
            struct_def_parser().map(Definition::Struct),
            impl_def_parser().map(Definition::Impl),
            enum_def_parser().map(Definition::Enum),
            module_def_parser(def).map(Definition::Module),
        ))
    })
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

/// Parse a binding pattern (for let bindings)
/// Supports: simple name, array destructuring, struct destructuring, tuple destructuring
fn binding_pattern_parser<'tokens, I>(
) -> impl Parser<'tokens, I, BindingPattern, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|pattern| {
        // Wildcard pattern: _
        let wildcard = just(Token::Underscore)
            .map_with(|_, e| BindingPattern::Simple(Ident::new("_", span_from_simple(e.span()))));

        // Simple name pattern
        let simple = ident_parser().map(BindingPattern::Simple);

        // Array pattern: [a, b, ...rest] or [a, _, ...] or [first, ..., last]
        let rest_pattern = just(Token::DotDotDot)
            .ignore_then(ident_parser().or_not())
            .map(ArrayPatternElement::Rest);

        let array_element = choice((
            rest_pattern,
            just(Token::Underscore).to(ArrayPatternElement::Wildcard),
            pattern.clone().map(ArrayPatternElement::Binding),
        ));

        let array_pattern = array_element
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map_with(|elements, e| BindingPattern::Array {
                elements,
                span: span_from_simple(e.span()),
            });

        // Struct pattern: {name, age as userAge}
        let struct_field = ident_parser()
            .then(just(Token::As).ignore_then(ident_parser()).or_not())
            .map(|(name, alias)| StructPatternField { name, alias });

        let struct_pattern = struct_field
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with(|fields, e| BindingPattern::Struct {
                fields,
                span: span_from_simple(e.span()),
            });

        // Tuple pattern: (a, b)
        let tuple_pattern = pattern
            .separated_by(just(Token::Comma))
            .at_least(1)
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map_with(|elements, e| BindingPattern::Tuple {
                elements,
                span: span_from_simple(e.span()),
            });

        choice((
            array_pattern,
            struct_pattern,
            tuple_pattern,
            wildcard,
            simple,
        ))
        .labelled("binding pattern")
    })
}

/// Parse generic parameters: <T> or <T, U> or <T: Trait, U: Other + Another>
fn generic_params_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<GenericParam>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Parse a single generic parameter: T or T: Trait or T: Trait1 + Trait2
    let generic_param = ident_parser()
        .then(
            just(Token::Colon)
                .ignore_then(
                    ident_parser()
                        .separated_by(just(Token::Plus))
                        .at_least(1)
                        .collect::<Vec<_>>(),
                )
                .or_not(),
        )
        .map_with(|(name, constraints), e| GenericParam {
            name,
            constraints: constraints
                .unwrap_or_default()
                .into_iter()
                .map(GenericConstraint::Trait)
                .collect(),
            span: span_from_simple(e.span()),
        });

    // Parse comma-separated list: <T, U: Trait, V>
    generic_param
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect()
        .delimited_by(just(Token::Lt), just(Token::Gt))
        .or_not()
        .map(|params| params.unwrap_or_default())
}

/// Parse a trait definition
fn trait_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, TraitDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Parse a field or mount field: "mount"? ident ":" Type
    let field_or_mount = just(Token::Mount)
        .or_not()
        .then(field_def_parser())
        .map(|(mount_opt, field)| (field, mount_opt.is_some()));

    visibility_parser()
        .then_ignore(just(Token::Trait))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(trait_composition_parser())
        .then(
            field_or_mount
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace))
                .or_not(),
        )
        .map_with(
            |((((visibility, name), generics), traits), fields_with_flags), e| {
                let fields_with_flags = fields_with_flags.unwrap_or_default();
                let mut fields = Vec::new();
                let mut mount_fields = Vec::new();

                for (field, is_mount) in fields_with_flags {
                    if is_mount {
                        mount_fields.push(field);
                    } else {
                        fields.push(field);
                    }
                }

                TraitDef {
                    visibility,
                    name,
                    generics,
                    traits,
                    fields,
                    mount_fields,
                    span: span_from_simple(e.span()),
                }
            },
        )
}

/// Parse trait composition (: A + B + C or nothing)
fn trait_composition_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<Ident>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    just(Token::Colon)
        .ignore_then(
            ident_parser()
                .separated_by(just(Token::Plus))
                .at_least(1)
                .collect(),
        )
        .or_not()
        .map(|traits| traits.unwrap_or_default())
}

/// Parse a field definition: mut? name: Type
fn field_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FieldDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    mutability_parser()
        .then(ident_parser())
        .then_ignore(just(Token::Colon).labelled("':'"))
        .then(type_parser().labelled("type"))
        .map_with(|((mutable, name), ty), e| FieldDef {
            mutable,
            name,
            ty,
            span: span_from_simple(e.span()),
        })
}

/// Parse a struct definition (unified - replaces model and view)
fn struct_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, StructDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Parse a field or mount field: "mount"? mut? ident ":" Type ("=" Expr)?
    let field_or_mount = just(Token::Mount)
        .or_not()
        .then(struct_field_parser())
        .map(|(mount_opt, field)| (field, mount_opt.is_some()));

    visibility_parser()
        .then_ignore(just(Token::Struct))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(trait_composition_parser())
        .then(
            field_or_mount
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace))
                .or_not(),
        )
        .map_with(
            |((((visibility, name), generics), traits), fields_with_flags), e| {
                let fields_with_flags = fields_with_flags.unwrap_or_default();
                let mut fields = Vec::new();
                let mut mount_fields = Vec::new();

                for (field, is_mount) in fields_with_flags {
                    if is_mount {
                        mount_fields.push(field);
                    } else {
                        fields.push(field);
                    }
                }

                StructDef {
                    visibility,
                    name,
                    generics,
                    traits,
                    fields,
                    mount_fields,
                    span: span_from_simple(e.span()),
                }
            },
        )
}

/// Parse a single struct field: mut? name: Type? = default
fn struct_field_parser<'tokens, I>(
) -> impl Parser<'tokens, I, StructField, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    mutability_parser()
        .then(ident_parser())
        .then_ignore(just(Token::Colon).labelled("':'"))
        .then(type_parser().labelled("type"))
        .then(
            just(Token::Equals)
                .ignore_then(expr_parser().labelled("default value"))
                .or_not(),
        )
        .map_with(|(((mutable, name), ty), default), e| {
            // Check if type is optional
            let optional = matches!(ty, Type::Optional(_));

            StructField {
                mutable,
                name,
                ty,
                optional,
                default,
                span: span_from_simple(e.span()),
            }
        })
}

/// Parse an impl block definition
/// Impl blocks contain field defaults: `impl Struct { field: value, field: value }`
fn impl_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, ImplDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    just(Token::Impl)
        .ignore_then(ident_parser())
        .then(generic_params_parser())
        .then(
            // Parse named field defaults: field: value, field: value, ...
            ident_parser()
                .then_ignore(just(Token::Colon).labelled("':'"))
                .then(expr_parser().labelled("default value"))
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((name, generics), defaults), e| ImplDef {
            name,
            generics,
            defaults,
            span: span_from_simple(e.span()),
        })
}

/// Parse an enum definition
fn enum_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, EnumDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then_ignore(just(Token::Enum))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(enum_variants_parser())
        .map_with(|(((visibility, name), generics), variants), e| EnumDef {
            visibility,
            name,
            generics,
            variants,
            span: span_from_simple(e.span()),
        })
}

/// Parse enum variants: { variant, variant(Type), ... }
fn enum_variants_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<EnumVariant>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    enum_variant_parser()
        .separated_by(just(Token::Comma))
        .at_least(1)
        .allow_trailing()
        .collect()
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
}

/// Parse a single enum variant: name or name(field: Type, field: Type)
fn enum_variant_parser<'tokens, I>(
) -> impl Parser<'tokens, I, EnumVariant, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    ident_parser()
        .then(
            field_def_parser()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect()
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .or_not(),
        )
        .map_with(|(name, fields), e| EnumVariant {
            name,
            fields: fields.unwrap_or_default(),
            span: span_from_simple(e.span()),
        })
}

/// Parse a module definition
fn module_def_parser<'tokens, I>(
    def_parser: impl Parser<'tokens, I, Definition, extra::Err<Rich<'tokens, Token>>> + Clone,
) -> impl Parser<'tokens, I, ModuleDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then_ignore(just(Token::Module))
        .then(ident_parser())
        .then(
            def_parser
                .repeated()
                .collect()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((visibility, name), definitions), e| ModuleDef {
            visibility,
            name,
            definitions,
            span: span_from_simple(e.span()),
        })
}

/// Parse a type expression
fn type_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Type, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|type_ref| {
        let primitive = choice((
            just(Token::StringType).to(Type::Primitive(PrimitiveType::String)),
            just(Token::NumberType).to(Type::Primitive(PrimitiveType::Number)),
            just(Token::BooleanType).to(Type::Primitive(PrimitiveType::Boolean)),
            just(Token::PathType).to(Type::Primitive(PrimitiveType::Path)),
            just(Token::RegexType).to(Type::Primitive(PrimitiveType::Regex)),
            just(Token::NeverType).to(Type::Primitive(PrimitiveType::Never)),
        ));

        // Parse identifier path (e.g., alignment::Horizontal) with optional generic arguments
        let ident_or_generic = ident_parser()
            .separated_by(just(Token::DoubleColon))
            .at_least(1)
            .collect::<Vec<_>>()
            .then(
                // Try to parse generic arguments: <Type, Type, ...>
                type_ref
                    .clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::Lt), just(Token::Gt))
                    .or_not(),
            )
            .map_with(|(path, args), e| {
                // Join path with :: to create a single identifier name
                let name_str = path
                    .iter()
                    .map(|id| id.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                let name = Ident::new(name_str, span_from_simple(e.span()));

                if let Some(args) = args {
                    // Generic type with arguments
                    Type::Generic {
                        name,
                        args,
                        span: span_from_simple(e.span()),
                    }
                } else {
                    // Simple identifier or module path
                    Type::Ident(name)
                }
            });

        // Array or Dictionary type: [Type] or [KeyType: ValueType]
        let array_or_dict = type_ref
            .clone()
            .then(just(Token::Colon).ignore_then(type_ref.clone()).or_not())
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map(|(key_or_elem, value_opt)| {
                if let Some(value) = value_opt {
                    // Dictionary: [KeyType: ValueType]
                    Type::Dictionary {
                        key: Box::new(key_or_elem),
                        value: Box::new(value),
                    }
                } else {
                    // Array: [Type]
                    Type::Array(Box::new(key_or_elem))
                }
            });

        // Named tuple type: (name1: Type1, name2: Type2, ...)
        let tuple_field = ident_parser()
            .then_ignore(just(Token::Colon).labelled("':'"))
            .then(type_ref.clone().labelled("type"))
            .map_with(|(name, ty), e| TupleField {
                name,
                ty,
                span: span_from_simple(e.span()),
            });

        let tuple = tuple_field
            .separated_by(just(Token::Comma))
            .at_least(1)
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map(Type::Tuple);

        // Grouped type: (Type) - used for applying modifiers like ? to closures
        let grouped_type = type_ref
            .clone()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        let base_type = choice((
            primitive,
            ident_or_generic,
            array_or_dict,
            tuple,
            grouped_type,
        ));

        // Type with optional modifier: Type?
        let optionable_type = base_type
            .then(just(Token::Question).or_not())
            .map(|(ty, opt)| {
                if opt.is_some() {
                    Type::Optional(Box::new(ty))
                } else {
                    ty
                }
            });

        // Closure type: () -> T, T -> U, or T, U -> V
        // No-param closure: () -> ReturnType
        let no_param_closure = just(Token::LParen)
            .ignore_then(just(Token::RParen))
            .ignore_then(just(Token::Arrow))
            .ignore_then(type_ref.clone())
            .map(|ret| Type::Closure {
                params: vec![],
                ret: Box::new(ret),
            });

        // Single or multi-param closure: Type -> ReturnType OR Type, Type, ... -> ReturnType
        let param_closure = optionable_type
            .clone()
            .separated_by(just(Token::Comma))
            .at_least(1)
            .collect::<Vec<_>>()
            .then_ignore(just(Token::Arrow))
            .then(type_ref)
            .map(|(params, ret)| Type::Closure {
                params,
                ret: Box::new(ret),
            });

        // Try closure types first (more specific), then fall back to regular type
        choice((no_param_closure, param_closure, optionable_type))
    })
}

/// Parse an expression
fn expr_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|expr| {
        // Literals
        let literal = choice((
            select! { Token::String(s) => Expr::Literal(Literal::String(s)) },
            select! { Token::Number(n) => Expr::Literal(Literal::Number(n)) },
            select! { Token::Regex(s) => {
                // Parse regex at parse time
                if let Some((pattern, flags)) = crate::lexer::parse_regex(&s) {
                    Expr::Literal(Literal::Regex { pattern, flags })
                } else {
                    // Fallback
                    Expr::Literal(Literal::Regex { pattern: String::new(), flags: String::new() })
                }
            }},
            select! { Token::Path(p) => Expr::Literal(Literal::Path(p)) },
            just(Token::True).to(Expr::Literal(Literal::Boolean(true))),
            just(Token::False).to(Expr::Literal(Literal::Boolean(false))),
            just(Token::Nil).to(Expr::Literal(Literal::Nil)),
        ));

        // Helper to parse named arguments for structs: name: expr, name: expr, ...
        // Allows empty parens for struct instantiation
        let struct_named_args = ident_parser()
            .then_ignore(just(Token::Colon).labelled("':'"))
            .then(expr.clone().labelled("value"))
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        // Helper to parse named arguments for enums: name: expr, name: expr, ...
        // Requires at least one argument if parens are present (no empty parens allowed)
        let enum_named_args = ident_parser()
            .then_ignore(just(Token::Colon).labelled("':'"))
            .then(expr.clone().labelled("value"))
            .separated_by(just(Token::Comma))
            .at_least(1) // Must have at least one arg if using parens
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        // Inferred enum instantiation: .variant(field: value, field: value, ...)
        let inferred_enum_instantiation = just(Token::Dot)
            .ignore_then(ident_parser())
            .then(enum_named_args.clone().or_not())
            .map_with(|(variant, data), e| Expr::InferredEnumInstantiation {
                variant,
                data: data.unwrap_or_default(),
                span: span_from_simple(e.span()),
            });

        // Enum instantiation: EnumType.variant(field: value, field: value, ...)
        // Supports module-qualified paths: module::EnumType.variant
        // Note: Uses ident_no_self_parser to prevent 'self.field' from being parsed as enum instantiation
        let enum_instantiation = ident_no_self_parser()
            .separated_by(just(Token::DoubleColon))
            .at_least(1)
            .collect::<Vec<_>>()
            .then_ignore(just(Token::Dot))
            .then(ident_parser())
            .then(enum_named_args.clone().or_not())
            .map_with(|((path, variant), data), e| {
                // Join module path into a single identifier
                let enum_name_str = path
                    .iter()
                    .map(|id| id.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                let enum_name = Ident::new(enum_name_str, span_from_simple(e.span()));

                Expr::EnumInstantiation {
                    enum_name,
                    variant,
                    data: data.unwrap_or_default(),
                    span: span_from_simple(e.span()),
                }
            });

        // Struct instantiation: StructName(field: value, ...) or StructName<Type>(field: value, ...)
        // With optional mount points: StructName(field: value) { mount: expr, ... }
        // Supports module-qualified paths: module::StructName(...)
        let instantiation = ident_parser()
            .separated_by(just(Token::DoubleColon))
            .at_least(1)
            .collect::<Vec<_>>()
            .then(
                // Optional generic arguments
                type_parser()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::Lt), just(Token::Gt))
                    .or_not(),
            )
            .then(struct_named_args.clone())
            .then(
                // Optional mount points (without extra parentheses)
                ident_parser()
                    .then_ignore(just(Token::Colon).labelled("':'"))
                    .then(
                        // Mounting block syntax: name: { ViewInst() {} ViewInst() {} }
                        expr.clone()
                            .repeated()
                            .collect()
                            .delimited_by(just(Token::LBrace), just(Token::RBrace))
                            .map_with(|elements, e| Expr::Array {
                                elements,
                                span: span_from_simple(e.span()),
                            })
                            // Regular expression: name: expr
                            .or(expr.clone().labelled("value")),
                    )
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace))
                    .or_not(),
            )
            .map_with(|(((path, type_args), args), mounts), e| {
                // Join module path into a single identifier
                let name_str = path
                    .iter()
                    .map(|id| id.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                let name = Ident::new(name_str, span_from_simple(e.span()));

                let type_args = type_args.unwrap_or_default();
                let mounts = mounts.unwrap_or_default();

                // Unified struct instantiation
                Expr::StructInstantiation {
                    name,
                    type_args,
                    args,
                    mounts,
                    span: span_from_simple(e.span()),
                }
            });

        // Reference: user:name (field access - deprecated, will be removed)
        // Note: Enum variants now use dot notation (Type.variant), not double colon
        let reference = ident_parser()
            .separated_by(just(Token::Colon))
            .at_least(1)
            .collect::<Vec<_>>()
            .map_with(|path, e| Expr::Reference {
                path,
                span: span_from_simple(e.span()),
            });

        // Dictionary entry: key_expr: value_expr
        let dict_entry = expr
            .clone()
            .then_ignore(just(Token::Colon))
            .then(expr.clone())
            .map(|(key, value)| (key, value));

        // Dictionary literal: ["key": value, "key2": value2] or [:] for empty
        let dict_literal = choice((
            // Empty dictionary: [:]
            just(Token::LBracket)
                .ignore_then(just(Token::Colon))
                .ignore_then(just(Token::RBracket))
                .map_with(|_, e| Expr::DictLiteral {
                    entries: vec![],
                    span: span_from_simple(e.span()),
                }),
            // Non-empty dictionary: [key: value, key2: value2]
            dict_entry
                .separated_by(just(Token::Comma))
                .at_least(1)
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBracket), just(Token::RBracket))
                .map_with(|entries, e| Expr::DictLiteral {
                    entries,
                    span: span_from_simple(e.span()),
                }),
        ));

        // Array literal: [expr, expr, ...] or [] for empty
        let array_literal = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map_with(|elements, e| Expr::Array {
                elements,
                span: span_from_simple(e.span()),
            });

        // Array or dictionary: try dictionary first (more specific)
        let array_or_dict = choice((dict_literal, array_literal));

        // Tuple literal: (name1: expr1, name2: expr2, ...)
        // Named tuple field: identifier : expression
        let tuple_field = ident_parser()
            .then_ignore(just(Token::Colon).labelled("':'"))
            .then(expr.clone().labelled("value"))
            .map(|(name, expr)| (name, expr));

        let tuple = tuple_field
            .separated_by(just(Token::Comma))
            .at_least(1)
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map_with(|fields, e| Expr::Tuple {
                fields,
                span: span_from_simple(e.span()),
            });

        // Grouped expression: (expr)
        // Note: This must come after tuple in the choice, since tuple is more specific
        let grouped = expr
            .clone()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map_with(|expr, e| Expr::Group {
                expr: Box::new(expr),
                span: span_from_simple(e.span()),
            });

        // Closure expression: () -> expr, x -> expr, x, y -> expr, x: T -> expr
        // Closure parameter: identifier with optional type annotation
        let closure_param = ident_parser()
            .then(just(Token::Colon).ignore_then(type_parser()).or_not())
            .map_with(|(name, ty), e| ClosureParam {
                name,
                ty,
                span: span_from_simple(e.span()),
            });

        // No-param closure: () -> expr
        let no_param_closure = just(Token::LParen)
            .ignore_then(just(Token::RParen))
            .ignore_then(just(Token::Arrow))
            .ignore_then(expr.clone())
            .map_with(|body, e| Expr::ClosureExpr {
                params: vec![],
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // Single or multi-param closure: x -> expr OR x, y -> expr OR x: T -> expr
        let param_closure = closure_param
            .separated_by(just(Token::Comma))
            .at_least(1)
            .collect::<Vec<_>>()
            .then_ignore(just(Token::Arrow))
            .then(expr.clone())
            .map_with(|(params, body), e| Expr::ClosureExpr {
                params,
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // Provides expression: provides item1, item2 { body }
        let provides_expr = just(Token::Provides)
            .ignore_then(
                expr.clone()
                    .then(just(Token::As).ignore_then(ident_parser()).or_not())
                    .map_with(|(expr, alias), e| ProvideItem {
                        expr: Box::new(expr),
                        alias,
                        span: span_from_simple(e.span()),
                    })
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .then(
                expr.clone()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|(items, body), e| Expr::ProvidesExpr {
                items,
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // Consumes expression: consumes name1, name2 { body }
        let consumes_expr = just(Token::Consumes)
            .ignore_then(
                ident_parser()
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .then(
                expr.clone()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|(names, body), e| Expr::ConsumesExpr {
                names,
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // For expression: for var in collection { body }
        let for_expr = just(Token::For)
            .ignore_then(ident_parser())
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .then(
                expr.clone()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|((var, collection), body), e| Expr::ForExpr {
                var,
                collection: Box::new(collection),
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // If expression: if condition { then } else { else }
        let if_expr = just(Token::If)
            .ignore_then(expr.clone())
            .then(
                expr.clone()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .then(
                just(Token::Else)
                    .ignore_then(
                        expr.clone()
                            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
                    )
                    .or_not(),
            )
            .map_with(|((condition, then_branch), else_branch), e| Expr::IfExpr {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: else_branch.map(Box::new),
                span: span_from_simple(e.span()),
            });

        // Match expression: match scrutinee { pattern: expr, ... }
        let match_expr = just(Token::Match)
            .ignore_then(expr.clone())
            .then(
                match_arm_parser(expr.clone())
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .allow_trailing()
                    .collect()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|(scrutinee, arms), e| Expr::MatchExpr {
                scrutinee: Box::new(scrutinee),
                arms,
                span: span_from_simple(e.span()),
            });

        // Let expression: let pattern = value body OR let pattern: Type = value body OR let mut pattern = value body
        let let_expr = just(Token::Let)
            .ignore_then(just(Token::Mut).or_not())
            .then(binding_pattern_parser())
            .then(just(Token::Colon).ignore_then(type_parser()).or_not())
            .then_ignore(just(Token::Equals))
            .then(expr.clone())
            .then(expr.clone())
            .map_with(
                |((((mutable, pattern), ty), value), body), e| Expr::LetExpr {
                    mutable: mutable.is_some(),
                    pattern,
                    ty,
                    value: Box::new(value),
                    body: Box::new(body),
                    span: span_from_simple(e.span()),
                },
            );

        // Atom: literal, instantiation, enum_instantiation, reference, array/dict, tuple, grouped, for, if, match, provides, consumes, closure, let
        // Order matters: try more specific parsers first
        let atom = choice((
            literal,
            provides_expr,
            consumes_expr,
            for_expr,
            if_expr,
            match_expr,
            let_expr,      // Let expressions
            array_or_dict, // Handles both array and dictionary literals
            tuple,         // Must come before grouped (tuple is more specific)
            grouped,
            no_param_closure, // () -> expr (must come before other closures and tuples)
            param_closure,    // x -> expr (must come before reference since starts with ident)
            inferred_enum_instantiation, // .variant is most specific
            enum_instantiation, // Must come before instantiation and reference (Type.variant(...))
            instantiation,    // Must come before reference (Type(...))
            reference,        // Most general (ident:ident:ident), now includes 'self'
        ));

        // Binary operators with precedence using pratt parser
        atom.pratt((
            // Multiplication, division, modulo (highest precedence: 6)
            infix(left(6), just(Token::Star), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Mul,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            infix(left(6), just(Token::Slash), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Div,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            infix(left(6), just(Token::Percent), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Mod,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            // Addition and subtraction (precedence: 5)
            infix(left(5), just(Token::Plus), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Add,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            infix(left(5), just(Token::Minus), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Sub,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            // Comparison operators (precedence: 4)
            infix(left(4), just(Token::Lt), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Lt,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            infix(left(4), just(Token::Gt), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Gt,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            infix(left(4), just(Token::Le), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Le,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            infix(left(4), just(Token::Ge), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Ge,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            // Equality operators (precedence: 3)
            infix(left(3), just(Token::EqEq), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Eq,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            infix(left(3), just(Token::Ne), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Ne,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            // Logical AND (precedence: 2)
            infix(left(2), just(Token::And), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::And,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            // Logical OR (precedence: 1, lowest)
            infix(left(1), just(Token::Or), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Or,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            // Dictionary/array access: expr[key] (highest precedence: 7)
            postfix(
                7,
                expr.clone()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket)),
                |dict, key, e| Expr::DictAccess {
                    dict: Box::new(dict),
                    key: Box::new(key),
                    span: span_from_simple(e.span()),
                },
            ),
            // Field access: expr.field (precedence: 7, same as array access)
            // Note: This handles general field access like foo.bar.baz or self.field
            // Enum instantiation Type.variant(args) is parsed as an atom, so won't conflict
            postfix(
                7,
                just(Token::Dot).ignore_then(ident_parser()),
                |object, field, e| {
                    // Convert object to a reference path and extend it with the field
                    match object {
                        Expr::Reference { mut path, .. } => {
                            // Extend existing reference path
                            path.push(field);
                            Expr::Reference {
                                path,
                                span: span_from_simple(e.span()),
                            }
                        }
                        _ => {
                            // For non-reference expressions, we'll need FieldAccess in the AST
                            // For now, treat this as an error case by creating a simple reference
                            // This is a limitation - proper field access on complex expressions
                            // would need a new AST node
                            Expr::Reference {
                                path: vec![field],
                                span: span_from_simple(e.span()),
                            }
                        }
                    }
                },
            ),
        ))
    })
}

/// Parse a match arm: pattern: expr
fn match_arm_parser<'tokens, I>(
    expr: impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone,
) -> impl Parser<'tokens, I, MatchArm, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    pattern_parser()
        .then_ignore(just(Token::Colon))
        .then(expr)
        .map_with(|(pattern, body), e| MatchArm {
            pattern,
            body,
            span: span_from_simple(e.span()),
        })
}

/// Parse a pattern: variant or variant(binding1, binding2) or .variant or .variant(binding1, binding2)
fn pattern_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Pattern, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Parse either .variant or variant (short form with dot or full form)
    choice((
        // Short form: .variant or .variant(bindings)
        just(Token::Dot).ignore_then(ident_parser()),
        // Full form: variant or variant(bindings)
        ident_parser(),
    ))
    .then(
        ident_parser()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .or_not(),
    )
    .map(|(name, bindings)| Pattern::Variant {
        name,
        bindings: bindings.unwrap_or_default(),
    })
}

/// Helper to convert SimpleSpan to our custom Span
fn span_from_simple(s: SimpleSpan) -> CustomSpan {
    CustomSpan::from_range(s.start, s.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse_type_str(input: &str) -> Result<Type, Vec<(String, CustomSpan)>> {
        // Parse the type as a struct field and extract it
        let wrapper = format!("struct Test {{ field: {} }}", input);
        let tokens = Lexer::tokenize_all(&wrapper);
        let result = parse_file(&tokens)?;

        // Extract the type from the parsed struct
        if let Some(Statement::Definition(Definition::Struct(s))) = result.statements.first() {
            if let Some(field) = s.fields.first() {
                return Ok(field.ty.clone());
            }
        }
        Err(vec![(
            "Could not extract type".to_string(),
            CustomSpan::default(),
        )])
    }

    #[test]
    fn test_never_type_parsing() {
        let result = parse_type_str("Never");
        assert!(result.is_ok(), "Failed to parse Never type: {:?}", result);
        let ty = result.unwrap();
        assert_eq!(ty, Type::Primitive(PrimitiveType::Never));
    }

    #[test]
    fn test_never_in_struct_field() {
        let input = r#"
            pub struct Empty: View {
                mount body: Never
            }
        "#;
        let tokens = Lexer::tokenize_all(input);
        let result = parse_file(&tokens);
        assert!(
            result.is_ok(),
            "Failed to parse struct with Never field: {:?}",
            result
        );
    }

    #[test]
    fn test_optional_never_type() {
        let result = parse_type_str("Never?");
        assert!(result.is_ok(), "Failed to parse Never? type: {:?}", result);
        let ty = result.unwrap();
        match ty {
            Type::Optional(inner) => {
                assert_eq!(*inner, Type::Primitive(PrimitiveType::Never));
            }
            _ => panic!("Expected Optional type, got {:?}", ty),
        }
    }

    #[test]
    fn test_array_of_never_type() {
        let result = parse_type_str("[Never]");
        assert!(result.is_ok(), "Failed to parse [Never] type: {:?}", result);
        let ty = result.unwrap();
        match ty {
            Type::Array(inner) => {
                assert_eq!(*inner, Type::Primitive(PrimitiveType::Never));
            }
            _ => panic!("Expected Array type, got {:?}", ty),
        }
    }

    #[test]
    fn test_dictionary_type_parsing() {
        let result = parse_type_str("[String: Number]");
        assert!(
            result.is_ok(),
            "Failed to parse [String: Number] type: {:?}",
            result
        );
        let ty = result.unwrap();
        match ty {
            Type::Dictionary { key, value } => {
                assert_eq!(*key, Type::Primitive(PrimitiveType::String));
                assert_eq!(*value, Type::Primitive(PrimitiveType::Number));
            }
            _ => panic!("Expected Dictionary type, got {:?}", ty),
        }
    }

    #[test]
    fn test_dictionary_in_struct_field() {
        let input = r#"
            pub struct Config {
                settings: [String: String]
            }
        "#;
        let tokens = Lexer::tokenize_all(input);
        let result = parse_file(&tokens);
        assert!(
            result.is_ok(),
            "Failed to parse struct with Dictionary field: {:?}",
            result
        );

        let file = result.unwrap();
        if let Some(Statement::Definition(Definition::Struct(s))) = file.statements.first() {
            if let Some(field) = s.fields.first() {
                match &field.ty {
                    Type::Dictionary { key, value } => {
                        assert_eq!(**key, Type::Primitive(PrimitiveType::String));
                        assert_eq!(**value, Type::Primitive(PrimitiveType::String));
                    }
                    _ => panic!("Expected Dictionary type, got {:?}", field.ty),
                }
            } else {
                panic!("No fields found");
            }
        } else {
            panic!("No struct found");
        }
    }

    #[test]
    fn test_nested_dictionary_type() {
        let result = parse_type_str("[String: [Number: Boolean]]");
        assert!(
            result.is_ok(),
            "Failed to parse nested dictionary type: {:?}",
            result
        );
        let ty = result.unwrap();
        match ty {
            Type::Dictionary { key, value } => {
                assert_eq!(*key, Type::Primitive(PrimitiveType::String));
                match *value {
                    Type::Dictionary {
                        key: inner_key,
                        value: inner_value,
                    } => {
                        assert_eq!(*inner_key, Type::Primitive(PrimitiveType::Number));
                        assert_eq!(*inner_value, Type::Primitive(PrimitiveType::Boolean));
                    }
                    _ => panic!("Expected inner Dictionary type, got {:?}", value),
                }
            }
            _ => panic!("Expected Dictionary type, got {:?}", ty),
        }
    }

    #[test]
    fn test_optional_dictionary_type() {
        let result = parse_type_str("[String: Number]?");
        assert!(
            result.is_ok(),
            "Failed to parse optional dictionary type: {:?}",
            result
        );
        let ty = result.unwrap();
        match ty {
            Type::Optional(inner) => match *inner {
                Type::Dictionary { key, value } => {
                    assert_eq!(*key, Type::Primitive(PrimitiveType::String));
                    assert_eq!(*value, Type::Primitive(PrimitiveType::Number));
                }
                _ => panic!("Expected Dictionary type inside Optional, got {:?}", inner),
            },
            _ => panic!("Expected Optional type, got {:?}", ty),
        }
    }

    #[test]
    fn test_dictionary_with_custom_types() {
        let result = parse_type_str("[UserId: UserData]");
        assert!(
            result.is_ok(),
            "Failed to parse dictionary with custom types: {:?}",
            result
        );
        let ty = result.unwrap();
        match ty {
            Type::Dictionary { key, value } => match (*key, *value) {
                (Type::Ident(k), Type::Ident(v)) => {
                    assert_eq!(k.name, "UserId");
                    assert_eq!(v.name, "UserData");
                }
                _ => panic!("Expected Ident types"),
            },
            _ => panic!("Expected Dictionary type, got {:?}", ty),
        }
    }

    // Helper to parse an expression from let binding
    fn parse_expr_from_let(input: &str) -> Result<Expr, Vec<(String, CustomSpan)>> {
        let wrapper = format!("let x = {}", input);
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
    fn test_dictionary_literal_parsing() {
        let result = parse_expr_from_let("[\"key\": 42, \"name\": 100]");
        assert!(
            result.is_ok(),
            "Failed to parse dictionary literal: {:?}",
            result
        );
        let expr = result.unwrap();
        match expr {
            Expr::DictLiteral { entries, .. } => {
                assert_eq!(entries.len(), 2);
                // Check first entry
                match (&entries[0].0, &entries[0].1) {
                    (Expr::Literal(Literal::String(k)), Expr::Literal(Literal::Number(v))) => {
                        assert_eq!(k, "key");
                        assert_eq!(*v, 42.0);
                    }
                    _ => panic!("Expected string key and number value"),
                }
            }
            _ => panic!("Expected DictLiteral, got {:?}", expr),
        }
    }

    #[test]
    fn test_empty_dictionary_literal() {
        let result = parse_expr_from_let("[:]");
        assert!(
            result.is_ok(),
            "Failed to parse empty dictionary: {:?}",
            result
        );
        let expr = result.unwrap();
        match expr {
            Expr::DictLiteral { entries, .. } => {
                assert!(entries.is_empty(), "Expected empty entries");
            }
            _ => panic!("Expected DictLiteral, got {:?}", expr),
        }
    }

    #[test]
    fn test_dictionary_access_parsing() {
        let result = parse_expr_from_let("data[\"key\"]");
        assert!(
            result.is_ok(),
            "Failed to parse dictionary access: {:?}",
            result
        );
        let expr = result.unwrap();
        match expr {
            Expr::DictAccess { dict, key, .. } => match (*dict, *key) {
                (Expr::Reference { path, .. }, Expr::Literal(Literal::String(k))) => {
                    assert_eq!(path[0].name, "data");
                    assert_eq!(k, "key");
                }
                _ => panic!("Expected reference and string key"),
            },
            _ => panic!("Expected DictAccess, got {:?}", expr),
        }
    }

    #[test]
    fn test_chained_dictionary_access() {
        let result = parse_expr_from_let("data[\"outer\"][\"inner\"]");
        assert!(
            result.is_ok(),
            "Failed to parse chained dict access: {:?}",
            result
        );
        let expr = result.unwrap();
        match expr {
            Expr::DictAccess { dict, key, .. } => {
                // Outer access: dict is another DictAccess, key is "inner"
                match (*key,) {
                    (Expr::Literal(Literal::String(k)),) => {
                        assert_eq!(k, "inner");
                    }
                    _ => panic!("Expected string key 'inner'"),
                }
                match *dict {
                    Expr::DictAccess {
                        dict: inner_dict,
                        key: inner_key,
                        ..
                    } => {
                        match (*inner_key,) {
                            (Expr::Literal(Literal::String(k)),) => {
                                assert_eq!(k, "outer");
                            }
                            _ => panic!("Expected string key 'outer'"),
                        }
                        match *inner_dict {
                            Expr::Reference { path, .. } => {
                                assert_eq!(path[0].name, "data");
                            }
                            _ => panic!("Expected reference 'data'"),
                        }
                    }
                    _ => panic!("Expected inner DictAccess"),
                }
            }
            _ => panic!("Expected DictAccess, got {:?}", expr),
        }
    }

    #[test]
    fn test_dictionary_with_expression_key() {
        let result = parse_expr_from_let("data[index]");
        assert!(
            result.is_ok(),
            "Failed to parse dict access with expr key: {:?}",
            result
        );
        let expr = result.unwrap();
        match expr {
            Expr::DictAccess { dict, key, .. } => match (*dict, *key) {
                (Expr::Reference { path: d, .. }, Expr::Reference { path: k, .. }) => {
                    assert_eq!(d[0].name, "data");
                    assert_eq!(k[0].name, "index");
                }
                _ => panic!("Expected two references"),
            },
            _ => panic!("Expected DictAccess, got {:?}", expr),
        }
    }

    // Closure type tests
    #[test]
    fn test_closure_type_no_params() {
        let result = parse_type_str("() -> Event");
        assert!(result.is_ok(), "Failed to parse () -> Event: {:?}", result);
        let ty = result.unwrap();
        match ty {
            Type::Closure { params, ret } => {
                assert!(params.is_empty(), "Expected empty params");
                match *ret {
                    Type::Ident(ident) => assert_eq!(ident.name, "Event"),
                    _ => panic!("Expected Ident return type"),
                }
            }
            _ => panic!("Expected Closure type, got {:?}", ty),
        }
    }

    #[test]
    fn test_closure_type_single_param() {
        let result = parse_type_str("String -> Event");
        assert!(
            result.is_ok(),
            "Failed to parse String -> Event: {:?}",
            result
        );
        let ty = result.unwrap();
        match ty {
            Type::Closure { params, ret } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0], Type::Primitive(PrimitiveType::String));
                match *ret {
                    Type::Ident(ident) => assert_eq!(ident.name, "Event"),
                    _ => panic!("Expected Ident return type"),
                }
            }
            _ => panic!("Expected Closure type, got {:?}", ty),
        }
    }

    #[test]
    fn test_closure_type_multi_params() {
        let result = parse_type_str("Number, Number -> Point");
        assert!(
            result.is_ok(),
            "Failed to parse Number, Number -> Point: {:?}",
            result
        );
        let ty = result.unwrap();
        match ty {
            Type::Closure { params, ret } => {
                assert_eq!(params.len(), 2);
                assert_eq!(params[0], Type::Primitive(PrimitiveType::Number));
                assert_eq!(params[1], Type::Primitive(PrimitiveType::Number));
                match *ret {
                    Type::Ident(ident) => assert_eq!(ident.name, "Point"),
                    _ => panic!("Expected Ident return type"),
                }
            }
            _ => panic!("Expected Closure type, got {:?}", ty),
        }
    }

    #[test]
    fn test_optional_closure_type() {
        let result = parse_type_str("(String -> Event)?");
        assert!(
            result.is_ok(),
            "Failed to parse (String -> Event)?: {:?}",
            result
        );
        let ty = result.unwrap();
        match ty {
            Type::Optional(inner) => match *inner {
                Type::Closure { params, .. } => {
                    assert_eq!(params.len(), 1);
                }
                _ => panic!("Expected Closure inside Optional"),
            },
            _ => panic!("Expected Optional type, got {:?}", ty),
        }
    }

    // Closure expression tests
    #[test]
    fn test_closure_expr_no_params() {
        let result = parse_expr_from_let("() -> .submit");
        assert!(
            result.is_ok(),
            "Failed to parse () -> .submit: {:?}",
            result
        );
        let expr = result.unwrap();
        match expr {
            Expr::ClosureExpr { params, body, .. } => {
                assert!(params.is_empty());
                match *body {
                    Expr::InferredEnumInstantiation { variant, .. } => {
                        assert_eq!(variant.name, "submit");
                    }
                    _ => panic!("Expected InferredEnumInstantiation, got {:?}", body),
                }
            }
            _ => panic!("Expected ClosureExpr, got {:?}", expr),
        }
    }

    #[test]
    fn test_closure_expr_single_param() {
        let result = parse_expr_from_let("x -> .changed(value: x)");
        assert!(
            result.is_ok(),
            "Failed to parse x -> .changed(...): {:?}",
            result
        );
        let expr = result.unwrap();
        match expr {
            Expr::ClosureExpr { params, .. } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name.name, "x");
                assert!(params[0].ty.is_none());
            }
            _ => panic!("Expected ClosureExpr, got {:?}", expr),
        }
    }

    #[test]
    fn test_closure_expr_multi_params() {
        let result = parse_expr_from_let("w, h -> .resized(width: w, height: h)");
        assert!(result.is_ok(), "Failed to parse w, h -> ...: {:?}", result);
        let expr = result.unwrap();
        match expr {
            Expr::ClosureExpr { params, .. } => {
                assert_eq!(params.len(), 2);
                assert_eq!(params[0].name.name, "w");
                assert_eq!(params[1].name.name, "h");
            }
            _ => panic!("Expected ClosureExpr, got {:?}", expr),
        }
    }

    #[test]
    fn test_closure_expr_with_type_annotation() {
        let result = parse_expr_from_let("x: String -> .textChanged(value: x)");
        assert!(
            result.is_ok(),
            "Failed to parse x: String -> ...: {:?}",
            result
        );
        let expr = result.unwrap();
        match expr {
            Expr::ClosureExpr { params, .. } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name.name, "x");
                match &params[0].ty {
                    Some(Type::Primitive(PrimitiveType::String)) => {}
                    _ => panic!("Expected String type annotation"),
                }
            }
            _ => panic!("Expected ClosureExpr, got {:?}", expr),
        }
    }

    #[test]
    fn test_closure_in_struct_field() {
        let input = r#"
            pub struct Button<E> {
                action: () -> E
            }
        "#;
        let tokens = Lexer::tokenize_all(input);
        let result = parse_file(&tokens);
        assert!(
            result.is_ok(),
            "Failed to parse struct with closure field: {:?}",
            result
        );
    }

    // Let expression tests
    #[test]
    fn test_let_expr_basic() {
        let result = parse_expr_from_let("let x = 42 x");
        assert!(result.is_ok(), "Failed to parse let x = 42 x: {:?}", result);
        let expr = result.unwrap();
        match expr {
            Expr::LetExpr {
                mutable,
                pattern,
                ty,
                value,
                body,
                ..
            } => {
                assert!(!mutable);
                match pattern {
                    BindingPattern::Simple(ident) => assert_eq!(ident.name, "x"),
                    _ => panic!("Expected simple pattern"),
                }
                assert!(ty.is_none());
                match *value {
                    Expr::Literal(Literal::Number(n)) => assert_eq!(n, 42.0),
                    _ => panic!("Expected number literal"),
                }
                match *body {
                    Expr::Reference { path, .. } => assert_eq!(path[0].name, "x"),
                    _ => panic!("Expected reference in body"),
                }
            }
            _ => panic!("Expected LetExpr, got {:?}", expr),
        }
    }

    #[test]
    fn test_let_expr_with_type() {
        let result = parse_expr_from_let("let count: Number = 100 count");
        assert!(
            result.is_ok(),
            "Failed to parse let with type: {:?}",
            result
        );
        let expr = result.unwrap();
        match expr {
            Expr::LetExpr { pattern, ty, .. } => {
                match pattern {
                    BindingPattern::Simple(ident) => assert_eq!(ident.name, "count"),
                    _ => panic!("Expected simple pattern"),
                }
                match ty {
                    Some(Type::Primitive(PrimitiveType::Number)) => {}
                    _ => panic!("Expected Number type annotation"),
                }
            }
            _ => panic!("Expected LetExpr, got {:?}", expr),
        }
    }

    #[test]
    fn test_let_expr_mutable() {
        let result = parse_expr_from_let("let mut counter = 0 counter");
        assert!(result.is_ok(), "Failed to parse let mut: {:?}", result);
        let expr = result.unwrap();
        match expr {
            Expr::LetExpr {
                mutable, pattern, ..
            } => {
                assert!(mutable);
                match pattern {
                    BindingPattern::Simple(ident) => assert_eq!(ident.name, "counter"),
                    _ => panic!("Expected simple pattern"),
                }
            }
            _ => panic!("Expected LetExpr, got {:?}", expr),
        }
    }

    #[test]
    fn test_let_expr_in_for() {
        let input = r#"
            struct App { content: [String] }
            impl App {
                content: for item in items {
                    let formatted = item
                    Label(text: formatted)
                }
            }
        "#;
        let tokens = Lexer::tokenize_all(input);
        let result = parse_file(&tokens);
        assert!(
            result.is_ok(),
            "Failed to parse let in for block: {:?}",
            result
        );
    }

    #[test]
    fn test_nested_let_exprs() {
        let result = parse_expr_from_let("let x = 1 let y = 2 x");
        assert!(result.is_ok(), "Failed to parse nested let: {:?}", result);
        let expr = result.unwrap();
        match expr {
            Expr::LetExpr { pattern, body, .. } => {
                match pattern {
                    BindingPattern::Simple(ident) => assert_eq!(ident.name, "x"),
                    _ => panic!("Expected simple pattern"),
                }
                match *body {
                    Expr::LetExpr {
                        pattern: inner_pattern,
                        ..
                    } => match inner_pattern {
                        BindingPattern::Simple(ident) => assert_eq!(ident.name, "y"),
                        _ => panic!("Expected simple pattern"),
                    },
                    _ => panic!("Expected nested LetExpr"),
                }
            }
            _ => panic!("Expected LetExpr, got {:?}", expr),
        }
    }
}
