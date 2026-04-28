//! Phase 2e: specialise every generic function for which a concrete call
//! site exists, then rewrite those call sites to point at the cloned
//! per-arg-tuple specialisations.
//!
//! Inferring the type-arg tuple uses structural unification of the
//! function's declared parameter types against the call site's argument
//! types. The clones have empty `generic_params` and survive Phase 3
//! compaction; the originals are dropped along with any unspecialised
//! generic structs/enums/traits.

use std::collections::{HashMap, HashSet};

use crate::error::CompilerError;
use crate::ir::{IrExpr, IrFunction, IrModule, ResolvedType};
use crate::location::Span;

use super::expr_walk::{iter_expr_children_mut, walk_expr};
use super::specialise::{substitute_expr_types, substitute_type, type_suffix};

/// `(function_name, type_arg_tuple)` — the unique key for a generic
/// function specialisation. Mirrors the struct/enum
/// [`super::specialise::Instantiation`] alias but functions live in
/// their own namespace and the base is addressed by name (not yet by
/// id, since generic functions never get `FunctionId`s before
/// specialisation).
type FunctionSpec = (String, Vec<ResolvedType>);

/// Phase 2e entry point: specialise every generic function for which a
/// concrete call site exists, rewrite those call sites, and recurse
/// until the worklist is empty.
pub(super) fn specialise_generic_functions(
    module: &mut IrModule,
) -> Result<(), Vec<CompilerError>> {
    // Map from `(original_name, type_arg_tuple)` to the specialised
    // function's name. Used both as the "already specialised" set and
    // as the rewrite table for call sites.
    let mut fn_mapping: HashMap<FunctionSpec, String> = HashMap::new();
    let mut errors: Vec<CompilerError> = Vec::new();

    // Snapshot the set of currently-generic function names so the
    // collector knows which call sites to consider — this is stable
    // across worklist iterations because we never *add* generic
    // functions, only specialised (non-generic) clones.
    let generic_fn_names: HashSet<String> = module
        .functions
        .iter()
        .filter(|f| !f.generic_params.is_empty())
        .map(|f| f.name.clone())
        .collect();
    if generic_fn_names.is_empty() {
        return Ok(());
    }

    // Worklist of `(original_name, args)` pairs to specialise. Each
    // newly-cloned body may discover further specialisations.
    let mut worklist: Vec<FunctionSpec> = Vec::new();
    collect_generic_fn_call_specs(module, &generic_fn_names, &mut worklist);

    while let Some(spec) = worklist.pop() {
        if fn_mapping.contains_key(&spec) {
            continue;
        }
        match specialise_function(module, &spec.0, &spec.1) {
            Ok((mangled_name, discovered)) => {
                fn_mapping.insert(spec, mangled_name);
                for d in discovered {
                    if !fn_mapping.contains_key(&d) {
                        worklist.push(d);
                    }
                }
            }
            Err(e) => errors.push(e),
        }
    }

    // Rewrite every call site that resolved to a generic-fn name.
    rewrite_function_call_paths(module, &fn_mapping, &generic_fn_names);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Walk every expression in the module looking for `FunctionCall`
/// sites whose path resolves to a generic function. For each, infer
/// the type-arg tuple from arg types and append `(name, args)` to the
/// worklist. Call sites whose inferred args still contain `TypeParam`
/// (i.e. the call lives inside a generic function body that hasn't
/// been specialised yet) are skipped — those will surface again in a
/// later worklist iteration after their containing function is cloned.
fn collect_generic_fn_call_specs(
    module: &IrModule,
    generic_fn_names: &HashSet<String>,
    out: &mut Vec<FunctionSpec>,
) {
    let mut visit = |expr: &IrExpr| {
        if let IrExpr::FunctionCall { path, args, .. } = expr {
            if let Some(name) = path.last() {
                if generic_fn_names.contains(name) {
                    if let Some(func) = module.functions.iter().find(|f| f.name == *name) {
                        if let Some(type_args) = infer_call_type_args(func, args) {
                            out.push((name.clone(), type_args));
                        }
                    }
                }
            }
        }
    };
    for f in &module.functions {
        if let Some(body) = &f.body {
            walk_expr(body, &mut visit);
        }
    }
    for imp in &module.impls {
        for f in &imp.functions {
            if let Some(body) = &f.body {
                walk_expr(body, &mut visit);
            }
        }
    }
    for l in &module.lets {
        walk_expr(&l.value, &mut visit);
    }
}

/// Match a generic function's declared parameter types against the
/// resolved types of the arguments at a call site, building a
/// substitution from each `TypeParam` to its concrete type. Returns
/// `Some(args_in_param_order)` when every generic param was inferred
/// to a concrete type; `None` otherwise (typically because the call
/// site sits inside an uninstantiated generic context).
fn infer_call_type_args(
    func: &IrFunction,
    call_args: &[(Option<String>, IrExpr)],
) -> Option<Vec<ResolvedType>> {
    let mut subs: HashMap<String, ResolvedType> = HashMap::new();
    for (i, param) in func.params.iter().enumerate() {
        let Some(declared) = &param.ty else { continue };
        // Match args by name when the call is named; otherwise by
        // position. Keeps the inference robust against label-style
        // calls (`f(x: 1, y: 2)`) used elsewhere in the lowerer.
        let arg_expr = call_args
            .iter()
            .find_map(|(n, e)| n.as_ref().filter(|name| **name == param.name).map(|_| e))
            .or_else(|| call_args.get(i).map(|(_, e)| e))?;
        unify_types(declared, arg_expr.ty(), &mut subs);
    }
    let mut out = Vec::with_capacity(func.generic_params.len());
    for gp in &func.generic_params {
        let concrete = subs.get(&gp.name)?;
        if contains_type_param(concrete) {
            return None;
        }
        out.push(concrete.clone());
    }
    Some(out)
}

/// Structural unification: walk `param` and `arg` in parallel; when a
/// `TypeParam(P)` appears on the param side, bind `P → arg`. Conflicts
/// (P bound to two different concrete types) are silently dropped —
/// semantic should have caught those, and the resulting partial map
/// merely fails the `infer_call_type_args` post-check.
fn unify_types(param: &ResolvedType, arg: &ResolvedType, subs: &mut HashMap<String, ResolvedType>) {
    match (param, arg) {
        (ResolvedType::TypeParam(name), concrete) => {
            subs.entry(name.clone()).or_insert_with(|| concrete.clone());
        }
        (ResolvedType::Array(p), ResolvedType::Array(a))
        | (ResolvedType::Range(p), ResolvedType::Range(a))
        | (ResolvedType::Optional(p), ResolvedType::Optional(a)) => {
            unify_types(p, a, subs);
        }
        (ResolvedType::Tuple(ps), ResolvedType::Tuple(as_)) => {
            for ((_, p), (_, a)) in ps.iter().zip(as_.iter()) {
                unify_types(p, a, subs);
            }
        }
        (
            ResolvedType::Dictionary {
                key_ty: pk,
                value_ty: pv,
            },
            ResolvedType::Dictionary {
                key_ty: ak,
                value_ty: av,
            },
        ) => {
            unify_types(pk, ak, subs);
            unify_types(pv, av, subs);
        }
        (
            ResolvedType::Closure {
                param_tys: pp,
                return_ty: pr,
            },
            ResolvedType::Closure {
                param_tys: ap,
                return_ty: ar,
            },
        ) => {
            for ((_, p), (_, a)) in pp.iter().zip(ap.iter()) {
                unify_types(p, a, subs);
            }
            unify_types(pr, ar, subs);
        }
        (
            ResolvedType::Generic { base: pb, args: pa },
            ResolvedType::Generic { base: ab, args: aa },
        ) if pb == ab => {
            for (p, a) in pa.iter().zip(aa.iter()) {
                unify_types(p, a, subs);
            }
        }
        // Concrete-vs-concrete or shape-mismatch: nothing to bind.
        _ => {}
    }
}

fn contains_type_param(ty: &ResolvedType) -> bool {
    match ty {
        ResolvedType::TypeParam(_) => true,
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            contains_type_param(inner)
        }
        ResolvedType::Tuple(fields) => fields.iter().any(|(_, t)| contains_type_param(t)),
        ResolvedType::Dictionary { key_ty, value_ty } => {
            contains_type_param(key_ty) || contains_type_param(value_ty)
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            param_tys.iter().any(|(_, t)| contains_type_param(t)) || contains_type_param(return_ty)
        }
        ResolvedType::Generic { args, .. } => args.iter().any(contains_type_param),
        ResolvedType::External { type_args, .. } => type_args.iter().any(contains_type_param),
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Error => false,
    }
}

