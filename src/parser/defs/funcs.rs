//! Function-related parsers: signatures, parameters, bodies, and full
//! standalone definitions.

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{BlockStatement, FnDef, FnParam, FnSig, FunctionDef, Ident, ParamConvention};
use crate::lexer::Token;

use super::super::{
    block_statements_to_expr, doc_comments_parser, exprs::expr_parser, ident_parser,
    span_from_simple, types::type_parser, visibility_parser,
};
use super::{binding_pattern_parser, fn_attributes_parser, generic_params_parser};

pub(super) fn fn_sig_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FnSig, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    doc_comments_parser()
        .ignore_then(fn_attributes_parser())
        .then_ignore(just(Token::Fn))
        .then(ident_parser())
        .then(fn_params_parser())
        .then(just(Token::Arrow).ignore_then(type_parser()).or_not())
        .map_with(|(((attributes, name), params), return_type), e| FnSig {
            name,
            params,
            return_type,
            attributes,
            span: span_from_simple(e.span()),
        })
}

/// Parse a function body as a brace-delimited block of statements with a
/// trailing result expression.
pub(super) fn fn_body_parser<'tokens, I>(
) -> impl Parser<'tokens, I, crate::ast::Expr, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
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

    let fn_assign = expr_parser()
        .then_ignore(just(Token::Equals))
        .then(expr_parser())
        .map_with(|(target, value), e| BlockStatement::Assign {
            target,
            value,
            span: span_from_simple(e.span()),
        });

    let fn_expr = expr_parser().map(BlockStatement::Expr);

    // Wrap each item in `recover_with(via_parser(...))` so a malformed
    // item (broken expression) is recovered by skipping to the next
    // `let` or `}`. Without this, one bad function body suppresses
    // diagnostics for the rest of the file. The first
    // token is consumed unconditionally on `Let` (so an item starting
    // with `let` whose value is broken can be recovered), but never on
    // `RBrace` (so the body's closing brace stays for `delimited_by`).
    let recovery_head = any().and_is(just(Token::RBrace).not()).ignored();
    let recovery_tail = any()
        .and_is(just(Token::Let).not())
        .and_is(just(Token::RBrace).not())
        .ignored()
        .repeated();
    let recovery = recovery_head.then(recovery_tail).map_with(|((), ()), e| {
        BlockStatement::Expr(crate::ast::Expr::Group {
            expr: Box::new(crate::ast::Expr::Literal {
                value: crate::ast::Literal::Nil,
                span: span_from_simple(e.span()),
            }),
            span: span_from_simple(e.span()),
        })
    });
    let fn_item = choice((fn_let, fn_assign, fn_expr)).recover_with(via_parser(recovery));

    fn_item
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
        .map_with(|statements, e| block_statements_to_expr(statements, span_from_simple(e.span())))
}

/// Parse a function definition: `fn name(params) -> Type { body }`
pub(super) fn fn_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FnDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    doc_comments_parser()
        .then(fn_attributes_parser())
        .then_ignore(just(Token::Fn))
        .then(ident_parser())
        .then(fn_params_parser())
        .then(just(Token::Arrow).ignore_then(type_parser()).or_not())
        .then(fn_body_parser())
        .map_with(
            |(((((doc, attributes), name), params), return_type), body), e| {
                let span = span_from_simple(e.span());
                FnDef {
                    name,
                    params,
                    return_type,
                    body: Some(body),
                    attributes,
                    doc,
                    span,
                }
            },
        )
}

/// Parse function parameters: `(self, mut self, x: Type, mut x: Type, sink x: Type, label name: Type)`
///
/// Parameters support an optional convention prefix (`mut` or `sink`) and
/// an optional external label: `fn foo(en name: String)` where `en` is
/// the call-site label and `name` is the internal parameter name.
pub(super) fn fn_params_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<FnParam>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let convention = choice((
        just(Token::Mut).to(ParamConvention::Mut),
        just(Token::Sink).to(ParamConvention::Sink),
    ))
    .or_not()
    .map(|c| c.unwrap_or(ParamConvention::Let));

    let self_param =
        convention
            .clone()
            .then(just(Token::SelfKeyword))
            .map_with(|(convention, _), e| FnParam {
                convention,
                external_label: None,
                name: Ident::new("self", span_from_simple(e.span())),
                ty: None,
                default: None,
                span: span_from_simple(e.span()),
            });

    let labeled_param = convention
        .clone()
        .then(ident_parser())
        .then(ident_parser())
        .then_ignore(just(Token::Colon))
        .then(type_parser())
        .then(just(Token::Equals).ignore_then(expr_parser()).or_not())
        .map_with(|((((convention, label), name), ty), default), e| FnParam {
            convention,
            external_label: Some(label),
            name,
            ty: Some(ty),
            default,
            span: span_from_simple(e.span()),
        });

    let typed_param = convention
        .clone()
        .then(ident_parser())
        .then_ignore(just(Token::Colon))
        .then(type_parser())
        .then(just(Token::Equals).ignore_then(expr_parser()).or_not())
        .map_with(|(((convention, name), ty), default), e| FnParam {
            convention,
            external_label: None,
            name,
            ty: Some(ty),
            default,
            span: span_from_simple(e.span()),
        });

    // `Type` only (Mode B overloading — positional, no name, no label).
    // Synthesise a unique name from the parameter's start offset
    // (`_arg<offset>`) so two type-only params in the same fn don't
    // share a scope-table key.
    let type_only_param = convention
        .clone()
        .then(type_parser())
        .map_with(|(convention, ty), e| {
            let span = span_from_simple(e.span());
            let synth = format!("_arg{}", span.start.offset);
            FnParam {
                convention,
                external_label: None,
                name: Ident::new(&synth, span),
                ty: Some(ty),
                default: None,
                span,
            }
        });

    // Order matters: longer matches first. `self_param` precedes the rest;
    // `labeled_param` (ident ident :) before `typed_param` (ident :) before
    // `type_only_param` (Type with no name) so a single `Foo: Bar` still
    // parses as a typed param, not as type `Foo::Bar` followed by junk.
    choice((self_param, labeled_param, typed_param, type_only_param))
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect()
        .delimited_by(just(Token::LParen), just(Token::RParen))
}

/// Parse a standalone function definition: `pub fn name(params) -> Type { body }`
pub(super) fn function_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FunctionDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then(fn_attributes_parser())
        .then_ignore(just(Token::Fn))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(fn_params_parser())
        .then(just(Token::Arrow).ignore_then(type_parser()).or_not())
        .then(fn_body_parser())
        .map_with(
            |((((((visibility, attributes), name), generics), params), return_type), body), e| {
                let span = span_from_simple(e.span());
                FunctionDef {
                    visibility,
                    name,
                    generics,
                    params,
                    return_type,
                    body: Some(body),
                    extern_abi: None,
                    attributes,
                    doc: None,
                    span,
                }
            },
        )
}
