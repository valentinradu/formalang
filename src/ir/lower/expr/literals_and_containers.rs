//! Lowering for literal, container and instantiation expressions:
//! `Literal`, `Array`, `Tuple`, `Dict{Literal,Access}`, struct/enum
//! instantiation and bare-function/struct invocation paths.

use crate::ast::{Expr, Literal, PrimitiveType};
use crate::ir::lower::IrLowerer;
use crate::ir::{IrBlockStatement, IrExpr, IrFunctionParam, ResolvedType};
use std::collections::{HashMap, HashSet};

/// True if `expr` (or any sub-expression) is an `IrExpr::Reference`
/// with a single-segment path matching one of `names`. Used to
/// decide whether a substituted default needs the let-wrapper that
/// binds preceding non-defaulted params to the call-site values.
fn expr_references_any_name(expr: &IrExpr, names: &HashSet<String>) -> bool {
    match expr {
        IrExpr::Reference { path, .. } => path
            .first()
            .is_some_and(|seg| names.contains(seg.as_str())),
        IrExpr::LetRef { name, .. } => names.contains(name.as_str()),
        IrExpr::BinaryOp { left, right, .. } => {
            expr_references_any_name(left, names) || expr_references_any_name(right, names)
        }
        IrExpr::UnaryOp { operand, .. } => expr_references_any_name(operand, names),
        IrExpr::FieldAccess { object, .. } => expr_references_any_name(object, names),
        IrExpr::FunctionCall { args, .. } | IrExpr::MethodCall { args, .. } => {
            args.iter().any(|(_, a)| expr_references_any_name(a, names))
        }
        IrExpr::CallClosure { closure, args, .. } => {
            expr_references_any_name(closure, names)
                || args.iter().any(|(_, a)| expr_references_any_name(a, names))
        }
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_references_any_name(condition, names)
                || expr_references_any_name(then_branch, names)
                || else_branch
                    .as_deref()
                    .is_some_and(|e| expr_references_any_name(e, names))
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            expr_references_any_name(scrutinee, names)
                || arms
                    .iter()
                    .any(|arm| expr_references_any_name(&arm.body, names))
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            statements.iter().any(|s| match s {
                IrBlockStatement::Let { value, .. } => expr_references_any_name(value, names),
                IrBlockStatement::Assign { target, value } => {
                    expr_references_any_name(target, names) || expr_references_any_name(value, names)
                }
                IrBlockStatement::Expr(e) => expr_references_any_name(e, names),
            }) || expr_references_any_name(result, names)
        }
        IrExpr::Array { elements, .. } => {
            elements.iter().any(|e| expr_references_any_name(e, names))
        }
        IrExpr::Tuple { fields, .. } => fields.iter().any(|(_, e)| expr_references_any_name(e, names)),
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            fields.iter().any(|(_, _, e)| expr_references_any_name(e, names))
        }
        IrExpr::DictLiteral { entries, .. } => entries
            .iter()
            .any(|(k, v)| expr_references_any_name(k, names) || expr_references_any_name(v, names)),
        IrExpr::DictAccess { dict, key, .. } => {
            expr_references_any_name(dict, names) || expr_references_any_name(key, names)
        }
        IrExpr::For {
            collection, body, ..
        } => expr_references_any_name(collection, names) || expr_references_any_name(body, names),
        IrExpr::Closure { body, .. } => expr_references_any_name(body, names),
        IrExpr::ClosureRef { env_struct, .. } => expr_references_any_name(env_struct, names),
        IrExpr::Literal { .. } | IrExpr::SelfFieldRef { .. } => false,
    }
}

