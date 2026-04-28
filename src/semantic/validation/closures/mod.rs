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
            | Expr::MethodCall { .. } => {}
        }
    }

    /// If `return_type` is a closure type, verify that every closure returned
    /// by `body` only captures bindings that outlive the function: outer-scope
    /// bindings (module-level or wider) and `sink` parameters. Local `let`
    /// bindings and `let`/`mut` parameters would die with the function frame
    /// and leave a dangling capture.
    pub(super) fn validate_function_return_escape(
        &mut self,
        return_type: Option<&Type>,
        body: &Expr,
    ) {
        // The legacy fast-path: function returns a closure type directly
        // (`fn make() -> () -> I32`). The recursive walk handles every
        // concrete return shape — closure literals, references to
        // closure bindings, branches, blocks. Tier-1 escape extension
        // also fires on aggregate returns (struct / enum / tuple /
        // array / dict): walking those is harmless when no closure
        // hides inside them, since `collect_returned_closure_captures`
        // simply returns an empty list.
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
        let escaping = self.collect_returned_closure_captures(body);
        if escaping.is_empty() {
            return;
        }
        for (captures, span) in escaping {
            self.validate_escaping_captures(&captures, span);
        }
    }

    /// Shared rule for "this closure value escapes the function frame".
    ///
    /// Captures are valid only when they refer to:
    ///
    /// - a `sink` parameter (ownership transfers into the closure;
    ///   binding is marked consumed),
    /// - a module-level `let` (outlives the function).
    ///
    /// `let`/`mut` parameters and function-local `let` bindings die
    /// with the frame and produce
    /// [`CompilerError::ClosureCaptureEscapesLocalBinding`].
    pub(super) fn validate_escaping_captures(&mut self, captures: &[String], span: Span) {
        let param_convs = self.current_fn_param_conventions.clone();
        for cap in captures {
            if let Some(convention) = param_convs.get(cap) {
                match convention {
                    crate::ast::ParamConvention::Sink => {
                        self.consumed_bindings.insert(cap.clone());
                    }
                    crate::ast::ParamConvention::Let | crate::ast::ParamConvention::Mut => {
                        self.errors
                            .push(CompilerError::ClosureCaptureEscapesLocalBinding {
                                binding: cap.clone(),
                                span,
                            });
                    }
                }
            } else if self.symbols.is_let(cap) {
                // Module-level let — outlives the function. OK.
            } else {
                // Function-local `let` (or any other shorter-lifetime
                // binding the block scope has popped by now). Dies
                // with the frame.
                self.errors
                    .push(CompilerError::ClosureCaptureEscapesLocalBinding {
                        binding: cap.clone(),
                        span,
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
        file: &File,
    ) {
        for param in params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty);
            }
        }
        if let Some(ty) = return_type {
            self.validate_type(ty);
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
        self.closure_param_scopes.push(param_scope);
        self.validate_expr(body, file);
        self.closure_param_scopes.pop();

        // when a pipe closure declares a return type, verify the
        // body's inferred type is compatible. Mirrors the function-return
        // mismatch check; reuses `FunctionReturnTypeMismatch` with a
        // synthetic `<closure>` function name since closures don't have one.
        if let Some(declared) = return_type {
            // Push the closure's typed params so the body sees them while
            // inferring (otherwise references like `x + 1` resolve to
            // `Unknown` and trip a spurious mismatch).
            let mut frame = HashMap::new();
            for p in params {
                if let Some(ty) = &p.ty {
                    frame.insert(p.name.name.clone(), Self::type_to_string(ty));
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
}
