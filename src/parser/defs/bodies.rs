//! The parsers of definitions with a body of items: `impl` blocks and
//! `mod` definitions. Each item of such a body is on a line of its own.

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{Definition, FnDef, ImplDef, ModuleDef};
use crate::lexer::Token;

use super::super::types::type_parser;
use super::super::{
    ident_parser, inner_doc_comments_parser, newlines, span_from_simple, visibility_parser,
};
use super::funcs::{fn_def_parser, fn_sig_parser};
use super::generic_params_parser;

/// Parse an impl block definition
/// Impl blocks contain only functions:
/// - `impl Struct { fn method(self) -> Type { body } }` - inherent impl
/// - `impl Trait for Struct { fn method(self) -> Type { body } }` - trait impl
pub(super) fn impl_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, ImplDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Parse optional "Trait[<X, Y, ...>] for" prefix.
    // Phase B: trait_args lets `impl Foo<X> for Y { ... }` parse with
    // the inner generic-trait instantiation preserved.
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

    just(Token::Impl)
        .ignore_then(trait_for)
        .then(ident_parser())
        .then(generic_params_parser())
        .then(impl_body_parser())
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
                is_extern: false,
                doc: None,
                span: span_from_simple(e.span()),
            }
        })
}

/// Parse the body of an impl block: `{ ... }` with one method or
/// signature on each line.
pub(super) fn impl_body_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<FnDef>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Full definition (with body) takes priority; fall back to bare signature.
    let impl_item = choice((
        fn_def_parser(),
        fn_sig_parser().map(|sig| FnDef {
            name: sig.name,
            generics: sig.generics,
            params: sig.params,
            return_type: sig.return_type,
            body: None,
            attributes: sig.attributes,
            doc: None,
            span: sig.span,
        }),
    ));
    newlines()
        .ignore_then(impl_item)
        .then_ignore(item_end())
        .then_ignore(newlines())
        .repeated()
        .collect::<Vec<_>>()
        .padded_by(newlines())
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
}

/// The end of one item in a `{ ... }` body of definitions: a line break
/// or the closing `}`. One item per line, as at the top level.
pub(super) fn item_end<'tokens, I>(
) -> impl Parser<'tokens, I, (), extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    choice((just(Token::Newline), just(Token::RBrace)))
        .ignored()
        .rewind()
        .labelled("end of line")
}

/// Parse a `mod name { ... }` definition. Each definition inside it is
/// on a line of its own. `//!` doc comments at the start of the body
/// document the module.
pub(super) fn module_def_parser<'tokens, I>(
    def_parser: impl Parser<'tokens, I, Definition, extra::Err<Rich<'tokens, Token>>> + Clone,
) -> impl Parser<'tokens, I, ModuleDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then_ignore(just(Token::Module))
        .then(ident_parser())
        .then(
            newlines()
                .ignore_then(inner_doc_comments_parser())
                .then(
                    newlines()
                        .ignore_then(def_parser)
                        .then_ignore(item_end())
                        .then_ignore(newlines())
                        .repeated()
                        .collect()
                        .padded_by(newlines()),
                )
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((visibility, name), (doc, definitions)), e| ModuleDef {
            visibility,
            name,
            definitions,
            doc,
            span: span_from_simple(e.span()),
        })
}
