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
        Statement::Definition(def) => fill_definition_span(def.as_mut(), source),
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
            for func in &mut i.functions {
                fill_span(&mut func.name.span, source);
                for p in &mut func.params {
                    fill_span(&mut p.name.span, source);
                    if let Some(ty) = &mut p.ty {
                        fill_type_span(ty, source);
                    }
                    fill_span(&mut p.span, source);
                }
                if let Some(ret) = &mut func.return_type {
                    fill_type_span(ret, source);
                }
                fill_expr_span(&mut func.body, source);
                fill_span(&mut func.span, source);
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
        Definition::Function(f) => {
            fill_span(&mut f.name.span, source);
            for p in &mut f.params {
                fill_span(&mut p.name.span, source);
                if let Some(ty) = &mut p.ty {
                    fill_type_span(ty, source);
                }
                fill_span(&mut p.span, source);
            }
            if let Some(ret) = &mut f.return_type {
                fill_type_span(ret, source);
            }
            fill_expr_span(&mut f.body, source);
            fill_span(&mut f.span, source);
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
        Expr::Invocation {
            path,
            type_args,
            args,
            mounts,
            span,
        } => {
            for ident in path {
                fill_span(&mut ident.span, source);
            }
            for ty_arg in type_args {
                fill_type_span(ty_arg, source);
            }
            for (arg_name, arg_expr) in args {
                if let Some(name) = arg_name {
                    fill_span(&mut name.span, source);
                }
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
        Expr::UnaryOp { operand, span, .. } => {
            fill_expr_span(operand, source);
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
        Expr::FieldAccess { object, field, span } => {
            fill_expr_span(object, source);
            fill_span(&mut field.span, source);
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
        Expr::MethodCall {
            receiver,
            method,
            args,
            span,
        } => {
            fill_expr_span(receiver, source);
            fill_span(&mut method.span, source);
            for arg_expr in args {
                fill_expr_span(arg_expr, source);
            }
            fill_span(span, source);
        }
        Expr::Block {
            statements,
            result,
            span,
        } => {
            for stmt in statements {
                match stmt {
                    BlockStatement::Let {
                        pattern,
                        ty,
                        value,
                        span: stmt_span,
                        ..
                    } => {
                        fill_binding_pattern_span(pattern, source);
                        if let Some(type_ann) = ty {
                            fill_type_span(type_ann, source);
                        }
                        fill_expr_span(value, source);
                        fill_span(stmt_span, source);
                    }
                    BlockStatement::Assign {
                        target,
                        value,
                        span: stmt_span,
                    } => {
                        fill_expr_span(target, source);
                        fill_expr_span(value, source);
                        fill_span(stmt_span, source);
                    }
                    BlockStatement::Expr(expr) => {
                        fill_expr_span(expr, source);
                    }
                }
            }
            fill_expr_span(result, source);
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
        Pattern::Wildcard => {
            // Wildcard has no spans to fill
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
                let last = path.pop().expect("path must have at least 1 element");
                UseItems::Single(last)
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

/// Parse a definition (trait, struct, impl, enum, module, or function)
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
            function_def_parser().map(|f| Definition::Function(Box::new(f))),
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

/// Parse an invocation target: identifier or WGSL type constructor
/// Accepts regular identifiers plus vec2, vec3, vec4, ivec2, etc. for type constructors
fn invocation_target_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Ident, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    select! {
        // Regular identifiers
        Token::Ident(name) = e => Ident::new(name, span_from_simple(e.span())),
        Token::SelfKeyword = e => Ident::new("self".to_string(), span_from_simple(e.span())),
        // WGSL scalar type constructors (for casting)
        Token::F32Type = e => Ident::new("f32".to_string(), span_from_simple(e.span())),
        Token::I32Type = e => Ident::new("i32".to_string(), span_from_simple(e.span())),
        Token::U32Type = e => Ident::new("u32".to_string(), span_from_simple(e.span())),
        Token::BoolType = e => Ident::new("bool".to_string(), span_from_simple(e.span())),
        // WGSL vector type constructors
        Token::Vec2Type = e => Ident::new("vec2".to_string(), span_from_simple(e.span())),
        Token::Vec3Type = e => Ident::new("vec3".to_string(), span_from_simple(e.span())),
        Token::Vec4Type = e => Ident::new("vec4".to_string(), span_from_simple(e.span())),
        Token::IVec2Type = e => Ident::new("ivec2".to_string(), span_from_simple(e.span())),
        Token::IVec3Type = e => Ident::new("ivec3".to_string(), span_from_simple(e.span())),
        Token::IVec4Type = e => Ident::new("ivec4".to_string(), span_from_simple(e.span())),
        Token::UVec2Type = e => Ident::new("uvec2".to_string(), span_from_simple(e.span())),
        Token::UVec3Type = e => Ident::new("uvec3".to_string(), span_from_simple(e.span())),
        Token::UVec4Type = e => Ident::new("uvec4".to_string(), span_from_simple(e.span())),
        // WGSL matrix type constructors
        Token::Mat2Type = e => Ident::new("mat2".to_string(), span_from_simple(e.span())),
        Token::Mat3Type = e => Ident::new("mat3".to_string(), span_from_simple(e.span())),
        Token::Mat4Type = e => Ident::new("mat4".to_string(), span_from_simple(e.span())),
    }
    .labelled("identifier or type constructor")
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
/// Impl blocks contain only functions:
/// - `impl Struct { fn method(self) -> Type { body } }` - inherent impl
/// - `impl Trait for Struct { fn method(self) -> Type { body } }` - trait impl
fn impl_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, ImplDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Parse optional "Trait for" prefix
    let trait_for = ident_parser().then_ignore(just(Token::For)).or_not();

    just(Token::Impl)
        .ignore_then(trait_for)
        .then(ident_parser())
        .then(generic_params_parser())
        .then(impl_body_parser())
        .map_with(|(((trait_name, name), generics), functions), e| ImplDef {
            trait_name,
            name,
            generics,
            functions,
            span: span_from_simple(e.span()),
        })
}

/// Parse the body of an impl block (functions only)
fn impl_body_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<FnDef>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    fn_def_parser()
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
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
    let last = statements.pop().expect("checked non-empty");
    let result = match last {
        BlockStatement::Expr(expr) => expr,
        // If last is a statement (not expr), push it back and use Nil as result
        stmt @ BlockStatement::Let { .. } | stmt @ BlockStatement::Assign { .. } => {
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

/// Parse a function body: `{ statements... result_expr }` or `{ }` for empty
///
/// The function body is parsed as a block with multiple statements followed by a result.
fn fn_body_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Let binding
    let fn_let = just(Token::Let)
        .ignore_then(just(Token::Mut).or_not())
        .then(binding_pattern_parser())
        .then(just(Token::Colon).ignore_then(type_parser()).or_not())
        .then_ignore(just(Token::Equals))
        .then(expr_parser())
        .map_with(|(((mutable, pattern), ty), value), e| BlockStatement::Let {
            mutable: mutable.is_some(),
            pattern,
            ty,
            value,
            span: span_from_simple(e.span()),
        });

    // Assignment: expr = expr
    let fn_assign = expr_parser()
        .then_ignore(just(Token::Equals))
        .then(expr_parser())
        .map_with(|(target, value), e| BlockStatement::Assign {
            target,
            value,
            span: span_from_simple(e.span()),
        });

    // Expression item
    let fn_expr = expr_parser().map(BlockStatement::Expr);

    // Parse item (let, assign, or expr - in that order)
    let fn_item = choice((fn_let, fn_assign, fn_expr));

    // Parse body: items inside braces
    fn_item
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
        .map_with(|statements, e| block_statements_to_expr(statements, span_from_simple(e.span())))
}

/// Parse a function definition: `fn name(params) -> Type { body }`
fn fn_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FnDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    just(Token::Fn)
        .ignore_then(ident_parser())
        .then(fn_params_parser())
        .then(
            // Optional return type: -> Type
            just(Token::Arrow).ignore_then(type_parser()).or_not(),
        )
        .then(
            // Function body in braces - parsed as a block with statements
            fn_body_parser(),
        )
        .map_with(|(((name, params), return_type), body), e| {
            let span = span_from_simple(e.span());
            FnDef {
                name,
                params,
                return_type,
                body,
                span,
            }
        })
}

/// Parse function parameters: `(self, x: Type, y: Type = default)`
fn fn_params_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<FnParam>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let self_param = just(Token::SelfKeyword).map_with(|_, e| FnParam {
        name: Ident::new("self", span_from_simple(e.span())),
        ty: None,
        default: None,
        span: span_from_simple(e.span()),
    });

    let typed_param = ident_parser()
        .then_ignore(just(Token::Colon))
        .then(type_parser())
        .then(just(Token::Equals).ignore_then(expr_parser()).or_not())
        .map_with(|((name, ty), default), e| FnParam {
            name,
            ty: Some(ty),
            default,
            span: span_from_simple(e.span()),
        });

    choice((self_param, typed_param))
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect()
        .delimited_by(just(Token::LParen), just(Token::RParen))
}

/// Parse a standalone function definition: `pub fn name(params) -> Type { body }`
fn function_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FunctionDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then_ignore(just(Token::Fn))
        .then(ident_parser())
        .then(fn_params_parser())
        .then(
            // Optional return type: -> Type
            just(Token::Arrow).ignore_then(type_parser()).or_not(),
        )
        .then(
            // Function body in braces - parsed as a block with statements
            fn_body_parser(),
        )
        .map_with(|((((visibility, name), params), return_type), body), e| {
            let span = span_from_simple(e.span());
            FunctionDef {
                visibility,
                name,
                params,
                return_type,
                body,
                span,
            }
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
            // GPU scalar types
            just(Token::F32Type).to(Type::Primitive(PrimitiveType::F32)),
            just(Token::I32Type).to(Type::Primitive(PrimitiveType::I32)),
            just(Token::U32Type).to(Type::Primitive(PrimitiveType::U32)),
            just(Token::BoolType).to(Type::Primitive(PrimitiveType::Bool)),
            // GPU vector types
            just(Token::Vec2Type).to(Type::Primitive(PrimitiveType::Vec2)),
            just(Token::Vec3Type).to(Type::Primitive(PrimitiveType::Vec3)),
            just(Token::Vec4Type).to(Type::Primitive(PrimitiveType::Vec4)),
            just(Token::IVec2Type).to(Type::Primitive(PrimitiveType::IVec2)),
            just(Token::IVec3Type).to(Type::Primitive(PrimitiveType::IVec3)),
            just(Token::IVec4Type).to(Type::Primitive(PrimitiveType::IVec4)),
            just(Token::UVec2Type).to(Type::Primitive(PrimitiveType::UVec2)),
            just(Token::UVec3Type).to(Type::Primitive(PrimitiveType::UVec3)),
            just(Token::UVec4Type).to(Type::Primitive(PrimitiveType::UVec4)),
            // GPU matrix types
            just(Token::Mat2Type).to(Type::Primitive(PrimitiveType::Mat2)),
            just(Token::Mat3Type).to(Type::Primitive(PrimitiveType::Mat3)),
            just(Token::Mat4Type).to(Type::Primitive(PrimitiveType::Mat4)),
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
            select! { Token::UnsignedInt(n) => Expr::Literal(Literal::UnsignedInt(n)) },
            select! { Token::SignedInt(n) => Expr::Literal(Literal::SignedInt(n)) },
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

        // Helper to parse invocation arguments: either named (name: expr) or positional (expr)
        // Returns Vec<(Option<Ident>, Expr)> where Some(name) is named, None is positional
        // Named args use lookahead to check for ident: pattern before committing
        let named_invoc_arg = ident_parser()
            .then(just(Token::Colon))
            .rewind() // Lookahead: check for ident: without consuming
            .ignore_then(
                ident_parser()
                    .then_ignore(just(Token::Colon))
                    .then(expr.clone()),
            )
            .map(|(name, value)| (Some(name), value));
        let positional_invoc_arg = expr.clone().map(|value| (None, value));
        let invocation_arg = named_invoc_arg.or(positional_invoc_arg);

        let invocation_args = invocation_arg
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        // Helper to parse named arguments for enums: name: expr, name: expr, ...
        // Requires at least one argument if parens are present (no empty parens allowed)
        // Uses lookahead: peek for ( ident : pattern before committing to parse
        let enum_named_args = just(Token::LParen)
            .ignore_then(ident_parser())
            .then(just(Token::Colon))
            .rewind() // Lookahead: if we see ( ident :, this is a named arg pattern
            .ignore_then(
                ident_parser()
                    .then_ignore(just(Token::Colon))
                    .then(expr.clone())
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            );

        // Inferred enum instantiation: .variant(field: value, field: value, ...)
        let inferred_enum_instantiation = just(Token::Dot)
            .ignore_then(ident_parser())
            .then(enum_named_args.clone().or_not())
            .map_with(|(variant, data), e| Expr::InferredEnumInstantiation {
                variant,
                data: data.unwrap_or_default(),
                span: span_from_simple(e.span()),
            });

        // Enum instantiation: EnumType.variant OR EnumType.variant(field: value, ...)
        // Supports module-qualified paths: module::EnumType.variant
        // Note: Uses ident_no_self_parser to prevent 'self.field' from being parsed as enum instantiation
        // IMPORTANT: If there are parens, they MUST contain named args (ident: value).
        // This prevents foo.bar(1) from being parsed as enum instantiation.
        // IMPORTANT: The type name (last path element) must start with uppercase to distinguish
        // from field access (e.g., `Status.active` vs `point.x`).
        let enum_base = ident_no_self_parser()
            .separated_by(just(Token::DoubleColon))
            .at_least(1)
            .collect::<Vec<_>>()
            .then_ignore(just(Token::Dot))
            .then(ident_parser())
            // Filter: only match if the type name (last path element) starts with uppercase
            // This distinguishes `Status.active` (enum) from `point.x` (field access)
            .try_map(|(path, variant), span| {
                let type_name = path.last().map(|id| id.name.as_str()).unwrap_or("");
                if type_name
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
                {
                    Ok((path, variant))
                } else {
                    Err(Rich::custom(
                        span,
                        "enum type names must start with uppercase",
                    ))
                }
            });
        // With named args: Type.variant(name: value, ...)
        let enum_with_args = enum_base
            .clone()
            .then(enum_named_args.clone())
            .map(|((path, variant), data)| (path, variant, data));
        // Without args: Type.variant (no parens at all - checked by NOT seeing LParen)
        let enum_without_args = enum_base
            .clone()
            .then(just(Token::LParen).not().rewind())
            .map(|((path, variant), _)| (path, variant, vec![]));
        // Try with-args first, then without-args
        let enum_instantiation =
            enum_with_args
                .or(enum_without_args)
                .map_with(|(path, variant, data), e| {
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
                        data,
                        span: span_from_simple(e.span()),
                    }
                });

        // Invocation: Name(arg: value, ...) or Name<Type>(arg: value, ...)
        // Can be struct instantiation, function call, or WGSL type constructor
        // With optional mount points (for structs): Name(arg: value) { mount: expr, ... }
        // Supports module-qualified paths: module::Name(...)
        // Also supports WGSL type constructors: vec2(x, y), mat4(...)
        let invocation = invocation_target_parser()
            .separated_by(just(Token::DoubleColon))
            .at_least(1)
            .collect::<Vec<_>>()
            .then(
                // Optional generic arguments (only valid for struct instantiation)
                type_parser()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::Lt), just(Token::Gt))
                    .or_not(),
            )
            .then(invocation_args.clone())
            .then(
                // Optional mount points (only valid for struct instantiation)
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
                // Keep path as Vec<Ident> for semantic analysis to resolve
                Expr::Invocation {
                    path,
                    type_args: type_args.unwrap_or_default(),
                    args,
                    mounts: mounts.unwrap_or_default(),
                    span: span_from_simple(e.span()),
                }
            });

        // Reference: single identifier (e.g., user, self, field)
        // Field access like foo.bar is handled by the postfix `.` operator
        // Colon-separated paths are no longer supported
        let reference = ident_parser().map_with(|ident, e| Expr::Reference {
            path: vec![ident],
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
        // Also supports pipe syntax: |x, y| expr, |x: T, y: T| -> T { body }
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
            .clone()
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

        // Pipe-delimited closure: |params| -> type { body } or |params| { body } or |params| expr
        // Also handles || { body } for empty params
        let pipe_closure = just(Token::Pipe)
            .ignore_then(
                closure_param
                    .clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then_ignore(just(Token::Pipe))
            .then(
                // Optional return type: -> Type
                just(Token::Arrow).ignore_then(type_parser()).or_not(),
            )
            .then(expr.clone())
            .map_with(|((params, _return_type), body), e| Expr::ClosureExpr {
                params,
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // Block item parsers using BlockStatement directly
        // Let binding in block
        let block_let_item = just(Token::Let)
            .ignore_then(just(Token::Mut).or_not())
            .then(binding_pattern_parser())
            .then(just(Token::Colon).ignore_then(type_parser()).or_not())
            .then_ignore(just(Token::Equals))
            .then(expr.clone())
            .map_with(|(((mutable, pattern), ty), value), e| BlockStatement::Let {
                mutable: mutable.is_some(),
                pattern,
                ty,
                value,
                span: span_from_simple(e.span()),
            });

        // Assignment: target = value
        let block_assign_item = expr
            .clone()
            .then_ignore(just(Token::Equals))
            .then(expr.clone())
            .map_with(|(target, value), e| BlockStatement::Assign {
                target,
                value,
                span: span_from_simple(e.span()),
            });

        // Expression item
        let block_expr_item = expr.clone().map(BlockStatement::Expr);

        // Parse a block item (let, assign, or expr - in that order)
        let block_item = choice((
            block_let_item.clone(),
            block_assign_item.clone(),
            block_expr_item.clone(),
        ));

        // Block body parser: { items... } -> Expr (Block or single expr)
        // Uses shared block_statements_to_expr helper
        // Reused in for_expr, if_expr
        let block_body = block_item
            .clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with(|stmts, e| block_statements_to_expr(stmts, span_from_simple(e.span())));

        // For expression: for var in collection { body }
        let for_expr = just(Token::For)
            .ignore_then(ident_parser())
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .then(block_body.clone())
            .map_with(|((var, collection), body), e| Expr::ForExpr {
                var,
                collection: Box::new(collection),
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // If expression: if condition { then } else { else }
        // Also handles else-if chains: if cond { } else if cond { } else { }
        let if_expr = recursive(|if_expr_rec| {
            just(Token::If)
                .ignore_then(expr.clone())
                .then(block_body.clone())
                .then(
                    just(Token::Else)
                        .ignore_then(
                            // Either another if expression (else-if chain) or a block { ... }
                            if_expr_rec.clone().or(block_body.clone()),
                        )
                        .or_not(),
                )
                .map_with(|((condition, then_branch), else_branch), e| Expr::IfExpr {
                    condition: Box::new(condition),
                    then_branch: Box::new(then_branch),
                    else_branch: else_branch.map(Box::new),
                    span: span_from_simple(e.span()),
                })
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

        // Atom: literal, instantiation, enum_instantiation, reference, array/dict, tuple, grouped, for, if, match, closure, let, block
        // Order matters: try more specific parsers first
        let atom = choice((
            literal,
            for_expr,
            if_expr,
            match_expr,
            let_expr,      // Let expressions
            block_body,    // Block expressions: { let x = 1; expr }
            array_or_dict, // Handles both array and dictionary literals
            tuple,         // Must come before grouped (tuple is more specific)
            grouped,
            pipe_closure,                // |x| expr or |x, y| -> T { body }
            no_param_closure,            // () -> expr (must come before other closures and tuples)
            param_closure, // x -> expr (must come before reference since starts with ident)
            inferred_enum_instantiation, // .variant is most specific
            enum_instantiation, // Must come before invocation and reference (Type.variant(...))
            invocation, // Unified struct instantiation / function call - resolved in semantic analysis
            reference,  // Most general (ident), now includes 'self'
        ));

        // Binary operators with precedence using pratt parser
        atom.pratt((
            // Unary operators (highest precedence: 9)
            prefix(9, just(Token::Minus), |_, operand, e| Expr::UnaryOp {
                op: UnaryOperator::Neg,
                operand: Box::new(operand),
                span: span_from_simple(e.span()),
            }),
            prefix(9, just(Token::Bang), |_, operand, e| Expr::UnaryOp {
                op: UnaryOperator::Not,
                operand: Box::new(operand),
                span: span_from_simple(e.span()),
            }),
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
            // Logical OR (precedence: 1)
            infix(left(1), just(Token::Or), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Or,
                right: Box::new(r),
                span: span_from_simple(e.span()),
            }),
            // Range (precedence: 0, lowest - so arithmetic binds tighter)
            infix(left(0), just(Token::DotDot), |l, _, r, e| Expr::BinaryOp {
                left: Box::new(l),
                op: BinaryOperator::Range,
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
            // Method call: expr.method(arg1, arg2, ...) (precedence: 8, higher than field access)
            // Must come before field access since it's more specific
            // Uses invocation_args to handle both named and positional arguments
            postfix(
                8,
                just(Token::Dot)
                    .ignore_then(ident_parser())
                    .then(invocation_args.clone()),
                |receiver, (method, args): (Ident, Vec<(Option<Ident>, Expr)>), e| {
                    Expr::MethodCall {
                        receiver: Box::new(receiver),
                        method,
                        args: args.into_iter().map(|(_, v)| v).collect(),
                        span: span_from_simple(e.span()),
                    }
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
                            // For non-reference expressions (e.g., -chord, (a+b)),
                            // use FieldAccess to preserve the base expression
                            Expr::FieldAccess {
                                object: Box::new(object),
                                field,
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

/// Parse a pattern: variant or variant(binding1, binding2) or .variant or .variant(binding1, binding2) or _
fn pattern_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Pattern, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Wildcard pattern: _
    let wildcard = just(Token::Underscore).to(Pattern::Wildcard);

    // Variant pattern: .variant or .variant(bindings) or variant or variant(bindings)
    let variant = choice((
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
    });

    choice((wildcard, variant))
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
        if let Some(Statement::Definition(def)) = file.statements.first() {
            if let Definition::Struct(s) = &**def {
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
        } else {
            panic!("No definition found");
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
            struct App {
                content: [String] = for item in items {
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

    #[test]
    fn test_block_expr_simple() {
        let result = parse_expr_from_let("{ let x = 1 x }");
        assert!(result.is_ok(), "Failed to parse block: {:?}", result);
    }

    #[test]
    fn test_block_expr_with_call() {
        let result = parse_expr_from_let("{ let v = foo.bar(1) Result(value: v) }");
        assert!(
            result.is_ok(),
            "Failed to parse block with call: {:?}",
            result
        );
    }

    #[test]
    fn test_block_expr_no_let() {
        // Block with just a result expression (no let statements)
        let result = parse_expr_from_let("{ Result(value: 1) }");
        assert!(result.is_ok(), "Failed to parse block no let: {:?}", result);
    }

    #[test]
    fn test_block_expr_let_simple_then_call() {
        // Block with let binding a literal, then a call
        let result = parse_expr_from_let("{ let v = 1 Result(value: v) }");
        assert!(
            result.is_ok(),
            "Failed to parse block let simple then call: {:?}",
            result
        );
    }

    #[test]
    fn test_block_expr_let_field_access() {
        // Block with let binding field access, then a reference
        let result = parse_expr_from_let("{ let v = foo.bar v }");
        assert!(
            result.is_ok(),
            "Failed to parse block let field access: {:?}",
            result
        );
    }

    #[test]
    fn test_block_expr_let_call_then_ref() {
        // Block with let binding a call, then a reference
        let result = parse_expr_from_let("{ let v = foo(1) v }");
        assert!(
            result.is_ok(),
            "Failed to parse block let call then ref: {:?}",
            result
        );
    }

    #[test]
    fn test_block_expr_let_method_call_then_ref() {
        // Block with let binding a method call, then a reference
        let result = parse_expr_from_let("{ let v = foo.bar(1) v }");
        assert!(
            result.is_ok(),
            "Failed to parse block let method call then ref: {:?}",
            result
        );
    }

    #[test]
    fn test_let_expr_method_call_then_ref() {
        // Let expression with method call value, then reference body
        // This uses the let EXPRESSION, not block statement
        let result = parse_expr_from_let("let v = foo.bar(1) v");
        assert!(
            result.is_ok(),
            "Failed to parse let expr method call then ref: {:?}",
            result
        );
    }

    #[test]
    fn test_let_expr_fn_call_then_ref() {
        // Let expression with function call value, then reference body
        let result = parse_expr_from_let("let v = foo(1) v");
        assert!(
            result.is_ok(),
            "Failed to parse let expr fn call then ref: {:?}",
            result
        );
    }

    #[test]
    fn test_let_expr_field_access_then_ref() {
        // Let expression with field access value, then reference body
        let result = parse_expr_from_let("let v = foo.bar v");
        assert!(
            result.is_ok(),
            "Failed to parse let expr field access then ref: {:?}",
            result
        );
    }

    #[test]
    fn test_method_call_standalone() {
        // Just a method call, no following expression
        let result = parse_expr_from_let("foo.bar(1)");
        assert!(
            result.is_ok(),
            "Failed to parse standalone method call: {:?}",
            result
        );
    }

    #[test]
    fn test_method_call_no_args() {
        // Method call with no args
        let result = parse_expr_from_let("foo.bar()");
        assert!(
            result.is_ok(),
            "Failed to parse method call no args: {:?}",
            result
        );
    }

    #[test]
    fn test_field_access_standalone() {
        // Field access (no parens)
        let result = parse_expr_from_let("foo.bar");
        assert!(result.is_ok(), "Failed to parse field access: {:?}", result);
    }

    #[test]
    fn test_reference_standalone() {
        // Just a reference
        let result = parse_expr_from_let("foo");
        assert!(result.is_ok(), "Failed to parse reference: {:?}", result);
    }

    #[test]
    fn test_method_call_on_self() {
        // Method call on self
        let result = parse_expr_from_let("self.bar(1)");
        assert!(
            result.is_ok(),
            "Failed to parse method call on self: {:?}",
            result
        );
    }

    #[test]
    fn test_method_call_on_this() {
        // Method call on 'this' (not a keyword, just an identifier)
        let result = parse_expr_from_let("this.bar(1)");
        assert!(
            result.is_ok(),
            "Failed to parse method call on this: {:?}",
            result
        );
    }

    #[test]
    fn test_invocation_simple() {
        // Simple invocation (should work)
        let result = parse_expr_from_let("foo(1)");
        assert!(result.is_ok(), "Failed to parse invocation: {:?}", result);
    }
}
