//! Let-binding and block-statement validation: declared/inferred type
//! agreement, closure-binding registration, and per-block scope save/restore
//! of consumption flags.

use super::super::collect_bindings_from_pattern;
use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{BlockStatement, Expr, File, Type};
use crate::error::CompilerError;
use std::collections::{HashMap, HashSet};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    #[expect(
        clippy::too_many_lines,
        reason = "validates several let-binding rules in sequence; splitting them obscures the shared `declared`/`inferred` derivation"
    )]
    pub(super) fn validate_let_statement(
        &mut self,
        let_binding: &crate::ast::LetBinding,
        file: &File,
    ) {
        if let Some(type_ann) = &let_binding.type_annotation {
            // Tier-1 item E2: surface
            // `let x: SomeTrait = ...` as TraitUsedAsValueType (and
            // any other invalid type in the annotation) before the
            // value/declared compatibility check would mask it.
            self.validate_type(type_ann);
        }
        self.validate_expr(&let_binding.value, file);
        // Reject nil-into-nonopt and any other mismatch between the
        // inferred value type and the declared annotation.
        if let Some(type_ann) = &let_binding.type_annotation {
            let declared = Self::type_to_string(type_ann);
            let inferred_sem = self.infer_type_sem(&let_binding.value, file);
            let inferred = inferred_sem.display();
            if matches!(inferred_sem, SemType::Nil) && !declared.ends_with('?') {
                self.errors.push(CompilerError::NilAssignedToNonOptional {
                    expected: declared,
                    span: let_binding.span,
                });
            } else {
                // General type-mismatch check, mirroring the field-default
                // rule in `validate_struct_expressions`.
                let nil_to_optional =
                    matches!(inferred_sem, SemType::Nil) && declared.ends_with('?');
                let inner_to_optional =
                    declared.ends_with('?') && declared.trim_end_matches('?') == inferred.as_str();
                let is_closure_pair = matches!(type_ann, Type::Closure { .. })
                    && matches!(let_binding.value, Expr::ClosureExpr { .. });
                if !nil_to_optional
                    && !inner_to_optional
                    && !inferred_sem.is_indeterminate()
                    && declared != "Unknown"
                    && !is_closure_pair
                    && !self.type_strings_compatible(&declared, &inferred)
                {
                    self.errors.push(CompilerError::TypeMismatch {
                        expected: declared,
                        found: inferred,
                        span: let_binding.span,
                    });
                }
            }
        }
        // when a let binding declares a closure type and is
        // assigned a closure literal, type-check the closure body
        // bidirectionally — push the declared param types into the
        // inference scope (covering literal params with no annotation)
        // and verify the body's inferred type matches the declared
        // return type. Without this, untyped closure params silently
        // resolve to "Unknown", which unifies with anything and
        // suppresses real return-type mismatches.
        if let (
            Some(Type::Closure {
                params: declared_params,
                ret: declared_ret,
            }),
            Expr::ClosureExpr {
                params: lit_params,
                body,
                return_type: lit_ret,
                ..
            },
        ) = (&let_binding.type_annotation, &let_binding.value)
        {
            if lit_params.len() == declared_params.len() {
                let mut seed = HashMap::new();
                for (lit, (_, dty)) in lit_params.iter().zip(declared_params.iter()) {
                    let ty_str = lit
                        .ty
                        .as_ref()
                        .map_or_else(|| Self::type_to_string(dty), Self::type_to_string);
                    seed.insert(lit.name.name.clone(), ty_str);
                }
                self.inference_scope_stack.borrow_mut().push(seed);
                let inferred_body_sem = self.infer_type_sem(body, file);
                self.inference_scope_stack.borrow_mut().pop();
                let expected_ret = lit_ret
                    .as_ref()
                    .map_or_else(|| Self::type_to_string(declared_ret), Self::type_to_string);
                if !inferred_body_sem.is_indeterminate()
                    && !self.type_strings_compatible(&expected_ret, &inferred_body_sem.display())
                {
                    self.errors.push(CompilerError::TypeMismatch {
                        expected: expected_ret,
                        found: inferred_body_sem.display(),
                        span: let_binding.span,
                    });
                }
            }
        }
        // Register closure-typed module-level bindings for call-site enforcement
        if let Some(Type::Closure { params, .. }) = &let_binding.type_annotation {
            let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
            // If the value is a closure literal, record its free
            // variables so we can detect use-after-sink at call sites.
            let captures = if let Expr::ClosureExpr {
                params: cparams,
                body,
                ..
            } = &let_binding.value
            {
                let param_set: HashSet<String> =
                    cparams.iter().map(|p| p.name.name.clone()).collect();
                Some(Self::collect_free_variables(body, &param_set))
            } else {
                None
            };
            for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                self.closure_binding_conventions
                    .insert(binding.name.clone(), conventions.clone());
                if let Some(caps) = &captures {
                    self.closure_binding_captures
                        .insert(binding.name.clone(), caps.clone());
                    self.fn_scope_closure_captures
                        .insert(binding.name, caps.clone());
                }
            }
        }
        self.validate_destructuring_pattern(
            &let_binding.pattern,
            &let_binding.value,
            let_binding.span,
            file,
        );
    }

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
            self.validate_type(type_ann);
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
            let inferred_ty = self.infer_type_sem(value, file).display();
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
                    self.validate_expr(value, file);
                    let ty_str = ty.as_ref().map_or_else(
                        || self.infer_type_sem(value, file).display(),
                        |t| Self::type_to_string(t),
                    );
                    // Collect free variables once if this is a closure literal,
                    // so we can reuse them across all bindings in the pattern.
                    let captures = if matches!(ty, Some(Type::Closure { .. })) {
                        if let Expr::ClosureExpr {
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
                            .insert(binding.name, (ty_str.clone(), *mutable));
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
                        if !self.type_strings_compatible(&target_type, &value_type) {
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
