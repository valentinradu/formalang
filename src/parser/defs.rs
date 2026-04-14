// Definition parsers: struct, trait, impl, enum, function, module definitions

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{
    ArrayPatternElement, BindingPattern, BlockStatement, Definition, EnumDef, EnumVariant,
    FieldDef, FnDef, FnParam, FunctionDef, GenericConstraint, GenericParam, Ident, ImplDef,
    ModuleDef, StructDef, StructField, StructPatternField, TraitDef, Type,
};
use crate::lexer::Token;

use super::span_from_simple;
use super::exprs::expr_parser;
use super::ident_parser;
use super::mutability_parser;
use super::types::type_parser;
use super::visibility_parser;
use super::block_statements_to_expr;

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
pub(super) fn generic_params_parser<'tokens, I>(
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

/// Parse a field definition: mut? name: Type
pub(super) fn field_def_parser<'tokens, I>(
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

/// Parse a trait definition
pub(super) fn trait_def_parser<'tokens, I>(
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

/// Parse a struct definition (unified - replaces model and view)
pub(super) fn struct_def_parser<'tokens, I>(
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
pub(super) fn struct_field_parser<'tokens, I>(
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
pub(super) fn impl_def_parser<'tokens, I>(
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
pub(super) fn impl_body_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<FnDef>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    fn_def_parser()
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
}

/// Parse a function body: `{ statements... result_expr }` or `{ }` for empty
///
/// The function body is parsed as a block with multiple statements followed by a result.
pub(super) fn fn_body_parser<'tokens, I>(
) -> impl Parser<'tokens, I, crate::ast::Expr, extra::Err<Rich<'tokens, Token>>> + Clone
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
pub(super) fn fn_def_parser<'tokens, I>(
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
pub(super) fn fn_params_parser<'tokens, I>(
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
pub(super) fn function_def_parser<'tokens, I>(
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
            span: span_from_simple(e.span()),
        })
}

/// Parse enum variants: { variant, variant(Type), ... }
pub(super) fn enum_variants_parser<'tokens, I>(
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
pub(super) fn enum_variant_parser<'tokens, I>(
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

/// Parse a definition (trait, struct, impl, enum, module, or function)
pub(super) fn definition_parser<'tokens, I>(
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
