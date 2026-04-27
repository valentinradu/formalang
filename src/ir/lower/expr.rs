//! Expression lowering helpers for the IR lowering pass.

use super::IrLowerer;
use crate::ast::{
    self, BinaryOperator, BindingPattern, BlockStatement, ClosureParam, Expr, Literal,
    ParamConvention, PrimitiveType, UnaryOperator,
};
use crate::error::CompilerError;
use crate::ir::{
    DispatchKind, ImplId, IrBlockStatement, IrExpr, IrMatchArm, ResolvedType, TraitId,
};
use std::collections::HashMap;

/// Substitute `TypeParam(name)` references inside `ty` using `subs`.
/// Used by `resolve_method_return_type` when the receiver is a
/// `Generic { base, args }` so the impl method's return type
/// (declared in terms of the struct's generic params) gets the
/// concrete instantiation's type arguments.
fn substitute_typeparam_in_resolved(ty: &mut ResolvedType, subs: &HashMap<String, ResolvedType>) {
    match ty {
        ResolvedType::TypeParam(name) => {
            if let Some(concrete) = subs.get(name) {
                *ty = concrete.clone();
            }
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            substitute_typeparam_in_resolved(inner, subs);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                substitute_typeparam_in_resolved(t, subs);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            substitute_typeparam_in_resolved(key_ty, subs);
            substitute_typeparam_in_resolved(value_ty, subs);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                substitute_typeparam_in_resolved(t, subs);
            }
            substitute_typeparam_in_resolved(return_ty, subs);
        }
        ResolvedType::Generic { args, .. } => {
            for a in args {
                substitute_typeparam_in_resolved(a, subs);
            }
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                substitute_typeparam_in_resolved(a, subs);
            }
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Error => {}
    }
}

