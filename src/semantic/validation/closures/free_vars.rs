//! Free-variable walks used by closure validation:
//! - `check_captures_rec`: emit `UseAfterSink` for any reference whose root
//!   binding has already been consumed at closure-creation time.
//! - `collect_free_variables` / `collect_free_vars_rec`: produce the ordered
//!   list of names captured by a closure body.

use super::super::super::collect_bindings_from_pattern;
use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use crate::ast::{BlockStatement, Expr};
use crate::error::CompilerError;
use std::collections::HashSet;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Walk `expr` and emit `UseAfterSink` for any `Reference` whose root
    /// binding is in `consumed` and is not shadowed by a closure parameter
    /// or a binding introduced inside `expr`.
    #[expect(
        clippy::too_many_lines,
        reason = "dispatcher match over all Expr and BlockStatement variants"
    )]
    pub(super) fn check_captures_rec(
        expr: &Expr,
        outer_params: &HashSet<String>,
        consumed: &HashSet<String>,
        errors: &mut Vec<CompilerError>,
        inner_scopes: &mut Vec<HashSet<String>>,
    ) {
        let is_shadowed = |name: &str| -> bool {
            if outer_params.contains(name) {
                return true;
            }
            inner_scopes.iter().any(|s| s.contains(name))
        };
        match expr {
            Expr::Reference { path, span } => {
                if let Some(first) = path.first() {
                    if !is_shadowed(&first.name) && consumed.contains(&first.name) {
                        errors.push(CompilerError::UseAfterSink {
                            name: first.name.clone(),
                            span: *span,
                        });
                    }
                }
            }
            Expr::Literal { .. } | Expr::InferredEnumInstantiation { .. } => {}
            Expr::Array { elements, .. } => {
                for e in elements {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, e) in fields {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::Invocation { args, .. } => {
                for (_, e) in args {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::EnumInstantiation { data, .. } => {
                for (_, e) in data {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::check_captures_rec(left, outer_params, consumed, errors, inner_scopes);
                Self::check_captures_rec(right, outer_params, consumed, errors, inner_scopes);
            }
            Expr::UnaryOp { operand, .. } => {
                Self::check_captures_rec(operand, outer_params, consumed, errors, inner_scopes);
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => {
                Self::check_captures_rec(collection, outer_params, consumed, errors, inner_scopes);
                let mut scope = HashSet::new();
                scope.insert(var.name.clone());
                inner_scopes.push(scope);
                Self::check_captures_rec(body, outer_params, consumed, errors, inner_scopes);
                inner_scopes.pop();
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::check_captures_rec(condition, outer_params, consumed, errors, inner_scopes);
                Self::check_captures_rec(then_branch, outer_params, consumed, errors, inner_scopes);
                if let Some(e) = else_branch {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                Self::check_captures_rec(scrutinee, outer_params, consumed, errors, inner_scopes);
                for arm in arms {
                    let mut scope = HashSet::new();
                    if let crate::ast::Pattern::Variant { bindings, .. } = &arm.pattern {
                        for b in bindings {
                            scope.insert(b.name.clone());
                        }
                    }
                    inner_scopes.push(scope);
                    Self::check_captures_rec(
                        &arm.body,
                        outer_params,
                        consumed,
                        errors,
                        inner_scopes,
                    );
                    inner_scopes.pop();
                }
            }
            Expr::Group { expr, .. } => {
                Self::check_captures_rec(expr, outer_params, consumed, errors, inner_scopes);
            }
            Expr::DictLiteral { entries, .. } => {
                for (k, v) in entries {
                    Self::check_captures_rec(k, outer_params, consumed, errors, inner_scopes);
                    Self::check_captures_rec(v, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                Self::check_captures_rec(dict, outer_params, consumed, errors, inner_scopes);
                Self::check_captures_rec(key, outer_params, consumed, errors, inner_scopes);
            }
            Expr::FieldAccess { object, .. } => {
                Self::check_captures_rec(object, outer_params, consumed, errors, inner_scopes);
            }
            Expr::ClosureExpr { params, body, .. } => {
                let mut scope = HashSet::new();
                for p in params {
                    scope.insert(p.name.name.clone());
                }
                inner_scopes.push(scope);
                Self::check_captures_rec(body, outer_params, consumed, errors, inner_scopes);
                inner_scopes.pop();
            }
            Expr::LetExpr {
                pattern,
                value,
                body,
                ..
            } => {
                Self::check_captures_rec(value, outer_params, consumed, errors, inner_scopes);
                let mut scope = HashSet::new();
                for b in collect_bindings_from_pattern(pattern) {
                    scope.insert(b.name);
                }
                inner_scopes.push(scope);
                Self::check_captures_rec(body, outer_params, consumed, errors, inner_scopes);
                inner_scopes.pop();
            }
            Expr::MethodCall { receiver, args, .. } => {
                Self::check_captures_rec(receiver, outer_params, consumed, errors, inner_scopes);
                for (_, e) in args {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                let mut scope = HashSet::new();
                for stmt in statements {
                    match stmt {
                        BlockStatement::Let { pattern, value, .. } => {
                            Self::check_captures_rec(
                                value,
                                outer_params,
                                consumed,
                                errors,
                                inner_scopes,
                            );
                            for b in collect_bindings_from_pattern(pattern) {
                                scope.insert(b.name);
                            }
                        }
                        BlockStatement::Assign { target, value, .. } => {
                            Self::check_captures_rec(
                                target,
                                outer_params,
                                consumed,
                                errors,
                                inner_scopes,
                            );
                            Self::check_captures_rec(
                                value,
                                outer_params,
                                consumed,
                                errors,
                                inner_scopes,
                            );
                        }
                        BlockStatement::Expr(e) => {
                            Self::check_captures_rec(
                                e,
                                outer_params,
                                consumed,
                                errors,
                                inner_scopes,
                            );
                        }
                    }
                }
                inner_scopes.push(scope);
                Self::check_captures_rec(result, outer_params, consumed, errors, inner_scopes);
                inner_scopes.pop();
            }
        }
    }

    /// Collect the free variables referenced in a closure body.
    ///
    /// A free variable is any single-segment `Expr::Reference` path whose root
    /// identifier is not bound by the closure's own parameters, nor by any
    /// binding introduced within the body (nested closure params, `for`/`match`
    /// bindings, block/LetExpr locals). Ordering of the returned list is the
    /// order first encountered; duplicates are suppressed.
    pub(in crate::semantic) fn collect_free_variables(
        body: &Expr,
        closure_params: &HashSet<String>,
    ) -> Vec<String> {
        let mut captures: Vec<String> = Vec::new();
        let mut inner_scopes: Vec<HashSet<String>> = Vec::new();
        Self::collect_free_vars_rec(body, closure_params, &mut inner_scopes, &mut captures);
        captures
    }

    #[expect(
        clippy::too_many_lines,
        reason = "dispatcher match over all Expr and BlockStatement variants"
    )]
    fn collect_free_vars_rec(
        expr: &Expr,
        outer_params: &HashSet<String>,
        inner_scopes: &mut Vec<HashSet<String>>,
        captures: &mut Vec<String>,
    ) {
        let is_bound = |name: &str, inner: &Vec<HashSet<String>>| -> bool {
            if outer_params.contains(name) {
                return true;
            }
            inner.iter().any(|s| s.contains(name))
        };
        match expr {
            Expr::Reference { path, .. } => {
                if path.len() == 1 {
                    if let Some(first) = path.first() {
                        let name = &first.name;
                        if !is_bound(name, inner_scopes)
                            && !captures.iter().any(|n| n == name)
                            && name != "self"
                        {
                            captures.push(name.clone());
                        }
                    }
                }
            }
            Expr::Literal { .. } | Expr::InferredEnumInstantiation { .. } => {}
            Expr::Array { elements, .. } => {
                for e in elements {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, e) in fields {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::Invocation { path, args, .. } => {
                // The function/struct name itself is a bound symbol or a name;
                // if it is a single-segment reference to a let binding, it
                // should count as a capture too (so we can detect calling a
                // captured closure binding that was consumed).
                if path.len() == 1 {
                    if let Some(first) = path.first() {
                        let name = &first.name;
                        if !is_bound(name, inner_scopes)
                            && !captures.iter().any(|n| n == name)
                            && name != "self"
                        {
                            captures.push(name.clone());
                        }
                    }
                }
                for (_, e) in args {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::EnumInstantiation { data, .. } => {
                for (_, e) in data {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::collect_free_vars_rec(left, outer_params, inner_scopes, captures);
                Self::collect_free_vars_rec(right, outer_params, inner_scopes, captures);
            }
            Expr::UnaryOp { operand, .. } => {
                Self::collect_free_vars_rec(operand, outer_params, inner_scopes, captures);
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => {
                Self::collect_free_vars_rec(collection, outer_params, inner_scopes, captures);
                let mut scope = HashSet::new();
                scope.insert(var.name.clone());
                inner_scopes.push(scope);
                Self::collect_free_vars_rec(body, outer_params, inner_scopes, captures);
                inner_scopes.pop();
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::collect_free_vars_rec(condition, outer_params, inner_scopes, captures);
                Self::collect_free_vars_rec(then_branch, outer_params, inner_scopes, captures);
                if let Some(e) = else_branch {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                Self::collect_free_vars_rec(scrutinee, outer_params, inner_scopes, captures);
                for arm in arms {
                    let mut scope = HashSet::new();
                    if let crate::ast::Pattern::Variant { bindings, .. } = &arm.pattern {
                        for b in bindings {
                            scope.insert(b.name.clone());
                        }
                    }
                    inner_scopes.push(scope);
                    Self::collect_free_vars_rec(&arm.body, outer_params, inner_scopes, captures);
                    inner_scopes.pop();
                }
            }
            Expr::Group { expr, .. } => {
                Self::collect_free_vars_rec(expr, outer_params, inner_scopes, captures);
            }
            Expr::DictLiteral { entries, .. } => {
                for (k, v) in entries {
                    Self::collect_free_vars_rec(k, outer_params, inner_scopes, captures);
                    Self::collect_free_vars_rec(v, outer_params, inner_scopes, captures);
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                Self::collect_free_vars_rec(dict, outer_params, inner_scopes, captures);
                Self::collect_free_vars_rec(key, outer_params, inner_scopes, captures);
            }
            Expr::FieldAccess { object, .. } => {
                Self::collect_free_vars_rec(object, outer_params, inner_scopes, captures);
            }
            Expr::ClosureExpr { params, body, .. } => {
                let mut scope = HashSet::new();
                for p in params {
                    scope.insert(p.name.name.clone());
                }
                inner_scopes.push(scope);
                Self::collect_free_vars_rec(body, outer_params, inner_scopes, captures);
                inner_scopes.pop();
            }
            Expr::LetExpr {
                pattern,
                value,
                body,
                ..
            } => {
                Self::collect_free_vars_rec(value, outer_params, inner_scopes, captures);
                let mut scope = HashSet::new();
                for b in collect_bindings_from_pattern(pattern) {
                    scope.insert(b.name);
                }
                inner_scopes.push(scope);
                Self::collect_free_vars_rec(body, outer_params, inner_scopes, captures);
                inner_scopes.pop();
            }
            Expr::MethodCall { receiver, args, .. } => {
                Self::collect_free_vars_rec(receiver, outer_params, inner_scopes, captures);
                for (_, e) in args {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                let mut scope = HashSet::new();
                for stmt in statements {
                    match stmt {
                        BlockStatement::Let { pattern, value, .. } => {
                            Self::collect_free_vars_rec(
                                value,
                                outer_params,
                                inner_scopes,
                                captures,
                            );
                            for b in collect_bindings_from_pattern(pattern) {
                                scope.insert(b.name);
                            }
                        }
                        BlockStatement::Assign { target, value, .. } => {
                            Self::collect_free_vars_rec(
                                target,
                                outer_params,
                                inner_scopes,
                                captures,
                            );
                            Self::collect_free_vars_rec(
                                value,
                                outer_params,
                                inner_scopes,
                                captures,
                            );
                        }
                        BlockStatement::Expr(e) => {
                            Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                        }
                    }
                }
                inner_scopes.push(scope);
                Self::collect_free_vars_rec(result, outer_params, inner_scopes, captures);
                inner_scopes.pop();
            }
        }
    }
}
