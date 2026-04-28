//! Destructuring let lowering: array, struct, and tuple patterns.
//!
//! Each destructuring pattern is expanded into one [`IrLet`] per
//! introduced name, with the synthesised access expression
//! (`arr[0]` / `tuple.field` / `struct.field`) as the value.
//!
//! The annotation-threading rule mirrors `lower_simple_let`: when the
//! let carries a type annotation, the value is lowered with
//! [`super::IrLowerer::expected_value_type`] set so closure literals
//! inside the value pick up their param types from the annotation
//! instead of falling back to [`ResolvedType::Error`].

use super::IrLowerer;
use crate::ast::{self, BindingPattern, LetBinding, Literal, PrimitiveType};
use crate::ir::{IrExpr, IrLet, ResolvedType};

impl IrLowerer<'_> {
    /// Lower an array destructuring let binding: `let [a, b, c] = value`.
    pub(super) fn lower_array_destructuring_let(
        &mut self,
        let_binding: &LetBinding,
        elements: &[ast::ArrayPatternElement],
    ) {
        let saved_expected = self.expected_value_type.take();
        self.expected_value_type = let_binding
            .type_annotation
            .as_ref()
            .map(|t| self.lower_type(t))
            .filter(|t| matches!(t, ResolvedType::Array(_)));
        let value_expr = self.lower_expr(&let_binding.value);
        self.expected_value_type = saved_expected;

        let bad_recv = value_expr.ty().clone();
        let elem_ty = if let ResolvedType::Array(inner) = &bad_recv {
            (**inner).clone()
        } else {
            self.internal_error_type_if_concrete(
                &bad_recv,
                format!("array-destructuring let receiver lowered to non-array type {bad_recv:?}"),
            )
        };
        for (i, element) in elements.iter().enumerate() {
            if let Some(name) = Self::extract_array_pattern_name(element) {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "array destructuring indices are small source-code positions that fit exactly in f64 mantissa"
                )]
                let index_key = IrExpr::Literal {
                    value: Literal::Number((i as f64).into()),
                    ty: ResolvedType::Primitive(PrimitiveType::I32),
                };
                let access_expr = IrExpr::DictAccess {
                    dict: Box::new(value_expr.clone()),
                    key: Box::new(index_key),
                    ty: elem_ty.clone(),
                };
                self.module.add_let(IrLet {
                    name,
                    visibility: let_binding.visibility,
                    mutable: let_binding.mutable,
                    ty: elem_ty.clone(),
                    value: access_expr,
                    doc: let_binding.doc.clone(),
                });
            }
        }
    }

    /// Lower a struct destructuring let binding:
    /// `let { field, other: alias } = value`.
    pub(super) fn lower_struct_destructuring_let(
        &mut self,
        let_binding: &LetBinding,
        fields: &[ast::StructPatternField],
    ) {
        let value_expr = self.lower_expr(&let_binding.value);
        for field in fields {
            let field_name = field.name.name.clone();
            let binding_name = field
                .alias
                .as_ref()
                .map_or_else(|| field_name.clone(), |a| a.name.clone());
            let field_ty = self.get_field_type_from_resolved(value_expr.ty(), &field_name);
            let access_expr = IrExpr::FieldAccess {
                object: Box::new(value_expr.clone()),
                field: field_name,
                field_idx: crate::ir::FieldIdx(0),
                ty: field_ty.clone(),
            };
            self.module.add_let(IrLet {
                name: binding_name,
                visibility: let_binding.visibility,
                mutable: let_binding.mutable,
                ty: field_ty,
                value: access_expr,
                doc: let_binding.doc.clone(),
            });
        }
    }

    /// Lower a tuple destructuring let binding: `let (a, b) = value`.
    pub(super) fn lower_tuple_destructuring_let(
        &mut self,
        let_binding: &LetBinding,
        elements: &[BindingPattern],
    ) {
        let saved_expected = self.expected_value_type.take();
        self.expected_value_type = let_binding
            .type_annotation
            .as_ref()
            .map(|t| self.lower_type(t))
            .filter(|t| matches!(t, ResolvedType::Tuple(_)));
        let value_expr = self.lower_expr(&let_binding.value);
        self.expected_value_type = saved_expected;

        let bad_recv = value_expr.ty().clone();
        let tuple_types = if let ResolvedType::Tuple(fields) = &bad_recv {
            fields.clone()
        } else {
            let _ = self.internal_error_type_if_concrete(
                &bad_recv,
                format!("tuple-destructuring let receiver lowered to non-tuple type {bad_recv:?}"),
            );
            Vec::new()
        };
        let overflow_ty = if elements.len() > tuple_types.len() && !tuple_types.is_empty() {
            self.internal_error_type(format!(
                "tuple-destructuring pattern binds {} names but the receiver has {} fields",
                elements.len(),
                tuple_types.len(),
            ))
        } else {
            ResolvedType::Error
        };
        for (i, element) in elements.iter().enumerate() {
            if let Some(name) = Self::extract_simple_binding_name(element) {
                let (field_name, ty) = tuple_types.get(i).map_or_else(
                    || (i.to_string(), overflow_ty.clone()),
                    |(n, t)| (n.clone(), t.clone()),
                );
                let access_expr = IrExpr::FieldAccess {
                    object: Box::new(value_expr.clone()),
                    field: field_name,
                    field_idx: crate::ir::FieldIdx(0),
                    ty: ty.clone(),
                };
                self.module.add_let(IrLet {
                    name,
                    visibility: let_binding.visibility,
                    mutable: let_binding.mutable,
                    ty,
                    value: access_expr,
                    doc: let_binding.doc.clone(),
                });
            }
        }
    }

    fn extract_array_pattern_name(element: &ast::ArrayPatternElement) -> Option<String> {
        match element {
            ast::ArrayPatternElement::Binding(pattern) => {
                Self::extract_simple_binding_name(pattern)
            }
            ast::ArrayPatternElement::Rest(Some(ident)) => Some(ident.name.clone()),
            ast::ArrayPatternElement::Rest(None) | ast::ArrayPatternElement::Wildcard => None,
        }
    }

    /// Extract binding name from a simple binding pattern.
    pub(super) fn extract_simple_binding_name(pattern: &BindingPattern) -> Option<String> {
        match pattern {
            BindingPattern::Simple(ident) => Some(ident.name.clone()),
            BindingPattern::Array { .. }
            | BindingPattern::Struct { .. }
            | BindingPattern::Tuple { .. } => None,
        }
    }
}
