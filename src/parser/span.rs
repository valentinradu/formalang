// Span-filling functions: post-process the AST to fill line/column info
// from source text after parsing (which only produces byte offsets).

use crate::ast::{
    ArrayPatternElement, BindingPattern, BlockStatement, Definition, Expr, File, GenericConstraint,
    Pattern, Statement, Type, UseItems,
};
use crate::location::Span as CustomSpan;

/// Fill in line/column information for all spans in the AST using source text
pub(super) fn fill_file_spans(file: &mut File, source: &str) {
    for stmt in &mut file.statements {
        fill_statement_span(stmt, source);
    }
}

/// Helper to fill a span's line/column info from source
pub(super) fn fill_span(span: &mut CustomSpan, source: &str) {
    if span.start.line == 0 && span.end.line == 0 {
        *span = CustomSpan::from_range_with_source(span.start.offset, span.end.offset, source);
    }
}

/// Fill spans in a statement
pub(super) fn fill_statement_span(stmt: &mut Statement, source: &str) {
    match stmt {
        Statement::Use(use_stmt) => {
            for ident in &mut use_stmt.path {
                fill_span(&mut ident.span, source);
            }
            match &mut use_stmt.items {
                UseItems::Single(ident) => fill_span(&mut ident.span, source),
                UseItems::Multiple(idents) => {
                    for ident in idents {
                        fill_span(&mut ident.span, source);
                    }
                }
                UseItems::Glob => {} // No spans to fill for glob
            }
            fill_span(&mut use_stmt.span, source);
        }
        Statement::Let(let_stmt) => {
            fill_binding_pattern_span(&mut let_stmt.pattern, source);
            fill_expr_span(&mut let_stmt.value, source);
            fill_span(&mut let_stmt.span, source);
        }
        Statement::Definition(def) => fill_definition_span(def.as_mut(), source),
    }
}

/// Fill spans in a definition
pub(super) fn fill_definition_span(def: &mut Definition, source: &str) {
    match def {
        Definition::Module(m) => {
            fill_span(&mut m.name.span, source);
            for def in &mut m.definitions {
                fill_definition_span(def, source);
            }
            fill_span(&mut m.span, source);
        }
        Definition::Trait(t) => fill_trait_def_spans(t, source),
        Definition::Struct(s) => fill_struct_def_spans(s, source),
        Definition::Impl(i) => fill_impl_def_spans(i, source),
        Definition::Enum(e) => fill_enum_def_spans(e, source),
        Definition::Function(f) => fill_function_def_spans(f, source),
    }
}

/// Fill generic parameter spans (shared by trait, struct, impl, enum).
fn fill_generic_params_spans(params: &mut [crate::ast::GenericParam], source: &str) {
    for param in params {
        fill_span(&mut param.name.span, source);
        for constraint in &mut param.constraints {
            match constraint {
                GenericConstraint::Trait { name, args } => {
                    fill_span(&mut name.span, source);
                    for arg in args {
                        fill_type_span(arg, source);
                    }
                }
            }
        }
        fill_span(&mut param.span, source);
    }
}

fn fill_trait_def_spans(t: &mut crate::ast::TraitDef, source: &str) {
    fill_span(&mut t.name.span, source);
    for base in &mut t.traits {
        fill_span(&mut base.span, source);
    }
    fill_generic_params_spans(&mut t.generics, source);
    for field in &mut t.fields {
        fill_span(&mut field.name.span, source);
        fill_type_span(&mut field.ty, source);
        fill_span(&mut field.span, source);
    }
    for m in &mut t.methods {
        fill_span(&mut m.name.span, source);
        fill_span(&mut m.span, source);
    }
    fill_span(&mut t.span, source);
}

fn fill_struct_def_spans(s: &mut crate::ast::StructDef, source: &str) {
    fill_span(&mut s.name.span, source);
    fill_generic_params_spans(&mut s.generics, source);
    for field in &mut s.fields {
        fill_span(&mut field.name.span, source);
        fill_type_span(&mut field.ty, source);
        if let Some(default) = &mut field.default {
            fill_expr_span(default, source);
        }
        fill_span(&mut field.span, source);
    }
    fill_span(&mut s.span, source);
}

