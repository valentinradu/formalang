//! Lowering a destructuring `let` — array, struct and tuple patterns.
//!
//! Split out of `control_flow.rs` to keep each file under the line
//! ceiling that `scripts/check_file_sizes.sh` enforces.

use crate::ast::{self, Literal, PrimitiveType};
use crate::ir::lower::IrLowerer;
use crate::ir::{IrBlockStatement, IrExpr, ResolvedType};

impl IrLowerer<'_> {
    /// The bindings that `pattern` makes from `value`: one
    /// `(name, type, value)` triple for each name, at any depth.
    ///
    /// An array element reads `value[i]`, a tuple or struct field reads
    /// `value.field`, and a nested pattern destructures that read in
    /// turn. A rest binding, `...rest`, holds the elements after the
    /// ones before it: `for e in value { e }.skip(count: i).collect()`.
    pub(in crate::ir::lower) fn destructure(
        &mut self,
        pattern: &ast::BindingPattern,
        value: IrExpr,
    ) -> Vec<(String, ResolvedType, IrExpr)> {
        match pattern {
            ast::BindingPattern::Simple(ident) => {
                vec![(ident.name.clone(), value.ty().clone(), value)]
            }
            ast::BindingPattern::Array { elements, .. } => self.destructure_array(elements, &value),
            ast::BindingPattern::Struct { fields, .. } => fields
                .iter()
                .map(|field| {
                    let field_name = field.name.name.clone();
                    let binding_name = field
                        .alias
                        .as_ref()
                        .map_or_else(|| field_name.clone(), |a| a.name.clone());
                    let field_ty = self.get_field_type_from_resolved(value.ty(), &field_name);
                    let access = IrExpr::FieldAccess {
                        object: Box::new(value.clone()),
                        field: field_name,
                        field_idx: crate::ir::FieldIdx(0),
                        ty: field_ty.clone(),
                        span: self.current_ir_span(),
                    };
                    (binding_name, field_ty, access)
                })
                .collect(),
            ast::BindingPattern::Tuple { elements, .. } => self.destructure_tuple(elements, &value),
        }
    }

    fn destructure_array(
        &mut self,
        elements: &[ast::ArrayPatternElement],
        value: &IrExpr,
    ) -> Vec<(String, ResolvedType, IrExpr)> {
        let bad_recv = value.ty().clone();
        let elem_ty = self.array_element_ty(&bad_recv).unwrap_or_else(|| {
            self.internal_error_type_if_concrete(
                &bad_recv,
                format!("array-destructuring receiver lowered to non-array type {bad_recv:?}"),
            )
        });
        // A rest element takes the elements between the names before
        // it and the names after it. A name after the rest reads its
        // index from the end: `value[value.len() - k]`.
        let rest_at = elements
            .iter()
            .position(|e| matches!(e, ast::ArrayPatternElement::Rest(_)));
        let after_rest = rest_at.map_or(0, |r| elements.len().saturating_sub(r).saturating_sub(1));
        let mut out = Vec::new();
        for (i, element) in elements.iter().enumerate() {
            match element {
                ast::ArrayPatternElement::Binding(inner) => {
                    let key = match rest_at {
                        Some(r) if i > r => {
                            let from_end = elements.len().saturating_sub(i);
                            let key = self.len_minus(from_end);
                            self.lower_over_source(value, &key)
                        }
                        _ => self.index_literal(i),
                    };
                    let access = IrExpr::DictAccess {
                        dict: Box::new(value.clone()),
                        key: Box::new(key),
                        ty: elem_ty.clone(),
                        span: self.current_ir_span(),
                    };
                    out.extend(self.destructure(inner, access));
                }
                ast::ArrayPatternElement::Rest(Some(ident)) => {
                    let rest = self.lower_rest(value, i, after_rest);
                    out.push((ident.name.clone(), rest.ty().clone(), rest));
                }
                ast::ArrayPatternElement::Rest(None) | ast::ArrayPatternElement::Wildcard => {}
            }
        }
        out
    }

    /// An `I32` literal for the array index `i`.
    fn index_literal(&self, i: usize) -> IrExpr {
        #[expect(
            clippy::cast_precision_loss,
            reason = "array destructuring indices are small source positions that fit in f64 mantissa"
        )]
        let value = Literal::Number((i as f64).into());
        IrExpr::Literal {
            value,
            ty: ResolvedType::Primitive(PrimitiveType::I32),
            span: self.current_ir_span(),
        }
    }

    fn destructure_tuple(
        &mut self,
        elements: &[ast::BindingPattern],
        value: &IrExpr,
    ) -> Vec<(String, ResolvedType, IrExpr)> {
        let bad_recv = value.ty().clone();
        let tuple_types = if let ResolvedType::Tuple(fields) = &bad_recv {
            fields.clone()
        } else {
            let _ = self.internal_error_type_if_concrete(
                &bad_recv,
                format!("tuple-destructuring receiver lowered to non-tuple type {bad_recv:?}"),
            );
            Vec::new()
        };
        let mut out = Vec::new();
        for (i, element) in elements.iter().enumerate() {
            let (field_name, ty) = if let Some((n, t)) = tuple_types.get(i) {
                (n.clone(), t.clone())
            } else {
                let ty = self.internal_error_type_if_concrete(
                    &bad_recv,
                    format!(
                        "tuple-destructuring pattern binds {} names but the receiver has {} fields",
                        elements.len(),
                        tuple_types.len()
                    ),
                );
                (i.to_string(), ty)
            };
            let access = IrExpr::FieldAccess {
                object: Box::new(value.clone()),
                field: field_name,
                field_idx: crate::ir::FieldIdx(0),
                ty,
                span: self.current_ir_span(),
            };
            out.extend(self.destructure(element, access));
        }
        out
    }

    /// The name that holds the array while a rest or an index from the
    /// end is computed. A program cannot write it, so it hides nothing.
    const SOURCE: &'static str = "rest#source";

    /// The AST of `source.len() - k`, where `source` is the array.
    fn len_minus(&self, k: usize) -> ast::Expr {
        let span = self.current_span;
        #[expect(
            clippy::cast_precision_loss,
            reason = "array destructuring indices are small source positions that fit in f64 mantissa"
        )]
        let count = ast::Expr::Literal {
            value: Literal::Number((k as f64).into()),
            span,
        };
        ast::Expr::BinaryOp {
            left: Box::new(ast::Expr::MethodCall {
                receiver: Box::new(ast::Expr::Reference {
                    path: vec![ast::Ident::new(Self::SOURCE, span)],
                    span,
                }),
                method: ast::Ident::new("len", span),
                args: Vec::new(),
                span,
            }),
            op: ast::BinaryOperator::Sub,
            right: Box::new(count),
            span,
        }
    }

    /// The elements of the array `value` after the first `skip`, less
    /// the last `keep_last`:
    /// `for e in source { e }.skip(count: skip).take(count: source.len() - (skip + keep_last)).collect()`.
    fn lower_rest(&mut self, value: &IrExpr, skip: usize, keep_last: usize) -> IrExpr {
        const ELEMENT: &str = "rest#element";
        let span = self.current_span;
        let ident = |name: &str| ast::Ident::new(name, span);
        let reference = |name: &str| ast::Expr::Reference {
            path: vec![ident(name)],
            span,
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "array destructuring indices are small source positions that fit in f64 mantissa"
        )]
        let count = ast::Expr::Literal {
            value: Literal::Number((skip as f64).into()),
            span,
        };
        let mut sequence = ast::Expr::MethodCall {
            receiver: Box::new(ast::Expr::ForExpr {
                var: ident(ELEMENT),
                collection: Box::new(reference(Self::SOURCE)),
                body: Box::new(reference(ELEMENT)),
                span,
            }),
            method: ident("skip"),
            args: vec![(Some(ident("count")), count)],
            span,
        };
        if keep_last > 0 {
            sequence = ast::Expr::MethodCall {
                receiver: Box::new(sequence),
                method: ident("take"),
                args: vec![(
                    Some(ident("count")),
                    self.len_minus(skip.saturating_add(keep_last)),
                )],
                span,
            };
        }
        let pipeline = ast::Expr::MethodCall {
            receiver: Box::new(sequence),
            method: ident("collect"),
            args: Vec::new(),
            span,
        };
        self.lower_over_source(value, &pipeline)
    }

    /// Lower `expr` in a block that first binds the array `value` to
    /// the source name: `{ let source = value; expr }`.
    fn lower_over_source(&mut self, value: &IrExpr, expr: &ast::Expr) -> IrExpr {
        let source_ty = value.ty().clone();
        let mut frame = std::collections::HashMap::new();
        frame.insert(
            Self::SOURCE.to_string(),
            (ast::ParamConvention::Let, source_ty.clone()),
        );
        self.local_binding_scopes.push(frame);
        let lowered = self.lower_expr(expr);
        self.local_binding_scopes.pop();
        let ty = lowered.ty().clone();
        IrExpr::Block {
            statements: vec![IrBlockStatement::Let {
                binding_id: crate::ir::BindingId(0),
                name: Self::SOURCE.to_string(),
                mutable: false,
                ty: Some(source_ty),
                value: value.clone(),
                span: self.current_ir_span(),
            }],
            result: Box::new(lowered),
            ty,
            span: self.current_ir_span(),
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
