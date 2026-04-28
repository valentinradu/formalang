//! Extern function and impl-block parsers.

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{ExternAbi, FnDef, FunctionDef, ImplDef};
use crate::lexer::Token;

use super::super::{ident_parser, span_from_simple, types::type_parser, visibility_parser};
use super::{
    fn_attributes_parser, fn_def_parser, fn_params_parser, fn_sig_parser, generic_params_parser,
};

/// Parse the optional ABI string after `extern`. Defaults to `"C"` when
/// the string is omitted; accepts `"C"` and `"system"`.
fn extern_abi_parser<'tokens, I>(
) -> impl Parser<'tokens, I, ExternAbi, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    select! { Token::String(s) => s }
        .try_map(|s, span: SimpleSpan| match s.as_str() {
            "C" => Ok(ExternAbi::C),
            "system" => Ok(ExternAbi::System),
            other => Err(Rich::custom(
                span,
                format!("unknown extern ABI \"{other}\"; expected \"C\" or \"system\""),
            )),
        })
        .or_not()
        .map(|abi| abi.unwrap_or(ExternAbi::C))
}

/// Parse an extern function declaration: `extern fn name(params) -> Type`
/// or `extern "C" fn name(...)` / `extern "system" fn name(...)`.
pub(super) fn extern_fn_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FunctionDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then(fn_attributes_parser())
        .then_ignore(just(Token::Extern))
        .then(extern_abi_parser())
        .then_ignore(just(Token::Fn))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(fn_params_parser())
        .then(just(Token::Arrow).ignore_then(type_parser()).or_not())
        .map_with(
            |((((((visibility, attributes), abi), name), generics), params), return_type), e| {
                let span = span_from_simple(e.span());
                FunctionDef {
                    visibility,
                    name,
                    generics,
                    params,
                    return_type,
                    body: None,
                    extern_abi: Some(abi),
                    attributes,
                    doc: None,
                    span,
                }
            },
        )
}

/// Parse an extern impl block: `extern impl Trait for Name<T> { fn_sig* }`
pub(super) fn extern_impl_parser<'tokens, I>(
) -> impl Parser<'tokens, I, ImplDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let trait_for = ident_parser()
        .then(
            type_parser()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::Lt), just(Token::Gt))
                .or_not(),
        )
        .then_ignore(just(Token::For))
        .or_not();

    // Each item is either a fn def (with body — invalid, but parsed so
    // semantic can emit ExternImplWithBody) or a bare fn sig.
    let extern_impl_item = choice((
        fn_def_parser(),
        fn_sig_parser().map(|sig| FnDef {
            name: sig.name,
            params: sig.params,
            return_type: sig.return_type,
            body: None,
            attributes: sig.attributes,
            doc: None,
            span: sig.span,
        }),
    ));

    just(Token::Extern)
        .ignore_then(just(Token::Impl))
        .ignore_then(trait_for)
        .then(ident_parser())
        .then(generic_params_parser())
        .then(
            extern_impl_item
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|(((trait_for_pair, name), generics), functions), e| {
            let (trait_name, trait_args) = match trait_for_pair {
                Some((tname, args)) => (Some(tname), args.unwrap_or_default()),
                None => (None, Vec::new()),
            };
            ImplDef {
                trait_name,
                trait_args,
                name,
                generics,
                functions,
                is_extern: true,
                doc: None,
                span: span_from_simple(e.span()),
            }
        })
}
