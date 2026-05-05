use super::unify_type_args;
use crate::ast::Expr;
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

    /// Resolve a `ResolvedType` to its enum type-name (used as the
    /// inferred-enum target for a struct-arg expression). Returns the
    /// empty string for non-enum, non-optional-of-enum types, which
    /// the caller filters out.
    pub(super) fn enum_name_of(module: &crate::ir::IrModule, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Enum(eid) => module
                .get_enum(*eid)
                .map_or_else(String::new, |e| e.name.clone()),
            ResolvedType::Generic {
                base: crate::ir::GenericBase::Enum(eid),
                args,
            } => {
                // Optional<T>: peel to its T so an inferred-enum target on
                // a `String?` field reaches the inner enum's variants.
                if Some(*eid) == module.prelude_optional_id() {
                    if let [t] = args.as_slice() {
                        return Self::enum_name_of(module, t);
                    }
                }
                module
                    .get_enum(*eid)
                    .map_or_else(String::new, |e| e.name.clone())
            }
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => String::new(),
        }
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
        let named_fields: Vec<(String, crate::ir::FieldIdx, IrExpr)> = args
            .iter()
            .filter_map(|(name_opt, expr)| {
                name_opt.as_ref().map(|n| {
                    let saved = self.current_function_return_type.take();
                    let saved_closure = self.expected_closure_type.take();
                    self.current_function_return_type = field_target
                        .get(&n.name)
                        .map(|t| Self::enum_name_of(&self.module, t))
                        .filter(|s| !s.is_empty());
                    // thread closure-typed field annotations
                    // into the closure-literal lowering so untyped params
                    // pick up the field's expected param types.
                    if let Some(t) = field_target.get(&n.name) {
                        if matches!(t, ResolvedType::Closure { .. }) {
                            self.expected_closure_type = Some(t.clone());
                        }
                    }
                    let lowered = self.lower_expr(expr);
                    self.expected_closure_type = saved_closure;
                    self.current_function_return_type = saved;
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
