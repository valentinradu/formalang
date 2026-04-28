//! Module-level type walkers shared by every phase of the monomorphisation
//! pass.
//!
//! Read-only and mutable walks over `ResolvedType` slots reachable from an
//! `IrModule`. Expression-only walkers (`walk_expr`, `iter_expr_children_mut`)
//! live in [`super::expr_walk`].

use crate::ir::{
    IrBlockStatement, IrEnum, IrExpr, IrField, IrFunction, IrImpl, IrModule, IrStruct, IrTrait,
    ResolvedType,
};

/// Read-only walk over every `ResolvedType` reachable from the module.
pub(super) fn walk_module_types(module: &IrModule, visit: &mut impl FnMut(&ResolvedType)) {
    for s in &module.structs {
        walk_struct_types(s, visit);
    }
    for t in &module.traits {
        walk_trait_types(t, visit);
    }
    for e in &module.enums {
        walk_enum_types(e, visit);
    }
    for imp in &module.impls {
        walk_impl_types(imp, visit);
    }
    for f in &module.functions {
        walk_function_types(f, visit);
    }
    for l in &module.lets {
        visit(&l.ty);
        walk_expr_types(&l.value, visit);
    }
}

fn walk_struct_types(s: &IrStruct, visit: &mut impl FnMut(&ResolvedType)) {
    for f in &s.fields {
        walk_field_types(f, visit);
    }
}

fn walk_trait_types(t: &IrTrait, visit: &mut impl FnMut(&ResolvedType)) {
    for f in &t.fields {
        walk_field_types(f, visit);
    }
    for sig in &t.methods {
        for p in &sig.params {
            if let Some(ty) = &p.ty {
                visit(ty);
            }
            if let Some(d) = &p.default {
                walk_expr_types(d, visit);
            }
        }
        if let Some(ty) = &sig.return_type {
            visit(ty);
        }
    }
}

fn walk_enum_types(e: &IrEnum, visit: &mut impl FnMut(&ResolvedType)) {
    for v in &e.variants {
        for f in &v.fields {
            walk_field_types(f, visit);
        }
    }
}

fn walk_impl_types(imp: &IrImpl, visit: &mut impl FnMut(&ResolvedType)) {
    for f in &imp.functions {
        walk_function_types(f, visit);
    }
}

fn walk_function_types(f: &IrFunction, visit: &mut impl FnMut(&ResolvedType)) {
    for p in &f.params {
        if let Some(ty) = &p.ty {
            visit(ty);
        }
        if let Some(d) = &p.default {
            walk_expr_types(d, visit);
        }
    }
    if let Some(ty) = &f.return_type {
        visit(ty);
    }
    if let Some(body) = &f.body {
        walk_expr_types(body, visit);
    }
}

fn walk_field_types(f: &IrField, visit: &mut impl FnMut(&ResolvedType)) {
    visit(&f.ty);
    if let Some(d) = &f.default {
        walk_expr_types(d, visit);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive walk over every IrExpr variant; splitting hides the structural recursion"
)]
fn walk_expr_types(expr: &IrExpr, visit: &mut impl FnMut(&ResolvedType)) {
    visit(expr.ty());
    match expr {
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                walk_expr_types(e, visit);
            }
        }
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            for (_, _, e) in fields {
                walk_expr_types(e, visit);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                walk_expr_types(e, visit);
            }
        }
        IrExpr::FieldAccess { object, .. } => walk_expr_types(object, visit),
        IrExpr::BinaryOp { left, right, .. } => {
            walk_expr_types(left, visit);
            walk_expr_types(right, visit);
        }
        IrExpr::UnaryOp { operand, .. } => walk_expr_types(operand, visit),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            walk_expr_types(condition, visit);
            walk_expr_types(then_branch, visit);
            if let Some(e) = else_branch {
                walk_expr_types(e, visit);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            walk_expr_types(collection, visit);
            walk_expr_types(body, visit);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr_types(scrutinee, visit);
            for arm in arms {
                walk_expr_types(&arm.body, visit);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, a) in args {
                walk_expr_types(a, visit);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            walk_expr_types(receiver, visit);
            for (_, a) in args {
                walk_expr_types(a, visit);
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr_types(k, visit);
                walk_expr_types(v, visit);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            walk_expr_types(dict, visit);
            walk_expr_types(key, visit);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                walk_block_stmt_types(stmt, visit);
            }
            walk_expr_types(result, visit);
        }
        IrExpr::Closure {
            params,
            captures,
            body,
            ..
        } => {
            for (_, _, ty) in params {
                visit(ty);
            }
            for (_, _, ty) in captures {
                visit(ty);
            }
            walk_expr_types(body, visit);
        }
        IrExpr::ClosureRef { env_struct, ty, .. } => {
            visit(ty);
            walk_expr_types(env_struct, visit);
        }
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
    }
}

