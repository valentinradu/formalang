//! Lowering for control-flow expressions: `if`, `for`, `match`, `block`,
//! `let` and pattern destructuring.

use crate::ast::{self, BindingPattern, BlockStatement, Expr, ParamConvention};
use crate::ir::lower::IrLowerer;
use crate::ir::{IrBlockStatement, IrExpr, IrMatchArm, ResolvedType};
use std::collections::HashMap;

impl IrLowerer<'_> {
    pub(super) fn lower_if_expr(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
    ) -> IrExpr {
        // Both branches sit in tail position, so both inherit the
        // type the context expects. Take it once and re-apply it per
        // branch: `lower_with_expected_value` consumes the slot, so
        // the else branch would otherwise get nothing and an inferred
        // `.variant` there would fail to resolve.
        // A closure-typed context arrives in the other slot; the
        // helper takes both, so the condition sees neither, and each
        // branch gets the whole type: `let f: () -> E = if c { () -> .a }
        // else { () -> .b }` types both closures.
        let expected = self.take_expected_type();
        let then_ir = self.lower_with_expected_value(then_branch, expected.as_ref());
        let ty = then_ir.ty().clone();
        let condition_ir = self.lower_expr(condition);
        let else_ir =
            else_branch.map(|e| Box::new(self.lower_with_expected_value(e, expected.as_ref())));
        IrExpr::If {
            condition: Box::new(condition_ir),
            then_branch: Box::new(then_ir),
            else_branch: else_ir,
            ty,
            span: self.current_ir_span(),
        }
    }

    pub(super) fn lower_for_expr(
        &mut self,
        var: &crate::ast::Ident,
        collection: &Expr,
        body: &Expr,
    ) -> IrExpr {
        let collection_ir = self.lower_expr(collection);
        let bad_collection = collection_ir.ty().clone();
        // For-loops iterate `Array<T>` and `Range<T>`; both are
        // prelude-defined generic structs after the built-in unification.
        let var_ty = self.iterator_element_ty(&bad_collection).unwrap_or_else(|| {
            self.internal_error_type_if_concrete(
                &bad_collection,
                format!(
                    "for-loop collection lowered to non-iterable type {bad_collection:?}; semantic should have caught this",
                ),
            )
        });
        // Make the loop variable visible while lowering the body, so
        // references to `var` inside the body resolve to the iterator
        // element type instead of falling through to UndefinedReference.
        let mut frame = HashMap::new();
        frame.insert(var.name.clone(), (ParamConvention::Let, var_ty.clone()));
        self.local_binding_scopes.push(frame);
        let body_ir = self.lower_expr(body);
        self.local_binding_scopes.pop();
        IrExpr::For {
            var: var.name.clone(),
            var_ty,
            var_binding_id: crate::ir::BindingId(0),
            collection: Box::new(collection_ir),
            body: Box::new(body_ir.clone()),
            ty: self
                .seq_of(body_ir.ty().clone())
                .unwrap_or(ResolvedType::Error),
            span: self.current_ir_span(),
        }
    }

    pub(super) fn lower_match_expr(
        &mut self,
        scrutinee: &Expr,
        arms: &[crate::ast::MatchArm],
    ) -> IrExpr {
        // Every arm body is a tail position; see `lower_if_expr`.
        let expected = self.expected_value_type.take();
        let expected_closure = self.expected_closure_type.take();
        let scrutinee_ir = self.lower_expr(scrutinee);
        let arms_ir: Vec<IrMatchArm> = arms
            .iter()
            .map(|arm| {
                let bindings = self.extract_pattern_bindings(&arm.pattern, &scrutinee_ir);
                // Pattern bindings (e.g. `urgency` from `.high(urgency)`)
                // need to be visible to the arm body. Without this frame
                // the body lowered with the binding as an UndefinedReference.
                let mut frame = HashMap::new();
                for (name, _binding_id, ty) in &bindings {
                    frame.insert(name.clone(), (ParamConvention::Let, ty.clone()));
                }
                self.local_binding_scopes.push(frame);
                self.expected_closure_type.clone_from(&expected_closure);
                let body = self.lower_with_expected_value(&arm.body, expected.as_ref());
                self.local_binding_scopes.pop();
                IrMatchArm {
                    variant: match &arm.pattern {
                        ast::Pattern::Variant { name, .. } => name.name.clone(),
                        ast::Pattern::Wildcard => String::new(),
                    },
                    variant_idx: crate::ir::VariantIdx(0),
                    is_wildcard: matches!(&arm.pattern, ast::Pattern::Wildcard),
                    bindings,
                    body,
                }
            })
            .collect();
        let ty = arms_ir.first().map_or_else(
            || self.internal_error_type("match expression with no arms reached IR lowering".into()),
            |a| a.body.ty().clone(),
        );
        IrExpr::Match {
            scrutinee: Box::new(scrutinee_ir),
            arms: arms_ir,
            ty,
            span: self.current_ir_span(),
        }
    }

    /// Lower a `let pat = val in body` expression into a block with the
    /// binding as one or more statements. Destructuring patterns are
    /// expanded into per-field let statements so the bindings actually
    /// reach the body — previously they collapsed to a single `_let`
    /// binding.
    pub(super) fn lower_let_expr(
        &mut self,
        mutable: bool,
        pattern: &BindingPattern,
        ty: Option<&ast::Type>,
        value: &Expr,
        body: &Expr,
    ) -> IrExpr {
        let ir_ty = ty.map(|t| self.lower_type(t));
        // The annotation is the expected type of the value, as in a
        // block `let`, so a closure literal takes its parameter types.
        let ir_value = self.lower_with_expected_value(value, ir_ty.as_ref());
        let statements = self.pattern_lets(pattern, mutable, ir_ty, ir_value);
        // Make the let-introduced names visible to the body, mirroring
        // `lower_block_expr`. Without this frame, `let x = ... in x` lowered
        // the body with no scope to find `x` in, and the reference fell back
        // to a stringly-typed placeholder.
        self.local_binding_scopes.push(HashMap::new());
        for s in &statements {
            if let IrBlockStatement::Let {
                name,
                mutable,
                ty,
                value,
                ..
            } = s
            {
                let resolved = ty.clone().unwrap_or_else(|| value.ty().clone());
                let convention = if *mutable {
                    ParamConvention::Mut
                } else {
                    ParamConvention::Let
                };
                if let Some(frame) = self.local_binding_scopes.last_mut() {
                    frame.insert(name.clone(), (convention, resolved));
                }
            }
        }
        let ir_body = self.lower_expr(body);
        self.local_binding_scopes.pop();
        let ty = ir_body.ty().clone();
        IrExpr::Block {
            statements,
            result: Box::new(ir_body),
            ty,
            span: self.current_ir_span(),
        }
    }

    /// The block `let` statements for `let pattern: ty = value`. A
    /// simple pattern keeps its annotation. A destructuring one binds
    /// each name to its part of the value; see `destructure`.
    fn pattern_lets(
        &mut self,
        pattern: &BindingPattern,
        mutable: bool,
        ty: Option<ResolvedType>,
        value: IrExpr,
    ) -> Vec<IrBlockStatement> {
        if let BindingPattern::Simple(ident) = pattern {
            return vec![IrBlockStatement::Let {
                binding_id: crate::ir::BindingId(0),
                name: ident.name.clone(),
                mutable,
                ty,
                value,
                span: crate::ir::IrSpan::default(),
            }];
        }
        self.destructure(pattern, value)
            .into_iter()
            .map(|(name, ty, value)| IrBlockStatement::Let {
                binding_id: crate::ir::BindingId(0),
                name,
                mutable,
                ty: Some(ty),
                value,
                span: crate::ir::IrSpan::default(),
            })
            .collect()
    }

    pub(super) fn lower_block_expr(
        &mut self,
        statements: &[BlockStatement],
        result: &Expr,
    ) -> IrExpr {
        // Fresh binding-scope frame so each block `let` is visible to
        // subsequent statements and `result`, then popped so siblings
        // don't see it. Required for accurate receiver types in dispatch
        // rewriting.
        // The result is the block's tail position, so it inherits the
        // expected type. Take it before the statements lower, since any
        // one of them could consume the slot first. A closure type is
        // in the other slot, and a closure in a statement would take it.
        let expected = self.take_expected_type();
        self.local_binding_scopes.push(HashMap::new());
        let mut ir_statements: Vec<IrBlockStatement> = Vec::new();
        for stmt in statements {
            for s in self.lower_block_statement(stmt) {
                if let IrBlockStatement::Let {
                    name,
                    mutable,
                    ty,
                    value,
                    ..
                } = &s
                {
                    let resolved = ty.clone().unwrap_or_else(|| value.ty().clone());
                    let convention = if *mutable {
                        crate::ast::ParamConvention::Mut
                    } else {
                        crate::ast::ParamConvention::Let
                    };
                    if let Some(frame) = self.local_binding_scopes.last_mut() {
                        frame.insert(name.clone(), (convention, resolved));
                    }
                }
                ir_statements.push(s);
            }
        }
        let ir_result = self.lower_with_expected_value(result, expected.as_ref());
        self.local_binding_scopes.pop();
        let ty = ir_result.ty().clone();
        if ir_statements.is_empty() {
            return ir_result;
        }
        IrExpr::Block {
            statements: ir_statements,
            result: Box::new(ir_result),
            ty,
            span: self.current_ir_span(),
        }
    }

    /// Lower an AST block statement to one or more IR block statements.
    pub(super) fn lower_block_statement(&mut self, stmt: &BlockStatement) -> Vec<IrBlockStatement> {
        match stmt {
            BlockStatement::Let {
                mutable,
                pattern,
                ty,
                value,
                ..
            } => {
                let ir_ty = ty.as_ref().map(|t| self.lower_type(t));
                // The annotation is the expected type for the value, so
                // a closure literal picks up its parameter types and an
                // inferred `.variant` finds its enum. Mirrors the
                // module-level path in `lower_simple_let`.
                let ir_value = self.lower_with_expected_value(value, ir_ty.as_ref());
                self.pattern_lets(pattern, *mutable, ir_ty, ir_value)
            }
            BlockStatement::Assign { target, value, .. } => {
                // The target's type is what the value must produce, so
                // it is the value's expected type. Without it a
                // `.variant` on the right has no enum to resolve
                // against: `s = .active` lowered to an unresolved
                // placeholder and was reported as "cannot infer enum
                // type", or reached the monomorphise pass as an
                // internal error.
                let lowered_target = self.lower_expr(target);
                let expected = lowered_target.ty().clone();
                vec![IrBlockStatement::Assign {
                    target: lowered_target,
                    value: self.lower_with_expected_value(value, Some(&expected)),

                    span: crate::ir::IrSpan::default(),
                }]
            }
            BlockStatement::Expr(expr) => {
                vec![IrBlockStatement::Expr(self.lower_expr(expr))]
            }
        }
    }
}
