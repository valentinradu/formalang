//! Enum-instantiation lowering: explicit `Enum::Variant {..}` and the
//! inferred `.Variant {..}` form whose enum target comes from the
//! current return-type context.

use std::collections::HashMap;

use crate::ast::Expr;
use crate::error::CompilerError;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};

impl IrLowerer<'_> {
    pub(in crate::ir::lower::expr) fn lower_enum_instantiation(
        &mut self,
        enum_name: &str,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        let (enum_id, ty) = self.module.enum_id(enum_name).map_or_else(
            || {
                self.try_external_type(enum_name, vec![]).map_or_else(
                    || (None, ResolvedType::TypeParam(enum_name.to_string())),
                    |external_ty| (None, external_ty),
                )
            },
            |id| (Some(id), ResolvedType::Enum(id)),
        );
        IrExpr::EnumInst {
            enum_id,
            variant: variant.to_string(),
            variant_idx: crate::ir::VariantIdx(0),
            fields: data
                .iter()
                .map(|(n, e)| (n.name.clone(), crate::ir::FieldIdx(0), self.lower_expr(e)))
                .collect(),
            ty,
            span: self.current_ir_span(),
        }
    }

    /// Peel `Optional<T>` and report the enum a `.variant` literal
    /// should build, given the type the context expects.
    ///
    /// `Some` for a local enum, a generic enum instantiation, or an
    /// external enum. `None` when the expected type is not an enum at
    /// all — a mismatch the semantic analyzer owns, so lowering must
    /// not raise a second diagnostic for it.
    fn enum_target_of(
        &self,
        expected: &ResolvedType,
    ) -> Option<(Option<crate::ir::EnumId>, ResolvedType)> {
        match expected {
            ResolvedType::Enum(id) => Some((Some(*id), ResolvedType::Enum(*id))),
            ResolvedType::Generic {
                base: crate::ir::GenericBase::Enum(id),
                args,
            } => {
                // `T?` is `Optional<T>`; peel it so `.variant` on an
                // optional-typed slot reaches the inner enum.
                if Some(*id) == self.module.prelude_optional_id() {
                    if let [inner] = args.as_slice() {
                        return self.enum_target_of(inner);
                    }
                }
                Some((Some(*id), expected.clone()))
            }
            ResolvedType::External {
                kind: crate::ir::ImportedKind::Enum,
                ..
            } => Some((None, expected.clone())),
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => None,
        }
    }

    pub(in crate::ir::lower::expr) fn lower_inferred_enum_instantiation(
        &mut self,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        // The context that knows which enum `.variant` belongs to is
        // whatever type the surrounding expression expects: a `let`
        // annotation, an array element slot, a call argument, a struct
        // or enum field, or the enclosing function's return type. Take
        // it — the site that set it restores the previous value.
        let expected = self
            .expected_value_type
            .take()
            .or_else(|| self.current_function_return_type.clone());
        let (enum_id, ty) =
            if let Some(target) = expected.as_ref().and_then(|t| self.enum_target_of(t)) {
                target
            } else {
                // No expected type reaches here, so the enum is genuinely
                // unknowable and the author must annotate. An expected
                // type that is not an enum is a type mismatch the semantic
                // analyzer already reports, so stay quiet about that one
                // and let its diagnostic stand alone.
                if expected.is_none() {
                    self.errors.push(CompilerError::CannotInferEnumType {
                        variant: variant.to_string(),
                        span: self.current_span,
                    });
                }
                (None, ResolvedType::Error)
            };

        // Each variant field lowers against its own declared type, so a
        // nested `.variant` inside the payload resolves too.
        let field_types: HashMap<String, ResolvedType> = enum_id
            .and_then(|id| self.module.get_enum(id))
            .and_then(|e| e.variants.iter().find(|v| v.name == variant))
            .map(|v| {
                v.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let fields = data
            .iter()
            .map(|(n, e)| {
                let lowered =
                    self.lower_with_expected_value(e, field_types.get(&n.name).cloned().as_ref());
                (n.name.clone(), crate::ir::FieldIdx(0), lowered)
            })
            .collect();

        IrExpr::EnumInst {
            enum_id,
            variant: variant.to_string(),
            variant_idx: crate::ir::VariantIdx(0),
            fields,
            ty,
            span: self.current_ir_span(),
        }
    }
}
