//! Container-literal lowering (`Array`, `Tuple`, `DictLiteral`,
//! `DictAccess`) plus the `lower_with_expected` helper that propagates
//! per-element / per-field expected types so closure-literal params
//! pick up annotations from a destructuring let.

use crate::ast::{Expr, PrimitiveType};
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};

impl IrLowerer<'_> {
    /// Lower `expr` with the appropriate expected-type slot set so a
    /// closure literal nested inside `expected` picks up its param types
    /// from the annotation. A direct closure forwards via
    /// `expected_closure_type`; a container forwards via
    /// `expected_value_type` so the next layer can peel and recurse.
    fn lower_with_expected(&mut self, expr: &Expr, expected: Option<&ResolvedType>) -> IrExpr {
        match expected {
            Some(t @ ResolvedType::Closure { .. }) => {
                let saved = self.expected_closure_type.take();
                self.expected_closure_type = Some(t.clone());
                let lowered = self.lower_expr(expr);
                self.expected_closure_type = saved;
                lowered
            }
            Some(t)
                if matches!(t, ResolvedType::Tuple(_))
                    || self.array_element_ty(t).is_some()
                    || self.dictionary_kv_ty(t).is_some() =>
            {
                let saved = self.expected_value_type.take();
                self.expected_value_type = Some(t.clone());
                let lowered = self.lower_expr(expr);
                self.expected_value_type = saved;
                lowered
            }
            _ => self.lower_expr(expr),
        }
    }

    pub(in crate::ir::lower::expr) fn lower_array_expr(&mut self, elements: &[Expr]) -> IrExpr {
        // If the surrounding context supplies an expected aggregate type
        // (e.g. a destructuring let `let [f]: [I32 -> I32] = [|x| x]`),
        // pass the element type down to each element's lowering. A
        // direct `Closure` element forwards via `expected_closure_type`;
        // a nested container (`Array`/`Tuple`/`Dictionary`) forwards via
        // `expected_value_type` so the next layer can peel and continue
        // the search. Without this, un-annotated closure params nested
        // inside container-of-container annotations lower to
        // `ResolvedType::Error`.
        let saved_expected = self.expected_value_type.take();
        let elem_expected: Option<ResolvedType> = saved_expected
            .as_ref()
            .and_then(|t| self.array_element_ty(t));
        let lowered: Vec<IrExpr> = elements
            .iter()
            .map(|e| self.lower_with_expected(e, elem_expected.as_ref()))
            .collect();
        // Empty array literal: type element as `Never` ("no values yet").
        // Matches `nil`'s representation as `Optional(Never)` and lets
        // the existing array-shape compatibility check accept assignment
        // to `let xs: [T] = []`.
        let elem_ty = lowered.first().map_or_else(
            || ResolvedType::Primitive(PrimitiveType::Never),
            |e| e.ty().clone(),
        );
        IrExpr::Array {
            elements: lowered,
            ty: self.array_of(elem_ty).unwrap_or(ResolvedType::Error),
            span: self.current_ir_span(),
        }
    }

    pub(in crate::ir::lower::expr) fn lower_tuple_expr(
        &mut self,
        fields: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        // Like `lower_array_expr`, propagate per-field expected types to
        // closure-literal field values when a destructuring let supplies
        // the aggregate annotation. Nested-container fields forward via
        // `expected_value_type` so a `(a: [I32 -> I32])` annotation
        // reaches the closure inside the array literal.
        let expected_fields: Option<Vec<(String, ResolvedType)>> =
            match self.expected_value_type.take() {
                Some(ResolvedType::Tuple(ts)) => Some(ts),
                _ => None,
            };
        let lowered: Vec<(String, IrExpr)> = fields
            .iter()
            .map(|(n, e)| {
                let expected_field_ty = expected_fields
                    .as_ref()
                    .and_then(|ts| ts.iter().find(|(name, _)| *name == n.name))
                    .map(|(_, t)| t.clone());
                let lowered_e = self.lower_with_expected(e, expected_field_ty.as_ref());
                (n.name.clone(), lowered_e)
            })
            .collect();
        let tuple_types: Vec<(String, ResolvedType)> = lowered
            .iter()
            .map(|(n, e)| (n.clone(), e.ty().clone()))
            .collect();
        IrExpr::Tuple {
            fields: lowered,
            ty: ResolvedType::Tuple(tuple_types),
            span: self.current_ir_span(),
        }
    }

    pub(in crate::ir::lower::expr) fn lower_dict_literal(
        &mut self,
        entries: &[(Expr, Expr)],
    ) -> IrExpr {
        // Like `lower_array_expr` / `lower_tuple_expr`, propagate the
        // `Dictionary { value_ty }` to closure-literal entry values
        // when a destructuring let / annotated context supplies one.
        // Without this, `let d: [String: I32 -> I32] = ["k": |x| x]`
        // produces a closure with `params: [(Let, "x", Error)]`. A
        // nested-container `value_ty` (e.g. `[I32 -> I32]`) is forwarded
        // via `expected_value_type` so the inner array can peel and
        // continue down to the closure.
        let saved_expected = self.expected_value_type.take();
        let value_expected: Option<ResolvedType> = saved_expected
            .as_ref()
            .and_then(|t| self.dictionary_kv_ty(t))
            .map(|(_, v)| v);
        let lowered_entries: Vec<(IrExpr, IrExpr)> = entries
            .iter()
            .map(|(k, v)| {
                let lowered_v = self.lower_with_expected(v, value_expected.as_ref());
                (self.lower_expr(k), lowered_v)
            })
            .collect();
        // Empty dict literal: both type args are `Never`. The
        // shape stays a `Dictionary`, so assignment to `let d: [K: V] = [:]`
        // matches via the existing structural compatibility check.
        let ty = if let Some((k, v)) = lowered_entries.first() {
            self.dictionary_of(k.ty().clone(), v.ty().clone())
                .unwrap_or(ResolvedType::Error)
        } else {
            self.dictionary_of(
                ResolvedType::Primitive(PrimitiveType::Never),
                ResolvedType::Primitive(PrimitiveType::Never),
            )
            .unwrap_or(ResolvedType::Error)
        };
        IrExpr::DictLiteral {
            entries: lowered_entries,
            ty,
            span: self.current_ir_span(),
        }
    }

    pub(in crate::ir::lower::expr) fn lower_dict_access(
        &mut self,
        dict: &Expr,
        key: &Expr,
    ) -> IrExpr {
        let dict_ir = self.lower_expr(dict);
        let key_ir = self.lower_expr(key);
        let receiver_ty = dict_ir.ty().clone();

        // SB-5: `s[i]` on a String receiver desugars to a method call
        // on the prelude's `extern impl String { fn byte_at(self, i: I32) -> I32 }`.
        // Backends only ever see `IrExpr::MethodCall`; no separate
        // primitive-indexing IR shape needed.
        if matches!(receiver_ty, ResolvedType::Primitive(PrimitiveType::String)) {
            let impl_id = self
                .module
                .impls
                .iter()
                .position(|imp| {
                    matches!(
                        imp.target,
                        crate::ir::ImplTarget::Primitive(PrimitiveType::String)
                    ) && imp.functions.iter().any(|f| f.name == "byte_at")
                })
                .map_or(crate::ir::ImplId(0), |idx| {
                    crate::ir::ImplId(u32::try_from(idx).unwrap_or(0))
                });
            return IrExpr::MethodCall {
                receiver: Box::new(dict_ir),
                method: "byte_at".to_string(),
                method_idx: crate::ir::MethodIdx(0),
                args: vec![(None, key_ir)],
                dispatch: crate::ir::DispatchKind::Static { impl_id },
                ty: ResolvedType::Primitive(PrimitiveType::I32),
                span: crate::ir::IrSpan::default(),
            };
        }

        // Both array indexing and dictionary lookup yield an optional
        // (the bound or key may be missing). Lower with `Optional<T>` /
        // `Optional<V>` so the IR signature matches the language
        // semantics; the runtime helper returns the optional cell.
        let ty = if let Some(elem) = self.array_element_ty(&receiver_ty) {
            self.optional_of(elem).unwrap_or(ResolvedType::Error)
        } else if let Some((_, value)) = self.dictionary_kv_ty(&receiver_ty) {
            self.optional_of(value).unwrap_or(ResolvedType::Error)
        } else {
            self.internal_error_type_if_concrete(
                &receiver_ty,
                format!(
                    "dict-access receiver lowered to non-indexable type {receiver_ty:?}; semantic should have caught this",
                ),
            )
        };
        IrExpr::DictAccess {
            dict: Box::new(dict_ir),
            key: Box::new(key_ir),
            ty,
            span: self.current_ir_span(),
        }
    }
}
