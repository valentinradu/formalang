// Definition parsers: struct, trait, impl, enum, function, module definitions

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{
    ArrayPatternElement, BindingPattern, BlockStatement, Definition, EnumDef, EnumVariant,
    FieldDef, FnDef, FnParam, FnSig, FunctionDef, GenericConstraint, GenericParam, Ident, ImplDef,
    ModuleDef, ParamConvention, StructDef, StructField, StructPatternField, TraitDef, Type,
};
use crate::lexer::Token;

use super::block_statements_to_expr;
use super::exprs::expr_parser;
use super::ident_parser;
use super::mutability_parser;
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

/// Parse a function signature (no body): `fn name(params) -> Type`
pub(super) fn fn_sig_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FnSig, extra::Err<Rich<'tokens, Token>>> + Clone
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
        .map_with(|((name, params), return_type), e| FnSig {
            name,
            params,
            return_type,
            span: span_from_simple(e.span()),
        })
}

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
                .separated_by(just(Token::Comma).or_not().ignored())
                .allow_leading()
                .allow_trailing()
                .collect::<Vec<_>>()
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
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace))
                .or_not(),
        )
        .map_with(|(((visibility, name), generics), fields), e| StructDef {
            visibility,
            name,
            generics,
            fields: fields.unwrap_or_default(),
            span: span_from_simple(e.span()),
        })
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
            is_extern: false,
            span: span_from_simple(e.span()),
        })
}

/// Parse an extern function declaration: `extern fn name(params) -> Type`
pub(super) fn extern_fn_parser<'tokens, I>(
) -> impl Parser<'tokens, I, FunctionDef, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    visibility_parser()
        .then_ignore(just(Token::Extern))
        .then_ignore(just(Token::Fn))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(fn_params_parser())
        .then(
            // Optional return type: -> Type
            just(Token::Arrow).ignore_then(type_parser()).or_not(),
        )
        .map_with(
            |((((visibility, name), generics), params), return_type), e| {
                let span = span_from_simple(e.span());
                FunctionDef {
                    visibility,
                    name,
                    generics,
                    params,
                    return_type,
                    body: None,
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
    // Parse optional "Trait for" prefix
    let trait_for = ident_parser().then_ignore(just(Token::For)).or_not();

    // Each item in extern impl is either a fn def (with body, which is invalid but
    // needs to be parsed so semantic analysis can emit ExternImplWithBody) or a fn sig.
    let extern_impl_item = choice((
        fn_def_parser(),
        fn_sig_parser().map(|sig| FnDef {
            name: sig.name,
            params: sig.params,
            return_type: sig.return_type,
            body: None,
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
        .map_with(|(((trait_name, name), generics), functions), e| ImplDef {
            trait_name,
            name,
            generics,
            functions,
            is_extern: true,
            span: span_from_simple(e.span()),
        })
}

/// Parse the body of an impl block (functions only).
///
/// Accepts both full function definitions (with body) and bare signatures (without body).
/// Functions missing a body are kept as-is so semantic analysis can emit
/// `RegularFnWithoutBody`.
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
            params: sig.params,
            return_type: sig.return_type,
            body: None,
            span: sig.span,
        }),
    ));
    impl_item
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

    // Parse item (let, assign, or expr - in that order). Audit #40:
    // wrap in `recover_with(via_parser(...))` so a malformed item inside
    // a function body (e.g. a syntactically broken expression) is
    // recovered by consuming tokens up to the next item start (`let`)
    // or the closing brace, producing an empty placeholder expression.
    // Without this, a parse failure inside one function body aborts
    // diagnostics for the rest of the file: the error is emitted by
    // chumsky and parsing continues with the next item. The first
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
                body: Some(body),
                span,
            }
        })
}

/// Parse function parameters: `(self, mut self, x: Type, mut x: Type, sink x: Type, label name: Type)`
///
/// Parameters support an optional convention prefix (`mut` or `sink`) and an optional
/// external label: `fn foo(en name: String)` where `en` is the label used at call sites
/// and `name` is the internal parameter name.
pub(super) fn fn_params_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Vec<FnParam>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    // Optional convention prefix: `mut` | `sink` | (nothing → Let)
    let convention = choice((
        just(Token::Mut).to(ParamConvention::Mut),
        just(Token::Sink).to(ParamConvention::Sink),
    ))
    .or_not()
    .map(|c| c.unwrap_or(ParamConvention::Let));

    // `[mut|sink]? self`
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

    // `[mut|sink]? label name: Type = default` — two identifiers before the colon
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

    // `[mut|sink]? name: Type = default` — single identifier before the colon
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

    // `Type` only (Mode B overloading — no name, no label). Audit #23.
    // The parameter is synthesised with a fresh name (`_argN`) so existing
    // plumbing can continue to reference parameters by name; the name is
    // not visible at the call site since these are always positional.
    let type_only_param = convention
        .clone()
        .then(type_parser())
        .map_with(|(convention, ty), e| FnParam {
            convention,
            external_label: None,
            name: Ident::new("_arg", span_from_simple(e.span())),
            ty: Some(ty),
            default: None,
            span: span_from_simple(e.span()),
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
        .then_ignore(just(Token::Fn))
        .then(ident_parser())
        .then(generic_params_parser())
        .then(fn_params_parser())
        .then(
            // Optional return type: -> Type
            just(Token::Arrow).ignore_then(type_parser()).or_not(),
        )
        .then(
            // Function body in braces - parsed as a block with statements
            fn_body_parser(),
        )
        .map_with(
            |(((((visibility, name), generics), params), return_type), body), e| {
                let span = span_from_simple(e.span());
                FunctionDef {
                    visibility,
                    name,
                    generics,
                    params,
                    return_type,
                    body: Some(body),
                    span,
                }
            },
        )
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
