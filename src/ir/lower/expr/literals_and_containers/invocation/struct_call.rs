use super::unify_type_args;
use crate::ast::Expr;
use crate::ir::lower::expr::type_params::substitute_typeparam_in_resolved;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};
use std::collections::HashMap;

impl IrLowerer<'_> {
    /// Infer the type arguments for a generic struct constructor invoked
    /// without explicit `<...>`. Walks each generic parameter, finds a
    /// field whose declared type mentions the param, and unifies it
    /// against the corresponding lowered argument's type. Returns the
    /// inferred argument vector when every parameter is bound, or an
    /// empty vector when inference can't cover all of them (in which
    /// case the caller falls back to the bare struct type).
    pub(super) fn infer_struct_type_args(
        &self,
        struct_id: crate::ir::StructId,
        field_target: &HashMap<String, ResolvedType>,
        named_fields: &[(String, crate::ir::FieldIdx, IrExpr)],
    ) -> Vec<ResolvedType> {
        let Some(struct_def) = self.module.get_struct(struct_id) else {
            return Vec::new();
        };
        if struct_def.generic_params.is_empty() {
            return Vec::new();
        }
        let param_names: Vec<String> = struct_def
            .generic_params
            .iter()
            .map(|p| p.name.clone())
            .collect();
        let mut bindings: HashMap<String, ResolvedType> = HashMap::new();
        for (arg_name, _, lowered) in named_fields {
            let Some(declared) = field_target.get(arg_name) else {
                continue;
            };
            unify_type_args(declared, lowered.ty(), &mut bindings);
        }
        let mut resolved = Vec::with_capacity(param_names.len());
        for name in &param_names {
            match bindings.get(name) {
                Some(ty) => resolved.push(ty.clone()),
                None => return Vec::new(),
            }
        }
        resolved
    }

    pub(super) fn lower_struct_invocation(
        &mut self,
        struct_id: crate::ir::StructId,
        type_args_resolved: Vec<ResolvedType>,
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        // build a name->type-name map of the
        // struct's fields so each named-arg lowers with the field's
        // declared type as the inferred-enum target. Without this,
        // `Size(width: .auto)` inherits whatever outer
        // `current_function_return_type` was set to and `.auto` can't
        // resolve.
        let field_target: HashMap<String, ResolvedType> = self
            .module
            .get_struct(struct_id)
            .map(|s| {
                s.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect()
            })
            .unwrap_or_default();
        // With `<...>` written, a field of a generic type expects the
        // written type: in `Form<Event>(onChange: (x) -> .changed)`, the
        // closure returns `Event`, not `E`.
        let subs = self.struct_type_arg_subs(struct_id, &type_args_resolved);
        let mut named_fields: Vec<(String, crate::ir::FieldIdx, IrExpr)> = args
            .iter()
            .filter_map(|(name_opt, expr)| {
                name_opt.as_ref().map(|n| {
                    let expected = field_target.get(&n.name).cloned().map(|mut ty| {
                        substitute_typeparam_in_resolved(&mut ty, &subs);
                        ty
                    });
                    let lowered = self.lower_with_expected_value(expr, expected.as_ref());
                    (n.name.clone(), crate::ir::FieldIdx(0), lowered)
                })
            })
            .collect();
        // Infer type args when the call site omits them. Walks each
        // generic parameter, finds the first struct field whose
        // declared type mentions the param, and unifies it against
        // the corresponding lowered arg's type to recover a concrete
        // binding (e.g. `Box(value: 7)` infers `Box<I32>`). Falls
        // back to the bare struct type when inference can't fill
        // every param.
        let inferred_type_args: Vec<ResolvedType> = if type_args_resolved.is_empty() {
            self.infer_struct_type_args(struct_id, &field_target, &named_fields)
        } else {
            type_args_resolved
        };
        // An optional field with no default that the call leaves out is
        // `nil`. The literal holds it, as if the call wrote
        // `field: nil`, so a backend reads each optional field.
        let optional_id = self.module.prelude_optional_id();
        let is_optional = |f: &crate::ir::IrField| {
            f.optional
                || matches!(
                    f.ty,
                    ResolvedType::Generic {
                        base: crate::ir::GenericBase::Enum(id),
                        ..
                    } if Some(id) == optional_id
                )
        };
        let left_out: Vec<(String, ResolvedType)> = self
            .module
            .get_struct(struct_id)
            .map(|s| {
                s.fields
                    .iter()
                    .filter(|f| is_optional(f) && f.default.is_none())
                    .filter(|f| named_fields.iter().all(|(n, _, _)| *n != f.name))
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let inferred_subs = self.struct_type_arg_subs(struct_id, &inferred_type_args);
        for (name, mut ty) in left_out {
            substitute_typeparam_in_resolved(&mut ty, &inferred_subs);
            let nil = Expr::Literal {
                value: crate::ast::Literal::Nil,
                span: self.current_span,
            };
            let lowered = self.lower_with_expected_value(&nil, Some(&ty));
            named_fields.push((name, crate::ir::FieldIdx(0), lowered));
        }
        let ty = if inferred_type_args.is_empty() {
            ResolvedType::Struct(struct_id)
        } else {
            ResolvedType::Generic {
                base: crate::ir::GenericBase::Struct(struct_id),
                args: inferred_type_args.clone(),
            }
        };
        IrExpr::StructInst {
            struct_id: Some(struct_id),
            type_args: inferred_type_args,
            fields: named_fields,
            ty,
            span: self.current_ir_span(),
        }
    }

    /// The map from each type parameter of the struct to its type
    /// argument. Empty when the count of arguments does not match.
    fn struct_type_arg_subs(
        &self,
        struct_id: crate::ir::StructId,
        type_args: &[ResolvedType],
    ) -> HashMap<String, ResolvedType> {
        self.module
            .get_struct(struct_id)
            .filter(|s| s.generic_params.len() == type_args.len())
            .map(|s| {
                s.generic_params
                    .iter()
                    .zip(type_args)
                    .map(|(p, a)| (p.name.clone(), a.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn lower_external_invocation(
        &mut self,
        external_ty: ResolvedType,
        type_args_resolved: Vec<ResolvedType>,
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let named_fields: Vec<(String, crate::ir::FieldIdx, IrExpr)> = args
            .iter()
            .filter_map(|(name_opt, expr)| {
                name_opt.as_ref().map(|n| {
                    (
                        n.name.clone(),
                        crate::ir::FieldIdx(0),
                        self.lower_expr(expr),
                    )
                })
            })
            .collect();
        IrExpr::StructInst {
            struct_id: None,
            type_args: type_args_resolved,
            fields: named_fields,
            ty: external_ty,
            span: self.current_ir_span(),
        }
    }
}
