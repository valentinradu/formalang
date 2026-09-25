//! Span-filling for expressions and (binding) patterns.

use super::defs::fill_type_span;
use super::fill_span;
use crate::ast::{ArrayPatternElement, BindingPattern, BlockStatement, Expr, Pattern};
use crate::location::LineIndex;

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive span-filling for all Expr variants"
)]
pub(super) fn fill_expr_span(expr: &mut Expr, index: &LineIndex<'_>) {
    match expr {
        Expr::Literal { span, .. } => fill_span(span, index),
        Expr::Invocation {
            path,
            type_args,
            args,
            span,
            ..
        } => {
            fill_invocation_expr_spans(path, type_args, args, span, index);
        }
        Expr::EnumInstantiation {
            enum_name,
            type_args,
            variant,
            data,
            span,
        } => {
            fill_span(&mut enum_name.span, index);
            for ty_arg in type_args {
                fill_type_span(ty_arg, index);
            }
            fill_span(&mut variant.span, index);
            fill_named_expr_list_spans(data, span, index);
        }
        Expr::InferredEnumInstantiation {
            variant,
            data,
            span,
        } => {
            fill_span(&mut variant.span, index);
            fill_named_expr_list_spans(data, span, index);
        }
        Expr::Array { elements, span } => {
            for elem in elements {
                fill_expr_span(elem, index);
            }
            fill_span(span, index);
        }
        Expr::Tuple { fields, span } => {
            for (field_name, field_expr) in fields {
                fill_span(&mut field_name.span, index);
                fill_expr_span(field_expr, index);
            }
            fill_span(span, index);
        }
        Expr::Reference { path, span } => {
            for ident in path {
                fill_span(&mut ident.span, index);
            }
            fill_span(span, index);
        }
        Expr::BinaryOp {
            left, right, span, ..
        } => {
            fill_expr_span(left, index);
            fill_expr_span(right, index);
            fill_span(span, index);
        }
        Expr::UnaryOp { operand, span, .. } => {
            fill_expr_span(operand, index);
            fill_span(span, index);
        }
        Expr::ForExpr {
            var,
            collection,
            body,
            span,
        } => {
            fill_span(&mut var.span, index);
            fill_expr_span(collection, index);
            fill_expr_span(body, index);
            fill_span(span, index);
        }
        Expr::IfExpr {
            condition,
            then_branch,
            else_branch,
            span,
        } => {
            fill_expr_span(condition, index);
            fill_expr_span(then_branch, index);
            if let Some(else_br) = else_branch {
                fill_expr_span(else_br, index);
            }
            fill_span(span, index);
        }
        Expr::MatchExpr {
            scrutinee,
            arms,
            span,
        } => {
            fill_expr_span(scrutinee, index);
            for arm in arms {
                fill_pattern_span(&mut arm.pattern, index);
                fill_expr_span(&mut arm.body, index);
                fill_span(&mut arm.span, index);
            }
            fill_span(span, index);
        }
        Expr::Group { expr, span } => {
            fill_expr_span(expr, index);
            fill_span(span, index);
        }
        Expr::DictLiteral { entries, span } => {
            for (key, value) in entries {
                fill_expr_span(key, index);
                fill_expr_span(value, index);
            }
            fill_span(span, index);
        }
        Expr::DictAccess { dict, key, span } => {
            fill_expr_span(dict, index);
            fill_expr_span(key, index);
            fill_span(span, index);
        }
        Expr::FieldAccess {
            object,
            field,
            span,
        } => {
            fill_expr_span(object, index);
            fill_span(&mut field.span, index);
            fill_span(span, index);
        }
        Expr::ClosureExpr {
            params,
            return_type,
            body,
            span,
        } => {
            fill_closure_expr_spans(params, return_type.as_mut(), body, span, index);
        }
        Expr::LetExpr {
            pattern,
            ty,
            value,
            body,
            span,
            ..
        } => {
            fill_let_expr_spans(pattern, ty, value, body, span, index);
        }
        Expr::Call { callee, args, span } => {
            fill_expr_span(callee, index);
            for (label, arg_expr) in args {
                if let Some(label_ident) = label {
                    fill_span(&mut label_ident.span, index);
                }
                fill_expr_span(arg_expr, index);
            }
            fill_span(span, index);
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
            span,
        } => {
            fill_expr_span(receiver, index);
            fill_span(&mut method.span, index);
            for (label, arg_expr) in args {
                if let Some(label_ident) = label {
                    fill_span(&mut label_ident.span, index);
                }
                fill_expr_span(arg_expr, index);
            }
            fill_span(span, index);
        }
        Expr::Block {
            statements,
            result,
            span,
        } => {
            fill_block_expr_spans(statements, result, span, index);
        }
    }
}