/// Clone a generic function for one concrete arg-tuple. Returns the
/// new specialised name plus any further generic-fn instantiations
/// discovered inside the cloned body (so the caller can extend the
/// worklist).
#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are bounded to a Vec<CompilerError> at the pass boundary"
)]
fn specialise_function(
    module: &mut IrModule,
    name: &str,
    args: &[ResolvedType],
) -> Result<(String, Vec<FunctionSpec>), CompilerError> {
    // (mangled_name, discovered)
    let Some(source) = module.functions.iter().find(|f| f.name == name).cloned() else {
        return Err(CompilerError::InternalError {
            detail: format!("monomorphise: missing generic function `{name}`"),
            span: Span::default(),
        });
    };
    if source.generic_params.len() != args.len() {
        return Err(CompilerError::GenericArityMismatch {
            name: source.name.clone(),
            expected: source.generic_params.len(),
            actual: args.len(),
            span: Span::default(),
        });
    }

    let subs: HashMap<String, ResolvedType> = source
        .generic_params
        .iter()
        .zip(args.iter())
        .map(|(p, a)| (p.name.clone(), a.clone()))
        .collect();

    let mangled = mangle_function_name(&source.name, args, module);
    let mut spec = source;
    spec.name.clone_from(&mangled);
    spec.generic_params.clear();
    for param in &mut spec.params {
        if let Some(t) = &mut param.ty {
            substitute_type(t, &subs);
        }
        if let Some(default) = &mut param.default {
            substitute_expr_types(default, &subs);
        }
    }
    if let Some(rt) = &mut spec.return_type {
        substitute_type(rt, &subs);
    }
    if let Some(body) = &mut spec.body {
        substitute_expr_types(body, &subs);
    }

    // After substitution, scan the new body for further generic-fn
    // calls that became concrete in the process.
    let generic_fn_names: HashSet<String> = module
        .functions
        .iter()
        .filter(|f| !f.generic_params.is_empty())
        .map(|f| f.name.clone())
        .collect();
    let mut discovered: Vec<FunctionSpec> = Vec::new();
    if let Some(body) = &spec.body {
        let mut visit = |expr: &IrExpr| {
            if let IrExpr::FunctionCall { path, args: a, .. } = expr {
                if let Some(callee) = path.last() {
                    if generic_fn_names.contains(callee) {
                        if let Some(callee_fn) = module.functions.iter().find(|f| f.name == *callee)
                        {
                            if let Some(type_args) = infer_call_type_args(callee_fn, a) {
                                discovered.push((callee.clone(), type_args));
                            }
                        }
                    }
                }
            }
        };
        walk_expr(body, &mut visit);
    }

    module.add_function(mangled.clone(), spec)?;
    Ok((mangled, discovered))
}

