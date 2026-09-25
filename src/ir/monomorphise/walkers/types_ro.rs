//! Read-only walk over every `ResolvedType` reachable from an `IrModule`.

use crate::ir::{
    IrBlockStatement, IrEnum, IrExpr, IrField, IrFunction, IrImpl, IrModule, IrStruct, IrTrait,
    ResolvedType,
};

/// Read-only walk over every `ResolvedType` reachable from the module.
pub(in crate::ir::monomorphise) fn walk_module_types(
    module: &IrModule,
    visit: &mut impl FnMut(&ResolvedType),
) {
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

pub(in crate::ir::monomorphise) fn walk_function_types(
    f: &IrFunction,
    visit: &mut impl FnMut(&ResolvedType),
) {
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
pub(in crate::ir::monomorphise) fn walk_expr_types(
    expr: &IrExpr,
    visit: &mut impl FnMut(&ResolvedType),
) {
    visit(expr.ty());
    match expr {
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                walk_expr_types(e, visit);
            }
        }
        IrExpr::StructInst {
            type_args, fields, ..
        } => {
            for ta in type_args {
                visit(ta);
            }
            for (_, _, e) in fields {
                walk_expr_types(e, visit);
            }
        }
        IrExpr::EnumInst { fields, .. } => {
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
                for (_, _, binding_ty) in &arm.bindings {
                    visit(binding_ty);
                }
                walk_expr_types(&arm.body, visit);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, a) in args {
                walk_expr_types(a, visit);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            walk_expr_types(closure, visit);
            for (_, a) in args {
                walk_expr_types(a, visit);
            }
        }
        IrExpr::MethodCall {
            receiver,
            args,
            dispatch,
            ..
        } => {
            if let crate::ir::DispatchKind::Virtual { trait_args, .. } = dispatch {
                for t in trait_args {
                    visit(t);
                }
            }
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
            for (_, _, _, ty) in params {
                visit(ty);
            }
            for (_, _, _, ty) in captures {
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
        IrBlockStatement::Assign { target, value, .. } => {
            walk_expr_types(target, visit);
            walk_expr_types(value, visit);
        }
        IrBlockStatement::Expr(e) => walk_expr_types(e, visit),
    }
}
