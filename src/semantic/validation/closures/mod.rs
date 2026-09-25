//! Closure escape and capture validation.
//!
//! Tracks the captures of closure values as they flow through the program
//! (assignment, sink-pass, struct field, return) so that ownership is
//! transferred (sink) or rejected (would dangle past the function frame).
//!
//! The free-variable walk used by these checks lives in the [`free_vars`]
//! sibling module.

mod free_vars;

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File, Type};
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::{HashMap, HashSet};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Captures for a closure value: tracked binding, literal, or `Group`
    /// wrapping one. `None` for anything else.
    pub(super) fn closure_captures_of_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        match expr {
            Expr::Reference { path, .. } => {
                if path.len() != 1 {
                    return None;
                }
                let name = &path.first()?.name;
                self.closure_binding_captures.get(name).cloned()
            }
            Expr::ClosureExpr { params, body, .. } => {
                let param_set: HashSet<String> =
                    params.iter().map(|p| p.name.name.clone()).collect();
                Some(Self::collect_free_variables(body, &param_set))
            }
            Expr::Group { expr, .. } => self.closure_captures_of_expr(expr),
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
            | Expr::FieldAccess { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Call { .. }
            | Expr::Block { .. } => None,
        }
    }

    /// Mark the captures of an escaping closure as consumed.
    ///
    /// Given an initial list of captured names, walks transitively through
    /// `closure_binding_captures`: if any captured name is itself a tracked
    /// closure binding, its captures are included too. Each reached name is
    /// inserted into `consumed_bindings`. A visited set prevents infinite
    /// recursion on cyclic capture chains.
    fn mark_captures_consumed(&mut self, initial: &[String]) {
        let mut visited: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = initial.to_vec();
        while let Some(name) = stack.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            // If `name` itself names a tracked closure binding, recurse into its captures.
            if let Some(nested) = self.closure_binding_captures.get(&name).cloned() {
                for cap in nested {
                    if !visited.contains(&cap) {
                        stack.push(cap);
                    }
                }
            }
            self.consumed_bindings.insert(name);
        }
    }

    /// Escape helper: if `expr` is a closure value (named binding or literal),
    /// mark its captures as consumed transitively.
    ///
    /// Used at escape sites: sink-pass, struct field assignment, array/dict
    /// element, and similar positions where the closure's owning scope changes.
    pub(super) fn escape_closure_value(&mut self, expr: &Expr) {
        if let Some(caps) = self.closure_captures_of_expr(expr) {
            self.mark_captures_consumed(&caps);
        }
    }

    /// Closures escaping via the function's result expression, with captures
    /// and span. Recurses through Block/LetExpr/IfExpr/MatchExpr results;
    /// if/match contribute one entry per branch for per-branch reporting.
    fn collect_returned_closure_captures(&self, expr: &Expr) -> Vec<(Vec<String>, Span)> {
        let mut results: Vec<(Vec<String>, Span)> = Vec::new();
        self.collect_returned_closure_captures_rec(expr, &mut results);
        results
    }

    fn collect_returned_closure_captures_rec(
        &self,
        expr: &Expr,
        out: &mut Vec<(Vec<String>, Span)>,
    ) {
        match expr {
            Expr::ClosureExpr {
                params, body, span, ..
            } => {
                let param_set: HashSet<String> =
                    params.iter().map(|p| p.name.name.clone()).collect();
                let caps = Self::collect_free_variables(body, &param_set);
                out.push((caps, *span));
            }
            Expr::Reference { path, span } => {
                if path.len() == 1 {
                    if let Some(first) = path.first() {
                        // Use the flat fn-scope map so bindings from popped
                        // nested blocks still carry captures.
                        if let Some(caps) = self
                            .fn_scope_closure_captures
                            .get(&first.name)
                            .or_else(|| self.closure_binding_captures.get(&first.name))
                        {
                            out.push((caps.clone(), *span));
                        }
                    }
                }
            }
            Expr::Group { expr, .. } => {
                self.collect_returned_closure_captures_rec(expr, out);
            }
            Expr::Block { result, .. } => {
                self.collect_returned_closure_captures_rec(result, out);
            }
            Expr::LetExpr { body, .. } => {
                self.collect_returned_closure_captures_rec(body, out);
            }
            Expr::IfExpr {
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_returned_closure_captures_rec(then_branch, out);
                if let Some(else_expr) = else_branch {
                    self.collect_returned_closure_captures_rec(else_expr, out);
                }
            }
            Expr::MatchExpr { arms, .. } => {
                for arm in arms {
                    self.collect_returned_closure_captures_rec(&arm.body, out);
                }
            }
            // Tier-1 escape extension: a closure stored into a struct
            // / enum field that becomes part of the returned aggregate
            // also escapes via return. Walk constructor args, but only
            // when the path resolves to a struct (or the enum variant
            // is named) — function-call invocations don't return their
            // arguments and would over-trigger.
            Expr::Invocation { path, args, .. } => {
                let is_struct = path
                    .last()
                    .is_some_and(|seg| self.symbols.get_struct(&seg.name).is_some());
                if is_struct {
                    for (_, arg) in args {
                        self.collect_returned_closure_captures_rec(arg, out);
                    }
                }
            }
            Expr::EnumInstantiation { data, .. } | Expr::InferredEnumInstantiation { data, .. } => {
                for (_, field_expr) in data {
                    self.collect_returned_closure_captures_rec(field_expr, out);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, field_expr) in fields {
                    self.collect_returned_closure_captures_rec(field_expr, out);
                }
            }
            Expr::Array { elements, .. } => {
                for elem in elements {
                    self.collect_returned_closure_captures_rec(elem, out);
                }
            }
            Expr::DictLiteral { entries, .. } => {
                for (k, v) in entries {
                    self.collect_returned_closure_captures_rec(k, out);
                    self.collect_returned_closure_captures_rec(v, out);
                }
            }
            Expr::Literal { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::MethodCall { .. }
            | Expr::Call { .. } => {}
        }
    }

    /// Mark the `sink` parameters that a returned closure captures as
    /// consumed.
    ///
    /// A closure captures by value when it is made, so a capture never
    /// outlives the value it copies. A returned closure can therefore
    /// capture a plain parameter, a `mut` parameter, a local `let` and a
    /// module `let`. A `sink` parameter moves into the closure, so the
    /// function cannot use it again.
    pub(super) fn validate_function_return_escape(
        &mut self,
        return_type: Option<&Type>,
        body: &Expr,
    ) {
        // The walk finds closure literals, references to closure
        // bindings, branches and blocks, also inside an aggregate: a
        // struct, a tuple, an array or a dictionary. When no closure
        // hides inside the result, the walk finds nothing.
        let return_carries_aggregate = matches!(
            return_type,
            Some(
                Type::Closure { .. }
                    | Type::Ident(_)
                    | Type::Generic { .. }
                    | Type::Tuple(_)
                    | Type::Array(_)
                    | Type::Optional(_)
                    | Type::Dictionary { .. }
            )
        );
        if !return_carries_aggregate {
            return;
        }
        for (captures, _) in self.collect_returned_closure_captures(body) {
            self.validate_escaping_captures(&captures);
        }
    }

    /// Mark each `sink` parameter in `captures` as consumed: the
    /// closure that holds the captures leaves the function, and the
    /// parameter moves with it. Any other capture is a copy.
    pub(super) fn validate_escaping_captures(&mut self, captures: &[String]) {
        for cap in captures {
            if self.current_fn_param_conventions.get(cap)
                == Some(&crate::ast::ParamConvention::Sink)
            {
                self.consumed_bindings.insert(cap.clone());
            }
        }
    }

    /// Check the declared type of each closure parameter, and that no
    /// name appears twice.
    fn check_closure_params(&mut self, params: &[crate::ast::ClosureParam]) {
        let mut names = HashSet::new();
        for param in params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty, param.span);
            }
            if !names.insert(param.name.name.as_str()) {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!("closure parameter '{}'", param.name.name),
                    span: param.name.span,
                });
            }
        }
    }

    /// Validate a closure expression
    ///
    /// Checks that the closure body does not capture any binding that has
    /// already been consumed by a sink parameter at closure-creation time.
    /// The complementary after-the-fact check — closure created with a live
    /// capture, capture consumed later, then closure invoked — fires at the
    /// invocation site (see the `closure_binding_captures` lookup in the
    /// closure-call branch of `validate_expr_invocation`), so dormant
    /// closures whose captures are consumed but never invoked are tolerated
    /// by design.
    pub(super) fn validate_expr_closure(
        &mut self,
        params: &[crate::ast::ClosureParam],
        return_type: Option<&crate::ast::Type>,
        body: &Expr,
        expected: Option<&crate::semantic::sem_type::SemType>,
        file: &File,
    ) {
        self.check_closure_params(params);
        self.check_closure_parameter_types(params, expected);
        let body_expected = Self::closure_body_expected(return_type, expected);
        if let Some(ty) = return_type {
            self.validate_type(ty, body.span());
        }
        let mut param_scope = HashSet::new();
        for param in params {
            param_scope.insert(param.name.name.clone());
        }
        // Detect closure bodies referencing bindings already consumed by a sink.
        let consumed = self.consumed_bindings.clone();
        let mut inner_scopes: Vec<HashSet<String>> = Vec::new();
        Self::check_captures_rec(
            body,
            &param_scope,
            &consumed,
            &mut self.errors,
            &mut inner_scopes,
        );
        // Give the body the type of each parameter: its annotation, or
        // the type that the expected closure gives it. A call through a
        // parameter then checks against that type, and a parameter
        // shadows an outer binding with the same name.
        let slots = match expected.map(Self::closure_slot) {
            Some(crate::semantic::sem_type::SemType::Closure { params: slots, .. })
                if slots.len() == params.len() =>
            {
                Some(slots)
            }
            _ => None,
        };
        let frame: HashMap<String, crate::semantic::sem_type::SemType> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ty = p.ty.as_ref().map_or_else(
                    || {
                        slots
                            .and_then(|s| s.get(i))
                            .map_or(crate::semantic::sem_type::SemType::Unknown, |(_, t)| {
                                t.clone()
                            })
                    },
                    crate::semantic::sem_type::SemType::from_ast,
                );
                // A name that no type declares is a generic parameter
                // that no substitution reached. Its type is not known.
                let ty = ty.unknown_names_to_unknown(&|n| self.names_a_type(n));
                (p.name.name.clone(), ty)
            })
            .collect();
        self.push_outer_frame(true);
        self.closure_param_scopes.push(param_scope);
        self.inference_scope_stack.borrow_mut().push(frame);
        self.validate_expr_expecting(body, body_expected, file);
        // The body type while the parameters are in scope.
        let body_sem = self.infer_type_sem(body, file);
        self.inference_scope_stack.borrow_mut().pop();
        self.closure_param_scopes.pop();
        self.pop_outer_frame();
        if let Some(crate::semantic::sem_type::SemType::Closure {
            params: slots,
            return_ty,
        }) = expected.map(Self::closure_slot)
        {
            if slots.len() == params.len() {
                self.check_closure_against_slot(
                    params,
                    return_type.is_some(),
                    &body_sem,
                    slots,
                    return_ty,
                    body.span(),
                );
            }
        }

        // when a pipe closure declares a return type, verify the
        // body's inferred type is compatible. Mirrors the function-return
        // mismatch check; reuses `FunctionReturnTypeMismatch` with a
        // synthetic `<closure>` function name since closures don't have one.
        if let Some(declared) = return_type {
            // Push the closure's typed params so the body sees them while
            // inferring (otherwise references like `x + 1` resolve to
            // `SemType::Unknown` and trip a spurious mismatch).
            let mut frame: HashMap<String, crate::semantic::sem_type::SemType> = HashMap::new();
            for p in params {
                if let Some(ty) = &p.ty {
                    frame.insert(
                        p.name.name.clone(),
                        crate::semantic::sem_type::SemType::from_ast(ty),
                    );
                }
            }
            self.inference_scope_stack.borrow_mut().push(frame);
            let body_sem = self.infer_type_sem(body, file);
            self.inference_scope_stack.borrow_mut().pop();
            let body_type = body_sem.display();
            let expected = Self::type_to_string(declared);
            if !self.type_strings_compatible(&expected, &body_type) {
                // cite the body span (the offending expression),
                // not the whole closure-position span — IDE goto-definition
                // and `cargo check` output now point at the wrong return.
                self.errors.push(CompilerError::FunctionReturnTypeMismatch {
                    function: "<closure>".to_string(),
                    expected,
                    actual: body_type,
                    span: body.span(),
                });
            }
        }
    }

    /// Check a closure literal against the closure type that its
    /// position expects: each parameter convention, each declared
    /// parameter type, and the body type when the literal declares no
    /// return type. A slot whose type is not known here is skipped.
    fn check_closure_against_slot(
        &mut self,
        params: &[crate::ast::ClosureParam],
        declares_return: bool,
        body_sem: &crate::semantic::sem_type::SemType,
        slots: &[(
            crate::ast::ParamConvention,
            crate::semantic::sem_type::SemType,
        )],
        return_ty: &crate::semantic::sem_type::SemType,
        span: crate::location::Span,
    ) {
        use crate::semantic::sem_type::SemType;
        let expected = SemType::Closure {
            params: slots.to_vec(),
            return_ty: Box::new(return_ty.clone()),
        };
        let found = || {
            let params = params
                .iter()
                .zip(slots)
                .map(|(p, (_, slot))| {
                    (
                        p.convention,
                        p.ty.as_ref()
                            .map_or_else(|| slot.clone(), SemType::from_ast),
                    )
                })
                .collect();
            SemType::Closure {
                params,
                return_ty: Box::new(body_sem.clone()),
            }
        };
        let known = |t: &SemType| {
            !t.is_indeterminate() && !crate::semantic::inference::holds_an_unbound_type_param(t)
        };
        let param_wrong = params.iter().zip(slots).any(|(p, (convention, slot))| {
            p.convention != *convention
                || p.ty.as_ref().is_some_and(|ty| {
                    let declared = SemType::from_ast(ty);
                    known(slot) && known(&declared) && declared != *slot
                })
        });
        let body_wrong = !declares_return
            && known(return_ty)
            && known(body_sem)
            && body_sem != return_ty
            && !self.value_satisfies_declared(&return_ty.display(), body_sem);
        if param_wrong || body_wrong {
            self.errors.push(CompilerError::TypeMismatch {
                expected: expected.display(),
                found: found().display(),
                span,
            });
        }
    }
}
