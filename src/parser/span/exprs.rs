//! Span-filling for expressions and (binding) patterns.

use super::defs::fill_type_span;
use super::fill_span;
use crate::ast::{ArrayPatternElement, BindingPattern, BlockStatement, Expr, Pattern};

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive span-filling for all Expr variants"
)]
pub(super) fn fill_expr_span(expr: &mut Expr, source: &str) {
    match expr {
        Expr::Literal { span, .. } => fill_span(span, source),
        Expr::Invocation {
            path,
            type_args,
            args,
            span,
            ..
        } => {
            fill_invocation_expr_spans(path, type_args, args, span, source);
        }
        Expr::EnumInstantiation {
            enum_name,
            variant,
            data,
            span,
        } => {
            fill_span(&mut enum_name.span, source);
            fill_span(&mut variant.span, source);
            fill_named_expr_list_spans(data, span, source);
        }
        Expr::InferredEnumInstantiation {
            variant,
            data,
            span,
        } => {
            fill_span(&mut variant.span, source);
            fill_named_expr_list_spans(data, span, source);
        }
        Expr::Array { elements, span } => {
            for elem in elements {
                fill_expr_span(elem, source);
            }
            fill_span(span, source);
        }
        Expr::Tuple { fields, span } => {
            for (field_name, field_expr) in fields {
                fill_span(&mut field_name.span, source);
                fill_expr_span(field_expr, source);
            }
            fill_span(span, source);
        }
        Expr::Reference { path, span } => {
            for ident in path {
                fill_span(&mut ident.span, source);
            }
            fill_span(span, source);
        }
        Expr::BinaryOp {
            left, right, span, ..
        } => {
            fill_expr_span(left, source);
            fill_expr_span(right, source);
            fill_span(span, source);
        }
        Expr::UnaryOp { operand, span, .. } => {
            fill_expr_span(operand, source);
            fill_span(span, source);
        }
        Expr::ForExpr {
            var,
            collection,
            body,
            span,
        } => {
            fill_span(&mut var.span, source);
            fill_expr_span(collection, source);
            fill_expr_span(body, source);
            fill_span(span, source);
        }
        Expr::IfExpr {
            condition,
            then_branch,
            else_branch,
            span,
        } => {
            fill_expr_span(condition, source);
            fill_expr_span(then_branch, source);
            if let Some(else_br) = else_branch {
                fill_expr_span(else_br, source);
            }
            fill_span(span, source);
        }
        Expr::MatchExpr {
            scrutinee,
            arms,
            span,
        } => {
            fill_expr_span(scrutinee, source);
            for arm in arms {
                fill_pattern_span(&mut arm.pattern, source);
                fill_expr_span(&mut arm.body, source);
                fill_span(&mut arm.span, source);
            }
            fill_span(span, source);
        }
        Expr::Group { expr, span } => {
            fill_expr_span(expr, source);
            fill_span(span, source);
        }
        Expr::DictLiteral { entries, span } => {
            for (key, value) in entries {
                fill_expr_span(key, source);
                fill_expr_span(value, source);
            }
            fill_span(span, source);
        }
        Expr::DictAccess { dict, key, span } => {
            fill_expr_span(dict, source);
            fill_expr_span(key, source);
            fill_span(span, source);
        }
        Expr::FieldAccess {
            object,
            field,
            span,
        } => {
            fill_expr_span(object, source);
            fill_span(&mut field.span, source);
            fill_span(span, source);
        }
        Expr::ClosureExpr {
            params,
            return_type,
            body,
            span,
        } => {
            fill_closure_expr_spans(params, return_type.as_mut(), body, span, source);
        }
        Expr::LetExpr {
            pattern,
            ty,
            value,
            body,
            span,
            ..
        } => {
            fill_let_expr_spans(pattern, ty, value, body, span, source);
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
            span,
        } => {
            fill_expr_span(receiver, source);
            fill_span(&mut method.span, source);
            for (label, arg_expr) in args {
                if let Some(label_ident) = label {
                    fill_span(&mut label_ident.span, source);
                }
                fill_expr_span(arg_expr, source);
            }
            fill_span(span, source);
        }
        Expr::Block {
            statements,
            result,
            span,
        } => {
            fill_block_expr_spans(statements, result, span, source);
        }
    }
}

