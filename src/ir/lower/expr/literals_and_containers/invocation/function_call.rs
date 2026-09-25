use super::expr_references_any_name;
use crate::ast::Expr;
use crate::ir::lower::expr::operators::{signature_of, ArgSlot};
use crate::ir::lower::expr::type_params::{
    holds_a_type_param, substitute_typeparam_in_resolved, unify_typeparam_in_resolved,
};
use crate::ir::lower::IrLowerer;
use crate::ir::{IrBlockStatement, IrExpr, IrFunction, IrFunctionParam, ResolvedType};
use std::collections::{HashMap, HashSet};

struct DefaultSubstitution {
    needs_let_wrapper: bool,
    /// The parameter name of each argument, in order.
    wrapper_param_names: Vec<String>,
    wrapper_param_types: Vec<Option<ResolvedType>>,
    /// True for an argument that the call wrote, false for a default.
    explicit: Vec<bool>,
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
        let arg_labels: Vec<Option<String>> = args
            .iter()
            .map(|(name_opt, _)| name_opt.as_ref().map(|n| n.name.clone()))
            .collect();
        let function_id = if path_strs.len() == 1 {
            self.find_overload_in_scope(fn_name, &arg_labels, args.len())
        } else {
            self.module
                .function_id(&path_strs.join("::"))
                .or_else(|| self.find_overload_in_scope(fn_name, &arg_labels, args.len()))
        };
        // The callee: the lowered function, or else its declared
        // signature when the call comes before the function in the
        // file. The declare pass lowered every signature first.
        let callee: Option<IrFunction> = function_id
            .and_then(|id| self.module.functions.get(id.0 as usize))
            .map(signature_of)
            .or_else(|| self.declared_callee(fn_name, &arg_labels, args.len()));
        let expected_param_tys: Vec<ArgSlot> = callee.as_ref().map_or_else(
            || self.lookup_function_param_types(fn_name),
            |f| f.params.iter().map(ArgSlot::of_param).collect(),
        );
        let generic = callee
            .as_ref()
            .is_some_and(|f| !f.generic_params.is_empty());
        let written: HashMap<String, ResolvedType> = callee
            .as_ref()
            .filter(|f| f.generic_params.len() == type_args_resolved.len())
            .map(|f| {
                f.generic_params
                    .iter()
                    .zip(type_args_resolved)
                    .map(|(p, a)| (p.name.clone(), a.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let (mut lowered_args, _) =
            self.lower_call_args(args, &expected_param_tys, generic, written);
        let substitution = self.substitute_defaults(function_id, &mut lowered_args);
        // Return type lookup uses the same id when available; the
        // bare-name lookup is the fallback for forward refs.
        let mut ty = callee
            .as_ref()
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
        if let Some(func) = callee.as_ref().filter(|f| !f.generic_params.is_empty()) {
            let subs: HashMap<String, ResolvedType> = if type_args_resolved.is_empty() {
                // No `<...>` at the call site, so the arguments pick
                // the types. Match each parameter's declared type
                // against what the argument lowered to. Without this
                // the call keeps `TypeParam(T)`, the `let` that binds
                // the result keeps it too, and the program fails with
                // an internal error: a field access on `TypeParam(T)`
                // at lowering time, or a leftover type parameter after
                // monomorphisation.
                infer_type_args_from_values(func, &lowered_args)
            } else if func.generic_params.len() == type_args_resolved.len() {
                func.generic_params
                    .iter()
                    .zip(type_args_resolved.iter())
                    .map(|(p, a)| (p.name.clone(), a.clone()))
                    .collect()
            } else {
                HashMap::new()
            };
            substitute_typeparam_in_resolved(&mut ty, &subs);
        }

        if substitution.needs_let_wrapper {
            self.wrap_call_with_let_bindings(
                path_strs,
                function_id,
                lowered_args,
                ty,
                &substitution,
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

    /// Substitute the defaults of the arguments that a call leaves out.
    ///
    /// A positional call appends the trailing defaults. A labelled call
    /// walks the parameters in order and fills each missing label. A
    /// default can read any earlier parameter, and that parameter can
    /// have a default too. When a default reads a parameter name, the
    /// caller wraps the call in a block that binds each parameter in
    /// order (see [`Self::wrap_call_with_let_bindings`]).
    fn substitute_defaults(
        &self,
        function_id: Option<crate::ir::FunctionId>,
        lowered_args: &mut Vec<(Option<String>, IrExpr)>,
    ) -> DefaultSubstitution {
        let mut out = DefaultSubstitution {
            needs_let_wrapper: false,
            wrapper_param_names: Vec::new(),
            wrapper_param_types: Vec::new(),
            explicit: Vec::new(),
        };
        let Some(func) = function_id.and_then(|id| self.module.functions.get(id.0 as usize)) else {
            return out;
        };
        let params: Vec<IrFunctionParam> = func
            .params
            .iter()
            .filter(|p| p.name != "self")
            .cloned()
            .collect();
        if lowered_args.len() >= params.len() {
            return out;
        }
        let param_names: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
        let any_labeled = lowered_args.iter().any(|(l, _)| l.is_some());
        let mut new_args: Vec<(Option<String>, IrExpr)> = Vec::with_capacity(params.len());
        for (index, param) in params.iter().enumerate() {
            let written = if any_labeled {
                lowered_args
                    .iter()
                    .position(|(l, _)| l.as_ref().is_some_and(|name| name == &param.name))
                    .map(|pos| lowered_args.remove(pos))
            } else if index < lowered_args.len() {
                // Positional arguments keep their place, so take a
                // copy and replace the list at the end.
                lowered_args.get(index).cloned()
            } else {
                None
            };
            if let Some(arg) = written {
                new_args.push(arg);
                out.explicit.push(true);
            } else if let Some(default) = &param.default {
                if expr_references_any_name(default, &param_names) {
                    out.needs_let_wrapper = true;
                }
                let label = any_labeled.then(|| param.name.clone());
                new_args.push((label, default.clone()));
                out.explicit.push(false);
            } else {
                // A required argument is missing. Semantic analysis
                // reports it, so stop at the first gap.
                break;
            }
            out.wrapper_param_names.push(param.name.clone());
            out.wrapper_param_types.push(param.ty.clone());
        }
        *lowered_args = new_args;
        out
    }

    /// Wrap a call whose defaults read parameter names in a block that
    /// binds each parameter in order:
    /// `{ let arg#a = <written a>; let a = arg#a; let b = <default of b>; f(a: a, b: b) }`.
    ///
    /// The written arguments are bound first, under names that a
    /// program cannot write. So a written argument that reads a caller
    /// binding with a parameter's name reads the caller's binding, not
    /// the parameter.
    fn wrap_call_with_let_bindings(
        &self,
        path_strs: Vec<String>,
        function_id: Option<crate::ir::FunctionId>,
        mut lowered_args: Vec<(Option<String>, IrExpr)>,
        ty: ResolvedType,
        substitution: &DefaultSubstitution,
    ) -> IrExpr {
        let reference = |name: &str, ty: &Option<ResolvedType>| IrExpr::Reference {
            path: vec![name.to_string()],
            target: crate::ir::ReferenceTarget::Unresolved,
            ty: ty.clone().unwrap_or(ResolvedType::Error),
            span: self.current_ir_span(),
        };
        let bind = |name: &str, ty: &Option<ResolvedType>, value: IrExpr| IrBlockStatement::Let {
            binding_id: crate::ir::BindingId(0),
            name: name.to_string(),
            mutable: false,
            ty: ty.clone(),
            value,
            span: self.current_ir_span(),
        };
        let mut written = Vec::new();
        let mut params = Vec::new();
        for (((name, ty), explicit), arg) in substitution
            .wrapper_param_names
            .iter()
            .zip(substitution.wrapper_param_types.iter())
            .zip(substitution.explicit.iter())
            .zip(lowered_args.iter_mut())
        {
            let value = std::mem::replace(&mut arg.1, reference(name, ty));
            if *explicit {
                let hidden = format!("arg#{name}");
                written.push(bind(&hidden, ty, value));
                params.push(bind(name, ty, reference(&hidden, ty)));
            } else {
                params.push(bind(name, ty, value));
            }
        }
        written.extend(params);
        let call = IrExpr::FunctionCall {
            path: path_strs,
            function_id,
            args: lowered_args,
            ty: ty.clone(),
            span: self.current_ir_span(),
        };
        IrExpr::Block {
            statements: written,
            result: Box::new(call),
            ty,
            span: self.current_ir_span(),
        }
    }
}

/// Build the type-parameter substitution for a call that wrote no
/// `<...>`, by matching each parameter's declared type against the
/// type its argument lowered to.
///
/// Returns an empty map unless every generic parameter reached a type
/// that holds no parameter of its own. A partial map would substitute
/// some slots and leave others, which reads as a stranger failure than
/// leaving the call alone for `MonomorphisePass` to finish.
fn infer_type_args_from_values(
    func: &crate::ir::IrFunction,
    lowered_args: &[(Option<String>, IrExpr)],
) -> HashMap<String, ResolvedType> {
    let mut subs: HashMap<String, ResolvedType> = HashMap::new();

    for (index, param) in func.params.iter().enumerate() {
        let Some(declared) = &param.ty else { continue };
        // A labelled call matches by name, a positional one by place.
        let arg = lowered_args
            .iter()
            .find_map(|(label, expr)| label.as_ref().filter(|l| **l == param.name).map(|_| expr))
            .or_else(|| lowered_args.get(index).map(|(_, expr)| expr));
        let Some(arg) = arg else { continue };
        unify_typeparam_in_resolved(declared, arg.ty(), &mut subs);
    }

    let complete = func.generic_params.iter().all(|p| {
        subs.get(&p.name)
            .is_some_and(|found| !holds_a_type_param(found))
    });
    if complete {
        subs
    } else {
        HashMap::new()
    }
}