impl IrLowerer<'_> {
    /// resolve a `ResolvedType` to its enum
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

    #[expect(
        clippy::too_many_lines,
        reason = "three branches (struct / external / function) each with their own arg-lowering plumbing — splitting hides the contract"
    )]
    pub(super) fn lower_invocation(
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
            // build a name->type-name map of the
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
            IrExpr::StructInst {
                struct_id: Some(id),
                type_args: type_args_resolved,
                fields: named_fields,
                ty,
            }
        } else if let Some(external_ty) = self.try_external_type(&name, type_args_resolved.clone())
        {
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
            }
        } else if let Some(call) = self.try_lower_closure_invocation(path, args) {
            call
        } else {
            let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();
            let fn_name = path_strs.last().map_or("", std::string::String::as_str);
            // Resolve the call to a `FunctionId` first — module-aware
            // for single-segment calls (try the current `mod`'s
            // qualified form, fall back to bare); joined-name lookup
            // for multi-segment. Cross-module / forward-reference
            // cases stay `None` and `ResolveReferencesPass` finishes
            // the job.
            let function_id = if path_strs.len() == 1 {
                self.find_function_in_scope(fn_name)
            } else {
                self.module
                    .function_id(&path_strs.join("::"))
                    .or_else(|| self.find_function_in_scope(fn_name))
            };
            // Derive expected param types from the resolved id
            // (covers cross-module qualified calls correctly), or
            // fall back to scanning by bare name for forward
            // references.
            let expected_param_tys: Vec<(String, ResolvedType)> = function_id
                .and_then(|id| self.module.functions.get(id.0 as usize))
                .map_or_else(
                    || self.lookup_function_param_types(fn_name),
                    |f| {
                        f.params
                            .iter()
                            .filter_map(|p| p.ty.as_ref().map(|t| (p.name.clone(), t.clone())))
                            .collect()
                    },
                );
            let mut lowered_args: Vec<(Option<String>, IrExpr)> = args
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
            // DP-2 / DP-4 / DP-7: substitute defaults for missing args.
            // For all-positional calls, append trailing defaults. For
            // labeled calls (mode A), walk callee params in order and
            // fill any whose label is missing from the call. If any
            // substituted default references a preceding non-defaulted
            // param by name, wrap the entire FunctionCall in a Block
            // whose Let statements bind those param names to the
            // explicit args (so the default's Reference resolves via
            // path lookup to the new binding, not to the callee's
            // stale binding-id, and side-effects don't duplicate).
            let mut needs_let_wrapper = false;
            let mut wrapper_param_names: Vec<String> = Vec::new();
            let mut wrapper_param_types: Vec<Option<ResolvedType>> = Vec::new();
            if let Some(func_id) = function_id {
                if let Some(func) = self.module.functions.get(func_id.0 as usize) {
                    let non_self_params: Vec<IrFunctionParam> = func
                        .params
                        .iter()
                        .filter(|p| p.name != "self")
                        .cloned()
                        .collect();
                    let want = non_self_params.len();
                    if lowered_args.len() < want {
                        let any_labeled = lowered_args.iter().any(|(l, _)| l.is_some());
                        // Names of params that already have a value in
                        // the call (used to detect earlier-param refs
                        // that need the let-wrapper).
                        let already_provided_names: HashSet<String> = if any_labeled {
                            lowered_args
                                .iter()
                                .filter_map(|(l, _)| l.clone())
                                .collect()
                        } else {
                            non_self_params
                                .iter()
                                .take(lowered_args.len())
                                .map(|p| p.name.clone())
                                .collect()
                        };
                        if any_labeled {
                            // Mode A — labeled call. Build a new
                            // ordered args list that walks callee
                            // params in order, picking up the
                            // explicit-call value when its label is
                            // present and substituting the default
                            // otherwise. Mid-list omissions get
                            // filled at the right position.
                            let mut new_args: Vec<(Option<String>, IrExpr)> =
                                Vec::with_capacity(want);
                            for param in &non_self_params {
                                if let Some(pos) = lowered_args.iter().position(|(l, _)| {
                                    l.as_ref().is_some_and(|name| name == &param.name)
                                }) {
                                    let (label, value) = lowered_args.remove(pos);
                                    new_args.push((label, value));
                                } else if let Some(default) = &param.default {
                                    if expr_references_any_name(default, &already_provided_names)
                                    {
                                        needs_let_wrapper = true;
                                    }
                                    new_args.push((Some(param.name.clone()), default.clone()));
                                } else {
                                    // Required label missing: validator
                                    // should have rejected. Stop on the
                                    // first gap to preserve some signal.
                                    break;
                                }
                            }
                            lowered_args = new_args;
                        } else {
                            // Positional — append trailing defaults.
                            for param in non_self_params.iter().skip(lowered_args.len()) {
                                if let Some(default) = &param.default {
                                    if expr_references_any_name(default, &already_provided_names) {
                                        needs_let_wrapper = true;
                                    }
                                    lowered_args.push((None, default.clone()));
                                } else {
                                    break;
                                }
                            }
                        }
                        if needs_let_wrapper {
                            for param in non_self_params
                                .iter()
                                .filter(|p| already_provided_names.contains(&p.name))
                            {
                                wrapper_param_names.push(param.name.clone());
                                wrapper_param_types.push(param.ty.clone());
                            }
                        }
                    }
                }
            }
            // Return type lookup uses the same id when available; the
            // legacy bare-name lookup is the fallback for forward
            // refs.
            let ty = function_id
                .and_then(|id| self.module.functions.get(id.0 as usize))
                .and_then(|f| f.return_type.clone())
                .unwrap_or_else(|| self.resolve_function_return_type(fn_name, &lowered_args));

            if needs_let_wrapper {
                // Build let bindings for each preceding non-defaulted
                // param. Move the explicit lowered_args[i] into the
                // let value; replace the call-site arg with a
                // Reference to the binding name. The default's
                // Reference{path:[name]} resolves to the let-binding
                // post-ResolveReferencesPass.
                let mut statements = Vec::with_capacity(wrapper_param_names.len());
                for (i, name) in wrapper_param_names.iter().enumerate() {
                    let value = std::mem::replace(
                        &mut lowered_args[i].1,
                        IrExpr::Reference {
                            path: vec![name.clone()],
                            target: crate::ir::ReferenceTarget::Unresolved,
                            ty: wrapper_param_types[i]
                                .clone()
                                .unwrap_or(ResolvedType::Error),
                        },
                    );
                    statements.push(IrBlockStatement::Let {
                        binding_id: crate::ir::BindingId(0),
                        name: name.clone(),
                        mutable: false,
                        ty: wrapper_param_types[i].clone(),
                        value,
                    });
                }
                let call = IrExpr::FunctionCall {
                    path: path_strs,
                    function_id,
                    args: lowered_args,
                    ty: ty.clone(),
                };
                IrExpr::Block {
                    statements,
                    result: Box::new(call),
                    ty,
                }
            } else {
                IrExpr::FunctionCall {
                    path: path_strs,
                    function_id,
                    args: lowered_args,
                    ty,
                }
            }
        }
    }

    /// Detect when an `Invocation` whose path is a single segment
    /// resolves to a closure-typed local binding rather than a top-
    /// level function, and lower it as [`IrExpr::CallClosure`]
    /// targeting that binding.
    ///
    /// Returns `None` when the path doesn't refer to a closure-typed
    /// local — the caller falls through to the regular
    /// [`IrExpr::FunctionCall`] path.
    fn try_lower_closure_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> Option<IrExpr> {
        let [ident] = path else {
            return None;
        };
        let name = &ident.name;
        let local_ty = self.lookup_local_binding(name)?.clone();
        let ResolvedType::Closure {
            param_tys,
            return_ty,
        } = &local_ty
        else {
            return None;
        };
        let return_ty = (**return_ty).clone();
        let expected_param_tys: Vec<(String, ResolvedType)> = param_tys
            .iter()
            .enumerate()
            .map(|(i, (_, ty))| (format!("__closure_arg_{i}"), ty.clone()))
            .collect();
        let lowered_args: Vec<(Option<String>, IrExpr)> = args
            .iter()
            .enumerate()
            .map(|(i, (arg_name, expr))| {
                let saved_closure = self.expected_closure_type.take();
                self.expected_closure_type =
                    Self::expected_arg_closure_ty(&expected_param_tys, i, arg_name.as_ref());
                let lowered = self.lower_expr(expr);
                self.expected_closure_type = saved_closure;
                (arg_name.as_ref().map(|n| n.name.clone()), lowered)
            })
            .collect();
        Some(IrExpr::CallClosure {
            closure: Box::new(IrExpr::LetRef {
                name: name.clone(),
                binding_id: crate::ir::BindingId(0),
                ty: local_ty,
            }),
            args: lowered_args,
            ty: return_ty,
        })
    }

    pub(super) fn lower_enum_instantiation(
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
        }
    }

    pub(super) fn lower_inferred_enum_instantiation(
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
        }
    }

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
            Some(
                t @ (ResolvedType::Array(_)
                | ResolvedType::Tuple(_)
                | ResolvedType::Dictionary { .. }),
            ) => {
                let saved = self.expected_value_type.take();
                self.expected_value_type = Some(t.clone());
                let lowered = self.lower_expr(expr);
                self.expected_value_type = saved;
                lowered
            }
            _ => self.lower_expr(expr),
        }
    }

    pub(super) fn lower_array_expr(&mut self, elements: &[Expr]) -> IrExpr {
        // If the surrounding context supplies an expected aggregate type
        // (e.g. a destructuring let `let [f]: [I32 -> I32] = [|x| x]`),
        // pass the element type down to each element's lowering. A
        // direct `Closure` element forwards via `expected_closure_type`;
        // a nested container (`Array`/`Tuple`/`Dictionary`) forwards via
        // `expected_value_type` so the next layer can peel and continue
        // the search. Without this, un-annotated closure params nested
        // inside container-of-container annotations lower to
        // `ResolvedType::Error`.
        let elem_expected: Option<ResolvedType> = match self.expected_value_type.take() {
            Some(ResolvedType::Array(inner)) => Some(*inner),
            _ => None,
        };
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
            ty: ResolvedType::Array(Box::new(elem_ty)),
        }
    }

    pub(super) fn lower_tuple_expr(&mut self, fields: &[(crate::ast::Ident, Expr)]) -> IrExpr {
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
        }
    }

    pub(super) fn lower_dict_literal(&mut self, entries: &[(Expr, Expr)]) -> IrExpr {
        // Like `lower_array_expr` / `lower_tuple_expr`, propagate the
        // `Dictionary { value_ty }` to closure-literal entry values
        // when a destructuring let / annotated context supplies one.
        // Without this, `let d: [String: I32 -> I32] = ["k": |x| x]`
        // produces a closure with `params: [(Let, "x", Error)]`. A
        // nested-container `value_ty` (e.g. `[I32 -> I32]`) is forwarded
        // via `expected_value_type` so the inner array can peel and
        // continue down to the closure.
        let value_expected: Option<ResolvedType> = match self.expected_value_type.take() {
            Some(ResolvedType::Dictionary { value_ty, .. }) => Some(*value_ty),
            _ => None,
        };
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

    pub(super) fn lower_dict_access(&mut self, dict: &Expr, key: &Expr) -> IrExpr {
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
            };
        }

        let ty = if let ResolvedType::Dictionary { value_ty, .. } = &receiver_ty {
            (**value_ty).clone()
        } else {
            self.internal_error_type_if_concrete(
                &receiver_ty,
                format!(
                    "dict-access receiver lowered to non-dictionary type {receiver_ty:?}; semantic should have caught this",
                ),
            )
        };
        IrExpr::DictAccess {
            dict: Box::new(dict_ir),
            key: Box::new(key_ir),
            ty,
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
            // existing `Optional` matching path.
            Literal::Nil => {
                ResolvedType::Optional(Box::new(ResolvedType::Primitive(PrimitiveType::Never)))
            }
        }
    }
}
