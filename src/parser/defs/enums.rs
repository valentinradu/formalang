//! Enum definition / variant parsers.

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{EnumDef, EnumVariant};
use crate::lexer::Token;

use super::super::{doc_comments_parser, ident_parser, span_from_simple, visibility_parser};
use super::{field_def_parser, generic_params_parser};

pub(super) fn enum_def_parser<'tokens, I>(
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
            doc: None,
            span: span_from_simple(e.span()),
        })
}

/// Parse enum variants: `{ variant, variant(Type), ... }`
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

/// Parse a single enum variant: `name` or `name(field: Type, field: Type)`.
fn enum_variant_parser<'tokens, I>(
) -> impl Parser<'tokens, I, EnumVariant, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    doc_comments_parser()
        .ignore_then(ident_parser())
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
