//! Top-level expression dispatcher: walks every `Expr` variant, recursing
//! through children before delegating variant-specific checks to the
//! sibling modules ([`reference`], [`literals`], [`operators`]) or to other
//! `validation` submodules (`invocation`, `method_call`, `control_flow`,
//! …).
//!
//! Recursion-depth guarding lives here too — the dispatcher is the single
//! entry point for descent through nested expressions, so the depth counter
//! is incremented and decremented around the variant match.

mod literals;
mod operators;
mod reference;

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};
use crate::error::CompilerError;
use std::collections::HashSet;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a single expression (recursively)
    #[expect(
        clippy::too_many_lines,
        reason = "dispatcher match over 18+ Expr variants; each arm is a single call"
    )]
    pub(in crate::semantic) fn validate_expr(&mut self, expr: &Expr, file: &File) {
        // Check recursion depth to prevent stack overflow
        const MAX_EXPR_DEPTH: usize = 500;
        self.validate_expr_depth = self.validate_expr_depth.saturating_add(1);
        if self.validate_expr_depth > MAX_EXPR_DEPTH {
            self.validate_expr_depth = self.validate_expr_depth.saturating_sub(1);
            self.errors
                .push(CompilerError::ExpressionDepthExceeded { span: expr.span() });
            return;
        }

        match expr {
            Expr::Literal { .. } => {}
            Expr::Array { elements, span } => {
                for elem in elements {
                    self.validate_expr(elem, file);
                }
                // Escape analysis: any closure value stored in the array escapes
                // with the collection — mark its captures as consumed.
                for elem in elements {
                    self.escape_closure_value(elem);
                }
                // unify the element types so a heterogeneous
                // array literal (`[1, "two"]`) surfaces as a real
                // TypeMismatch instead of silently using the first
                // element's type.
                self.validate_array_homogeneity(elements, *span, file);
            }
            Expr::Tuple { fields, .. } => {
                for (_, field_expr) in fields {
                    self.validate_expr(field_expr, file);
                }
                // Escape analysis: closure values stored in a tuple escape.
                for (_, field_expr) in fields {
                    self.escape_closure_value(field_expr);
                }
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
            }
            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                span,
            } => {
                for (_, data_expr) in data {
                    self.validate_expr(data_expr, file);
                }
                self.validate_enum_instantiation(enum_name, variant, data, *span, file);
            }
            Expr::InferredEnumInstantiation { data, .. } => {
                for (_, data_expr) in data {
                    self.validate_expr(data_expr, file);
                }
            }
            Expr::BinaryOp {
                left,
                op,
                right,
                span,
            } => {
                self.validate_expr(left, file);
                self.validate_expr(right, file);
                self.validate_binary_op(left, *op, right, *span, file);
            }
            Expr::UnaryOp { operand, .. } => {
                self.validate_expr(operand, file);
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                span,
            } => {
                self.validate_expr(collection, file);
                let mut scope = HashSet::new();
                scope.insert(var.name.clone());
                self.loop_var_scopes.push(scope);
                self.validate_expr(body, file);
                self.loop_var_scopes.pop();
                self.validate_for_loop(collection, *span, file);
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                self.validate_expr(condition, file);
                // For optional conditions like `if user.nickname`, expose the
                // unwrapped value to the then-branch under its trailing name.
                let (auto_binding_name, auto_binding_prev) =
                    self.bind_optional_auto_binding(condition, file);
                // Snapshot consumed_bindings; the post-join union is
                // conservative (may over-report UseAfterSink, never miss).
                let pre_if = self.consumed_bindings.clone();
                self.validate_expr(then_branch, file);
                // Restore any auto-binding we installed before entering the
                // else branch (which does not see the unwrapped binding).
                if let Some(name) = auto_binding_name.as_ref() {
                    match auto_binding_prev {
                        Some(prev) => {
                            self.local_let_bindings.insert(name.clone(), prev);
                        }
                        None => {
                            self.local_let_bindings.remove(name);
                        }
                    }
                }
                // after_then takes over `self.consumed_bindings`; swap pre_if in
                // so the else branch starts from pre-branch state.
                let after_then = std::mem::replace(&mut self.consumed_bindings, pre_if);
                if let Some(else_expr) = else_branch {
                    self.validate_expr(else_expr, file);
                    // Branch types must unify under optional widening
                    // (T + Nil → T?, T + T? → T?).
                    let then_sem = self.infer_type_sem(then_branch, file);
                    let else_sem = self.infer_type_sem(else_expr, file);
                    // Skip when either type is indeterminate (Unknown / nested Unknown / InferredEnum).
                    if !then_sem.is_indeterminate()
                        && !else_sem.is_indeterminate()
                        && !SemType::unifies_with_optional_widening(&then_sem, &else_sem)
                    {
                        let then_type = then_sem.display();
                        let else_type = else_sem.display();
                        if !self.type_strings_compatible(&then_type, &else_type) {
                            self.errors.push(CompilerError::TypeMismatch {
                                expected: then_type,
                                found: else_type,
                                span: *span,
                            });
                        }
                    }
                }
                // Current state = after_else (or pre_if if no else branch).
                // Fold in after_then to produce union.
                self.consumed_bindings.extend(after_then);
                self.validate_if_condition(condition, *span, file);
            }
            Expr::MatchExpr {
                scrutinee,
                arms,
                span,
            } => {
                self.validate_expr(scrutinee, file);
                let pre_match = self.consumed_bindings.clone();
                let mut post_union: HashSet<String> = HashSet::new();
                let mut arm_sems: Vec<SemType> = Vec::new();
                for arm in arms {
                    self.consumed_bindings.clone_from(&pre_match);
                    if let crate::ast::Pattern::Variant { bindings, .. } = &arm.pattern {
                        let scope: HashSet<String> =
                            bindings.iter().map(|b| b.name.clone()).collect();
                        self.closure_param_scopes.push(scope);
                        self.validate_expr(&arm.body, file);
                        self.closure_param_scopes.pop();
                    } else {
                        self.validate_expr(&arm.body, file);
                    }
                    arm_sems.push(self.infer_type_sem(&arm.body, file));
                    // Drain the per-arm state into post_union without cloning.
                    post_union.extend(self.consumed_bindings.drain());
                }
                // Include pre_match (pass-through when no arm is taken).
                post_union.extend(pre_match);
                self.consumed_bindings = post_union;
                // Check that all arm types are compatible with the first arm's type.
                // Widening: variations of T and T?/Nil unify to T?.
                if let Some(first_sem) = arm_sems.first().cloned() {
                    if !first_sem.is_indeterminate() {
                        let first_type = first_sem.display();
                        for (arm, arm_sem) in arms.iter().zip(arm_sems.iter()).skip(1) {
                            if arm_sem.is_indeterminate()
                                || SemType::unifies_with_optional_widening(&first_sem, arm_sem)
                            {
                                continue;
                            }
                            let arm_type = arm_sem.display();
                            if !self.type_strings_compatible(&first_type, &arm_type) {
                                self.errors.push(CompilerError::TypeMismatch {
                                    expected: first_type.clone(),
                                    found: arm_type,
                                    span: arm.span,
                                });
                            }
                        }
                    }
                }
                self.validate_match(scrutinee, arms, *span, file);
            }
            Expr::Group { expr, .. } => self.validate_expr(expr, file),
            Expr::DictLiteral { entries, span, .. } => {
                for (key, value) in entries {
                    self.validate_expr(key, file);
                    self.validate_expr(value, file);
                }
                // Escape analysis: closure values stored as dict keys/values escape.
                for (key, value) in entries {
                    self.escape_closure_value(key);
                    self.escape_closure_value(value);
                }
                // unify key types and value types across
                // entries so a heterogeneous dict literal
                // (`["a": 1, "b": "two"]`) surfaces as a real
                // TypeMismatch instead of silently using the first
                // entry's type.
                self.validate_dict_homogeneity(entries, *span, file);
            }
            Expr::DictAccess { dict, key, span } => {
                self.validate_expr(dict, file);
                self.validate_expr(key, file);
                // Validate key type against declared dict type.
                // Structural unpacking — no string scanning needed.
                if let SemType::Dictionary {
                    key: expected_key, ..
                } = self.infer_type_sem(dict, file)
                {
                    let actual_key_sem = self.infer_type_sem(key, file);
                    if !actual_key_sem.is_unknown() && actual_key_sem != *expected_key {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: expected_key.display(),
                            found: actual_key_sem.display(),
                            span: *span,
                        });
                    }
                }
            }
            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                self.validate_expr(object, file);
                let obj_sem = self.infer_type_sem(object, file);
                if !obj_sem.is_unknown() {
                    // Field access on an optional type requires unwrapping
                    if let SemType::Optional(inner) = &obj_sem {
                        let base = inner.display();
                        if base != "Unknown" && self.symbols.get_struct(&base).is_some() {
                            self.errors.push(CompilerError::OptionalUsedAsNonOptional {
                                actual: obj_sem.display(),
                                expected: base,
                                span: *span,
                            });
                        }
                    } else {
                        // Field must exist on the struct
                        let base_type = obj_sem.display();
                        if let Some(struct_info) = self.symbols.get_struct(&base_type) {
                            if !struct_info.fields.iter().any(|f| f.name == field.name) {
                                self.errors.push(CompilerError::UnknownField {
                                    field: field.name.clone(),
                                    type_name: base_type,
                                    span: field.span,
                                });
                            }
                        }
                    }
                }
            }
            Expr::ClosureExpr {
                params,
                return_type,
                body,
                ..
            } => {
                self.validate_expr_closure(params, return_type.as_ref(), body, file);
            }
            Expr::LetExpr { .. } => {
                self.validate_expr_let(expr, file);
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                span,
            } => {
                self.validate_expr_method_call(receiver, method, args.as_slice(), *span, file);
            }
            Expr::Block {
                statements, result, ..
            } => {
                self.validate_expr_block(statements, result, file);
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
            | Expr::Block { .. } => None,
        }
    }
}