/// Mangle a function name with its concrete type args. Mirrors
/// `mangle_name` for structs/enums but checks the function namespace
/// for collisions.
fn mangle_function_name(base: &str, args: &[ResolvedType], module: &IrModule) -> String {
    let mut out = base.to_string();
    for a in args {
        out.push_str("__");
        type_suffix(a, &mut out);
    }
    if module.function_id(&out).is_none() {
        return out;
    }
    let mut n: u32 = 2;
    loop {
        let candidate = format!("{out}#{n}");
        if module.function_id(&candidate).is_none() {
            return candidate;
        }
        n = n.saturating_add(1);
        if n == u32::MAX {
            return candidate;
        }
    }
}

/// Walk every `FunctionCall` site and rewrite the path's last segment
/// from a generic-function name to the specialised clone's name.
/// Calls whose `(name, inferred_args)` pair has no entry in the
/// mapping (typically because they sit inside an unspecialised
/// generic body) are left untouched and will be dropped along with
/// their containing function in compaction.
fn rewrite_function_call_paths(
    module: &mut IrModule,
    fn_mapping: &HashMap<FunctionSpec, String>,
    generic_fn_names: &HashSet<String>,
) {
    // Snapshot the function map so we can read declared param types
    // while mutating expressions inside the same function vector.
    let snapshot: Vec<IrFunction> = module.functions.clone();
    for f in &mut module.functions {
        if let Some(body) = &mut f.body {
            rewrite_call_paths_expr(body, fn_mapping, generic_fn_names, &snapshot);
        }
        for param in &mut f.params {
            if let Some(default) = &mut param.default {
                rewrite_call_paths_expr(default, fn_mapping, generic_fn_names, &snapshot);
            }
        }
    }
    for imp in &mut module.impls {
        for f in &mut imp.functions {
            if let Some(body) = &mut f.body {
                rewrite_call_paths_expr(body, fn_mapping, generic_fn_names, &snapshot);
            }
            for param in &mut f.params {
                if let Some(default) = &mut param.default {
                    rewrite_call_paths_expr(default, fn_mapping, generic_fn_names, &snapshot);
                }
            }
        }
    }
    for s in &mut module.structs {
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                rewrite_call_paths_expr(default, fn_mapping, generic_fn_names, &snapshot);
            }
        }
    }
    for e in &mut module.enums {
        for variant in &mut e.variants {
            for field in &mut variant.fields {
                if let Some(default) = &mut field.default {
                    rewrite_call_paths_expr(default, fn_mapping, generic_fn_names, &snapshot);
                }
            }
        }
    }
    for l in &mut module.lets {
        rewrite_call_paths_expr(&mut l.value, fn_mapping, generic_fn_names, &snapshot);
    }
}

