//! Expression-only walkers used by phases that need to recurse through an
//! `IrExpr` tree without invoking the type visitor.
//!
//! - [`walk_expr`] is a read-only walk that visits each node before its
//!   children. Used by Phase 1 (collection of generic-fn call sites) and
//!   Phase 4 (leftover scanner).
//! - [`iter_expr_children_mut`] returns a `Vec<&mut IrExpr>` of one
//!   expression's direct child expressions so callers can recurse without
//!   spinning up a full visitor. Used by dispatch rewriting, call-path
//!   rewriting, devirtualisation, and impl-index remapping.
//! - [`for_each_module_expr_mut`] visits every expression in a module,
//!   children before parents. Each phase that rewrites a kind of node
//!   uses it, so every phase reaches the same places.

use crate::ir::{IrBlockStatement, IrExpr, IrModule};

/// Read-only walk over an expression and all its children. Visits each
/// node before recursing.
pub(super) fn walk_expr(expr: &IrExpr, visit: &mut impl FnMut(&IrExpr)) {
    visit(expr);
    match expr {
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
        IrExpr::BinaryOp { left, right, .. } => {
            walk_expr(left, visit);
            walk_expr(right, visit);
        }
        IrExpr::UnaryOp { operand, .. } => walk_expr(operand, visit),
        IrExpr::Array { elements, .. } => {
            for e in elements {
                walk_expr(e, visit);
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr(k, visit);
                walk_expr(v, visit);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            walk_expr(dict, visit);
            walk_expr(key, visit);
        }
        IrExpr::FieldAccess { object, .. } => walk_expr(object, visit),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            walk_expr(condition, visit);
            walk_expr(then_branch, visit);
            if let Some(eb) = else_branch {
                walk_expr(eb, visit);
            }
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, visit);
            for arm in arms {
                walk_expr(&arm.body, visit);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            walk_expr(collection, visit);
            walk_expr(body, visit);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                match stmt {
                    IrBlockStatement::Let { value, .. } => walk_expr(value, visit),
                    IrBlockStatement::Assign { target, value, .. } => {
                        walk_expr(target, visit);
                        walk_expr(value, visit);
                    }
                    IrBlockStatement::Expr(e) => walk_expr(e, visit),
                }
            }
            walk_expr(result, visit);
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                walk_expr(e, visit);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            walk_expr(closure, visit);
            for (_, e) in args {
                walk_expr(e, visit);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            walk_expr(receiver, visit);
            for (_, e) in args {
                walk_expr(e, visit);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                walk_expr(e, visit);
            }
        }
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            for (_, _, e) in fields {
                walk_expr(e, visit);
            }
        }
        IrExpr::Closure { body, .. } => walk_expr(body, visit),
        IrExpr::ClosureRef { env_struct, .. } => walk_expr(env_struct, visit),
    }
}

/// Mutable iterator over a single expression's direct child expressions.
/// Callers recurse manually using the returned slice; useful for phases
/// that don't follow the type-visitor shape (dispatch rewriting,
/// devirtualisation, etc.).
pub(super) fn iter_expr_children_mut(expr: &mut IrExpr) -> Vec<&mut IrExpr> {
    let mut out: Vec<&mut IrExpr> = Vec::new();
    match expr {
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
        IrExpr::BinaryOp { left, right, .. } => {
            out.push(left.as_mut());
            out.push(right.as_mut());
        }
        IrExpr::UnaryOp { operand, .. } => out.push(operand.as_mut()),
        IrExpr::Array { elements, .. } => out.extend(elements.iter_mut()),
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                out.push(k);
                out.push(v);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            out.push(dict.as_mut());
            out.push(key.as_mut());
        }
        IrExpr::FieldAccess { object, .. } => out.push(object.as_mut()),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            out.push(condition.as_mut());
            out.push(then_branch.as_mut());
            if let Some(eb) = else_branch {
                out.push(eb.as_mut());
            }
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            out.push(scrutinee.as_mut());
            for arm in arms {
                out.push(&mut arm.body);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            out.push(collection.as_mut());
            out.push(body.as_mut());
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                match stmt {
                    IrBlockStatement::Let { value, .. } => out.push(value),
                    IrBlockStatement::Assign { target, value, .. } => {
                        out.push(target);
                        out.push(value);
                    }
                    IrBlockStatement::Expr(e) => out.push(e),
                }
            }
            out.push(result.as_mut());
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                out.push(e);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            out.push(closure.as_mut());
            for (_, e) in args {
                out.push(e);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            out.push(receiver.as_mut());
            for (_, e) in args {
                out.push(e);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                out.push(e);
            }
        }
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            for (_, _, e) in fields {
                out.push(e);
            }
        }
        IrExpr::Closure { body, .. } => out.push(body.as_mut()),
        IrExpr::ClosureRef { env_struct, .. } => out.push(env_struct.as_mut()),
    }
    out
}

/// Visit every expression in the module, children before parents: the
/// bodies and parameter defaults of the functions and of the impl
/// methods, the field defaults of the structs and of the enum variants,
/// and the module-level lets.
///
/// Each phase that rewrites one kind of node held its own copy of this
/// walk, and the copies differed: one skipped the parameter defaults
/// and the enum field defaults. A place that holds expressions is added
/// here once.
pub(super) fn for_each_module_expr_mut(module: &mut IrModule, visit: &mut impl FnMut(&mut IrExpr)) {
    fn walk(expr: &mut IrExpr, visit: &mut impl FnMut(&mut IrExpr)) {
        for child in iter_expr_children_mut(expr) {
            walk(child, visit);
        }
        visit(expr);
    }
    let functions = module.functions.iter_mut().chain(
        module
            .impls
            .iter_mut()
            .flat_map(|imp| imp.functions.iter_mut()),
    );
    for f in functions {
        if let Some(body) = &mut f.body {
            walk(body, visit);
        }
        for param in &mut f.params {
            if let Some(default) = &mut param.default {
                walk(default, visit);
            }
        }
    }
    for s in &mut module.structs {
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                walk(default, visit);
            }
        }
    }
    for e in &mut module.enums {
        for field in e.variants.iter_mut().flat_map(|v| v.fields.iter_mut()) {
            if let Some(default) = &mut field.default {
                walk(default, visit);
            }
        }
    }
    for l in &mut module.lets {
        walk(&mut l.value, visit);
    }
}
