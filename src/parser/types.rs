// Type expression parsers

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{Ident, PrimitiveType, TupleField, Type};
use crate::lexer::Token;

use super::ident_parser;
use super::span_from_simple;

/// Parse a type expression
#[expect(
    clippy::too_many_lines,
    reason = "parser combinator composition — local parsers are captured by closures and cannot be extracted without restructuring"
)]
pub(super) fn type_parser<'tokens, I>(
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
                    .map(|id: &Ident| id.name.as_str())
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
