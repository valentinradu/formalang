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
            }
            fill_span(&mut use_stmt.span, source);
        }
        Statement::Let(let_stmt) => {
            fill_span(&mut let_stmt.name.span, source);
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
            for body_expr in &mut i.body {
                fill_expr_span(body_expr, source);
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
        Expr::ContextExpr { items, body, span } => {
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

/// Parse use items (single or multiple)
fn use_items_parser<'tokens, I>(
) -> impl Parser<'tokens, I, UseItems, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    choice((
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
        .then(ident_parser())
        .then_ignore(just(Token::Equals))
        .then(expr_parser())
        .map_with(|(((visibility, mutable), name), value), e| LetBinding {
            visibility,
            mutable,
            name,
            value,
            span: span_from_simple(e.span()),
        })
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
        Token::Ident(name) = e => Ident::new(name, span_from_simple(e.span()))
    }
    .labelled("identifier")
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
fn impl_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, ImplDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    just(Token::Impl)
        .ignore_then(ident_parser())
        .then(generic_params_parser())
        .then(
            expr_parser()
                .repeated()
                .collect()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((name, generics), body), e| ImplDef {
            name,
            generics,
            body,
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

        let array = type_ref
            .clone()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map(|t| Type::Array(Box::new(t)));

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

        let base_type = choice((primitive, ident_or_generic, array, tuple));

        // Optional type: Type?
        base_type
            .then(just(Token::Question).or_not())
            .map(|(ty, opt)| {
                if opt.is_some() {
                    Type::Optional(Box::new(ty))
                } else {
                    ty
                }
            })
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
        let enum_instantiation = ident_parser()
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

        // Array literal: [expr, expr, ...] (commas required for now to avoid ambiguity)
        let array = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map_with(|elements, e| Expr::Array {
                elements,
                span: span_from_simple(e.span()),
            });

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

        // Context expression: context mut? items { body } (kept for backward compatibility)
        let context_expr = just(Token::Context)
            .ignore_then(
                mutability_parser()
                    .then(expr.clone())
                    .then(just(Token::As).ignore_then(ident_parser()).or_not())
                    .map_with(|((mutable, expr), alias), e| ContextItem {
                        mutable,
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
            .map_with(|(items, body), e| Expr::ContextExpr {
                items,
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

        // Atom: literal, instantiation, enum_instantiation, reference, array, tuple, grouped, for, if, match, context, provides, consumes
        // Order matters: try more specific parsers first
        let atom = choice((
            literal,
            context_expr,  // Kept for backward compatibility
            provides_expr, // New provides syntax
            consumes_expr, // New consumes syntax
            for_expr,
            if_expr,
            match_expr,
            array,
            tuple, // Must come before grouped (tuple is more specific)
            grouped,
            inferred_enum_instantiation, // Must come first (.variant is most specific)
            enum_instantiation, // Must come before instantiation and reference (Type.variant(...))
            instantiation,      // Must come before reference (Type(...))
            reference,          // Most general (ident:ident:ident)
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

