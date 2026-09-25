// Definition parsers: struct, trait, impl, enum, function, module definitions

mod bodies;
mod enums;
mod extern_decls;
mod funcs;

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{
    ArrayPatternElement, AttributeAnnotation, BindingPattern, Definition, FieldDef, FnSig,
    FunctionAttribute, GenericConstraint, GenericParam, Ident, StructDef, StructField,
    StructPatternField, TraitDef, Type,
};
use crate::lexer::Token;

use bodies::{impl_def_parser, module_def_parser};
use enums::enum_def_parser;
use extern_decls::{extern_fn_parser, extern_impl_parser};
use funcs::{fn_def_parser, fn_sig_parser, free_fn_params_parser, function_def_parser};

use super::exprs::expr_parser;
use super::ident_parser;
use super::newlines;
use super::span_from_simple;
use super::types::type_parser;
use super::visibility_parser;

/// Parse a binding pattern (for let bindings)
/// Supports: simple name, array destructuring, struct destructuring, tuple destructuring
pub(super) fn binding_pattern_parser<'tokens, I>(
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

        // An array pattern holds at most one rest pattern: with two, the
        // split of the elements between them is not defined.
        let array_pattern = array_element
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .validate(|elements: Vec<ArrayPatternElement>, e, emitter| {
                let rests = elements
                    .iter()
                    .filter(|element| matches!(element, ArrayPatternElement::Rest(_)))
                    .count();
                if rests > 1 {
                    emitter.emit(Rich::custom(
                        e.span(),
                        "an array pattern holds at most one rest pattern `...`",
                    ));
                }
                elements
            })
            .map_with(|elements, e| BindingPattern::Array {
                elements,
                span: span_from_simple(e.span()),
            });

        // Struct pattern: {name, age as userAge}
        let struct_field = ident_parser()
            .then(just(Token::As).ignore_then(ident_parser()).or_not())
            .map(|(name, alias)| StructPatternField { name, alias });

        let struct_pattern = struct_field
            .separated_by(just(Token::Comma).padded_by(newlines()))
            .allow_trailing()
            .collect::<Vec<_>>()
            .padded_by(newlines())
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
pub(super) fn generic_params_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<GenericParam>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // A single trait constraint: `Foo` or `Foo<X, Y>`. Args are
    // parsed via `type_parser()` so any legal type — including
    // nested generics — can appear (`<T: Container<Box<I32>>>`).
    let trait_constraint = ident_parser()
        .then(
            type_parser()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::Lt), just(Token::Gt))
                .or_not(),
        )
        .map(|(name, args)| GenericConstraint::Trait {
            name,
            args: args.unwrap_or_default(),
        });

    // Parse a single generic parameter:
    //   T
    //   T: Trait
    //   T: Trait<X>
    //   T: Trait1 + Trait2<Y>
    let generic_param = ident_parser()
        .then(
            just(Token::Colon)
                .ignore_then(
                    trait_constraint
                        .separated_by(just(Token::Plus))
                        .at_least(1)
                        .collect::<Vec<_>>(),
                )
                .or_not(),
        )
        .map_with(|(name, constraints), e| GenericParam {
            name,
            constraints: constraints.unwrap_or_default(),
            span: span_from_simple(e.span()),
        });

    // Parse comma-separated list: <T, U: Trait, V>
    generic_param
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect()
        .delimited_by(just(Token::Lt), just(Token::Gt))
        .or_not()
        .map(std::option::Option::unwrap_or_default)
}

/// Parse trait composition (: A + B + C or nothing)
pub(super) fn trait_composition_parser<'tokens, I>(
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
        .map(std::option::Option::unwrap_or_default)
}

/// Parse a field definition: name: Type
///
/// leading `///` doc comments are captured into `FieldDef.doc`.
///
/// Field-level mutability was removed: mutability is a property of the
/// binding (`let mut`), not of a field declaration. The `mutable` bool
/// on `FieldDef` is kept for serialisation stability and is always
/// `false` at parse time.
pub(super) fn field_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FieldDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    super::doc_comments_parser()
        .then(ident_parser())
        .then_ignore(just(Token::Colon).labelled("':'"))
        .then(type_parser().labelled("type"))
        .map_with(|((doc, name), ty), e| FieldDef {
            mutable: false,
            name,
            ty,
            doc,
            span: span_from_simple(e.span()),
        })
}