fn fill_impl_def_spans(i: &mut crate::ast::ImplDef, source: &str) {
    fill_span(&mut i.name.span, source);
    if let Some(t) = &mut i.trait_name {
        fill_span(&mut t.span, source);
    }
    for arg in &mut i.trait_args {
        fill_type_span(arg, source);
    }
    fill_generic_params_spans(&mut i.generics, source);
    for func in &mut i.functions {
        fill_span(&mut func.name.span, source);
        for p in &mut func.params {
            if let Some(label) = &mut p.external_label {
                fill_span(&mut label.span, source);
            }
            fill_span(&mut p.name.span, source);
            if let Some(ty) = &mut p.ty {
                fill_type_span(ty, source);
            }
            fill_span(&mut p.span, source);
        }
        if let Some(ret) = &mut func.return_type {
            fill_type_span(ret, source);
        }
        if let Some(body) = &mut func.body {
            fill_expr_span(body, source);
        }
        fill_span(&mut func.span, source);
    }
    fill_span(&mut i.span, source);
}

fn fill_enum_def_spans(e: &mut crate::ast::EnumDef, source: &str) {
    fill_span(&mut e.name.span, source);
    fill_generic_params_spans(&mut e.generics, source);
    for variant in &mut e.variants {
        fill_span(&mut variant.name.span, source);
        for field in &mut variant.fields {
            fill_span(&mut field.name.span, source);
            fill_type_span(&mut field.ty, source);
            fill_span(&mut field.span, source);
        }
        fill_span(&mut variant.span, source);
    }
    fill_span(&mut e.span, source);
}

fn fill_function_def_spans(f: &mut crate::ast::FunctionDef, source: &str) {
    fill_span(&mut f.name.span, source);
    for p in &mut f.params {
        if let Some(label) = &mut p.external_label {
            fill_span(&mut label.span, source);
        }
        fill_span(&mut p.name.span, source);
        if let Some(ty) = &mut p.ty {
            fill_type_span(ty, source);
        }
        fill_span(&mut p.span, source);
    }
    if let Some(ret) = &mut f.return_type {
        fill_type_span(ret, source);
    }
    if let Some(body) = &mut f.body {
        fill_expr_span(body, source);
    }
    fill_span(&mut f.span, source);
}

/// Fill spans in a type
pub(super) fn fill_type_span(ty: &mut Type, source: &str) {
    match ty {
        Type::Primitive(_) => {}
        Type::Ident(ident) => fill_span(&mut ident.span, source),
        Type::Array(inner) | Type::Optional(inner) => fill_type_span(inner, source),
        Type::Tuple(fields) => {
            for field in fields {
                fill_span(&mut field.name.span, source);
                fill_type_span(&mut field.ty, source);
                fill_span(&mut field.span, source);
            }
        }
        Type::Generic { name, args, span } => {
            fill_span(&mut name.span, source);
            for arg in args {
                fill_type_span(arg, source);
            }
            fill_span(span, source);
        }
        Type::Dictionary { key, value } => {
            fill_type_span(key, source);
            fill_type_span(value, source);
        }
        Type::Closure { params, ret } => {
            for (_, param) in params {
                fill_type_span(param, source);
            }
            fill_type_span(ret, source);
        }
    }
}

/// Fill spans in an expression
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

/// Fill spans in a pattern
pub(super) fn fill_pattern_span(pattern: &mut Pattern, source: &str) {
    match pattern {
        Pattern::Variant { name, bindings } => {
            fill_span(&mut name.span, source);
            for binding in bindings {
                fill_span(&mut binding.span, source);
            }
        }
        Pattern::Wildcard => {
            // Wildcard has no spans to fill
        }
    }
}

/// Fill spans in a binding pattern (for let destructuring)
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
