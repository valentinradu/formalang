//! Top-level expression dispatcher: walks every `Expr` variant, recursing
//! through children before delegating variant-specific checks to the
//! sibling modules ([`reference`], [`literals`], [`operators`]) or to other
//! `validation` submodules (`invocation`, `method_call`, `control_flow`,
//! …).
//!
//! Recursion-depth guarding lives here too — the dispatcher is the single
//! entry point for descent through nested expressions, so the depth counter
//! is incremented and decremented around the variant match.

mod compound;
mod enums;
mod index;
mod literals;
mod operators;
mod reference;

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::type_resolution::float_key_in;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};
use crate::error::CompilerError;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a single expression (recursively)
    #[expect(
        clippy::too_many_lines,
        reason = "dispatcher match over 18+ Expr variants; each arm is a single call"
    )]
    pub(in crate::semantic) fn validate_expr(&mut self, expr: &Expr, file: &File) {
        // Check recursion depth to prevent stack overflow
        const MAX_EXPR_DEPTH: usize = 500;
        // The expected type is for this expression only. Each child
        // gets its own through `validate_expr_expecting`.
        let expected = self.expected_type.take();
        // A float key in the expected type is reported where the type
        // is written; the value does not report it again.
        let key_reported = expected
            .as_ref()
            .is_some_and(|t| float_key_in(t, true).is_some());
        self.validate_expr_depth = self.validate_expr_depth.saturating_add(1);
        if self.validate_expr_depth > MAX_EXPR_DEPTH {
            self.validate_expr_depth = self.validate_expr_depth.saturating_sub(1);
            self.errors
                .push(CompilerError::ExpressionDepthExceeded { span: expr.span() });
            return;
        }

        match expr {
            Expr::Literal { value, span } => {
                let target = self.record_literal_type(expr, value, expected.as_ref());
                self.validate_numeric_literal(value, target, *span);
            }
            Expr::Array { elements, span } => {
                self.validate_array_expr(elements, *span, expected.as_ref(), file);
            }
            Expr::Tuple { fields, .. } => {
                self.validate_tuple_expr(fields, expected.as_ref(), file);
            }
            Expr::Reference { path, span } => {
                self.validate_expr_reference(path, *span, file);
            }
            Expr::Invocation {
                path,
                type_args,
                args,
                span,
            } => {
                self.validate_expr_invocation(path, type_args, args, *span, file);
                if !key_reported {
                    self.check_function_float_key(path, type_args, args, *span, file);
                }
            }
            Expr::EnumInstantiation {
                enum_name,
                type_args,
                variant,
                data,
                span,
            } => {
                let written = self.validate_enum_path_type_args(
                    &enum_name.name,
                    type_args,
                    *span,
                    expected.as_ref(),
                );
                self.validate_full_enum_expr(
                    expr,
                    (enum_name, variant, data),
                    *span,
                    written.as_ref().or(expected.as_ref()),
                    file,
                );
            }
            Expr::InferredEnumInstantiation {
                variant,
                data,
                span,
            } => {
                self.validate_dot_enum_expr(expr, variant, data, *span, expected.as_ref(), file);
            }
            Expr::BinaryOp {
                left,
                op,
                right,
                span,
            } => {
                self.validate_binary_expr((left, *op, right), *span, expected.as_ref(), file);
            }
            Expr::UnaryOp { op, operand, .. } => {
                // `-(2 * 3)` in an `I64` position: the literals inside
                // take the type that the negation takes.
                let operand_expected = match op {
                    crate::ast::UnaryOperator::Neg => expected.clone(),
                    crate::ast::UnaryOperator::Not => None,
                };
                self.validate_expr_expecting(operand, operand_expected, file);
                self.validate_unary_operand(*op, operand, file);
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                span,
            } => {
                self.validate_for_expr(var, collection, body, *span, file);
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                self.validate_if_expr(
                    condition,
                    then_branch,
                    else_branch.as_deref(),
                    *span,
                    expected.as_ref(),
                    file,
                );
            }
            Expr::MatchExpr {
                scrutinee,
                arms,
                span,
            } => {
                self.validate_match_expr(scrutinee, arms, *span, expected.as_ref(), file);
            }
            Expr::Group { expr, .. } => self.validate_expr_expecting(expr, expected, file),
            Expr::DictLiteral { entries, span, .. } => {
                self.validate_dict_literal(entries, *span, expected.as_ref(), key_reported, file);
            }
            Expr::DictAccess { dict, key, span } => {
                self.validate_index_expr(dict, key, *span, file);
            }
            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                self.validate_field_access_expr(object, field, *span, file);
            }
            Expr::ClosureExpr {
                params,
                return_type,
                body,
                ..
            } => {
                self.validate_expr_closure(
                    params,
                    return_type.as_ref(),
                    body,
                    expected.as_ref(),
                    file,
                );
            }
            Expr::LetExpr { .. } => {
                self.validate_expr_let(expr, expected, file);
            }
            Expr::Call { callee, args, span } => {
                self.validate_expr_call(callee, args, *span, file);
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                span,
            } => {
                let call = (receiver.as_ref(), method, args.as_slice());
                self.validate_expr_method_call(call, *span, key_reported, file);
            }
            Expr::Block {
                statements, result, ..
            } => {
                self.validate_expr_block(statements, result, expected, file);
            }
        }
        self.validate_expr_depth = self.validate_expr_depth.saturating_sub(1);
    }

    /// Name of the leftmost binding referenced by `expr`, walking through
    /// `FieldAccess`, `Group`, and `Reference`. `None` for non-place
    /// expressions (literals, calls). Used to mark the root binding consumed
    /// when a compound place (`x.field`) is passed to a sink parameter.
    pub(in crate::semantic::validation) fn root_binding(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Reference { path, .. } => path.first().map(|id| id.name.clone()),
            Expr::FieldAccess { object, .. } => Self::root_binding(object),
            Expr::Group { expr, .. } => Self::root_binding(expr),
            Expr::Literal { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Call { .. }
            | Expr::Block { .. } => None,
        }
    }

    /// Whether a value of this type takes an index.
    ///
    /// An array by position, a dictionary by key, and a string by byte
    /// offset. Nothing else: the IR lowering pass reads exactly these
    /// three shapes, and every other receiver reached it as an
    /// internal error.
    /// The name of the struct that declares this type's fields.
    ///
    /// The four built-in compound shapes are declared as generic
    /// structs in the prelude, under names that their display form
    /// does not spell: `[I32]` displays as `[I32]` but its fields live
    /// on `Array`. See `src/prelude.fv`. The method-call validator
    /// makes the same mapping.
    pub(in crate::semantic) fn field_owner_name(ty: &SemType) -> String {
        match ty {
            SemType::Array(_) => "Array".to_string(),
            SemType::Dictionary { .. } => "Dictionary".to_string(),
            SemType::Optional(_) => "Optional".to_string(),
            SemType::Generic { base, .. } => base.clone(),
            SemType::Primitive(_)
            | SemType::Named(_)
            | SemType::Tuple(_)
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => ty.display(),
        }
    }

    /// Whether a value of this type carries named fields at all.
    ///
    /// A struct and a tuple do. A number, a boolean, a closure, an
    /// enum value and a range do not, so a field access on one is a
    /// mistake however the field is spelled. A [`SemType::Named`] may
    /// be a struct this file cannot see — an import, or a generic
    /// parameter — so it stays out of this list and the field check
    /// leaves it alone.
    const fn holds_named_fields(ty: &SemType) -> bool {
        !matches!(
            ty,
            SemType::Primitive(_) | SemType::Closure { .. } | SemType::Nil
        )
    }
}
