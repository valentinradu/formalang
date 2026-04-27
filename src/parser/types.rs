// Type expression parsers

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{Ident, ParamConvention, PrimitiveType, TupleField, Type};
use crate::lexer::Token;

use super::ident_parser;
use super::span_from_simple;

/// Map a single-segment type identifier to its primitive type, if any.
///
/// Returns `Some(primitive)` for the primitive type names (`String`, `Number`,
/// `I32`, `I64`, `F32`, `F64`, `Boolean`, `Path`, `Regex`, `Never`) and `None`
/// for any other identifier.
fn primitive_from_name(name: &str) -> Option<PrimitiveType> {
    match name {
        "String" => Some(PrimitiveType::String),
        "Number" => Some(PrimitiveType::Number),
        "I32" => Some(PrimitiveType::I32),
        "I64" => Some(PrimitiveType::I64),
        "F32" => Some(PrimitiveType::F32),
        "F64" => Some(PrimitiveType::F64),
        "Boolean" => Some(PrimitiveType::Boolean),
        "Path" => Some(PrimitiveType::Path),
        "Regex" => Some(PrimitiveType::Regex),
        "Never" => Some(PrimitiveType::Never),
        _ => None,
    }
}

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
        // Parse identifier path (e.g., alignment::Horizontal) with optional generic arguments.
        //
        // Primitive type names (`String`, `Number`, `I32`, `I64`, `F32`, `F64`,
        // `Boolean`, `Path`, `Regex`, `Never`) are recognized here by string-matching a
        // single-segment identifier, and are mapped to `Type::Primitive`. This lets
        // struct/enum/trait/fn definitions with those names parse successfully so the
        // semantic pass can emit `PrimitiveRedefinition` for them.
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
                // Map single-segment primitive names to `Type::Primitive` (only when no
                // generic args are supplied — `Number<T>` etc. are not primitives).
                if args.is_none() && path.len() == 1 {
                    if let Some(first) = path.first() {
                        if let Some(prim) = primitive_from_name(first.name.as_str()) {
                            return Type::Primitive(prim);
                        }
                    }
                }

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

        let base_type = choice((ident_or_generic, array_or_dict, tuple, grouped_type));

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

        // Closure type: () -> T, T -> U, mut T -> U, or T, mut U -> V
        // Convention prefix on each param type position
        let closure_convention = choice((
            just(Token::Mut).to(ParamConvention::Mut),
            just(Token::Sink).to(ParamConvention::Sink),
        ))
        .or_not()
        .map(|c| c.unwrap_or(ParamConvention::Let));

        // No-param closure: () -> ReturnType
        let no_param_closure = just(Token::LParen)
            .ignore_then(just(Token::RParen))
            .ignore_then(just(Token::Arrow))
            .ignore_then(type_ref.clone())
            .map(|ret| Type::Closure {
                params: vec![],
                ret: Box::new(ret),
            });

        // Single or multi-param closure: [mut|sink]? Type -> ReturnType OR [mut|sink]? Type, ...
        let param_closure = closure_convention
            .clone()
            .then(optionable_type.clone())
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
        choice((no_param_closure, param_closure, optionable_type)).labelled("type")
    })
}