impl IrLowerer<'_> {
    pub(super) fn lower_expr(&mut self, expr: &Expr) -> IrExpr {
        // Track the span of the current expression so that InternalError
        // diagnostics surfaced during lowering carry a real source
        // location. See audit finding #31.
        let prev_span = self.current_span;
        self.current_span = expr.span();
        let out = self.lower_expr_inner(expr);
        self.current_span = prev_span;
        out
    }

    fn lower_expr_inner(&mut self, expr: &Expr) -> IrExpr {
        match expr {
            Expr::Literal { value: lit, .. } => IrExpr::Literal {
                value: lit.clone(),
                ty: Self::literal_type(lit),
            },
            Expr::Invocation {
                path,
                type_args,
                args,
                ..
            } => self.lower_invocation(path, type_args, args),
            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                ..
            } => self.lower_enum_instantiation(&enum_name.name, &variant.name, data),
            Expr::InferredEnumInstantiation { variant, data, .. } => {
                self.lower_inferred_enum_instantiation(&variant.name, data)
            }
            Expr::Array { elements, .. } => self.lower_array_expr(elements),
            Expr::Tuple { fields, .. } => self.lower_tuple_expr(fields),
            Expr::Reference { path, .. } => self.lower_reference(path),
            Expr::BinaryOp {
                left, op, right, ..
            } => self.lower_binary_op_expr(left, *op, right),
            Expr::UnaryOp { op, operand, .. } => self.lower_unary_op_expr(*op, operand),
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.lower_if_expr(condition, then_branch, else_branch.as_deref()),
            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => self.lower_for_expr(var, collection, body),
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => self.lower_match_expr(scrutinee, arms),
            Expr::Group { expr, .. } => self.lower_expr(expr),
            Expr::LetExpr {
                mutable,
                pattern,
                ty,
                value,
                body,
                ..
            } => self.lower_let_expr(*mutable, pattern, ty.as_ref(), value, body),
            Expr::DictLiteral { entries, .. } => self.lower_dict_literal(entries),
            Expr::DictAccess { dict, key, .. } => self.lower_dict_access(dict, key),
            Expr::ClosureExpr {
                params,
                return_type,
                body,
                ..
            } => self.lower_closure(params, return_type.as_ref(), body),
            Expr::FieldAccess { object, field, .. } => {
                let object_ir = self.lower_expr(object);
                let ty = self.resolve_field_type(object_ir.ty(), &field.name);
                IrExpr::FieldAccess {
                    object: Box::new(object_ir),
                    field: field.name.clone(),
                    ty,
                }
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => self.lower_method_call(receiver, &method.name, args.as_slice()),
            Expr::Block {
                statements, result, ..
            } => self.lower_block_expr(statements, result),
        }
    }

    /// Audit2 B18 follow-up: resolve a `ResolvedType` to its enum
    /// type-name (used as the inferred-enum target for a struct-arg
    /// expression). Returns the empty string for non-enum, non-optional-
    /// of-enum types, which the caller filters out.
    fn enum_name_of(module: &crate::ir::IrModule, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Enum(eid) => module
                .get_enum(*eid)
                .map_or_else(String::new, |e| e.name.clone()),
            ResolvedType::Optional(inner) => Self::enum_name_of(module, inner),
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Array(_)
            | ResolvedType::Range(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => String::new(),
        }
    }

    fn lower_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");
        let type_args_resolved: Vec<ResolvedType> =
            type_args.iter().map(|t| self.lower_type(t)).collect();

        if let Some(id) = self.module.struct_id(&name) {
            let ty = if type_args_resolved.is_empty() {
                ResolvedType::Struct(id)
            } else {
                ResolvedType::Generic {
                    base: crate::ir::GenericBase::Struct(id),
                    args: type_args_resolved.clone(),
                }
            };
            // Audit2 B18 follow-up: build a name->type-name map of the
            // struct's fields so each named-arg lowers with the field's
            // declared type as the inferred-enum target. Without this,
            // `Size(width: .auto)` inherits whatever outer
            // `current_function_return_type` was set to and `.auto` can't
            // resolve.
            let field_target: HashMap<String, ResolvedType> = self
                .module
                .get_struct(id)
                .map(|s| {
                    s.fields
                        .iter()
                        .map(|f| (f.name.clone(), f.ty.clone()))
                        .collect()
                })
                .unwrap_or_default();
            let named_fields: Vec<(String, IrExpr)> = args
                .iter()
                .filter_map(|(name_opt, expr)| {
                    name_opt.as_ref().map(|n| {
                        let saved = self.current_function_return_type.take();
                        let saved_closure = self.expected_closure_type.take();
                        self.current_function_return_type = field_target
                            .get(&n.name)
                            .map(|t| Self::enum_name_of(&self.module, t))
                            .filter(|s| !s.is_empty());
                        // Audit2 B19: thread closure-typed field annotations
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
                        (n.name.clone(), lowered)
                    })
                })
                .collect();
            IrExpr::StructInst {
                struct_id: Some(id),
                type_args: type_args_resolved,
                fields: named_fields,
                ty,
            }
        } else if let Some(external_ty) = self.try_external_type(&name, type_args_resolved.clone())
        {
            let named_fields: Vec<(String, IrExpr)> = args
                .iter()
                .filter_map(|(name_opt, expr)| {
                    name_opt
                        .as_ref()
                        .map(|n| (n.name.clone(), self.lower_expr(expr)))
                })
                .collect();
            IrExpr::StructInst {
                struct_id: None,
                type_args: type_args_resolved,
                fields: named_fields,
                ty: external_ty,
            }
        } else {
            let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();
            // Audit2 B19: look up the function's expected parameter
            // types so a closure literal passed as an argument
            // (`fn apply(f: Number -> Number) ... apply(x -> x + 1)`)
            // lowers with `x: Number` instead of `ResolvedType::Error`.
            // Falls back to None when the function isn't in the IR yet
            // (forward reference) or the argument can't be matched by
            // name to a parameter.
            let fn_name = path_strs.last().map_or("", std::string::String::as_str);
            let expected_param_tys = self.lookup_function_param_types(fn_name);
            let lowered_args: Vec<(Option<String>, IrExpr)> = args
                .iter()
                .enumerate()
                .map(|(i, (name_opt, expr))| {
                    let saved_closure = self.expected_closure_type.take();
                    self.expected_closure_type =
                        Self::expected_arg_closure_ty(&expected_param_tys, i, name_opt.as_ref());
                    let lowered = self.lower_expr(expr);
                    self.expected_closure_type = saved_closure;
                    (name_opt.as_ref().map(|n| n.name.clone()), lowered)
                })
                .collect();
            let ty = self.resolve_function_return_type(fn_name, &lowered_args);
            IrExpr::FunctionCall {
                path: path_strs,
                args: lowered_args,
                ty,
            }
        }
    }

    /// Audit2 B19 helper: find the IR function with the given name and
    /// return its parameter list as `(param_name, param_ty)` pairs. The
    /// caller uses the list to seed `expected_closure_type` for each
    /// argument before lowering. Returns an empty vec when the function
    /// isn't yet in the IR (forward reference) — in that case we fall
    /// back to `Unknown` for closure-literal params, same as before.
    fn lookup_function_param_types(&self, fn_name: &str) -> Vec<(String, ResolvedType)> {
        if let Some(f) = self.module.functions.iter().find(|f| f.name == fn_name) {
            return f
                .params
                .iter()
                .filter_map(|p| p.ty.as_ref().map(|t| (p.name.clone(), t.clone())))
                .collect();
        }
        Vec::new()
    }

    /// Audit2 B19 helper: pick the expected parameter type for arg
    /// position `i`, preferring name match (for named args like
    /// `apply(callback: x -> x + 1)`) and falling back to positional
    /// index. Returns `Some(ty)` only when the matched parameter is a
    /// `Closure { .. }` — non-closure expected types don't influence
    /// closure-literal lowering.
    fn expected_arg_closure_ty(
        expected: &[(String, ResolvedType)],
        i: usize,
        name: Option<&crate::ast::Ident>,
    ) -> Option<ResolvedType> {
        let candidate = name.map_or_else(
            || expected.get(i).map(|(_, t)| t.clone()),
            |n| {
                expected
                    .iter()
                    .find(|(pname, _)| pname == &n.name)
                    .map(|(_, t)| t.clone())
            },
        );
        candidate.filter(|t| matches!(t, ResolvedType::Closure { .. }))
    }

    fn lower_enum_instantiation(
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
            fields: data
                .iter()
                .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                .collect(),
            ty,
        }
    }

    fn lower_inferred_enum_instantiation(
        &mut self,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        // Inferred-enum uses outside a return-typed context (e.g. struct
        // field defaults, top-level lets) are a known gap — the upstream
        // context-threading work in audit finding #27 will surface the
        // real signal, so we leave a TypeParam placeholder without error.
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
            fields: data
                .iter()
                .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                .collect(),
            ty,
        }
    }

    fn lower_array_expr(&mut self, elements: &[Expr]) -> IrExpr {
        let lowered: Vec<IrExpr> = elements.iter().map(|e| self.lower_expr(e)).collect();
        // Empty array literal: type element as `Never` ("no values yet").
        // Matches `nil`'s representation as `Optional(Never)` and lets
        // the existing array-shape compatibility check accept assignment
        // to `let xs: [T] = []`. Audit finding #8.
        let elem_ty = lowered.first().map_or_else(
            || ResolvedType::Primitive(PrimitiveType::Never),
            |e| e.ty().clone(),
        );
        IrExpr::Array {
            elements: lowered,
            ty: ResolvedType::Array(Box::new(elem_ty)),
        }
    }

    fn lower_tuple_expr(&mut self, fields: &[(crate::ast::Ident, Expr)]) -> IrExpr {
        let lowered: Vec<(String, IrExpr)> = fields
            .iter()
            .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
            .collect();
        let tuple_types: Vec<(String, ResolvedType)> = lowered
            .iter()
            .map(|(n, e)| (n.clone(), e.ty().clone()))
            .collect();
        IrExpr::Tuple {
            fields: lowered,
            ty: ResolvedType::Tuple(tuple_types),
        }
    }

    fn lower_reference(&mut self, path: &[crate::ast::Ident]) -> IrExpr {
        let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();

        // Check for self.field pattern — bounds verified by len() == 2 check
        #[expect(
            clippy::indexing_slicing,
            reason = "len == 2 check above guarantees indices 0 and 1"
        )]
        if path_strs.len() == 2 && path_strs[0] == "self" {
            let field_name = &path_strs[1];
            let ty = self.resolve_self_field_type(field_name);
            return IrExpr::SelfFieldRef {
                field: field_name.clone(),
                ty,
            };
        }

        // Check for bare "self" in impl context — bounds verified by len() == 1 check
        #[expect(
            clippy::indexing_slicing,
            reason = "len == 1 check above guarantees index 0"
        )]
        if path_strs.len() == 1 && path_strs[0] == "self" {
            if let Some(impl_name) = self.current_impl_struct.clone() {
                let ty = self.resolve_impl_self_type(&impl_name);
                return IrExpr::Reference {
                    path: path_strs,
                    ty,
                };
            }
        }

        // Check for module-level let binding reference
        if path_strs.len() == 1 {
            #[expect(
                clippy::indexing_slicing,
                reason = "len == 1 check above guarantees index 0"
            )]
            let name = &path_strs[0];
            if let Some(let_type) = self.symbols.get_let_type(name).map(str::to_string) {
                // Audit Tier-1: prefer the simple-name resolution; fall
                // back to the value's known type for composite type
                // strings the helper can't reparse (closures, tuples,
                // arrays, etc.). The let was previously lowered, so its
                // resolved type is already cached on the IR side.
                if let Some(ty) = self.string_to_resolved_type(&let_type) {
                    return IrExpr::LetRef {
                        name: name.clone(),
                        ty,
                    };
                }
                let ty = self
                    .module
                    .lets
                    .iter()
                    .find(|l| l.name == *name)
                    .map_or_else(|| ResolvedType::Error, |l| l.value.ty().clone());
                return IrExpr::LetRef {
                    name: name.clone(),
                    ty,
                };
            }
        }

        // Resolve the root of the path. Try, in order:
        //   1. a local binding (function param, `self`, closure capture),
        //   2. a module-level `let` — needed for multi-segment paths like
        //      `sample.tags` where the single-segment LetRef branch above
        //      doesn't apply.
        // For multi-segment paths, walk each subsequent segment as a field
        // access so `u.x.y` resolves to `y`'s actual field type. A root
        // segment that resolves to nothing is a real unresolved reference;
        // surface it as `UndefinedReference` and return `Error`.
        let root = path_strs.first().and_then(|n| {
            self.lookup_local_binding(n).cloned().or_else(|| {
                self.module
                    .lets
                    .iter()
                    .find(|l| l.name == *n)
                    .map(|l| l.value.ty().clone())
            })
        });
        let ty = if let Some(root_ty) = root {
            let mut current = root_ty;
            for seg in path_strs.iter().skip(1) {
                current = self.resolve_field_type(&current, seg);
            }
            current
        } else {
            let span = path.first().map_or(self.current_span, |i| i.span);
            self.errors.push(CompilerError::UndefinedReference {
                name: path_strs.join("."),
                span,
            });
            ResolvedType::Error
        };
        IrExpr::Reference {
            path: path_strs,
            ty,
        }
    }

    fn lower_binary_op_expr(&mut self, left: &Expr, op: BinaryOperator, right: &Expr) -> IrExpr {
        let left_ir = self.lower_expr(left);
        let right_ir = self.lower_expr(right);
        let ty = match op {
            BinaryOperator::Eq
            | BinaryOperator::Ne
            | BinaryOperator::Lt
            | BinaryOperator::Le
            | BinaryOperator::Gt
            | BinaryOperator::Ge
            | BinaryOperator::And
            | BinaryOperator::Or => ResolvedType::Primitive(PrimitiveType::Boolean),
            BinaryOperator::Add
            | BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod => left_ir.ty().clone(),
            BinaryOperator::Range => ResolvedType::Range(Box::new(left_ir.ty().clone())),
        };
        IrExpr::BinaryOp {
            left: Box::new(left_ir),
            op,
            right: Box::new(right_ir),
            ty,
        }
    }

    fn lower_unary_op_expr(&mut self, op: UnaryOperator, operand: &Expr) -> IrExpr {
        let operand_ir = self.lower_expr(operand);
        let ty = match op {
            UnaryOperator::Not => ResolvedType::Primitive(PrimitiveType::Boolean),
            UnaryOperator::Neg => operand_ir.ty().clone(),
        };
        IrExpr::UnaryOp {
            op,
            operand: Box::new(operand_ir),
            ty,
        }
    }

    fn lower_if_expr(
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

    fn lower_for_expr(
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
            collection: Box::new(collection_ir),
            body: Box::new(body_ir.clone()),
            ty: ResolvedType::Array(Box::new(body_ir.ty().clone())),
        }
    }

    fn lower_match_expr(&mut self, scrutinee: &Expr, arms: &[crate::ast::MatchArm]) -> IrExpr {
        let scrutinee_ir = self.lower_expr(scrutinee);
        let arms_ir: Vec<IrMatchArm> = arms
            .iter()
            .map(|arm| {
                let bindings = self.extract_pattern_bindings(&arm.pattern, &scrutinee_ir);
                // Pattern bindings (e.g. `urgency` from `.high(urgency)`)
                // need to be visible to the arm body. Without this frame
                // the body lowered with the binding as an UndefinedReference.
                let mut frame = HashMap::new();
                for (name, ty) in &bindings {
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

    fn lower_dict_literal(&mut self, entries: &[(Expr, Expr)]) -> IrExpr {
        let lowered_entries: Vec<(IrExpr, IrExpr)> = entries
            .iter()
            .map(|(k, v)| (self.lower_expr(k), self.lower_expr(v)))
            .collect();
        // Empty dict literal: both type args are `Never` (audit #8). The
        // shape stays a `Dictionary`, so assignment to `let d: [K: V] = [:]`
        // matches via the existing structural compatibility check.
        let ty = if let Some((k, v)) = lowered_entries.first() {
            ResolvedType::Dictionary {
                key_ty: Box::new(k.ty().clone()),
                value_ty: Box::new(v.ty().clone()),
            }
        } else {
            ResolvedType::Dictionary {
                key_ty: Box::new(ResolvedType::Primitive(PrimitiveType::Never)),
                value_ty: Box::new(ResolvedType::Primitive(PrimitiveType::Never)),
            }
        };
        IrExpr::DictLiteral {
            entries: lowered_entries,
            ty,
        }
    }

    fn lower_dict_access(&mut self, dict: &Expr, key: &Expr) -> IrExpr {
        let dict_ir = self.lower_expr(dict);
        let key_ir = self.lower_expr(key);
        let bad_dict = dict_ir.ty().clone();
        let ty = if let ResolvedType::Dictionary { value_ty, .. } = &bad_dict {
            (**value_ty).clone()
        } else {
            self.internal_error_type_if_concrete(
                &bad_dict,
                format!(
                    "dict-access receiver lowered to non-dictionary type {bad_dict:?}; semantic should have caught this",
                ),
            )
        };
        IrExpr::DictAccess {
            dict: Box::new(dict_ir),
            key: Box::new(key_ir),
            ty,
        }
    }

    fn lower_method_call(
        &mut self,
        receiver: &Expr,
        method_name: &str,
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let receiver_ir = self.lower_expr(receiver);
        // Audit2 B19: same idea as the function-call path — pull the
        // method's expected param types so closure-literal arguments
        // get their `x` typed against what the method expects.
        let expected_param_tys = self.lookup_method_param_types(receiver_ir.ty(), method_name);
        let lowered_args: Vec<(Option<String>, IrExpr)> = args
            .iter()
            .enumerate()
            .map(|(i, (label, expr))| {
                let saved_closure = self.expected_closure_type.take();
                self.expected_closure_type =
                    Self::expected_arg_closure_ty(&expected_param_tys, i, label.as_ref());
                let lowered = self.lower_expr(expr);
                self.expected_closure_type = saved_closure;
                (label.as_ref().map(|l| l.name.clone()), lowered)
            })
            .collect();
        let ty = self.resolve_method_return_type(receiver_ir.ty(), method_name);
        let dispatch = self.resolve_dispatch_kind(receiver_ir.ty(), method_name);
        IrExpr::MethodCall {
            receiver: Box::new(receiver_ir),
            method: method_name.to_string(),
            args: lowered_args,
            dispatch,
            ty,
        }
    }

    /// Audit2 B19 helper: locate the impl method matching
    /// `(receiver_ty, method_name)` and return its non-self parameter
    /// list as `(name, type)` pairs. The caller uses these to seed
    /// `expected_closure_type` for closure-literal arguments. Returns
    /// an empty vec when the method can't be resolved (forward
    /// reference, generic dispatch via trait, etc.) — in that case the
    /// arg-lowering falls back to `Unknown` for closure-literal params.
    fn lookup_method_param_types(
        &self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> Vec<(String, ResolvedType)> {
        let target = match receiver_ty {
            ResolvedType::Generic { base, .. } => match base {
                crate::ir::GenericBase::Struct(id) => Some(crate::ir::ImplTarget::Struct(*id)),
                crate::ir::GenericBase::Enum(id) => Some(crate::ir::ImplTarget::Enum(*id)),
                // A generic trait base can't be a method-call
                // receiver (FormaLang has no dynamic dispatch). Phase
                // E2 rejects trait values; this branch is here only
                // to keep the match exhaustive.
                crate::ir::GenericBase::Trait(_) => None,
            },
            ResolvedType::Struct(id) => Some(crate::ir::ImplTarget::Struct(*id)),
            ResolvedType::Enum(id) => Some(crate::ir::ImplTarget::Enum(*id)),
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Array(_)
            | ResolvedType::Range(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => None,
        };
        let Some(target) = target else {
            return Vec::new();
        };
        for impl_block in &self.module.impls {
            if impl_block.target != target {
                continue;
            }
            if let Some(func) = impl_block.functions.iter().find(|f| f.name == method_name) {
                return func
                    .params
                    .iter()
                    .filter(|p| p.name != "self")
                    .filter_map(|p| p.ty.as_ref().map(|t| (p.name.clone(), t.clone())))
                    .collect();
            }
        }
        Vec::new()
    }

    /// Resolve the dispatch kind for a method call.
    ///
    /// * Concrete struct/enum receivers resolve to `Static` dispatch pointing
    ///   at the impl block that provides the method body. When the call site
    ///   is inside the impl that is still being lowered, the `ImplId` refers
    ///   to the slot that impl will occupy in `module.impls` once finalized.
    /// * Type-parameter receivers (`T: Trait`) and trait-object receivers
    ///   resolve to `Virtual` dispatch through the relevant trait.
    /// * Other receiver shapes (primitives, arrays, tuples, etc.) are a
    ///   compiler bug at this layer — semantic analysis should have rejected
    ///   them. We record an `InternalError` and return a sentinel
    ///   `Virtual` dispatch pointing at `TraitId(u32::MAX)` so downstream
    ///   code never silently emits against a bogus trait id.
    fn resolve_dispatch_kind(
        &mut self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> DispatchKind {
        // Unwrap a Generic wrapper to its base so `Box<T>.method()` and
        // `Option<T>.method()` dispatch the same way a concrete Struct/Enum
        // receiver would.
        let concrete = match receiver_ty {
            ResolvedType::Generic { base, .. } => match base {
                crate::ir::GenericBase::Struct(id) => Some(ResolvedType::Struct(*id)),
                crate::ir::GenericBase::Enum(id) => Some(ResolvedType::Enum(*id)),
                // A trait base wouldn't appear here for a method
                // call receiver post item E2. Stay None and let the
                // resolver fall through to the existing error path.
                crate::ir::GenericBase::Trait(_) => None,
            },
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Range(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => None,
        };
        let effective_ty = concrete.as_ref().unwrap_or(receiver_ty);

        if let ResolvedType::Struct(struct_id) = effective_ty {
            if let Some(impl_id) = self.find_impl_for_struct(*struct_id, method_name) {
                return DispatchKind::Static { impl_id };
            }
            return DispatchKind::Static {
                impl_id: self.next_impl_id_or_record(),
            };
        }

        if let ResolvedType::Enum(enum_id) = effective_ty {
            if let Some(impl_id) = self.find_impl_for_enum(*enum_id, method_name) {
                return DispatchKind::Static { impl_id };
            }
            return DispatchKind::Static {
                impl_id: self.next_impl_id_or_record(),
            };
        }

        if let ResolvedType::TypeParam(param_name) = receiver_ty {
            if let Some(trait_id) = self.find_trait_for_method(param_name, method_name) {
                return DispatchKind::Virtual {
                    trait_id,
                    method_name: method_name.to_string(),
                };
            }
        }

        if let ResolvedType::Trait(trait_id) = receiver_ty {
            // Tier-1 item E2: trait values are banned at semantic time
            // (TraitUsedAsValueType). A receiver of `ResolvedType::Trait`
            // means semantic let one through — surface as an
            // InternalError instead of silently emitting Virtual
            // dispatch that the language doesn't otherwise permit.
            self.errors.push(CompilerError::InternalError {
                detail: format!(
                    "IR lowering: receiver type `Trait({})` reached method dispatch — \
                     semantic should have rejected the trait value at the call site",
                    trait_id.0
                ),
                span: self.current_span,
            });
            return DispatchKind::Virtual {
                trait_id: *trait_id,
                method_name: method_name.to_string(),
            };
        }

        self.errors.push(CompilerError::InternalError {
            detail: format!(
                "IR lowering: cannot resolve dispatch for method `{method_name}` on receiver {receiver_ty:?}"
            ),
            span: self.current_span,
        });
        DispatchKind::Virtual {
            trait_id: TraitId(u32::MAX),
            method_name: method_name.to_string(),
        }
    }

    /// Return the `ImplId` that will be assigned to the next impl block added.
    /// On u32 overflow, records a `TooManyDefinitions` error and returns a
    /// sentinel ID so compilation fails loudly rather than producing wrong dispatch.
    fn next_impl_id_or_record(&mut self) -> ImplId {
        self.module.next_impl_id().unwrap_or_else(|| {
            self.errors.push(CompilerError::TooManyDefinitions {
                kind: "impl",
                span: self.current_span,
            });
            ImplId(u32::MAX)
        })
    }

    /// Record `TooManyDefinitions` for an impl index that does not fit in `u32`
    /// and return a sentinel `ImplId`. Callers should have already established
    /// an `add_impl`-enforced invariant; this path exists purely to keep the
    /// compiler type-safe without an unchecked cast.
    fn impl_id_from_idx(&mut self, idx: usize) -> ImplId {
        if let Ok(v) = u32::try_from(idx) {
            ImplId(v)
        } else {
            self.errors.push(CompilerError::TooManyDefinitions {
                kind: "impl",
                span: self.current_span,
            });
            ImplId(u32::MAX)
        }
    }

    fn trait_id_from_idx(&mut self, idx: usize) -> TraitId {
        if let Ok(v) = u32::try_from(idx) {
            TraitId(v)
        } else {
            self.errors.push(CompilerError::TooManyDefinitions {
                kind: "trait",
                span: self.current_span,
            });
            TraitId(u32::MAX)
        }
    }

    fn find_impl_for_struct(
        &mut self,
        id: crate::ir::StructId,
        method_name: &str,
    ) -> Option<ImplId> {
        let found_idx = self.module.impls.iter().enumerate().find_map(|(idx, b)| {
            if b.struct_id() == Some(id) && b.functions.iter().any(|f| f.name == method_name) {
                Some(idx)
            } else {
                None
            }
        })?;
        Some(self.impl_id_from_idx(found_idx))
    }

    fn find_impl_for_enum(&mut self, id: crate::ir::EnumId, method_name: &str) -> Option<ImplId> {
        let found_idx = self.module.impls.iter().enumerate().find_map(|(idx, b)| {
            if b.enum_id() == Some(id) && b.functions.iter().any(|f| f.name == method_name) {
                Some(idx)
            } else {
                None
            }
        })?;
        Some(self.impl_id_from_idx(found_idx))
    }

    /// Look up the trait that declares `method_name` among the constraints
    /// attached to generic parameter `param_name`. Walks the innermost
    /// generic scope outwards, finds the param by name, then scans its
    /// trait constraints. Falls back to a module-wide search only when the
    /// param is not in any active scope (e.g. a lowering invariant was
    /// violated upstream) — this matches the pre-#12 behaviour so we don't
    /// regress on cases where the scope hasn't been populated.
    fn find_trait_for_method(&mut self, param_name: &str, method_name: &str) -> Option<TraitId> {
        for frame in self.generic_scopes.iter().rev() {
            if let Some(param) = frame.iter().find(|p| p.name == param_name) {
                for constraint in &param.constraints {
                    let idx = constraint.trait_id.0 as usize;
                    if let Some(trait_def) = self.module.traits.get(idx) {
                        if trait_def.methods.iter().any(|m| m.name == method_name) {
                            return Some(constraint.trait_id);
                        }
                    }
                }
                // Param is in scope but none of its constraints declare the
                // method — the semantic analyser should already have flagged
                // this; return None rather than picking an unrelated trait.
                return None;
            }
        }
        // Fallback: no matching scope frame — behave as before.
        let found_idx = self
            .module
            .traits
            .iter()
            .enumerate()
            .find_map(|(idx, trait_def)| {
                trait_def
                    .methods
                    .iter()
                    .any(|m| m.name == method_name)
                    .then_some(idx)
            })?;
        Some(self.trait_id_from_idx(found_idx))
    }

    /// Lower a `let pat = val in body` expression into a block with the
    /// binding as one or more statements. Destructuring patterns are
    /// expanded into per-field let statements so the bindings actually
    /// reach the body — previously they collapsed to a single `_let`
    /// binding. See audit finding #21.
    fn lower_let_expr(
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

    fn lower_block_expr(&mut self, statements: &[BlockStatement], result: &Expr) -> IrExpr {
        // Push a fresh binding-scope frame so each block-scoped `let`
        // becomes visible to subsequent statements and to `result`. The
        // frame is popped on exit so siblings don't see this block's
        // bindings. Audit finding #5b (and a long-standing inference
        // gap that surfaced once dispatch rewriting needed accurate
        // receiver types).
        self.local_binding_scopes.push(HashMap::new());
        let mut ir_statements: Vec<IrBlockStatement> = Vec::new();
        for stmt in statements {
            for s in self.lower_block_statement(stmt) {
                if let IrBlockStatement::Let {
                    name,
                    mutable,
                    ty,
                    value,
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
                        ty: ResolvedType::Primitive(PrimitiveType::Number),
                    };
                    IrBlockStatement::Let {
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
                    name: binding_name,
                    mutable,
                    ty: Some(field_ty.clone()),
                    value: IrExpr::FieldAccess {
                        object: Box::new(ir_value.clone()),
                        field: field_name,
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
                        name,
                        mutable,
                        ty: Some(ty.clone()),
                        value: IrExpr::FieldAccess {
                            object: Box::new(ir_value.clone()),
                            field: field_name,
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

    pub(super) fn literal_type(lit: &Literal) -> ResolvedType {
        match lit {
            Literal::String(_) => ResolvedType::Primitive(PrimitiveType::String),
            Literal::Number(n) => ResolvedType::Primitive(n.primitive_type()),
            Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
            Literal::Path(_) => ResolvedType::Primitive(PrimitiveType::Path),
            Literal::Regex { .. } => ResolvedType::Primitive(PrimitiveType::Regex),
            // `nil` is the zero value of every optional type. Modelled as
            // `Optional(Never)` — backends destructure this as "missing
            // value, no payload" and assignments to `T?` widen via the
            // existing `Optional` matching path. Audit finding #8.
            Literal::Nil => {
                ResolvedType::Optional(Box::new(ResolvedType::Primitive(PrimitiveType::Never)))
            }
        }
    }

    /// Resolve the type of a field access on an expression.
    ///
    /// Handles struct field access by looking up the field in the struct
    /// definition. Anything the semantic layer should have caught that
    /// still reaches here (missing field, field access on a non-struct
    /// type) records an `InternalError` so compilation fails loudly.
    pub(super) fn resolve_field_type(
        &mut self,
        object_ty: &ResolvedType,
        field_name: &str,
    ) -> ResolvedType {
        match object_ty {
            ResolvedType::Struct(struct_id) => {
                if let Some(struct_def) = self.module.get_struct(*struct_id) {
                    for field in &struct_def.fields {
                        if field.name == field_name {
                            return field.ty.clone();
                        }
                    }
                    self.errors.push(CompilerError::InternalError {
                        detail: format!(
                            "IR lowering: struct `{}` has no field `{field_name}`",
                            struct_def.name
                        ),
                        span: self.current_span,
                    });
                } else {
                    self.errors.push(CompilerError::InternalError {
                        detail: format!(
                            "IR lowering: struct id {} out of bounds during field access `{field_name}`",
                            struct_id.0
                        ),
                        span: self.current_span,
                    });
                }
                ResolvedType::Primitive(PrimitiveType::Never)
            }
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Range(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. } => {
                self.errors.push(CompilerError::InternalError {
                    detail: format!(
                        "IR lowering: cannot access field `{field_name}` on non-struct receiver {object_ty:?}"
                    ),
                    span: self.current_span,
                });
                ResolvedType::Primitive(PrimitiveType::Never)
            }
            // Receiver was already an upstream error; the original
            // `CompilerError` has been recorded — propagate without cascading.
            ResolvedType::Error => ResolvedType::Error,
        }
    }

    /// Resolve the return type of a method call.
    ///
    /// Looks up user-defined methods in impl blocks. Records an
    /// `InternalError` when the method cannot be resolved on a concrete
    /// receiver — those cases should have been caught by semantic
    /// analysis and reaching here indicates a compiler bug.
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive resolution: pre-installed methods, struct/enum/Generic/TypeParam/Trait dispatch arms"
    )]
    pub(super) fn resolve_method_return_type(
        &mut self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> ResolvedType {
        // If we are mid-lowering an impl block, its method set is recorded
        // in `current_impl_method_returns`. Forward references like
        // `self.other()` resolve against that map before the impl is
        // installed into `module.impls`.
        if let Some(returns) = &self.current_impl_method_returns {
            if let Some(entry) = returns.get(method_name) {
                return entry
                    .clone()
                    .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
            }
        }

        if let ResolvedType::Struct(struct_id) = receiver_ty {
            for impl_block in &self.module.impls {
                if impl_block.struct_id() == Some(*struct_id) {
                    for func in &impl_block.functions {
                        if func.name == method_name {
                            return func
                                .return_type
                                .clone()
                                .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                                .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                        }
                    }
                }
            }
            self.errors.push(CompilerError::InternalError {
                detail: format!(
                    "IR lowering: no impl method `{method_name}` for struct id {}",
                    struct_id.0
                ),
                span: self.current_span,
            });
            return ResolvedType::Primitive(PrimitiveType::Never);
        }

        // Generic receiver (`Box<Number>`): look up the impl on the
        // generic base, then substitute the impl method's TypeParams
        // with the concrete type arguments.
        if let ResolvedType::Generic { base, args } = receiver_ty {
            let (target_struct_id, target_enum_id) = match base {
                crate::ir::GenericBase::Struct(id) => (Some(*id), None),
                crate::ir::GenericBase::Enum(id) => (None, Some(*id)),
                // A trait base wouldn't appear here as a method-call
                // receiver post item E2. Skip and fall through.
                crate::ir::GenericBase::Trait(_) => (None, None),
            };
            let generic_params: Vec<String> = if let Some(sid) = target_struct_id {
                self.module
                    .get_struct(sid)
                    .map(|s| s.generic_params.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default()
            } else if let Some(eid) = target_enum_id {
                self.module
                    .get_enum(eid)
                    .map(|e| e.generic_params.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            for impl_block in &self.module.impls {
                let matches_target = match impl_block.target {
                    crate::ir::ImplTarget::Struct(id) => Some(id) == target_struct_id,
                    crate::ir::ImplTarget::Enum(id) => Some(id) == target_enum_id,
                };
                if !matches_target {
                    continue;
                }
                for func in &impl_block.functions {
                    if func.name == method_name {
                        let mut ret = func
                            .return_type
                            .clone()
                            .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                            .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                        let subs: std::collections::HashMap<String, ResolvedType> = generic_params
                            .iter()
                            .cloned()
                            .zip(args.iter().cloned())
                            .collect();
                        substitute_typeparam_in_resolved(&mut ret, &subs);
                        return ret;
                    }
                }
            }
        }

        if let ResolvedType::Enum(enum_id) = receiver_ty {
            for impl_block in &self.module.impls {
                if impl_block.enum_id() == Some(*enum_id) {
                    for func in &impl_block.functions {
                        if func.name == method_name {
                            return func
                                .return_type
                                .clone()
                                .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                                .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                        }
                    }
                }
            }
            self.errors.push(CompilerError::InternalError {
                detail: format!(
                    "IR lowering: no impl method `{method_name}` for enum id {}",
                    enum_id.0
                ),
                span: self.current_span,
            });
            return ResolvedType::Primitive(PrimitiveType::Never);
        }

        // TypeParam (generic parameter) or Trait receiver: look up the
        // method's return type on any trait declaring it. Semantic analysis
        // has already verified the bound is in scope.
        if let ResolvedType::TypeParam(name) = receiver_ty {
            if let Some(trait_id) = self.find_trait_for_method(name, method_name) {
                if let Some(trait_def) = self.module.get_trait(trait_id) {
                    if let Some(sig) = trait_def.methods.iter().find(|m| m.name == method_name) {
                        return sig
                            .return_type
                            .clone()
                            .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                    }
                }
            }
        }
        if let ResolvedType::Trait(trait_id) = receiver_ty {
            if let Some(trait_def) = self.module.get_trait(*trait_id) {
                if let Some(sig) = trait_def.methods.iter().find(|m| m.name == method_name) {
                    return sig
                        .return_type
                        .clone()
                        .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                }
            }
        }

        self.errors.push(CompilerError::InternalError {
            detail: format!(
                "IR lowering: cannot resolve return type of `{method_name}` on receiver {receiver_ty:?}"
            ),
            span: self.current_span,
        });
        ResolvedType::Primitive(PrimitiveType::Never)
    }

    /// Resolve the return type of a function call.
    ///
    /// Looks first in the already-lowered IR (`module.functions`), then
    /// falls back to the semantic symbol table so forward references to
    /// functions declared later in the file resolve to their declared
    /// return types. Records an `InternalError` only when neither source
    /// has an entry — in that case semantic analysis has missed the
    /// reference, which is a compiler bug.
    pub(super) fn resolve_function_return_type(
        &mut self,
        fn_name: &str,
        _args: &[(Option<String>, IrExpr)],
    ) -> ResolvedType {
        if let Some(func_id) = self.module.function_id(fn_name) {
            if let Some(func) = self.module.get_function(func_id) {
                return func
                    .return_type
                    .clone()
                    .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                    .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
            }
        }

        if let Some(info) = self.symbols.get_function(fn_name) {
            return info
                .return_type
                .as_ref()
                .map_or(ResolvedType::Primitive(PrimitiveType::Never), |t| {
                    self.lower_type(t)
                });
        }

        self.errors.push(CompilerError::InternalError {
            detail: format!(
                "IR lowering: unknown function `{fn_name}` reached codegen — should have been caught by semantic analysis"
            ),
            span: self.current_span,
        });
        ResolvedType::Primitive(PrimitiveType::Never)
    }

    /// Lower a closure expression.
    ///
    /// Lowers parameters and body to a `Closure` IR node, and collects the
    /// free variables (captures) referenced by the body. The regular lowering
    /// path handles all closure cases uniformly, including closures whose body
    /// is an enum variant construction.
    fn lower_closure(
        &mut self,
        params: &[ClosureParam],
        return_type: Option<&ast::Type>,
        body: &Expr,
    ) -> IrExpr {
        // Audit2 B19: when the surrounding context (a call argument,
        // a closure-typed struct field, etc.) supplies an expected
        // closure type, fall back to its param/return types for any
        // closure-literal slots the AST didn't annotate. This turns
        // `array.map(x -> x + 1)` into a closure with concrete
        // `x: <element type>` instead of `ResolvedType::Error`.
        let expected = self.expected_closure_type.take();
        let expected_param_tys: Vec<Option<ResolvedType>> = match expected.as_ref() {
            Some(ResolvedType::Closure { param_tys, .. }) if param_tys.len() == params.len() => {
                param_tys.iter().map(|(_, t)| Some(t.clone())).collect()
            }
            _ => vec![None; params.len()],
        };
        let expected_return_ty: Option<ResolvedType> = match expected.as_ref() {
            Some(ResolvedType::Closure { return_ty, .. }) => Some((**return_ty).clone()),
            _ => None,
        };

        // General closure: lower params and body
        let lowered_params: Vec<(ParamConvention, String, ResolvedType)> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ty = p.ty.as_ref().map_or_else(
                    || {
                        expected_param_tys
                            .get(i)
                            .and_then(std::clone::Clone::clone)
                            .unwrap_or(ResolvedType::Error)
                    },
                    |t| self.lower_type(t),
                );
                (p.convention, p.name.name.clone(), ty)
            })
            .collect();

        // Push a binding-scope frame for the closure's own parameters so that
        // (a) References inside the body resolve to declared types and
        // (b) nested closures see them when computing their own captures.
        let mut closure_frame: HashMap<String, (ParamConvention, ResolvedType)> = HashMap::new();
        for (conv, name, ty) in &lowered_params {
            closure_frame.insert(name.clone(), (*conv, ty.clone()));
        }
        self.local_binding_scopes.push(closure_frame);

        // Audit2 B18: set `current_function_return_type` from the
        // closure's declared return type so an inferred-enum
        // `.variant` inside the body resolves against the closure's
        // own return type, not the surrounding context (which after B18
        // can be the *outer* type, e.g. the field's `Closure` type).
        let saved_return_type = self.current_function_return_type.take();
        self.current_function_return_type = return_type.map(super::IrLowerer::type_name);

        let body_ir = self.lower_expr(body);

        self.current_function_return_type = saved_return_type;
        // Audit #38 + audit2 B19: prefer the declared return type when
        // present, then the expected return type from the surrounding
        // context, then the inferred body type (which may be `Unknown`
        // or narrower).
        let return_ty = return_type.map_or_else(
            || {
                if matches!(body_ir.ty(), ResolvedType::TypeParam(_)) {
                    expected_return_ty
                        .clone()
                        .unwrap_or_else(|| body_ir.ty().clone())
                } else {
                    body_ir.ty().clone()
                }
            },
            |t| self.lower_type(t),
        );

        // Pop the closure's own frame before resolving captures so that
        // capture lookups consult only the enclosing scopes.
        self.local_binding_scopes.pop();

        let param_names: std::collections::HashSet<String> =
            lowered_params.iter().map(|(_, n, _)| n.clone()).collect();
        let mut captures: Vec<(String, ResolvedType)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        collect_free_refs(&body_ir, &param_names, &mut captures, &mut seen);

        // Audit #32: each capture inherits the convention of the outer
        // binding it refers to — function parameter convention, mutable-let
        // in a block, outer closure parameter convention, or module-level
        // `let mut`. Bindings whose convention can't be located default to
        // `Let` (immutable view is the safest backend assumption).
        let captures_with_mode: Vec<(String, ParamConvention, ResolvedType)> = captures
            .into_iter()
            .map(|(name, ty)| {
                let convention = self
                    .lookup_local_binding_entry(&name)
                    .map(|(c, _)| *c)
                    .or_else(|| {
                        self.module.lets.iter().find(|l| l.name == name).map(|l| {
                            if l.mutable {
                                ParamConvention::Mut
                            } else {
                                ParamConvention::Let
                            }
                        })
                    })
                    .unwrap_or(ParamConvention::Let);
                (name, convention, ty)
            })
            .collect();

        let ty = ResolvedType::Closure {
            param_tys: lowered_params
                .iter()
                .map(|(c, _, t)| (*c, t.clone()))
                .collect(),
            return_ty: Box::new(return_ty),
        };

        IrExpr::Closure {
            params: lowered_params,
            captures: captures_with_mode,
            body: Box::new(body_ir),
            ty,
        }
    }

    pub(super) fn extract_pattern_bindings(
        &mut self,
        pattern: &ast::Pattern,
        scrutinee: &IrExpr,
    ) -> Vec<(String, ResolvedType)> {
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
                        (ident.name.clone(), ty)
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

/// Walk `expr` and collect every single-name `Reference` whose name is not
/// bound inside the expression itself — i.e. the closure's free variables.
///
/// Captures are appended to `out` in first-reference order and deduplicated
/// via `seen`. The caller seeds `bound` with the closure's own parameter
/// names; nested lets and inner closures extend it locally during the walk.
#[expect(
    clippy::too_many_lines,
    reason = "exhaustive dispatch over every IrExpr variant — extracting arms would hide the structural walk"
)]
fn collect_free_refs(
    expr: &IrExpr,
    bound: &std::collections::HashSet<String>,
    out: &mut Vec<(String, ResolvedType)>,
    seen: &mut std::collections::HashSet<String>,
) {
    match expr {
        IrExpr::Reference { path, ty } => {
            if let [name] = path.as_slice() {
                if !bound.contains(name) && seen.insert(name.clone()) {
                    out.push((name.clone(), ty.clone()));
                }
            }
        }
        IrExpr::LetRef { name, ty } => {
            if !bound.contains(name) && seen.insert(name.clone()) {
                out.push((name.clone(), ty.clone()));
            }
        }
        IrExpr::Literal { .. } | IrExpr::SelfFieldRef { .. } => {}
        IrExpr::StructInst { fields, .. }
        | IrExpr::EnumInst { fields, .. }
        | IrExpr::Tuple { fields, .. } => {
            for (_, field_expr) in fields {
                collect_free_refs(field_expr, bound, out, seen);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                collect_free_refs(e, bound, out, seen);
            }
        }
        IrExpr::FieldAccess { object, .. } => collect_free_refs(object, bound, out, seen),
        IrExpr::BinaryOp { left, right, .. } => {
            collect_free_refs(left, bound, out, seen);
            collect_free_refs(right, bound, out, seen);
        }
        IrExpr::UnaryOp { operand, .. } => collect_free_refs(operand, bound, out, seen),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_free_refs(condition, bound, out, seen);
            collect_free_refs(then_branch, bound, out, seen);
            if let Some(e) = else_branch {
                collect_free_refs(e, bound, out, seen);
            }
        }
        IrExpr::For {
            var,
            collection,
            body,
            ..
        } => {
            collect_free_refs(collection, bound, out, seen);
            let mut inner = bound.clone();
            inner.insert(var.clone());
            collect_free_refs(body, &inner, out, seen);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            collect_free_refs(scrutinee, bound, out, seen);
            for arm in arms {
                let mut inner = bound.clone();
                for (name, _) in &arm.bindings {
                    inner.insert(name.clone());
                }
                collect_free_refs(&arm.body, &inner, out, seen);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, a) in args {
                collect_free_refs(a, bound, out, seen);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            collect_free_refs(receiver, bound, out, seen);
            for (_, a) in args {
                collect_free_refs(a, bound, out, seen);
            }
        }
        IrExpr::Closure {
            params, captures, ..
        } => {
            // Inner closure: its own captures are already computed relative to
            // its body. Any capture that is bound in the outer scope is not
            // free at this level; the rest bubble up as outer-closure captures.
            let inner_params: std::collections::HashSet<String> =
                params.iter().map(|(_, n, _)| n.clone()).collect();
            for (name, _, ty) in captures {
                if !inner_params.contains(name)
                    && !bound.contains(name)
                    && seen.insert(name.clone())
                {
                    out.push((name.clone(), ty.clone()));
                }
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                collect_free_refs(k, bound, out, seen);
                collect_free_refs(v, bound, out, seen);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            collect_free_refs(dict, bound, out, seen);
            collect_free_refs(key, bound, out, seen);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            let mut inner = bound.clone();
            for stmt in statements {
                match stmt {
                    IrBlockStatement::Let { name, value, .. } => {
                        collect_free_refs(value, &inner, out, seen);
                        inner.insert(name.clone());
                    }
                    IrBlockStatement::Assign { target, value } => {
                        collect_free_refs(target, &inner, out, seen);
                        collect_free_refs(value, &inner, out, seen);
                    }
                    IrBlockStatement::Expr(e) => collect_free_refs(e, &inner, out, seen),
                }
            }
            collect_free_refs(result, &inner, out, seen);
        }
    }
}
