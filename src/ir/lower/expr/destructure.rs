//! Lowering a destructuring `let` — array, struct and tuple patterns.
//!
//! Split out of `control_flow.rs` to keep each file under the line
//! ceiling that `scripts/check_file_sizes.sh` enforces.

use crate::ast::{self, Literal, PrimitiveType};
use crate::ir::lower::IrLowerer;
use crate::ir::{IrBlockStatement, IrExpr, ResolvedType};

impl IrLowerer<'_> {
    pub(super) fn lower_let_array_destructure(
        &mut self,
        elements: &[crate::ast::ArrayPatternElement],
        mutable: bool,
        ir_value: &IrExpr,
    ) -> Vec<IrBlockStatement> {
        let bad_recv = ir_value.ty().clone();
        let elem_ty = self.array_element_ty(&bad_recv).unwrap_or_else(|| {
            self.internal_error_type_if_concrete(
                &bad_recv,
                format!("let array-destructure receiver lowered to non-array type {bad_recv:?}"),
            )
        });
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
                        span: self.current_ir_span(),
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
                            span: self.current_ir_span(),
                        },

                        span: crate::ir::IrSpan::default(),
                    }
                })
            })
            .collect()
    }

    pub(super) fn lower_let_struct_destructure(
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
                        span: self.current_ir_span(),
                    },

                    span: crate::ir::IrSpan::default(),
                }
            })
            .collect()
    }

    pub(super) fn lower_let_tuple_destructure(
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
                            span: self.current_ir_span(),
                        },

                        span: crate::ir::IrSpan::default(),
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
