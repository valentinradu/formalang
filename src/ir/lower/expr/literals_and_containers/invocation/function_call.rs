use super::expr_references_any_name;
use crate::ast::Expr;
use crate::ir::lower::expr::helpers::substitute_typeparam_in_resolved;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrBlockStatement, IrExpr, IrFunctionParam, ResolvedType};
use std::collections::{HashMap, HashSet};

struct DefaultSubstitution {
    needs_let_wrapper: bool,
    wrapper_param_names: Vec<String>,
    wrapper_param_types: Vec<Option<ResolvedType>>,
}

impl IrLowerer<'_> {
    pub(super) fn lower_function_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        type_args_resolved: &[ResolvedType],
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();
        let fn_name = path_strs.last().map_or("", std::string::String::as_str);
        // Resolve the call to a `FunctionId` first; module-aware
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
                let expected = Self::expected_arg_ty(&expected_param_tys, i, name_opt.as_ref());
                let lowered = self.lower_with_expected_value(expr, expected.as_ref());
                (name_opt.as_ref().map(|n| n.name.clone()), lowered)
            })
            .collect();
        let substitution = self.substitute_defaults(function_id, &mut lowered_args);
        // Return type lookup uses the same id when available; the
        // bare-name lookup is the fallback for forward refs.
        let mut ty = function_id
            .and_then(|id| self.module.functions.get(id.0 as usize))
            .and_then(|f| f.return_type.clone())
            .unwrap_or_else(|| self.resolve_function_return_type(fn_name, &lowered_args));
        // Substitute the callee's generic-parameter slots in the
        // returned `ty` using the explicit `<...>` type arguments
        // at the call site. Without this, a call like
        // `pair_of<I32>(x: 3, y: 4)` keeps `Pair<T, T>` as its IR
        // type slot until `MonomorphisePass`, and downstream uses
        // (a `let p = (); p.first` field access typed at lowering
        // time) carry `TypeParam(T)` until the leftover scanner
        // surfaces them.
        if !type_args_resolved.is_empty() {
            if let Some(func) = function_id
                .and_then(|id| self.module.functions.get(id.0 as usize))
                .filter(|f| !f.generic_params.is_empty())
            {
                if func.generic_params.len() == type_args_resolved.len() {
                    let subs: HashMap<String, ResolvedType> = func
                        .generic_params
                        .iter()
                        .zip(type_args_resolved.iter())
                        .map(|(p, a)| (p.name.clone(), a.clone()))
                        .collect();
                    substitute_typeparam_in_resolved(&mut ty, &subs);
                }
            }
        }

        if substitution.needs_let_wrapper {
            self.wrap_call_with_let_bindings(
                path_strs,
                function_id,
                lowered_args,
                ty,
                &substitution.wrapper_param_names,
                &substitution.wrapper_param_types,
            )
        } else {
            IrExpr::FunctionCall {
                path: path_strs,
                function_id,
                args: lowered_args,
                ty,
                span: self.current_ir_span(),
            }
        }
    }

    /// DP-2 / DP-4 / DP-7: substitute defaults for missing args.
    /// For all-positional calls, append trailing defaults. For
    /// labeled calls (mode A), walk callee params in order and
    /// fill any whose label is missing from the call. If any
    /// substituted default references a preceding non-defaulted
    /// param by name, the caller wraps the entire `FunctionCall`
    /// in a `Block` whose `Let` statements bind those param names
    /// to the explicit args (so the default's `Reference` resolves
    /// via path lookup to the new binding, not to the callee's
    /// stale binding-id, and side-effects don't duplicate).
    fn substitute_defaults(
        &self,
        function_id: Option<crate::ir::FunctionId>,
        lowered_args: &mut Vec<(Option<String>, IrExpr)>,
    ) -> DefaultSubstitution {
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
                        lowered_args.iter().filter_map(|(l, _)| l.clone()).collect()
                    } else {
                        non_self_params
                            .iter()
                            .take(lowered_args.len())
                            .map(|p| p.name.clone())
                            .collect()
                    };
                    if any_labeled {
                        // Mode A; labeled call. Build a new
                        // ordered args list that walks callee
                        // params in order, picking up the
                        // explicit-call value when its label is
                        // present and substituting the default
                        // otherwise. Mid-list omissions get
                        // filled at the right position.
                        let mut new_args: Vec<(Option<String>, IrExpr)> = Vec::with_capacity(want);
                        for param in &non_self_params {
                            if let Some(pos) = lowered_args.iter().position(|(l, _)| {
                                l.as_ref().is_some_and(|name| name == &param.name)
                            }) {
                                let (label, value) = lowered_args.remove(pos);
                                new_args.push((label, value));
                            } else if let Some(default) = &param.default {
                                if expr_references_any_name(default, &already_provided_names) {
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
                        *lowered_args = new_args;
                    } else {
                        // Positional; append trailing defaults.
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
        DefaultSubstitution {
            needs_let_wrapper,
            wrapper_param_names,
            wrapper_param_types,
        }
    }

    /// Build let bindings for each preceding non-defaulted
    /// param. Move the explicit `lowered_args[i]` into the
    /// let value; replace the call-site arg with a
    /// `Reference` to the binding name. The default's
    /// `Reference{path:[name]}` resolves to the let-binding
    /// post-`ResolveReferencesPass`.
    fn wrap_call_with_let_bindings(
        &self,
        path_strs: Vec<String>,
        function_id: Option<crate::ir::FunctionId>,
        mut lowered_args: Vec<(Option<String>, IrExpr)>,
        ty: ResolvedType,
        wrapper_param_names: &[String],
        wrapper_param_types: &[Option<ResolvedType>],
    ) -> IrExpr {
        let mut statements = Vec::with_capacity(wrapper_param_names.len());
        for ((name, ty), arg) in wrapper_param_names
            .iter()
            .zip(wrapper_param_types.iter())
            .zip(lowered_args.iter_mut())
        {
            let value = std::mem::replace(
                &mut arg.1,
                IrExpr::Reference {
                    path: vec![name.clone()],
                    target: crate::ir::ReferenceTarget::Unresolved,
                    ty: ty.clone().unwrap_or(ResolvedType::Error),
                    span: self.current_ir_span(),
                },
            );
            statements.push(IrBlockStatement::Let {
                binding_id: crate::ir::BindingId(0),
                name: name.clone(),
                mutable: false,
                ty: ty.clone(),
                value,
                span: self.current_ir_span(),
            });
        }
        let call = IrExpr::FunctionCall {
            path: path_strs,
            function_id,
            args: lowered_args,
            ty: ty.clone(),
            span: self.current_ir_span(),
        };
        IrExpr::Block {
            statements,
            result: Box::new(call),
            ty,
            span: self.current_ir_span(),
        }
    }
}
