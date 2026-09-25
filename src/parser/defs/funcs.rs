//! Function-related parsers: signatures, parameters, bodies, and full
//! standalone definitions.

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{BlockStatement, FnDef, FnParam, FnSig, FunctionDef, Ident, ParamConvention};
use crate::lexer::Token;

use super::super::recovery::{skip_failed_statement, statement_end};
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
        .then(generic_params_parser())
        .then(fn_params_parser())
        .then(just(Token::Arrow).ignore_then(type_parser()).or_not())
        .map_with(
            |((((attributes, name), generics), params), return_type), e| FnSig {
                name,
                generics,
                params,
                return_type,
                attributes,
                span: span_from_simple(e.span()),
            },
        )
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

    // Assignment (`target = value`) or a bare expression.
    //
    // One parser, not two: an assignment is an expression followed by
    // `= value`, so parsing them as separate alternatives read the
    // expression twice for every statement that is not an assignment.
    // `block_item` in `src/parser/exprs/mod.rs` had the same shape and
    // the same cost.
    //
    // A statement that starts with `let` is only ever `fn_let`. When
    // that fails, the `let ... in` expression fails on the same tokens,
    // so it is not tried: the second attempt doubled the time for each
    // level of nesting.
    let fn_assign_or_expr = just(Token::Let)
        .not()
        .ignore_then(expr_parser())
        .then(just(Token::Equals).ignore_then(expr_parser()).or_not())
        .map_with(|(target, value), e| match value {
            Some(value) => BlockStatement::Assign {
                target,
                value,
                span: span_from_simple(e.span()),
            },
            None => BlockStatement::Expr(target),
        });

    // Wrap each item in `recover_with(via_parser(...))` so a malformed
    // item (broken expression) is recovered by skipping to the next
    // `let` or `}`. Without this, one bad function body suppresses
    // diagnostics for the rest of the file. See `skip_failed_statement`
    // for what the recovery skips.
    let recovery = skip_failed_statement().map_with(|(), e| {
        BlockStatement::Expr(crate::ast::Expr::Group {
            expr: Box::new(crate::ast::Expr::Literal {
                value: crate::ast::Literal::Nil,
                span: span_from_simple(e.span()),
            }),
            span: span_from_simple(e.span()),
        })
    });
    // Statements are separated by newlines. The lexer keeps only the
    // newlines that end a statement, so consuming them here is what
    // stops one statement running into the next.
    let breaks = just(Token::Newline).repeated().ignored();
    let fn_item = breaks
        .clone()
        .ignore_then(choice((fn_let, fn_assign_or_expr)))
        .then_ignore(statement_end())
        .then_ignore(breaks.clone())
        .recover_with(via_parser(recovery));

    fn_item
        .repeated()
        .collect::<Vec<_>>()
        .then_ignore(breaks)
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
        .map_with(|statements, e| block_statements_to_expr(statements, span_from_simple(e.span())))
}

/// Parse a method definition: `fn name<generics>(params) -> Type { body }`
pub(super) fn fn_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FnDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    doc_comments_parser()
        .then(fn_attributes_parser())
        .then_ignore(just(Token::Fn))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(fn_params_parser())
        .then(just(Token::Arrow).ignore_then(type_parser()).or_not())
        .then(fn_body_parser())
        .map_with(
            |((((((doc, attributes), name), generics), params), return_type), body), e| {
                let span = span_from_simple(e.span());
                FnDef {
                    name,
                    generics,
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

/// Parse method parameters: `(self, mut self, x: Type, mut x: Type, sink x: Type, label name: Type)`
///
/// A `self` parameter may come only first. Parameters support an optional convention prefix (`mut` or `sink`) and
/// an optional external label: `fn foo(en name: String)` where `en` is
/// the call-site label and `name` is the internal parameter name.
pub(super) fn fn_params_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<FnParam>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let self_param =
        convention_parser()
            .then(just(Token::SelfKeyword))
            .map_with(|(convention, _), e| FnParam {
                convention,
                external_label: None,
                name: Ident::new("self", span_from_simple(e.span())),
                ty: None,
                default: None,
                span: span_from_simple(e.span()),
            });
    let rest = plain_params_parser();
    let with_self = self_param
        .then(just(Token::Comma).ignore_then(rest.clone()).or_not())
        .map(|(receiver, rest)| {
            let mut params = vec![receiver];
            params.extend(rest.unwrap_or_default());
            params
        });
    choice((with_self, rest)).delimited_by(just(Token::LParen), just(Token::RParen))
}

/// Parse the parameters of a free function: `(x: Type, label name: Type)`.
///
/// A free function has no receiver, so `self` is not a parameter here.
pub(super) fn free_fn_params_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<FnParam>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    plain_params_parser().delimited_by(just(Token::LParen), just(Token::RParen))
}

fn convention_parser<'tokens, I>(
) -> impl Parser<'tokens, I, ParamConvention, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    choice((
        just(Token::Mut).to(ParamConvention::Mut),
        just(Token::Sink).to(ParamConvention::Sink),
    ))
    .or_not()
    .map(|c| c.unwrap_or(ParamConvention::Let))
}

/// Parse a comma-separated list of parameters that are not `self`.
fn plain_params_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<FnParam>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let convention = convention_parser();

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

    // Order matters: longer matches first. `labeled_param` (ident ident :) before `typed_param` (ident :) before
    // `type_only_param` (Type with no name) so a single `Foo: Bar` still
    // parses as a typed param, not as type `Foo::Bar` followed by junk.
    choice((labeled_param, typed_param, type_only_param))
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect()
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
        .then(free_fn_params_parser())
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