fn fill_named_expr_list_spans(
    data: &mut [(crate::ast::Ident, Expr)],
    span: &mut crate::location::Span,
    source: &str,
) {
    for (field_name, expr) in data {
        fill_span(&mut field_name.span, source);
        fill_expr_span(expr, source);
    }
    fill_span(span, source);
}

fn fill_invocation_expr_spans(
    path: &mut [crate::ast::Ident],
    type_args: &mut [crate::ast::Type],
    args: &mut [(Option<crate::ast::Ident>, Expr)],
    span: &mut crate::location::Span,
    source: &str,
) {
    for ident in path {
        fill_span(&mut ident.span, source);
    }
    for ty_arg in type_args {
        fill_type_span(ty_arg, source);
    }
    for (arg_name, arg_expr) in args {
        if let Some(name) = arg_name {
            fill_span(&mut name.span, source);
        }
        fill_expr_span(arg_expr, source);
    }
    fill_span(span, source);
}

fn fill_closure_expr_spans(
    params: &mut [crate::ast::ClosureParam],
    return_type: Option<&mut crate::ast::Type>,
    body: &mut Expr,
    span: &mut crate::location::Span,
    source: &str,
) {
    for param in params {
        fill_span(&mut param.name.span, source);
        if let Some(ty) = &mut param.ty {
            fill_type_span(ty, source);
        }
        fill_span(&mut param.span, source);
    }
    if let Some(ty) = return_type {
        fill_type_span(ty, source);
    }
    fill_expr_span(body, source);
    fill_span(span, source);
}

fn fill_let_expr_spans(
    pattern: &mut BindingPattern,
    ty: &mut Option<crate::ast::Type>,
    value: &mut Expr,
    body: &mut Expr,
    span: &mut crate::location::Span,
    source: &str,
) {
    fill_binding_pattern_span(pattern, source);
    if let Some(type_ann) = ty {
        fill_type_span(type_ann, source);
    }
    fill_expr_span(value, source);
    fill_expr_span(body, source);
    fill_span(span, source);
}

fn fill_block_expr_spans(
    statements: &mut [BlockStatement],
    result: &mut Expr,
    span: &mut crate::location::Span,
    source: &str,
) {
    for stmt in statements {
        match stmt {
            BlockStatement::Let {
                pattern,
                ty,
                value,
                span: stmt_span,
                ..
            } => {
                fill_binding_pattern_span(pattern, source);
                if let Some(type_ann) = ty {
                    fill_type_span(type_ann, source);
                }
                fill_expr_span(value, source);
                fill_span(stmt_span, source);
            }
            BlockStatement::Assign {
                target,
                value,
                span: stmt_span,
            } => {
                fill_expr_span(target, source);
                fill_expr_span(value, source);
                fill_span(stmt_span, source);
            }
            BlockStatement::Expr(expr) => {
                fill_expr_span(expr, source);
            }
        }
    }
    fill_expr_span(result, source);
    fill_span(span, source);
}

pub(super) fn fill_pattern_span(pattern: &mut Pattern, source: &str) {
    match pattern {
        Pattern::Variant { name, bindings } => {
            fill_span(&mut name.span, source);
            for binding in bindings {
                fill_span(&mut binding.span, source);
            }
        }
        Pattern::Wildcard => {}
    }
}

pub(super) fn fill_binding_pattern_span(pattern: &mut BindingPattern, source: &str) {
    match pattern {
        BindingPattern::Simple(ident) => {
            fill_span(&mut ident.span, source);
        }
        BindingPattern::Array { elements, span } => {
            for elem in elements {
                match elem {
                    ArrayPatternElement::Binding(p) => fill_binding_pattern_span(p, source),
                    ArrayPatternElement::Rest(Some(ident)) => fill_span(&mut ident.span, source),
                    ArrayPatternElement::Rest(None) | ArrayPatternElement::Wildcard => {}
                }
            }
            fill_span(span, source);
        }
        BindingPattern::Struct { fields, span } => {
            for field in fields {
                fill_span(&mut field.name.span, source);
                if let Some(alias) = &mut field.alias {
                    fill_span(&mut alias.span, source);
                }
            }
            fill_span(span, source);
        }
        BindingPattern::Tuple { elements, span } => {
            for elem in elements {
                fill_binding_pattern_span(elem, source);
            }
            fill_span(span, source);
        }
    }
}
