//! Enum-instantiation lowering: explicit `Enum::Variant {..}` and the
//! inferred `.Variant {..}` form whose enum target comes from the
//! current return-type context.

use crate::ast::Expr;
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

    pub(in crate::ir::lower::expr) fn lower_inferred_enum_instantiation(
        &mut self,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        // Inferred-enum uses outside a return-typed context (struct field
        // defaults, top-level lets) are a known gap; leave a TypeParam
        // placeholder — context-threading work upstream will surface it.
        #[expect(
            clippy::option_if_let_else,
            reason = "three-branch resolution (local enum / external / error) reads clearer as if/else"
        )]
        let (enum_id, ty) = match self.current_function_return_type.clone() {
            None => (None, ResolvedType::TypeParam("InferredEnum".to_string())),
            Some(name) => {
                if let Some(id) = self.module.enum_id(&name) {
                    (Some(id), ResolvedType::Enum(id))
                } else if let Some(external_ty) = self.try_external_type(&name, vec![]) {
                    (None, external_ty)
                } else {
                    (
                        None,
                        self.internal_error_type(format!(
                            "inferred-enum `.{variant}` has no resolvable return-type enum `{name}`",
                        )),
                    )
                }
            }
        };
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
}
