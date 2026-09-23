//! Phase 2e: specialise every generic function for which a concrete call
//! site exists, then rewrite those call sites to point at the cloned
//! per-arg-tuple specialisations.
//!
//! Inferring the type-arg tuple uses structural unification of the
//! function's declared parameter types against the call site's argument
//! types. The clones have empty `generic_params` and survive Phase 3
//! compaction; the originals are dropped along with any unspecialised
//! generic structs/enums/traits.
//!
//! A generic method with a body is copied in the same worklist; see
//! `methods.rs`.

use std::collections::{HashMap, HashSet};

use crate::error::CompilerError;
use crate::ir::{IrExpr, IrFunction, IrModule, ResolvedType};
use crate::location::Span;

use super::expr_walk::{for_each_module_expr_mut, walk_expr};
use super::methods::{self, MethodSpec};
use super::specialise::{substitute_expr_types, substitute_type, type_suffix};

/// `(function_name, type_arg_tuple)` — the unique key for a generic
/// function specialisation. Mirrors the struct/enum
/// [`super::specialise::Instantiation`] alias but functions live in
/// their own namespace and the base is addressed by name (not yet by
/// id, since generic functions never get `FunctionId`s before
/// specialisation).
type FunctionSpec = (String, Vec<ResolvedType>);

/// One copy that the worklist makes: of a generic function, or of a
/// generic method with a body.
enum Spec {
    Function(FunctionSpec),
    Method(MethodSpec),
}

