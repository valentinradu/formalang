//! Walker for expression trees.

use super::{NodeAtPosition, NodeFinder};
use crate::ast::{BlockStatement, Expr};
use crate::location::Span;
use crate::semantic::position::span_contains_offset;

impl<'ast> NodeFinder<'ast> {
    /// Visit an expression
    pub(super) fn visit_expr(&mut self, expr: &'ast Expr) {
        if let Some(span) = expr_span(expr) {
            if span_contains_offset(&span, self.offset) {
                self.parents.push(NodeAtPosition::Expression(expr));
                self.visit_expr_inner(expr);
                if self.found_node.is_none() {
                    self.found_node = Some(NodeAtPosition::Expression(expr));
                }
                self.parents.pop();
            }
        }
    }

    /// Dispatch to per-variant expression visitors.
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive per-Expr-variant traversal; each arm is a short recursive walk"
    )]
    fn visit_expr_inner(&mut self, expr: &'ast Expr) {
        match expr {
            Expr::Reference { path, .. } => {
                for ident in path {
                    if span_contains_offset(&ident.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(ident));
                        return;
                    }
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.visit_expr(left);
                if self.found_node.is_none() {
                    self.visit_expr(right);
                }
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => {
                if span_contains_offset(&var.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(var));
                } else {
                    self.visit_expr(collection);
                    if self.found_node.is_none() {
                        self.visit_expr(body);
                    }
                }
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.visit_expr(condition);
                if self.found_node.is_none() {
                    self.visit_expr(then_branch);
                }
                if self.found_node.is_none() {
                    if let Some(else_expr) = else_branch {
                        self.visit_expr(else_expr);
                    }
                }
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                self.visit_expr(scrutinee);
                if self.found_node.is_none() {
                    for arm in arms {
                        self.visit_expr(&arm.body);
                        if self.found_node.is_some() {
                            break;
                        }
                    }
                }
            }
            Expr::Group { expr, .. } => {
                self.visit_expr(expr);
            }
            Expr::Array { elements, .. } => {
                for elem in elements {
                    self.visit_expr(elem);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::Tuple { fields, .. } => {
                for (name, field_expr) in fields {
                    if span_contains_offset(&name.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(name));
                        break;
                    }
                    self.visit_expr(field_expr);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::Invocation { args, .. } => {
                for (_, arg) in args {
                    self.visit_expr(arg);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::EnumInstantiation { data, .. } | Expr::InferredEnumInstantiation { data, .. } => {
                for (_, v) in data {
                    self.visit_expr(v);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::UnaryOp { operand, .. } => self.visit_expr(operand),
            Expr::DictLiteral { entries, .. } => {
                for (key, value) in entries {
                    self.visit_expr(key);
                    if self.found_node.is_some() {
                        break;
                    }
                    self.visit_expr(value);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                self.visit_expr(dict);
                if self.found_node.is_none() {
                    self.visit_expr(key);
                }
            }
            Expr::FieldAccess { object, .. } => self.visit_expr(object),
            Expr::ClosureExpr { body, .. } => self.visit_expr(body),
            Expr::LetExpr { value, body, .. } => {
                self.visit_expr(value);
                if self.found_node.is_none() {
                    self.visit_expr(body);
                }
            }
            Expr::MethodCall { receiver, args, .. } => {
                self.visit_expr(receiver);
                if self.found_node.is_some() {
                    return;
                }
                for (_, arg) in args {
                    self.visit_expr(arg);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                for stmt in statements {
                    match stmt {
                        BlockStatement::Let { value, .. } => self.visit_expr(value),
                        BlockStatement::Assign { target, value, .. } => {
                            self.visit_expr(target);
                            if self.found_node.is_none() {
                                self.visit_expr(value);
                            }
                        }
                        BlockStatement::Expr(expr) => self.visit_expr(expr),
                    }
                    if self.found_node.is_some() {
                        break;
                    }
                }
                if self.found_node.is_none() {
                    self.visit_expr(result);
                }
            }
            Expr::Literal { .. } => {}
        }
    }
}

/// Get the span of an expression. Every `Expr` variant now carries a
/// real span, so this is always `Some` — kept for
/// backwards compatibility with the existing callers.
#[expect(
    clippy::unnecessary_wraps,
    reason = "callers pattern-match on Option to skip nodes with no span; preserved for API stability"
)]
const fn expr_span(expr: &Expr) -> Option<Span> {
    Some(expr.span())
}
