// Type expression parsers

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{Ident, ParamConvention, PrimitiveType, TupleField, Type};
use crate::lexer::Token;

use super::ident_parser;
use super::span_from_simple;

/// Map a single-segment type identifier to its primitive type, if any.
///
/// Returns `Some(primitive)` for the primitive type names (`String`, `I32`,
/// `I64`, `F32`, `F64`, `Boolean`, `Never`) and `None` for any other
/// identifier.
fn primitive_from_name(name: &str) -> Option<PrimitiveType> {
    match name {
        "String" => Some(PrimitiveType::String),
        "I32" => Some(PrimitiveType::I32),
        "I64" => Some(PrimitiveType::I64),
        "F32" => Some(PrimitiveType::F32),
        "F64" => Some(PrimitiveType::F64),
        "Boolean" => Some(PrimitiveType::Boolean),
        "Never" => Some(PrimitiveType::Never),
        _ => None,
    }
}

/// One item between the `(` and the `)` of a type.
enum ParenItem {
    /// `name: Type`, a field of a named tuple.
    Named(TupleField),
    /// `Type`, `mut Type` or `sink Type`: a closure parameter, or the
    /// single type of a grouped type.
    Positional(ParamConvention, Type),
}

/// Select the type that a `( items )` group makes.
///
/// With a return type, the group is a closure. A `?` after a closure
/// binds to its return type, because the return type parser takes it.
/// Without a return type, the group is a named tuple when every item
/// has a name, or a grouped type when it holds one plain type.
fn paren_type_from_parts(
    items: Vec<ParenItem>,
    trailing_comma: bool,
    ret: Option<Type>,
) -> Result<Type, &'static str> {
    if let Some(ret) = ret {
        let params = items
            .into_iter()
            .map(|item| match item {
                ParenItem::Positional(convention, ty) => Ok((convention, ty)),
                ParenItem::Named(_) => Err("a closure type takes no parameter names"),
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Type::Closure {
            params,
            ret: Box::new(ret),
        });
    }
    if !items.is_empty() && items.iter().all(|i| matches!(i, ParenItem::Named(_))) {
        let fields = items
            .into_iter()
            .filter_map(|item| match item {
                ParenItem::Named(field) => Some(field),
                ParenItem::Positional(..) => None,
            })
            .collect();
        return Ok(Type::Tuple(fields));
    }
    let mut items = items.into_iter();
    match (items.next(), items.next(), trailing_comma) {
        (Some(ParenItem::Positional(ParamConvention::Let, ty)), None, false) => Ok(ty),
        (Some(ParenItem::Named(_)), ..) | (_, Some(ParenItem::Named(_)), _) => {
            Err("a tuple type gives a name to each field")
        }
        _ => Err("expected '->' after a closure parameter list"),
    }
}

/// Parse a type expression
pub(super) fn type_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Type, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|type_ref| {
        // Parse identifier path (e.g., alignment::Horizontal) with optional generic arguments.
        //
        // Primitive type names (`String`, `I32`, `I64`, `F32`, `F64`, `Boolean`,
        // `Never`) are recognized here by string-matching a single-segment
        // identifier, and are mapped to `Type::Primitive`. This lets
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
                // generic args are supplied — `I32<T>` etc. are not primitives).
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

        // One parser for every form that starts with `(`: the closure,
        // the named tuple and the grouped type. It parses each item one
        // time, and selects the form after the `)` and the `->`. Three
        // alternatives each parsed the full group before, so each
        // nested `(` doubled the time.
        let closure_convention = choice((
            just(Token::Mut).to(ParamConvention::Mut),
            just(Token::Sink).to(ParamConvention::Sink),
        ))
        .or_not()
        .map(|c| c.unwrap_or(ParamConvention::Let));

        let named_item = ident_parser()
            .then_ignore(just(Token::Colon).labelled("':'"))
            .then(type_ref.clone().labelled("type"))
            .map_with(|(name, ty), e| {
                ParenItem::Named(TupleField {
                    name,
                    ty,
                    span: span_from_simple(e.span()),
                })
            });
        let positional_item = closure_convention
            .then(type_ref.clone())
            .map(|(convention, ty)| ParenItem::Positional(convention, ty));

        let paren_type = choice((named_item, positional_item))
            .separated_by(just(Token::Comma))
            .collect::<Vec<_>>()
            .then(just(Token::Comma).or_not().map(|c| c.is_some()))
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .then(just(Token::Arrow).ignore_then(type_ref).or_not())
            .then(just(Token::Question).or_not().map(|q| q.is_some()))
            .try_map(|(((items, trailing_comma), ret), optional), span| {
                let ty = paren_type_from_parts(items, trailing_comma, ret)
                    .map_err(|message| Rich::custom(span, message))?;
                Ok(if optional {
                    Type::Optional(Box::new(ty))
                } else {
                    ty
                })
            });

        let base_type = choice((ident_or_generic, array_or_dict));

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

        choice((paren_type, optionable_type)).labelled("type")
    })
}