fn walk_block_stmt_types(stmt: &IrBlockStatement, visit: &mut impl FnMut(&ResolvedType)) {
    match stmt {
        IrBlockStatement::Let { value, .. } => walk_expr_types(value, visit),
        IrBlockStatement::Assign { target, value } => {
            walk_expr_types(target, visit);
            walk_expr_types(value, visit);
        }
        IrBlockStatement::Expr(e) => walk_expr_types(e, visit),
    }
}

/// Mutable walk over every `ResolvedType` reachable from the module. The
/// closure is called once per type slot; mutations there are reflected
/// back into the IR.
pub(super) fn walk_module_types_mut(
    module: &mut IrModule,
    mut visit: impl FnMut(&mut ResolvedType),
) {
    for s in &mut module.structs {
        for f in &mut s.fields {
            visit(&mut f.ty);
            if let Some(d) = &mut f.default {
                walk_expr_types_mut(d, &mut visit);
            }
        }
    }
    for t in &mut module.traits {
        for f in &mut t.fields {
            visit(&mut f.ty);
            if let Some(d) = &mut f.default {
                walk_expr_types_mut(d, &mut visit);
            }
        }
        for sig in &mut t.methods {
            for p in &mut sig.params {
                if let Some(ty) = &mut p.ty {
                    visit(ty);
                }
                if let Some(d) = &mut p.default {
                    walk_expr_types_mut(d, &mut visit);
                }
            }
            if let Some(ty) = &mut sig.return_type {
                visit(ty);
            }
        }
    }
    for e in &mut module.enums {
        for v in &mut e.variants {
            for f in &mut v.fields {
                visit(&mut f.ty);
                if let Some(d) = &mut f.default {
                    walk_expr_types_mut(d, &mut visit);
                }
            }
        }
    }
    for imp in &mut module.impls {
        for f in &mut imp.functions {
            walk_function_types_mut(f, &mut visit);
        }
    }
    for f in &mut module.functions {
        walk_function_types_mut(f, &mut visit);
    }
    for l in &mut module.lets {
        visit(&mut l.ty);
        walk_expr_types_mut(&mut l.value, &mut visit);
    }
}

pub(super) fn walk_function_types_mut(
    f: &mut IrFunction,
    visit: &mut impl FnMut(&mut ResolvedType),
) {
    for p in &mut f.params {
        if let Some(ty) = &mut p.ty {
            visit(ty);
        }
        if let Some(d) = &mut p.default {
            walk_expr_types_mut(d, visit);
        }
    }
    if let Some(ty) = &mut f.return_type {
        visit(ty);
    }
    if let Some(body) = &mut f.body {
        walk_expr_types_mut(body, visit);
    }
}

pub(super) fn walk_expr_types_mut(expr: &mut IrExpr, visit: &mut impl FnMut(&mut ResolvedType)) {
    walk_expr_types_mut_inner(expr, visit);
}