fn rewrite_call_paths_expr(
    expr: &mut IrExpr,
    fn_mapping: &HashMap<FunctionSpec, String>,
    generic_fn_names: &HashSet<String>,
    snapshot: &[IrFunction],
) {
    for child in iter_expr_children_mut(expr) {
        rewrite_call_paths_expr(child, fn_mapping, generic_fn_names, snapshot);
    }
    if let IrExpr::FunctionCall {
        path,
        function_id: _,
        args,
        ty,
    } = expr
    {
        let Some(last) = path.last() else { return };
        if !generic_fn_names.contains(last) {
            return;
        }
        let Some(callee) = snapshot.iter().find(|f| f.name == *last) else {
            return;
        };
        let Some(type_args) = infer_call_type_args(callee, args) else {
            return;
        };
        if let Some(specialised) = fn_mapping.get(&(last.clone(), type_args.clone())) {
            if let Some(seg) = path.last_mut() {
                seg.clone_from(specialised);
            }
            // Rewrite the call's stored return type by substituting
            // each generic param with the inferred concrete arg.
            // Without this, `let n: I32 = identity(1)` keeps the
            // call's `ty: TypeParam(T)` and the leftover scanner
            // flags it.
            let subs: HashMap<String, ResolvedType> = callee
                .generic_params
                .iter()
                .zip(type_args.iter())
                .map(|(p, a)| (p.name.clone(), a.clone()))
                .collect();
            substitute_type(ty, &subs);
        }
    }
}