/// Phase 2e entry point: specialise every generic function and every
/// generic method for which a concrete call site exists, rewrite those
/// call sites, and recurse until the worklist is empty.
///
/// The two kinds share one worklist, because each may call the other:
/// a copy of either can hold the first concrete call of both.
pub(super) fn specialise_generic_functions(
    module: &mut IrModule,
) -> Result<(), Vec<CompilerError>> {
    // Map from `(original_name, type_arg_tuple)` to the specialised
    // function's name. Used both as the "already specialised" set and
    // as the rewrite table for call sites.
    let mut fn_mapping: HashMap<FunctionSpec, String> = HashMap::new();
    let mut method_mapping: HashMap<MethodSpec, u32> = HashMap::new();
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
    if generic_fn_names.is_empty() && !methods::any_template(module) {
        return Ok(());
    }

    // Worklist of copies to make. Each newly-cloned body may discover
    // further specialisations.
    let mut worklist: Vec<Spec> = Vec::new();
    collect_generic_fn_call_specs(module, &generic_fn_names, &mut worklist);

    while let Some(spec) = worklist.pop() {
        match spec {
            Spec::Function(spec) => {
                if fn_mapping.contains_key(&spec) {
                    continue;
                }
                match specialise_function(module, &spec.0, &spec.1) {
                    Ok(mangled_name) => {
                        let body = module
                            .function_id(&mangled_name)
                            .and_then(|id| module.functions.get(id.0 as usize))
                            .and_then(|f| f.body.as_ref());
                        if let Some(body) = body {
                            discover(module, &generic_fn_names, body, &mut worklist);
                        }
                        fn_mapping.insert(spec, mangled_name);
                    }
                    Err(e) => errors.push(e),
                }
            }
            Spec::Method(spec) => {
                if method_mapping.contains_key(&spec) {
                    continue;
                }
                match methods::specialise_method(module, &spec) {
                    Ok(index) => {
                        let body = module
                            .impls
                            .get(spec.0 as usize)
                            .and_then(|imp| imp.functions.get(index as usize))
                            .and_then(|f| f.body.as_ref());
                        if let Some(body) = body {
                            discover(module, &generic_fn_names, body, &mut worklist);
                        }
                        method_mapping.insert(spec, index);
                    }
                    Err(e) => errors.push(e),
                }
            }
        }
    }

    // Rewrite every call site that resolved to a generic-fn name.
    rewrite_function_call_paths(module, &fn_mapping, &generic_fn_names);
    if let Err(mut e) = methods::rewrite_method_calls(module, &method_mapping) {
        errors.append(&mut e);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Push a copy for each concrete call in `expr` of a generic function
/// or of a generic method with a body.
fn discover(
    module: &IrModule,
    generic_fn_names: &HashSet<String>,
    expr: &IrExpr,
    out: &mut Vec<Spec>,
) {
    walk_expr(expr, &mut |e| {
        if let IrExpr::FunctionCall { path, args, .. } = e {
            if let Some(name) = matching_generic_name(path, generic_fn_names) {
                if let Some(func) = module.functions.iter().find(|f| f.name == name) {
                    if let Some(type_args) = infer_call_type_args(func, args) {
                        out.push(Spec::Function((name, type_args)));
                    }
                }
            }
        }
        if let Some(spec) = methods::method_call_spec(&module.impls, e) {
            out.push(Spec::Method(spec));
        }
    });
}

/// Which generic function, if any, a call's path names.
///
/// A function declared inside a module is registered under its
/// qualified name — `m::identity` — while the call that reaches it
/// writes `m::identity(...)`, whose last segment is just `identity`.
/// Matching on the last segment alone therefore never found it, so a
/// generic inside a module was never specialised: the template was
/// compacted away and the call was left pointing at whatever took its
/// index, which for a small module was the caller itself.
fn matching_generic_name(path: &[String], generic_fn_names: &HashSet<String>) -> Option<String> {
    let joined = path.join("::");
    if generic_fn_names.contains(&joined) {
        return Some(joined);
    }
    path.last()
        .filter(|last| generic_fn_names.contains(*last))
        .cloned()
}

/// Walk every expression in the module looking for calls of a generic
/// function or of a generic method with a body. For each, infer the
/// type-arg tuple from arg types and append it to the worklist. Call
/// sites whose inferred args still contain `TypeParam` (i.e. the call
/// lives inside a generic body that hasn't been specialised yet) are
/// skipped — those will surface again in a later worklist iteration
/// after their containing definition is cloned.
fn collect_generic_fn_call_specs(
    module: &IrModule,
    generic_fn_names: &HashSet<String>,
    out: &mut Vec<Spec>,
) {
    let functions = module
        .functions
        .iter()
        .chain(module.impls.iter().flat_map(|imp| &imp.functions));
    for f in functions {
        if let Some(body) = &f.body {
            discover(module, generic_fn_names, body, out);
        }
    }
    for l in &module.lets {
        discover(module, generic_fn_names, &l.value, out);
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
pub(super) fn unify_types(
    param: &ResolvedType,
    arg: &ResolvedType,
    subs: &mut HashMap<String, ResolvedType>,
) {
    match (param, arg) {
        (ResolvedType::TypeParam(name), concrete) => {
            subs.entry(name.clone()).or_insert_with(|| concrete.clone());
        }
        (ResolvedType::Tuple(ps), ResolvedType::Tuple(as_)) => {
            for ((_, p), (_, a)) in ps.iter().zip(as_.iter()) {
                unify_types(p, a, subs);
            }
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

pub(super) fn contains_type_param(ty: &ResolvedType) -> bool {
    match ty {
        ResolvedType::TypeParam(_) => true,
        ResolvedType::Tuple(fields) => fields.iter().any(|(_, t)| contains_type_param(t)),
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
/// new specialised name; the caller scans the clone's body for further
/// copies to make.
#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are bounded to a Vec<CompilerError> at the pass boundary"
)]
fn specialise_function(
    module: &mut IrModule,
    name: &str,
    args: &[ResolvedType],
) -> Result<String, CompilerError> {
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

    module.add_function(mangled.clone(), spec)?;
    Ok(mangled)
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
    for_each_module_expr_mut(module, &mut |expr| {
        rewrite_call_path_node(expr, fn_mapping, generic_fn_names, &snapshot);
    });
}

fn rewrite_call_path_node(
    expr: &mut IrExpr,
    fn_mapping: &HashMap<FunctionSpec, String>,
    generic_fn_names: &HashSet<String>,
    snapshot: &[IrFunction],
) {
    if let IrExpr::FunctionCall {
        path,
        function_id: _,
        args,
        ty,
        ..
    } = expr
    {
        let Some(name) = matching_generic_name(path, generic_fn_names) else {
            return;
        };
        let Some(callee) = snapshot.iter().find(|f| f.name == name) else {
            return;
        };
        let Some(type_args) = infer_call_type_args(callee, args) else {
            return;
        };
        if let Some(specialised) = fn_mapping.get(&(name, type_args.clone())) {
            // The specialised name already carries any module prefix,
            // so it replaces the whole path rather than its last
            // segment — otherwise `m::identity` would become
            // `m::m::identity__I32`.
            *path = vec![specialised.clone()];
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