#[expect(
    clippy::too_many_lines,
    clippy::match_same_arms,
    reason = "exhaustive match over every IrExpr variant; merging similar arms would hide which variants have which children"
)]
fn walk_expr_types_mut_inner(expr: &mut IrExpr, visit: &mut impl FnMut(&mut ResolvedType)) {
    // Visit this node's type first, then descend into children.
    match expr {
        IrExpr::Literal { ty, .. } => visit(ty),
        IrExpr::StructInst { fields, ty, .. } => {
            visit(ty);
            for (_, _, e) in fields {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::EnumInst { fields, ty, .. } => {
            visit(ty);
            for (_, _, e) in fields {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::Tuple { fields, ty, .. } => {
            visit(ty);
            for (_, e) in fields {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::Array { elements, ty, .. } => {
            visit(ty);
            for e in elements {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::Reference { ty, .. }
        | IrExpr::SelfFieldRef { ty, .. }
        | IrExpr::LetRef { ty, .. } => visit(ty),
        IrExpr::FieldAccess { object, ty, .. } => {
            visit(ty);
            walk_expr_types_mut_inner(object, visit);
        }
        IrExpr::BinaryOp {
            left, right, ty, ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(left, visit);
            walk_expr_types_mut_inner(right, visit);
        }
        IrExpr::UnaryOp { operand, ty, .. } => {
            visit(ty);
            walk_expr_types_mut_inner(operand, visit);
        }
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ty,
            ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(condition, visit);
            walk_expr_types_mut_inner(then_branch, visit);
            if let Some(e) = else_branch {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::For {
            collection,
            body,
            ty,
            ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(collection, visit);
            walk_expr_types_mut_inner(body, visit);
        }
        IrExpr::Match {
            scrutinee,
            arms,
            ty,
            ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(scrutinee, visit);
            for arm in arms {
                walk_expr_types_mut_inner(&mut arm.body, visit);
            }
        }
        IrExpr::FunctionCall { args, ty, .. } => {
            visit(ty);
            for (_, a) in args {
                walk_expr_types_mut_inner(a, visit);
            }
        }
        IrExpr::MethodCall {
            receiver, args, ty, ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(receiver, visit);
            for (_, a) in args {
                walk_expr_types_mut_inner(a, visit);
            }
        }
        IrExpr::DictLiteral { entries, ty, .. } => {
            visit(ty);
            for (k, v) in entries {
                walk_expr_types_mut_inner(k, visit);
                walk_expr_types_mut_inner(v, visit);
            }
        }
        IrExpr::DictAccess { dict, key, ty, .. } => {
            visit(ty);
            walk_expr_types_mut_inner(dict, visit);
            walk_expr_types_mut_inner(key, visit);
        }
        IrExpr::Block {
            statements,
            result,
            ty,
            ..
        } => {
            visit(ty);
            for stmt in statements {
                walk_block_stmt_types_mut(stmt, visit);
            }
            walk_expr_types_mut_inner(result, visit);
        }
        IrExpr::Closure {
            params,
            captures,
            body,
            ty,
            ..
        } => {
            visit(ty);
            for (_, _, ty) in params {
                visit(ty);
            }
            for (_, _, ty) in captures {
                visit(ty);
            }
            walk_expr_types_mut_inner(body, visit);
        }
        IrExpr::ClosureRef { env_struct, ty, .. } => {
            visit(ty);
            walk_expr_types_mut_inner(env_struct, visit);
        }
    }
}

fn walk_block_stmt_types_mut(
    stmt: &mut IrBlockStatement,
    visit: &mut impl FnMut(&mut ResolvedType),
) {
    match stmt {
        IrBlockStatement::Let { value, .. } => walk_expr_types_mut_inner(value, visit),
        IrBlockStatement::Assign { target, value } => {
            walk_expr_types_mut_inner(target, visit);
            walk_expr_types_mut_inner(value, visit);
        }
        IrBlockStatement::Expr(e) => walk_expr_types_mut_inner(e, visit),
    }
}
