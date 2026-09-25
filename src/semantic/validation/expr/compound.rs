//! The checks of the compound expressions: arrays, tuples, binary
//! operations, `for`, `if`, `match` and field access.
//!
//! Each check is its own function and not an arm of `validate_expr`. In
//! a debug build, a function's stack frame holds the locals of every
//! arm of its `match`, and `validate_expr` recurses once for each level
//! of nesting. Small arms keep that frame small, so a deep program does
//! not overflow the stack.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::sem_type::SemType;
use super::super::super::SemanticAnalyzer;
use crate::ast::{BinaryOperator, Expr, File, Ident, MatchArm};
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::{HashMap, HashSet};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a call of the value of an expression: `make()(4)`.
    ///
    /// The callee must be a closure. Its type gives the conventions, the
    /// count and the type of each argument.
    pub(super) fn validate_expr_call(
        &mut self,
        callee: &Expr,
        args: &[(Option<Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        self.validate_expr(callee, file);
        let found = self.infer_type_sem(callee, file);
        if let SemType::Closure { params, .. } = &found {
            for (index, (_, arg)) in args.iter().enumerate() {
                let expected = params.get(index).map(|(_, ty)| ty.clone());
                self.validate_expr_expecting(arg, expected, file);
            }
            let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
            self.validate_closure_call_conventions(&conventions, args, span, file);
            self.validate_closure_call_shape(params, args, span, file);
            return;
        }
        for (_, arg) in args {
            self.validate_expr(arg, file);
        }
        if !found.is_indeterminate() {
            self.errors.push(CompilerError::TypeMismatch {
                expected: "a closure".to_string(),
                found: found.display(),
                span: callee.span(),
            });
        }
    }

    /// Validate an array literal.
    pub(super) fn validate_array_expr(
        &mut self,
        elements: &[Expr],
        span: Span,
        expected: Option<&SemType>,
        file: &File,
    ) {
        let element_expected = Self::expected_element(expected);
        for elem in elements {
            self.validate_expr_expecting(elem, element_expected.clone(), file);
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
        self.validate_array_homogeneity(elements, span, file);
    }

    /// Validate a tuple literal.
    pub(super) fn validate_tuple_expr(
        &mut self,
        fields: &[(Ident, Expr)],
        expected: Option<&SemType>,
        file: &File,
    ) {
        // A label may appear once in a tuple.
        let mut seen = HashSet::new();
        for (name, _) in fields {
            if !seen.insert(name.name.as_str()) {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!("tuple label '{}'", name.name),
                    span: name.span,
                });
            }
        }
        for (name, field_expr) in fields {
            let field_expected = Self::expected_tuple_field(expected, &name.name);
            self.validate_expr_expecting(field_expr, field_expected, file);
        }
        // Escape analysis: closure values stored in a tuple escape.
        for (_, field_expr) in fields {
            self.escape_closure_value(field_expr);
        }
    }

    /// Validate a binary operation.
    ///
    /// An operand that is an unsuffixed numeric literal, or arithmetic
    /// of such literals, takes its type from the other operand: `n + 1`
    /// with `n: I64` makes `1` an `I64`. When both operands are such
    /// literals, an arithmetic operator gives them the type that the
    /// context expects: `let b: I64 = 2 * 3`. The literal types go into
    /// the same record as the type of a lone literal.
    pub(super) fn validate_binary_expr(
        &mut self,
        (left, op, right): (&Expr, BinaryOperator, &Expr),
        span: Span,
        expected: Option<&SemType>,
        file: &File,
    ) {
        // A leading-dot variant on one side takes its enum from
        // the other side, as in `s == .a`.
        let (left_dot, right_dot) = self.operand_expectations(left, right, file);
        let arithmetic = matches!(
            op,
            BinaryOperator::Add
                | BinaryOperator::Sub
                | BinaryOperator::Mul
                | BinaryOperator::Div
                | BinaryOperator::Mod
        );
        let numeric = !matches!(op, BinaryOperator::And | BinaryOperator::Or);
        match (numeric, is_flexible(left), is_flexible(right)) {
            (true, false, true) => {
                self.validate_expr_expecting(left, left_dot, file);
                let other = self.infer_type_sem(left, file);
                self.validate_expr_expecting(right, right_dot.or(Some(other)), file);
            }
            (true, true, false) => {
                self.validate_expr_expecting(right, right_dot, file);
                let other = self.infer_type_sem(right, file);
                self.validate_expr_expecting(left, left_dot.or(Some(other)), file);
            }
            (true, true, true) if arithmetic => {
                self.validate_expr_expecting(left, expected.cloned(), file);
                self.validate_expr_expecting(right, expected.cloned(), file);
            }
            _ => {
                self.validate_expr_expecting(left, left_dot, file);
                self.validate_expr_expecting(right, right_dot, file);
            }
        }
        self.validate_binary_op(left, op, right, span, file);
    }

    /// Validate a `for` expression.
    pub(super) fn validate_for_expr(
        &mut self,
        var: &Ident,
        collection: &Expr,
        body: &Expr,
        span: Span,
        file: &File,
    ) {
        self.validate_expr(collection, file);
        // Bind the loop variable to the collection's element
        // type, so the body infers against a real type instead
        // of `Unknown`.
        let element = self.infer_type_sem(collection, file).iteration_element();
        let mut scope = HashMap::new();
        scope.insert(var.name.clone(), element);
        self.push_outer_frame(false);
        self.loop_var_scopes.push(scope);
        self.validate_expr(body, file);
        self.loop_var_scopes.pop();
        self.pop_outer_frame();
        self.validate_for_loop(collection, span, file);
    }

    /// Validate an `if` expression.
    pub(super) fn validate_if_expr(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
        span: Span,
        expected: Option<&SemType>,
        file: &File,
    ) {
        self.validate_expr(condition, file);
        // To bind the inner value of an optional in the truthy
        // branch, use Rust-style `if let pat = optional { … }
        // else { … }` — see `docs/user/control-flow.md`.
        // Snapshot consumed_bindings; the post-join union is
        // conservative (may over-report UseAfterSink, never miss).
        let pre_if = self.consumed_bindings.clone();
        self.validate_expr_expecting(then_branch, expected.cloned(), file);
        // after_then takes over `self.consumed_bindings`; swap pre_if in
        // so the else branch starts from pre-branch state.
        let after_then = std::mem::replace(&mut self.consumed_bindings, pre_if);
        if let Some(else_expr) = else_branch {
            self.validate_expr_expecting(else_expr, expected.cloned(), file);
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
                        span,
                    });
                }
            }
        }
        // Current state = after_else (or pre_if if no else branch).
        // Fold in after_then to produce union.
        self.consumed_bindings.extend(after_then);
        self.validate_if_condition(condition, span, file);
    }

    /// Validate a `match` expression.
    pub(super) fn validate_match_expr(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
        expected: Option<&SemType>,
        file: &File,
    ) {
        self.validate_expr(scrutinee, file);
        // Inferred once for the whole match, and only when the
        // scrutinee could hold a closure at all. Doing this per
        // arm cost more than the rest of a small compile put
        // together.
        let scrutinee_ty = self.infer_type_sem(scrutinee, file);
        let pre_match = self.consumed_bindings.clone();
        let mut post_union: HashSet<String> = HashSet::new();
        let mut arm_sems: Vec<SemType> = Vec::new();
        for arm in arms {
            self.consumed_bindings.clone_from(&pre_match);
            if let crate::ast::Pattern::Variant { bindings, .. } = &arm.pattern {
                // A name may be bound once in an arm.
                let mut bound = HashSet::new();
                for binding in bindings.iter().filter(|b| b.name != "_") {
                    if !bound.insert(binding.name.as_str()) {
                        self.errors.push(CompilerError::DuplicateDefinition {
                            name: binding.name.clone(),
                            span: binding.span,
                        });
                    }
                }
                let scope: HashSet<String> = bindings.iter().map(|b| b.name.clone()).collect();
                self.closure_param_scopes.push(scope);
                // A binding that holds a closure is callable in
                // the arm. `if let` is desugared at parse time
                // to a match on `.some` / `.none`, so
                // `if let g = xs[0] { g(n) }` arrives here —
                // and registering only `let`-bound closures
                // left `g(n)` reported as an undefined
                // reference, while the same closure bound by a
                // plain `let` worked.
                // Each binding has the type of its variant field.
                let frame = self.build_match_arm_scope_for_type(&scrutinee_ty, &arm.pattern);
                self.inference_scope_stack.borrow_mut().push(frame);
                self.validate_expr_expecting(&arm.body, expected.cloned(), file);
                // Infer the arm's type while its bindings are
                // in scope, so a binding shadows an outer name.
                arm_sems.push(self.infer_type_sem(&arm.body, file));
                self.inference_scope_stack.borrow_mut().pop();
                self.closure_param_scopes.pop();
            } else {
                self.validate_expr_expecting(&arm.body, expected.cloned(), file);
                arm_sems.push(self.infer_type_sem(&arm.body, file));
            }
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
        self.validate_match(scrutinee, arms, span, file);
    }

    /// Validate a field access.
    pub(super) fn validate_field_access_expr(
        &mut self,
        object: &Expr,
        field: &Ident,
        span: Span,
        file: &File,
    ) {
        self.validate_expr(object, file);
        let obj_sem = self.infer_type_sem(object, file);
        if !obj_sem.is_unknown() {
            // Field access on an optional type requires unwrapping
            if let SemType::Optional(inner) = &obj_sem {
                if !inner.is_indeterminate() {
                    let base = inner.display();
                    if self.symbols.get_struct(&base).is_some() {
                        self.errors.push(CompilerError::OptionalUsedAsNonOptional {
                            actual: obj_sem.display(),
                            expected: base,
                            span,
                        });
                    }
                }
            } else if let SemType::Tuple(fields) = &obj_sem {
                // A tuple names its fields, so the same rule
                // applies. Nothing checked this before, and the
                // IR lowering pass reported the missing field
                // as an internal error, which told the user to
                // file a bug for a typo in their own program.
                if !fields.iter().any(|(name, _)| name == &field.name) {
                    self.errors.push(CompilerError::UnknownField {
                        field: field.name.clone(),
                        type_name: obj_sem.display(),
                        span: field.span,
                    });
                }
            } else if !Self::holds_named_fields(&obj_sem) {
                // A number, a boolean, a closure: nothing on
                // the left has fields, so the access is wrong
                // however the field is spelled. Nothing
                // checked this, and the IR lowering pass
                // reported it as an internal error.
                self.errors.push(CompilerError::UnknownField {
                    field: field.name.clone(),
                    type_name: obj_sem.display(),
                    span: field.span,
                });
            } else {
                // Field must exist on the struct
                let base_type = Self::field_owner_name(&obj_sem);
                let known = self
                    .symbols
                    .get_struct(&base_type)
                    .map(|info| info.fields.iter().any(|f| f.name == field.name));
                // An enum value carries no fields of its own:
                // a payload is read by a `match` arm, which
                // binds it by name. So a field access on one
                // is wrong however it is spelled.
                let is_enum = self.symbols.get_enum_qualified(&base_type).is_some();
                if known == Some(false) || (known.is_none() && is_enum) {
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

/// Whether `expr` has no numeric type of its own: an unsuffixed numeric
/// literal, or arithmetic of such literals. Such an operand takes its
/// type from the other operand or from the context.
fn is_flexible(expr: &Expr) -> bool {
    if let Expr::Literal {
        value: crate::ast::Literal::Number(n),
        ..
    } = expr
    {
        return n.suffix.is_none();
    }
    if let Expr::Group { expr, .. } = expr {
        return is_flexible(expr);
    }
    if let Expr::UnaryOp {
        op: crate::ast::UnaryOperator::Neg,
        operand,
        ..
    } = expr
    {
        return is_flexible(operand);
    }
    if let Expr::BinaryOp {
        left, op, right, ..
    } = expr
    {
        return matches!(
            op,
            BinaryOperator::Add
                | BinaryOperator::Sub
                | BinaryOperator::Mul
                | BinaryOperator::Div
                | BinaryOperator::Mod
        ) && is_flexible(left)
            && is_flexible(right);
    }
    false
}
