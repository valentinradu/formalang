// Expression parsers

mod literals;
mod operators;
mod patterns;

pub(super) use patterns::match_arm_parser;

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{BlockStatement, ClosureParam, Expr, Ident, Literal, ParamConvention};
use crate::lexer::Token;

use super::block_statements_to_expr;
use super::defs::binding_pattern_parser;
use super::ident_no_self_parser;
use super::ident_parser;
use super::invocation_target_parser;
use super::span_from_simple;
use super::types::type_parser;

/// Parse an expression
#[expect(
    clippy::too_many_lines,
    reason = "parser combinator composition — local parsers are captured by closures and cannot be extracted without restructuring"
)]
pub(super) fn expr_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Expr, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|expr| {
        let literal = literals::literal_parser();

        // Helper to parse invocation arguments: either named (name: expr) or positional (expr)
        // Returns Vec<(Option<Ident>, Expr)> where Some(name) is named, None is positional
        // Named args use lookahead to check for ident: pattern before committing
        let named_invoc_arg = ident_parser()
            .then(just(Token::Colon))
            .rewind() // Lookahead: check for ident: without consuming
            .ignore_then(
                ident_parser()
                    .then_ignore(just(Token::Colon))
                    .then(expr.clone()),
            )
            .map(|(name, value)| (Some(name), value));
        let positional_invoc_arg = expr.clone().map(|value| (None, value));
        let invocation_arg = named_invoc_arg.or(positional_invoc_arg);

        let invocation_args = invocation_arg
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        // Helper to parse named arguments for enums: name: expr, name: expr, ...
        // Requires at least one argument if parens are present (no empty parens allowed)
        // Uses lookahead: peek for ( ident : pattern before committing to parse
        let enum_named_args = just(Token::LParen)
            .ignore_then(ident_parser())
            .then(just(Token::Colon))
            .rewind() // Lookahead: if we see ( ident :, this is a named arg pattern
            .ignore_then(
                ident_parser()
                    .then_ignore(just(Token::Colon))
                    .then(expr.clone())
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            );

        // Inferred enum instantiation: .variant(field: value, field: value, ...)
        let inferred_enum_instantiation = just(Token::Dot)
            .ignore_then(ident_parser())
            .then(enum_named_args.clone().or_not())
            .map_with(|(variant, data), e| Expr::InferredEnumInstantiation {
                variant,
                data: data.unwrap_or_default(),
                span: span_from_simple(e.span()),
            });

        // Enum instantiation: EnumType.variant OR EnumType.variant(field: value, ...)
        // Supports module-qualified paths: module::EnumType.variant
        // Note: Uses ident_no_self_parser to prevent 'self.field' from being parsed as enum instantiation
        // IMPORTANT: If there are parens, they MUST contain named args (ident: value).
        // This prevents foo.bar(1) from being parsed as enum instantiation.
        // IMPORTANT: The type name (last path element) must start with uppercase to distinguish
        // from field access (e.g., `Status.active` vs `point.x`).
        let enum_base = ident_no_self_parser()
            .separated_by(just(Token::DoubleColon))
            .at_least(1)
            .collect::<Vec<_>>()
            .then_ignore(just(Token::Dot))
            .then(ident_parser())
            // Filter: only match if the type name (last path element) starts with uppercase
            // This distinguishes `Status.active` (enum) from `point.x` (field access)
            .try_map(|(path, variant), span| {
                let type_name = path.last().map_or("", |id| id.name.as_str());
                if type_name.chars().next().is_some_and(char::is_uppercase) {
                    Ok((path, variant))
                } else {
                    Err(Rich::custom(
                        span,
                        "enum type names must start with uppercase",
                    ))
                }
            });
        // With named args: Type.variant(name: value, ...)
        let enum_with_args = enum_base
            .clone()
            .then(enum_named_args.clone())
            .map(|((path, variant), data)| (path, variant, data));
        // Without args: Type.variant (no parens at all - checked by NOT seeing LParen)
        let enum_without_args = enum_base
            .clone()
            .then(just(Token::LParen).not().rewind())
            .map(|((path, variant), ())| (path, variant, vec![]));
        // Try with-args first, then without-args
        let enum_instantiation =
            enum_with_args
                .or(enum_without_args)
                .map_with(|(path, variant, data), e| {
                    // Join module path into a single identifier
                    let enum_name_str = path
                        .iter()
                        .map(|id: &Ident| id.name.as_str())
                        .collect::<Vec<_>>()
                        .join("::");
                    let enum_name = Ident::new(enum_name_str, span_from_simple(e.span()));

                    Expr::EnumInstantiation {
                        enum_name,
                        variant,
                        data,
                        span: span_from_simple(e.span()),
                    }
                });

        // Invocation: Name(arg: value, ...) or Name<Type>(arg: value, ...)
        // Can be struct instantiation or function call.
        // Supports module-qualified paths: module::Name(...)
        let invocation = invocation_target_parser()
            .separated_by(just(Token::DoubleColon))
            .at_least(1)
            .collect::<Vec<_>>()
            .then(
                // Optional generic arguments (only valid for struct instantiation)
                type_parser()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::Lt), just(Token::Gt))
                    .or_not(),
            )
            .then(invocation_args.clone())
            .map_with(|((path, type_args), args), e| {
                // Keep path as Vec<Ident> for semantic analysis to resolve
                Expr::Invocation {
                    path,
                    type_args: type_args.unwrap_or_default(),
                    args,
                    span: span_from_simple(e.span()),
                }
            });

        // Reference: single identifier (e.g., user, self, field)
        // Field access like foo.bar is handled by the postfix `.` operator
        // Colon-separated paths are no longer supported
        let reference = ident_parser().map_with(|ident, e| Expr::Reference {
            path: vec![ident],
            span: span_from_simple(e.span()),
        });

        // Dictionary entry: key_expr: value_expr
        let dict_entry = expr
            .clone()
            .then_ignore(just(Token::Colon))
            .then(expr.clone())
            .map(|(key, value)| (key, value));

        // Dictionary literal: ["key": value, "key2": value2] or [:] for empty
        let dict_literal = choice((
            // Empty dictionary: [:]
            just(Token::LBracket)
                .ignore_then(just(Token::Colon))
                .ignore_then(just(Token::RBracket))
                .map_with(|_, e| Expr::DictLiteral {
                    entries: vec![],
                    span: span_from_simple(e.span()),
                }),
            // Non-empty dictionary: [key: value, key2: value2]
            dict_entry
                .separated_by(just(Token::Comma))
                .at_least(1)
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBracket), just(Token::RBracket))
                .map_with(|entries, e| Expr::DictLiteral {
                    entries,
                    span: span_from_simple(e.span()),
                }),
        ));

        // Array literal: [expr, expr, ...] or [] for empty
        let array_literal = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map_with(|elements, e| Expr::Array {
                elements,
                span: span_from_simple(e.span()),
            });

        // Array or dictionary: try dictionary first (more specific)
        let array_or_dict = choice((dict_literal, array_literal));

        // Tuple literal: (name1: expr1, name2: expr2, ...)
        // Named tuple field: identifier : expression
        let tuple_field = ident_parser()
            .then_ignore(just(Token::Colon).labelled("':'"))
            .then(expr.clone().labelled("value"))
            .map(|(name, expr)| (name, expr));

        let tuple = tuple_field
            .separated_by(just(Token::Comma))
            .at_least(1)
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map_with(|fields, e| Expr::Tuple {
                fields,
                span: span_from_simple(e.span()),
            });

        // Grouped expression: (expr)
        // Note: This must come after tuple in the choice, since tuple is more specific
        let grouped = expr
            .clone()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map_with(|expr, e| Expr::Group {
                expr: Box::new(expr),
                span: span_from_simple(e.span()),
            });

        // Closure expression: () -> expr, x -> expr, x, y -> expr, x: T -> expr
        // Also supports pipe syntax: |x, y| expr, |x: T, y: T| -> T { body }
        // Closure parameter: [mut|sink]? identifier with optional type annotation
        let closure_convention = choice((
            just(Token::Mut).to(ParamConvention::Mut),
            just(Token::Sink).to(ParamConvention::Sink),
        ))
        .or_not()
        .map(|c| c.unwrap_or(ParamConvention::Let));

        let closure_param = closure_convention
            .then(ident_parser())
            .then(just(Token::Colon).ignore_then(type_parser()).or_not())
            .map_with(|((convention, name), ty), e| ClosureParam {
                convention,
                name,
                ty,
                span: span_from_simple(e.span()),
            });

        // No-param closure: () -> expr
        let no_param_closure = just(Token::LParen)
            .ignore_then(just(Token::RParen))
            .ignore_then(just(Token::Arrow))
            .ignore_then(expr.clone())
            .map_with(|body, e| Expr::ClosureExpr {
                params: vec![],
                return_type: None,
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // Single or multi-param closure: x -> expr OR x, y -> expr OR x: T -> expr
        let param_closure = closure_param
            .clone()
            .separated_by(just(Token::Comma))
            .at_least(1)
            .collect::<Vec<_>>()
            .then_ignore(just(Token::Arrow))
            .then(expr.clone())
            .map_with(|(params, body), e| Expr::ClosureExpr {
                params,
                return_type: None,
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // Pipe-delimited closure: |params| -> type { body } or |params| { body } or |params| expr
        // Also handles || { body } for empty params
        let pipe_closure = just(Token::Pipe)
            .ignore_then(
                closure_param
                    .clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then_ignore(just(Token::Pipe))
            .then(
                // Optional return type: -> Type
                just(Token::Arrow).ignore_then(type_parser()).or_not(),
            )
            .then(expr.clone())
            .map_with(|((params, return_type), body), e| Expr::ClosureExpr {
                params,
                return_type,
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // Block item parsers using BlockStatement directly
        // Let binding in block
        let block_let_item = just(Token::Let)
            .ignore_then(just(Token::Mut).or_not())
            .then(binding_pattern_parser())
            .then(just(Token::Colon).ignore_then(type_parser()).or_not())
            .then_ignore(just(Token::Equals))
            .then(expr.clone())
            .map_with(|(((mutable, pattern), ty), value), e| BlockStatement::Let {
                mutable: mutable.is_some(),
                pattern,
                ty,
                value,
                span: span_from_simple(e.span()),
            });

        // Assignment: target = value
        let block_assign_item = expr
            .clone()
            .then_ignore(just(Token::Equals))
            .then(expr.clone())
            .map_with(|(target, value), e| BlockStatement::Assign {
                target,
                value,
                span: span_from_simple(e.span()),
            });

        // Expression item
        let block_expr_item = expr.clone().map(BlockStatement::Expr);

        // recover_with(via_parser) so a malformed block item doesn't
        // abort parsing of later items. Mirrors `fn_body_parser` — the
        // first token is consumed except when it's `}` (which must reach
        // `delimited_by`).
        let block_recovery_head = any().and_is(just(Token::RBrace).not()).ignored();
        let block_recovery_tail = any()
            .and_is(just(Token::Let).not())
            .and_is(just(Token::RBrace).not())
            .ignored()
            .repeated();
        let block_recovery =
            block_recovery_head
                .then(block_recovery_tail)
                .map_with(|((), ()), e| {
                    BlockStatement::Expr(Expr::Group {
                        expr: Box::new(Expr::Literal {
                            value: Literal::Nil,
                            span: span_from_simple(e.span()),
                        }),
                        span: span_from_simple(e.span()),
                    })
                });
        let block_item = choice((
            block_let_item.clone(),
            block_assign_item.clone(),
            block_expr_item.clone(),
        ))
        .recover_with(via_parser(block_recovery));

        // Block body parser: { items... } -> Expr (Block or single expr)
        // Uses shared block_statements_to_expr helper
        // Reused in for_expr, if_expr
        let block_body = block_item
            .clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with(|stmts, e| block_statements_to_expr(stmts, span_from_simple(e.span())));

        // For expression: for var in collection { body }
        let for_expr = just(Token::For)
            .ignore_then(ident_parser())
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .then(block_body.clone())
            .map_with(|((var, collection), body), e| Expr::ForExpr {
                var,
                collection: Box::new(collection),
                body: Box::new(body),
                span: span_from_simple(e.span()),
            });

        // If expression: if condition { then } else { else }
        // Also handles else-if chains: if cond { } else if cond { } else { }
        let if_expr = recursive(|if_expr_rec| {
            just(Token::If)
                .ignore_then(expr.clone())
                .then(block_body.clone())
                .then(
                    just(Token::Else)
                        .ignore_then(
                            // Either another if expression (else-if chain) or a block { ... }
                            if_expr_rec.clone().or(block_body.clone()),
                        )
                        .or_not(),
                )
                .map_with(|((condition, then_branch), else_branch), e| Expr::IfExpr {
                    condition: Box::new(condition),
                    then_branch: Box::new(then_branch),
                    else_branch: else_branch.map(Box::new),
                    span: span_from_simple(e.span()),
                })
        });

        // Match expression: match scrutinee { pattern: expr, ... }
        let match_expr = just(Token::Match)
            .ignore_then(expr.clone())
            .then(
                match_arm_parser(expr.clone())
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .allow_trailing()
                    .collect()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|(scrutinee, arms), e| Expr::MatchExpr {
                scrutinee: Box::new(scrutinee),
                arms,
                span: span_from_simple(e.span()),
            });

        // Let expression: `let pattern = value in body` (with optional
        // `mut` and `: Type`). The explicit `in` separator removes the
        // value/body ambiguity that greedy parsing used to resolve.
        let let_expr = just(Token::Let)
            .ignore_then(just(Token::Mut).or_not())
            .then(binding_pattern_parser())
            .then(just(Token::Colon).ignore_then(type_parser()).or_not())
            .then_ignore(just(Token::Equals))
            .then(expr.clone())
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .map_with(
                |((((mutable, pattern), ty), value), body), e| Expr::LetExpr {
                    mutable: mutable.is_some(),
                    pattern,
                    ty,
                    value: Box::new(value),
                    body: Box::new(body),
                    span: span_from_simple(e.span()),
                },
            );

        // Atom: literal, instantiation, enum_instantiation, reference, array/dict, tuple, grouped, for, if, match, closure, let, block
        // Order matters: try more specific parsers first
        let atom = choice((
            literal,
            for_expr,
            if_expr,
            match_expr,
            let_expr,      // Let expressions
            block_body,    // Block expressions: { let x = 1; expr }
            array_or_dict, // Handles both array and dictionary literals
            tuple,         // Must come before grouped (tuple is more specific)
            grouped,
            pipe_closure.labelled("closure expression"), // |x| expr or |x, y| -> T { body }
            no_param_closure.labelled("closure expression"), // () -> expr (must come before other closures and tuples)
            param_closure.labelled("closure expression"), // x -> expr (must come before reference since starts with ident)
            inferred_enum_instantiation,                  // .variant is most specific
            enum_instantiation, // Must come before invocation and reference (Type.variant(...))
            invocation, // Unified struct instantiation / function call - resolved in semantic analysis
            reference,  // Most general (ident), now includes 'self'
        ))
        .labelled("expression");


        operators::apply_operators(atom, expr.clone(), invocation_args.clone())
    })
}
