//! A walk over every expression of a file, with write access.
//!
//! The walk visits the children of an expression before the
//! expression itself. So a rewrite of a node sees children that the
//! rewrite has already changed.

use crate::ast::{BlockStatement, Definition, Expr, File, FnParam, MatchArm, Statement};

/// Call `visit` on each expression in `file`, children first.
pub(super) fn visit_exprs_mut(file: &mut File, visit: &mut dyn FnMut(&mut Expr)) {
    for statement in &mut file.statements {
        match statement {
            Statement::Use(_) => {}
            Statement::Let(binding) => visit_expr(&mut binding.value, visit),
            Statement::Definition(def) => visit_definition(def, visit),
        }
    }
}

fn visit_definition(def: &mut Definition, visit: &mut dyn FnMut(&mut Expr)) {
    match def {
        Definition::Trait(t) => {
            for method in &mut t.methods {
                visit_params(&mut method.params, visit);
            }
        }
        Definition::Struct(s) => {
            for field in &mut s.fields {
                if let Some(default) = &mut field.default {
                    visit_expr(default, visit);
                }
            }
        }
        Definition::Impl(i) => {
            for f in &mut i.functions {
                visit_params(&mut f.params, visit);
                if let Some(body) = &mut f.body {
                    visit_expr(body, visit);
                }
            }
        }
        Definition::Enum(_) => {}
        Definition::Module(m) => {
            for inner in &mut m.definitions {
                visit_definition(inner, visit);
            }
        }
        Definition::Function(f) => {
            visit_params(&mut f.params, visit);
            if let Some(body) = &mut f.body {
                visit_expr(body, visit);
            }
        }
    }
}

fn visit_params(params: &mut [FnParam], visit: &mut dyn FnMut(&mut Expr)) {
    for param in params {
        if let Some(default) = &mut param.default {
            visit_expr(default, visit);
        }
    }
}

fn visit_expr(expr: &mut Expr, visit: &mut dyn FnMut(&mut Expr)) {
    match expr {
        Expr::Literal { .. } | Expr::Reference { .. } => {}
        Expr::Invocation { args, .. } => {
            for (_, arg) in args {
                visit_expr(arg, visit);
            }
        }
        Expr::MethodCall { receiver, args, .. }
        | Expr::Call {
            callee: receiver,
            args,
            ..
        } => {
            visit_expr(receiver, visit);
            for (_, arg) in args {
                visit_expr(arg, visit);
            }
        }
        Expr::EnumInstantiation { data, .. }
        | Expr::InferredEnumInstantiation { data, .. }
        | Expr::Tuple { fields: data, .. } => {
            for (_, value) in data {
                visit_expr(value, visit);
            }
        }
        Expr::Array { elements, .. } => {
            for element in elements {
                visit_expr(element, visit);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            visit_expr(left, visit);
            visit_expr(right, visit);
        }
        Expr::UnaryOp { operand: inner, .. }
        | Expr::Group { expr: inner, .. }
        | Expr::ClosureExpr { body: inner, .. }
        | Expr::FieldAccess { object: inner, .. } => visit_expr(inner, visit),
        Expr::ForExpr {
            collection, body, ..
        } => {
            visit_expr(collection, visit);
            visit_expr(body, visit);
        }
        Expr::IfExpr {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visit_expr(condition, visit);
            visit_expr(then_branch, visit);
            if let Some(else_branch) = else_branch {
                visit_expr(else_branch, visit);
            }
        }
        Expr::MatchExpr {
            scrutinee, arms, ..
        } => {
            visit_expr(scrutinee, visit);
            for MatchArm { body, .. } in arms {
                visit_expr(body, visit);
            }
        }
        Expr::DictLiteral { entries, .. } => {
            for (key, value) in entries {
                visit_expr(key, visit);
                visit_expr(value, visit);
            }
        }
        Expr::DictAccess { dict, key, .. } => {
            visit_expr(dict, visit);
            visit_expr(key, visit);
        }
        Expr::LetExpr { value, body, .. } => {
            visit_expr(value, visit);
            visit_expr(body, visit);
        }
        Expr::Block {
            statements, result, ..
        } => {
            for statement in statements {
                match statement {
                    BlockStatement::Let { value, .. } | BlockStatement::Expr(value) => {
                        visit_expr(value, visit);
                    }
                    BlockStatement::Assign { target, value, .. } => {
                        visit_expr(target, visit);
                        visit_expr(value, visit);
                    }
                }
            }
            visit_expr(result, visit);
        }
    }
    visit(expr);
}
