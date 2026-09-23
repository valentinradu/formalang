//! Container-literal lowering (`Array`, `Tuple`, `DictLiteral`,
//! `DictAccess`) plus the `lower_with_expected` helper that propagates
//! per-element / per-field expected types so closure-literal params
//! pick up annotations from a destructuring let.

use crate::ast::{Expr, PrimitiveType};
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};

impl IrLowerer<'_> {
    /// Lower `expr` with the appropriate expected-type slot set, so
    /// whatever sits inside `expected` picks up the type the context
    /// declares.
    ///
    /// A closure type goes to `expected_closure_type`, which the
    /// closure lowerer reads for un-annotated params. Every other type
    /// goes to `expected_value_type`: a container peels one layer and
    /// recurses, and an inferred-enum literal resolves `.variant`
    /// against it. An optional closure type goes to both: its closure
    /// to `expected_closure_type` for a closure literal, and the whole
    /// type to `expected_value_type` for `nil` or `.none`.
    /// Take the expected type of the expression that is lowering now,
    /// from whichever slot [`Self::lower_with_expected_value`] put it
    /// in. The value slot holds the whole type when both are set: an
    /// optional closure type.
    pub(in crate::ir::lower) fn take_expected_type(&mut self) -> Option<ResolvedType> {
        let value = self.expected_value_type.take();
        let closure = self.expected_closure_type.take();
        value.or(closure)
    }

    pub(in crate::ir::lower) fn lower_with_expected_value(
        &mut self,
        expr: &Expr,
        expected: Option<&ResolvedType>,
    ) -> IrExpr {
        match expected {
            Some(t @ ResolvedType::Closure { .. }) => {
                let saved = self.expected_closure_type.take();
                self.expected_closure_type = Some(t.clone());
                let lowered = self.lower_expr(expr);
                self.expected_closure_type = saved;
                lowered
            }
            Some(t) => {
                let inner_closure = self
                    .module
                    .optional_inner_ty(t)
                    .filter(|inner| matches!(inner, ResolvedType::Closure { .. }))
                    .cloned();
                let saved = self.expected_value_type.take();
                let saved_closure = self.expected_closure_type.take();
                self.expected_value_type = Some(t.clone());
                self.expected_closure_type = inner_closure;
                let lowered = self.lower_expr(expr);
                self.expected_value_type = saved;
                self.expected_closure_type = saved_closure;
                lowered
            }
            None => self.lower_expr(expr),
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
            .map(|e| self.lower_with_expected_value(e, elem_expected.as_ref()))
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
                let lowered_e = self.lower_with_expected_value(e, expected_field_ty.as_ref());
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
        let kv_expected = saved_expected
            .as_ref()
            .and_then(|t| self.dictionary_kv_ty(t));
        // The key needs the expected type as much as the value does: a
        // `.variant` key has no enum to resolve against without it, and
        // lowering it blind produced a `ResolvedType::Error` that
        // reached the monomorphise pass as an internal error. So
        // `let m: [Status: I32] = [.pending: 1]` told the user to file
        // a bug for a correct program.
        let key_expected = kv_expected.as_ref().map(|(k, _)| k.clone());
        let value_expected = kv_expected.map(|(_, v)| v);
        let lowered_entries: Vec<(IrExpr, IrExpr)> = entries
            .iter()
            .map(|(k, v)| {
                let lowered_k = self.lower_with_expected_value(k, key_expected.as_ref());
                let lowered_v = self.lower_with_expected_value(v, value_expected.as_ref());
                (lowered_k, lowered_v)
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
        let receiver_ty = dict_ir.ty().clone();
        // The key is looked up in this dictionary, so the dictionary's
        // key type is what the key expression must produce. Reading
        // `m[.pending]` without it left the `.variant` with no enum to
        // resolve against, the same way writing `[.pending: 1]` did.
        let key_expected = self.dictionary_kv_ty(&receiver_ty).map(|(k, _)| k);
        let key_ir = self.lower_with_expected_value(key, key_expected.as_ref());

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
