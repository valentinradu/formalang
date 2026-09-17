//! The `let pat = value { body }` expression and the block walk.
//!
//! Split out of `let_and_block.rs` to keep each file under the line
//! ceiling that `scripts/check_file_sizes.sh` enforces.

use super::super::collect_bindings_from_pattern;
use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{BlockStatement, Expr, File, Type};
use crate::error::CompilerError;
use std::collections::HashSet;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a let expression
    ///
    /// Like block statements, `let ... in body` introduces bindings that are
    /// scoped to `body` and must not leak out. Snapshots are taken on entry
    /// and restored on exit.
    pub(super) fn validate_expr_let(&mut self, expr: &Expr, file: &File) {
        let Expr::LetExpr {
            mutable,
            pattern,
            ty,
            value,
            body,
            span,
        } = expr
        else {
            return;
        };
        if let Some(type_ann) = ty {
            self.validate_type(type_ann, *span);
        }
        self.validate_expr(value, file);
        // nil literals must not be assigned to non-optional types
        if let Some(type_ann) = ty {
            let declared = Self::type_to_string(type_ann);
            let inferred_sem = self.infer_type_sem(value, file);
            if matches!(inferred_sem, SemType::Nil) && !declared.ends_with('?') {
                self.errors.push(CompilerError::NilAssignedToNonOptional {
                    expected: declared,
                    span: *span,
                });
            }
        }
        self.validate_destructuring_pattern(pattern, value, *span, file);
        let saved_let_bindings = self.local_let_bindings.clone();
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_consumed = self.consumed_bindings.clone();
        // Collect closure captures once for reuse across all pattern bindings.
        let captures = if matches!(ty, Some(Type::Closure { .. })) {
            if let Expr::ClosureExpr {
                params: cparams,
                body,
                ..
            } = &**value
            {
                let param_set: HashSet<String> =
                    cparams.iter().map(|p| p.name.name.clone()).collect();
                Some(Self::collect_free_variables(body, &param_set))
            } else {
                None
            }
        } else {
            None
        };
        for binding in collect_bindings_from_pattern(pattern) {
            if super::super::is_primitive_name(&binding.name) {
                self.errors.push(CompilerError::PrimitiveRedefinition {
                    name: binding.name.clone(),
                    span: binding.span,
                });
                continue;
            }
            let inferred_ty = self.infer_type_sem(value, file);
            // If annotated as a closure type, record param conventions for call-site enforcement
            if let Some(Type::Closure { params, .. }) = ty {
                let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
                self.closure_binding_conventions
                    .insert(binding.name.clone(), conventions);
            }
            if let Some(caps) = &captures {
                self.closure_binding_captures
                    .insert(binding.name.clone(), caps.clone());
                self.fn_scope_closure_captures
                    .insert(binding.name.clone(), caps.clone());
            }
            self.local_let_bindings
                .insert(binding.name, (inferred_ty, *mutable));
        }
        self.validate_expr(body, file);
        // Preserve consumption for outer-scope names (function locals, module
        // lets, closure captures). Drop only names introduced by this LetExpr.
        let mut restored_consumed = saved_consumed;
        for name in &self.consumed_bindings {
            let introduced_here = self.local_let_bindings.contains_key(name)
                && !saved_let_bindings.contains_key(name);
            if !introduced_here {
                restored_consumed.insert(name.clone());
            }
        }
        self.local_let_bindings = saved_let_bindings;
        self.closure_binding_conventions = saved_closure_conventions;
        self.closure_binding_captures = saved_closure_captures;
        self.consumed_bindings = restored_consumed;
    }

    /// Validate a block expression (statements + result)
    ///
    /// Block-local let bindings, closure conventions, and sink-consumption flags
    /// are isolated from the enclosing scope: snapshots are taken on entry and
    /// restored on exit so block-internal names do not leak.
    #[expect(
        clippy::too_many_lines,
        reason = "linear pass over block statements + assign-time escape check; splitting hides flow"
    )]
    pub(super) fn validate_expr_block(
        &mut self,
        statements: &[BlockStatement],
        result: &Expr,
        file: &File,
    ) {
        let saved_let_bindings = self.local_let_bindings.clone();
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_consumed = self.consumed_bindings.clone();
        for stmt in statements {
            match stmt {
                BlockStatement::Let {
                    mutable,
                    pattern,
                    value,
                    ty,
                    ..
                } => {
                    // A block-level `let` annotation went unvalidated,
                    // so an undefined or mis-kinded type inside a
                    // function body reached IR lowering and surfaced as
                    // an internal error. Validate it the way the
                    // module-level path does.
                    if let Some(type_ann) = ty {
                        self.validate_type(type_ann, value.span());
                    }
                    self.validate_expr(value, file);
                    // Compare the value against the annotation, the
                    // same way the module-level path does.
                    if let Some(type_ann) = ty {
                        self.check_let_annotation(type_ann, value, value.span(), file);
                        self.check_optional_elements_are_used(
                            type_ann,
                            value,
                            *mutable,
                            value.span(),
                        );
                    }
                    self.check_inferred_enum_has_a_context(ty.as_ref(), value, file);
                    self.check_closure_literal_against_annotation(
                        ty.as_ref(),
                        value,
                        value.span(),
                        file,
                    );
                    let value_sem = ty
                        .as_ref()
                        .map_or_else(|| self.infer_type_sem(value, file), SemType::from_ast);
                    // Collect free variables (captures) once when the value
                    // is a closure literal, regardless of whether the let
                    // carried an explicit closure type annotation. Without
                    // this, the call-site validator can't recognise an
                    // unannotated `let f = |...| ...; f(x)` as a closure
                    // call and emits `UndefinedReference`.
                    let captures = if let Expr::ClosureExpr {
                        params: cparams,
                        body,
                        ..
                    } = value
                    {
                        let param_set: HashSet<String> =
                            cparams.iter().map(|p| p.name.name.clone()).collect();
                        Some(Self::collect_free_variables(body, &param_set))
                    } else {
                        None
                    };
                    let conventions_opt: Option<Vec<crate::ast::ParamConvention>> =
                        match (ty.as_ref(), value) {
                            (Some(Type::Closure { params, .. }), _) => {
                                Some(params.iter().map(|(c, _)| *c).collect())
                            }
                            (None, Expr::ClosureExpr { params, .. }) => {
                                Some(params.iter().map(|p| p.convention).collect())
                            }
                            _ => None,
                        };
                    let binding_pairs = self.pattern_binding_types(pattern, &value_sem, file);
                    let binding_spans: std::collections::HashMap<String, crate::location::Span> =
                        collect_bindings_from_pattern(pattern)
                            .into_iter()
                            .map(|b| (b.name, b.span))
                            .collect();
                    for (binding_name, binding_sem) in binding_pairs {
                        if super::super::is_primitive_name(&binding_name) {
                            self.errors.push(CompilerError::PrimitiveRedefinition {
                                name: binding_name.clone(),
                                span: binding_spans
                                    .get(&binding_name)
                                    .copied()
                                    .unwrap_or_default(),
                            });
                            continue;
                        }
                        if let Some(conventions) = &conventions_opt {
                            self.closure_binding_conventions
                                .insert(binding_name.clone(), conventions.clone());
                        }
                        if let Some(caps) = &captures {
                            self.closure_binding_captures
                                .insert(binding_name.clone(), caps.clone());
                            self.fn_scope_closure_captures
                                .insert(binding_name.clone(), caps.clone());
                        }
                        self.local_let_bindings
                            .insert(binding_name, (binding_sem, *mutable));
                    }
                }
                BlockStatement::Assign {
                    target,
                    value,
                    span,
                } => {
                    self.validate_expr(target, file);
                    self.validate_expr(value, file);
                    if !self.is_expr_mutable(target, file) {
                        self.errors
                            .push(CompilerError::AssignmentToImmutable { span: *span });
                    }
                    // Check that value type is compatible with target's declared type
                    let value_sem = self.infer_type_sem(value, file);
                    let target_sem = self.infer_type_sem(target, file);
                    if !value_sem.is_indeterminate() && !target_sem.is_indeterminate() {
                        let value_type = value_sem.display();
                        let target_type = target_sem.display();
                        // Through the shared rule: assignment applied
                        // none of the optional allowances, so neither
                        // `v = nil` nor `v = 1` was accepted for a
                        // `v: I32?`.
                        if !self.value_satisfies_declared(&target_type, &value_sem) {
                            self.errors.push(CompilerError::TypeMismatch {
                                expected: target_type,
                                found: value_type,
                                span: *span,
                            });
                        }
                    }
                    // A closure assigned to an outer-scope `mut` binding
                    // outlives this block; its captures must outlive the
                    // function frame. `saved_let_bindings` holds only
                    // pre-block bindings, so this filters out locals.
                    if let Expr::Reference { path, .. } = target {
                        if let [seg] = path.as_slice() {
                            if saved_let_bindings.contains_key(&seg.name) {
                                if let Some(caps) = self.closure_captures_of_expr(value) {
                                    self.validate_escaping_captures(&caps, *span);
                                }
                            }
                        }
                    }
                }
                BlockStatement::Expr(expr) => {
                    self.validate_expr(expr, file);
                }
            }
        }
        self.validate_expr(result, file);
        // Restore outer let/closure-convention scope. For consumption flags, keep
        // any binding consumed inside the block that belongs to an outer scope
        // (block did not introduce it). This preserves consumption of outer
        // locals AND module-level lets — dropping only flags for names the
        // block itself introduced.
        let mut restored_consumed = saved_consumed;
        for name in &self.consumed_bindings {
            let introduced_here = self.local_let_bindings.contains_key(name)
                && !saved_let_bindings.contains_key(name);
            if !introduced_here {
                restored_consumed.insert(name.clone());
            }
        }
        self.local_let_bindings = saved_let_bindings;
        self.closure_binding_conventions = saved_closure_conventions;
        self.closure_binding_captures = saved_closure_captures;
        self.consumed_bindings = restored_consumed;
    }
}