fn fill_named_expr_list_spans(
    data: &mut [(crate::ast::Ident, Expr)],
    span: &mut crate::location::Span,
    index: &LineIndex<'_>,
) {
    for (field_name, expr) in data {
        fill_span(&mut field_name.span, index);
        fill_expr_span(expr, index);
    }
    fill_span(span, index);
}

fn fill_invocation_expr_spans(
    path: &mut [crate::ast::Ident],
    type_args: &mut [crate::ast::Type],
    args: &mut [(Option<crate::ast::Ident>, Expr)],
    span: &mut crate::location::Span,
    index: &LineIndex<'_>,
) {
    for ident in path {
        fill_span(&mut ident.span, index);
    }
    for ty_arg in type_args {
        fill_type_span(ty_arg, index);
    }
    for (arg_name, arg_expr) in args {
        if let Some(name) = arg_name {
            fill_span(&mut name.span, index);
        }
        fill_expr_span(arg_expr, index);
    }
    fill_span(span, index);
}

fn fill_closure_expr_spans(
    params: &mut [crate::ast::ClosureParam],
    return_type: Option<&mut crate::ast::Type>,
    body: &mut Expr,
    span: &mut crate::location::Span,
    index: &LineIndex<'_>,
) {
    for param in params {
        fill_span(&mut param.name.span, index);
        if let Some(ty) = &mut param.ty {
            fill_type_span(ty, index);
        }
        fill_span(&mut param.span, index);
    }
    if let Some(ty) = return_type {
        fill_type_span(ty, index);
    }
    fill_expr_span(body, index);
    fill_span(span, index);
}

fn fill_let_expr_spans(
    pattern: &mut BindingPattern,
    ty: &mut Option<crate::ast::Type>,
    value: &mut Expr,
    body: &mut Expr,
    span: &mut crate::location::Span,
    index: &LineIndex<'_>,
) {
    fill_binding_pattern_span(pattern, index);
    if let Some(type_ann) = ty {
        fill_type_span(type_ann, index);
    }
    fill_expr_span(value, index);
    fill_expr_span(body, index);
    fill_span(span, index);
}

fn fill_block_expr_spans(
    statements: &mut [BlockStatement],
    result: &mut Expr,
    span: &mut crate::location::Span,
    index: &LineIndex<'_>,
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
                fill_binding_pattern_span(pattern, index);
                if let Some(type_ann) = ty {
                    fill_type_span(type_ann, index);
                }
                fill_expr_span(value, index);
                fill_span(stmt_span, index);
            }
            BlockStatement::Assign {
                target,
                value,
                span: stmt_span,
            } => {
                fill_expr_span(target, index);
                fill_expr_span(value, index);
                fill_span(stmt_span, index);
            }
            BlockStatement::Expr(expr) => {
                fill_expr_span(expr, index);
            }
        }
    }
    fill_expr_span(result, index);
    fill_span(span, index);
}

pub(super) fn fill_pattern_span(pattern: &mut Pattern, index: &LineIndex<'_>) {
    match pattern {
        Pattern::Variant { name, bindings } => {
            fill_span(&mut name.span, index);
            for binding in bindings {
                fill_span(&mut binding.span, index);
            }
        }
        Pattern::Wildcard => {}
    }
}

pub(super) fn fill_binding_pattern_span(pattern: &mut BindingPattern, index: &LineIndex<'_>) {
    match pattern {
        BindingPattern::Simple(ident) => {
            fill_span(&mut ident.span, index);
        }
        BindingPattern::Array { elements, span } => {
            for elem in elements {
                match elem {
                    ArrayPatternElement::Binding(p) => fill_binding_pattern_span(p, index),
                    ArrayPatternElement::Rest(Some(ident)) => fill_span(&mut ident.span, index),
                    ArrayPatternElement::Rest(None) | ArrayPatternElement::Wildcard => {}
                }
            }
            fill_span(span, index);
        }
        BindingPattern::Struct { fields, span } => {
            for field in fields {
                fill_span(&mut field.name.span, index);
                if let Some(alias) = &mut field.alias {
                    fill_span(&mut alias.span, index);
                }
            }
            fill_span(span, index);
        }
        BindingPattern::Tuple { elements, span } => {
            for elem in elements {
                fill_binding_pattern_span(elem, index);
            }
            fill_span(span, index);
        }
    }
}