/// Parse zero or more codegen-hint attributes that may appear before
/// the `fn` keyword (`inline`, `no_inline`, `cold`). Order is preserved
/// so the AST faithfully mirrors source. Each entry carries the span of
/// the keyword token so diagnostics can point at it.
pub(super) fn fn_attributes_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<AttributeAnnotation>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    choice((
        just(Token::Inline).to(FunctionAttribute::Inline),
        just(Token::NoInline).to(FunctionAttribute::NoInline),
        just(Token::Cold).to(FunctionAttribute::Cold),
    ))
    .map_with(|kind, e| AttributeAnnotation {
        kind,
        span: span_from_simple(e.span()),
    })
    .repeated()
    .collect::<Vec<_>>()
}

/// Parse a function signature (no body): `fn name(params) -> Type`
///
/// optional leading `///` doc comments are consumed; the
/// docstring is ignored at the `FnSig` level (signatures live inside
/// traits where doc storage is on `FnDef` only).
/// Parse a trait definition
pub(super) fn trait_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, TraitDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    /// Local enum to distinguish methods from fields in a trait body.
    #[derive(Clone)]
    enum TraitItem {
        Method(FnSig),
        Field(FieldDef),
    }

    // Each item in a trait body is either a fn signature or a field def.
    // Items may be separated by commas (optional) or just newline-delimited.
    let trait_item = choice((
        fn_sig_parser().map(TraitItem::Method),
        field_def_parser().map(TraitItem::Field),
    ));

    visibility_parser()
        .then_ignore(just(Token::Trait))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(trait_composition_parser())
        .then(
            trait_item
                .separated_by(just(Token::Comma).or_not().ignored().padded_by(newlines()))
                .allow_leading()
                .allow_trailing()
                .collect::<Vec<_>>()
                .padded_by(newlines())
                .delimited_by(just(Token::LBrace), just(Token::RBrace))
                .or_not(),
        )
        .map_with(|((((visibility, name), generics), traits), items), e| {
            let items = items.unwrap_or_default();
            let mut fields = Vec::new();
            let mut methods = Vec::new();

            for item in items {
                match item {
                    TraitItem::Method(sig) => methods.push(sig),
                    TraitItem::Field(field) => fields.push(field),
                }
            }

            TraitDef {
                visibility,
                name,
                generics,
                traits,
                fields,
                methods,
                doc: None,
                span: span_from_simple(e.span()),
            }
        })
}

/// Parse a struct definition
pub(super) fn struct_def_parser<'tokens, I>(
) -> impl Parser<'tokens, I, StructDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then_ignore(just(Token::Struct))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(
            struct_field_parser()
                .separated_by(just(Token::Comma).padded_by(newlines()))
                .allow_trailing()
                .collect::<Vec<_>>()
                .padded_by(newlines())
                .delimited_by(just(Token::LBrace), just(Token::RBrace))
                .or_not(),
        )
        .map_with(|(((visibility, name), generics), fields), e| StructDef {
            visibility,
            name,
            generics,
            fields: fields.unwrap_or_default(),
            doc: None,
            span: span_from_simple(e.span()),
        })
}

/// Parse a single struct field: name: Type? = default
///
/// leading `///` doc comments are captured into
/// `StructField.doc` instead of being silently dropped.
///
/// Field-level mutability was removed: mutability is a property of the
/// binding (`let mut`), not the field declaration. The `mutable` bool
/// on `StructField` is kept for serialisation stability and is always
/// `false` at parse time. Writing `mut field: T` is a parse error.
pub(super) fn struct_field_parser<'tokens, I>(
) -> impl Parser<'tokens, I, StructField, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    super::doc_comments_parser()
        .then(ident_parser())
        .then_ignore(just(Token::Colon).labelled("':'"))
        .then(type_parser().labelled("type"))
        .then(
            just(Token::Equals)
                .ignore_then(expr_parser().labelled("default value"))
                .or_not(),
        )
        .map_with(|(((doc, name), ty), default), e| {
            // Check if type is optional
            let optional = matches!(ty, Type::Optional(_));

            StructField {
                mutable: false,
                name,
                ty,
                optional,
                default,
                doc,
                span: span_from_simple(e.span()),
            }
        })
}

/// Parse a definition (trait, struct, impl, enum, module, function, or extern)
pub(super) fn definition_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Definition, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|def| {
        choice((
            // Extern variants must come before their non-extern counterparts
            extern_impl_parser().map(Definition::Impl),
            extern_fn_parser().map(|f| Definition::Function(Box::new(f))),
            trait_def_parser().map(Definition::Trait),
            struct_def_parser().map(Definition::Struct),
            impl_def_parser().map(Definition::Impl),
            enum_def_parser().map(Definition::Enum),
            module_def_parser(def).map(Definition::Module),
            function_def_parser().map(|f| Definition::Function(Box::new(f))),
        ))
        .labelled("definition (struct, enum, trait, impl, fn, extern, or mod)")
    })
}
