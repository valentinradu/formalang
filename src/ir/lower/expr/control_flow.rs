//! Lowering for control-flow expressions: `if`, `for`, `match`, `block`,
//! `let` and pattern destructuring.

use crate::ast::{
    self, BindingPattern, BlockStatement, Expr, Literal, ParamConvention, PrimitiveType,
};
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
        let then_ir = self.lower_expr(then_branch);
        let ty = then_ir.ty().clone();
        IrExpr::If {
            condition: Box::new(self.lower_expr(condition)),
            then_branch: Box::new(then_ir),
            else_branch: else_branch.map(|e| Box::new(self.lower_expr(e))),
            ty,
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
        let var_ty = if let ResolvedType::Array(inner) | ResolvedType::Range(inner) =
            &bad_collection
        {
            (**inner).clone()
        } else {
            self.internal_error_type_if_concrete(
                &bad_collection,
                format!(
                    "for-loop collection lowered to non-iterable type {bad_collection:?}; semantic should have caught this",
                ),
            )
        };
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
            ty: ResolvedType::Array(Box::new(body_ir.ty().clone())),
        }
    }

    pub(super) fn lower_match_expr(
        &mut self,
        scrutinee: &Expr,
        arms: &[crate::ast::MatchArm],
    ) -> IrExpr {
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
                let body = self.lower_expr(&arm.body);
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
        let ir_value = self.lower_expr(value);
        let ir_ty = ty.map(|t| self.lower_type(t));

        let statements: Vec<IrBlockStatement> = match pattern {
            BindingPattern::Simple(ident) => vec![IrBlockStatement::Let {
                binding_id: crate::ir::BindingId(0),
                name: ident.name.clone(),
                mutable,
                ty: ir_ty,
                value: ir_value,
            }],
            BindingPattern::Array { elements, .. } => {
                self.lower_let_array_destructure(elements, mutable, &ir_value)
            }
            BindingPattern::Struct { fields, .. } => {
                self.lower_let_struct_destructure(fields, mutable, &ir_value)
            }
            BindingPattern::Tuple { elements, .. } => {
                self.lower_let_tuple_destructure(elements, mutable, &ir_value)
            }
        };
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
        }
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
        let ir_result = self.lower_expr(result);
        self.local_binding_scopes.pop();
        let ty = ir_result.ty().clone();
        if ir_statements.is_empty() {
            return ir_result;
        }
        IrExpr::Block {
            statements: ir_statements,
            result: Box::new(ir_result),
            ty,
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
                let ir_value = self.lower_expr(value);
                let ir_ty = ty.as_ref().map(|t| self.lower_type(t));
                match pattern {
                    BindingPattern::Simple(ident) => vec![IrBlockStatement::Let {
                        binding_id: crate::ir::BindingId(0),
                        name: ident.name.clone(),
                        mutable: *mutable,
                        ty: ir_ty,
                        value: ir_value,
                    }],
                    BindingPattern::Array { elements, .. } => {
                        self.lower_let_array_destructure(elements, *mutable, &ir_value)
                    }
                    BindingPattern::Struct { fields, .. } => {
                        self.lower_let_struct_destructure(fields, *mutable, &ir_value)
                    }
                    BindingPattern::Tuple { elements, .. } => {
                        self.lower_let_tuple_destructure(elements, *mutable, &ir_value)
                    }
                }
            }
            BlockStatement::Assign { target, value, .. } => {
                vec![IrBlockStatement::Assign {
                    target: self.lower_expr(target),
                    value: self.lower_expr(value),
                }]
            }
            BlockStatement::Expr(expr) => {
                vec![IrBlockStatement::Expr(self.lower_expr(expr))]
            }
        }
    }

    fn lower_let_array_destructure(
        &mut self,
        elements: &[crate::ast::ArrayPatternElement],
        mutable: bool,
        ir_value: &IrExpr,
    ) -> Vec<IrBlockStatement> {
        let bad_recv = ir_value.ty().clone();
        let elem_ty = if let ResolvedType::Array(inner) = &bad_recv {
            (**inner).clone()
        } else {
            self.internal_error_type_if_concrete(
                &bad_recv,
                format!("let array-destructure receiver lowered to non-array type {bad_recv:?}"),
            )
        };
        elements
            .iter()
            .enumerate()
            .filter_map(|(i, elem)| {
                Self::extract_block_binding_name(elem).map(|name| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "array indices are small positions that fit in f64 mantissa"
                    )]
                    let key = IrExpr::Literal {
                        value: Literal::Number((i as f64).into()),
                        ty: ResolvedType::Primitive(PrimitiveType::I32),
                    };
                    IrBlockStatement::Let {
                        binding_id: crate::ir::BindingId(0),
                        name,
                        mutable,
                        ty: Some(elem_ty.clone()),
                        value: IrExpr::DictAccess {
                            dict: Box::new(ir_value.clone()),
                            key: Box::new(key),
                            ty: elem_ty.clone(),
                        },
                    }
                })
            })
            .collect()
    }

    fn lower_let_struct_destructure(
        &mut self,
        fields: &[crate::ast::StructPatternField],
        mutable: bool,
        ir_value: &IrExpr,
    ) -> Vec<IrBlockStatement> {
        fields
            .iter()
            .map(|field| {
                let field_name = field.name.name.clone();
                let binding_name = field
                    .alias
                    .as_ref()
                    .map_or_else(|| field_name.clone(), |a| a.name.clone());
                let field_ty = self.get_field_type_from_resolved(ir_value.ty(), &field_name);
                IrBlockStatement::Let {
                    binding_id: crate::ir::BindingId(0),
                    name: binding_name,
                    mutable,
                    ty: Some(field_ty.clone()),
                    value: IrExpr::FieldAccess {
                        object: Box::new(ir_value.clone()),
                        field: field_name,
                        field_idx: crate::ir::FieldIdx(0),
                        ty: field_ty,
                    },
                }
            })
            .collect()
    }

    fn lower_let_tuple_destructure(
        &mut self,
        elements: &[crate::ast::BindingPattern],
        mutable: bool,
        ir_value: &IrExpr,
    ) -> Vec<IrBlockStatement> {
        let bad_tuple = ir_value.ty().clone();
        let tuple_types = if let ResolvedType::Tuple(fields) = &bad_tuple {
            fields.clone()
        } else {
            let _ = self.internal_error_type_if_concrete(
                &bad_tuple,
                format!("let tuple-destructure receiver lowered to non-tuple type {bad_tuple:?}"),
            );
            Vec::new()
        };
        // The "out-of-range" placeholder is only used if a binding index
        // overshoots the tuple's fields; we lazily build it to avoid
        // pushing a spurious error on every well-formed destructure.
        let out_of_range_ty = if elements.len() > tuple_types.len() && !tuple_types.is_empty() {
            self.internal_error_type(format!(
                "let tuple-destructure binds {} names but receiver has {} fields",
                elements.len(),
                tuple_types.len(),
            ))
        } else {
            ResolvedType::Error
        };
        elements
            .iter()
            .enumerate()
            .filter_map(|(i, elem)| {
                IrLowerer::extract_simple_binding_name(elem).map(|name| {
                    let (field_name, ty) = tuple_types.get(i).map_or_else(
                        || (i.to_string(), out_of_range_ty.clone()),
                        |(n, t)| (n.clone(), t.clone()),
                    );
                    IrBlockStatement::Let {
                        binding_id: crate::ir::BindingId(0),
                        name,
                        mutable,
                        ty: Some(ty.clone()),
                        value: IrExpr::FieldAccess {
                            object: Box::new(ir_value.clone()),
                            field: field_name,
                            field_idx: crate::ir::FieldIdx(0),
                            ty,
                        },
                    }
                })
            })
            .collect()
    }

    fn extract_block_binding_name(elem: &crate::ast::ArrayPatternElement) -> Option<String> {
        match elem {
            crate::ast::ArrayPatternElement::Binding(p) => {
                IrLowerer::extract_simple_binding_name(p)
            }
            crate::ast::ArrayPatternElement::Rest(Some(ident)) => Some(ident.name.clone()),
            crate::ast::ArrayPatternElement::Rest(None)
            | crate::ast::ArrayPatternElement::Wildcard => None,
        }
    }

    pub(super) fn extract_pattern_bindings(
        &mut self,
        pattern: &ast::Pattern,
        scrutinee: &IrExpr,
    ) -> Vec<(String, crate::ir::BindingId, ResolvedType)> {
        match pattern {
            ast::Pattern::Variant { name, bindings } => {
                // Try to find variant field types from the enum
                let variant_fields = self.get_variant_fields(scrutinee.ty(), &name.name);
                let has_overflow = bindings.len() > variant_fields.len();
                // Only emit an error when the scrutinee's type is already a
                // concrete enum; if it's a TypeParam (unresolved path), the
                // overflow is a downstream artefact of the upstream gap.
                let out_of_range_ty = if has_overflow
                    && !matches!(scrutinee.ty(), ResolvedType::TypeParam(_))
                {
                    self.internal_error_type(format!(
                        "match pattern `{}` binds more names ({}) than the variant has fields ({}); semantic should have caught this",
                        name.name,
                        bindings.len(),
                        variant_fields.len(),
                    ))
                } else {
                    ResolvedType::Error
                };
                bindings
                    .iter()
                    .enumerate()
                    .map(|(i, ident)| {
                        let ty = variant_fields
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| out_of_range_ty.clone());
                        (ident.name.clone(), crate::ir::BindingId(0), ty)
                    })
                    .collect()
            }
            ast::Pattern::Wildcard => {
                // Wildcard has no bindings
                Vec::new()
            }
        }
    }
}
