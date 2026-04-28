//! Expression lowering helpers for the IR lowering pass.
//!
//! The submodules group related expression-lowering routines so that no
//! single file becomes a 1,900-line wall:
//!
//! * [`literals_and_containers`] — literal/array/tuple/dict/enum-instantiation
//!   plus `lower_invocation` (struct construction and bare function calls).
//! * [`control_flow`] — `if`, `for`, `match`, `block`, `let` and pattern
//!   destructuring.
//! * [`operators`] — binary/unary ops, references, method calls and helpers
//!   for closure-typed argument inference.
//! * [`closures`] — closure-literal lowering and free-variable collection.
//! * [`dispatch`] — `(receiver_ty, method_name)` to `DispatchKind` resolution
//!   plus the impl-/trait-id lookup helpers it shares with `helpers`.
//! * [`helpers`] — type substitution and field/method/function return-type
//!   lookups shared across the other submodules.
//!
//! All submodules add methods to a single `impl IrLowerer<'_>` block; cross-file
//! callers therefore use `pub(super)` visibility (super = `lower::expr`).

mod closures;
mod control_flow;
mod dispatch;
mod helpers;
mod literals_and_containers;
mod operators;

use super::IrLowerer;
use crate::ast::Expr;
use crate::ir::IrExpr;

impl IrLowerer<'_> {
    pub(super) fn lower_expr(&mut self, expr: &Expr) -> IrExpr {
        // Track the span of the current expression so that InternalError
        // diagnostics surfaced during lowering carry a real source
        // location.
        let prev_span = self.current_span;
        self.current_span = expr.span();
        let out = self.lower_expr_inner(expr);
        self.current_span = prev_span;
        out
    }

    fn lower_expr_inner(&mut self, expr: &Expr) -> IrExpr {
        match expr {
            Expr::Literal { value: lit, .. } => IrExpr::Literal {
                value: lit.clone(),
                ty: Self::literal_type(lit),
            },
            Expr::Invocation {
                path,
                type_args,
                args,
                ..
            } => self.lower_invocation(path, type_args, args),
            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                ..
            } => self.lower_enum_instantiation(&enum_name.name, &variant.name, data),
            Expr::InferredEnumInstantiation { variant, data, .. } => {
                self.lower_inferred_enum_instantiation(&variant.name, data)
            }
            Expr::Array { elements, .. } => self.lower_array_expr(elements),
            Expr::Tuple { fields, .. } => self.lower_tuple_expr(fields),
            Expr::Reference { path, .. } => self.lower_reference(path),
            Expr::BinaryOp {
                left, op, right, ..
            } => self.lower_binary_op_expr(left, *op, right),
            Expr::UnaryOp { op, operand, .. } => self.lower_unary_op_expr(*op, operand),
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.lower_if_expr(condition, then_branch, else_branch.as_deref()),
            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => self.lower_for_expr(var, collection, body),
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => self.lower_match_expr(scrutinee, arms),
            Expr::Group { expr, .. } => self.lower_expr(expr),
            Expr::LetExpr {
                mutable,
                pattern,
                ty,
                value,
                body,
                ..
            } => self.lower_let_expr(*mutable, pattern, ty.as_ref(), value, body),
            Expr::DictLiteral { entries, .. } => self.lower_dict_literal(entries),
            Expr::DictAccess { dict, key, .. } => self.lower_dict_access(dict, key),
            Expr::ClosureExpr {
                params,
                return_type,
                body,
                ..
            } => self.lower_closure(params, return_type.as_ref(), body),
            Expr::FieldAccess { object, field, .. } => {
                let object_ir = self.lower_expr(object);
                let ty = self.resolve_field_type(object_ir.ty(), &field.name);
                IrExpr::FieldAccess {
                    object: Box::new(object_ir),
                    field: field.name.clone(),
                    ty,
                }
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => self.lower_method_call(receiver, &method.name, args.as_slice()),
            Expr::Block {
                statements, result, ..
            } => self.lower_block_expr(statements, result),
        }
    }
}
