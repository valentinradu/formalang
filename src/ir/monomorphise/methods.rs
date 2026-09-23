//! Phase 2d, methods: specialise every method that declares its own
//! type parameters and has a body, once per set of type arguments that
//! a call gives it.
//!
//! A method call takes no `<...>`, so the arguments give the types:
//! each parameter's declared type is matched against its argument's
//! type. The copy gets a name that holds the types (`pair__I32`), empty
//! `generic_params`, and a place at the end of its impl block; each call
//! that means it is rewritten to its name and index. The generic
//! originals are then dropped, and the index of every call into their
//! impl blocks is renumbered.
//!
//! An extern method has no body to copy. It keeps its type parameters,
//! and each call carries the concrete types.
//!
//! The worklist lives in `functions.rs`: a generic function may call a
//! generic method and a generic method may call a generic function, so
//! the two kinds of copy are found together.

use std::collections::HashMap;

use crate::error::CompilerError;
use crate::ir::{DispatchKind, IrExpr, IrFunction, IrImpl, IrModule, MethodIdx, ResolvedType};
use crate::location::Span;

use super::expr_walk::for_each_module_expr_mut;
use super::functions::{contains_type_param, unify_types};
use super::specialise::{substitute_expr_types, substitute_type, type_suffix};

/// `(impl index, method index, type arguments)`: one copy of a generic
/// method.
pub(super) type MethodSpec = (u32, u32, Vec<ResolvedType>);

/// A method that has type parameters of its own and a body to copy.
pub(super) fn is_template(f: &IrFunction) -> bool {
    !f.generic_params.is_empty() && f.body.is_some()
}

/// Whether any impl block holds a generic method with a body.
pub(super) fn any_template(module: &IrModule) -> bool {
    module
        .impls
        .iter()
        .any(|imp| imp.functions.iter().any(is_template))
}

/// The copy that a method call needs, when `expr` calls a generic
/// method with a body and its arguments give every type parameter a
/// concrete type.
pub(super) fn method_call_spec(impls: &[IrImpl], expr: &IrExpr) -> Option<MethodSpec> {
    let IrExpr::MethodCall {
        dispatch: DispatchKind::Static { impl_id },
        method_idx,
        args,
        ..
    } = expr
    else {
        return None;
    };
    let method = impls
        .get(impl_id.0 as usize)?
        .functions
        .get(method_idx.0 as usize)?;
    if !is_template(method) {
        return None;
    }
    let type_args = infer_method_type_args(method, args)?;
    Some((impl_id.0, method_idx.0, type_args))
}

/// Match the declared type of each parameter against the type of the
/// argument that fills it. `None` unless every type parameter of the
/// method reaches a type with no type parameter in it.
fn infer_method_type_args(
    method: &IrFunction,
    args: &[(Option<String>, IrExpr)],
) -> Option<Vec<ResolvedType>> {
    let mut subs: HashMap<String, ResolvedType> = HashMap::new();
    let params = method.params.iter().filter(|p| p.name != "self");
    for (index, param) in params.enumerate() {
        let Some(declared) = &param.ty else { continue };
        // A labelled argument names the parameter by its name or its
        // external label; an unlabelled one fills it by position.
        let arg = args
            .iter()
            .find_map(|(label, e)| {
                label
                    .as_ref()
                    .filter(|l| **l == param.name || param.external_label.as_ref() == Some(l))
                    .map(|_| e)
            })
            .or_else(|| {
                args.get(index)
                    .filter(|(label, _)| label.is_none())
                    .map(|(_, e)| e)
            });
        if let Some(arg) = arg {
            unify_types(declared, arg.ty(), &mut subs);
        }
    }
    method
        .generic_params
        .iter()
        .map(|p| {
            subs.get(&p.name)
                .filter(|ty| !contains_type_param(ty))
                .cloned()
        })
        .collect()
}

/// Copy the generic method that `spec` names for its type arguments,
/// and append the copy to the same impl block. Returns the copy's index
/// in that block.
#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are bounded to a Vec<CompilerError> at the pass boundary"
)]
pub(super) fn specialise_method(
    module: &mut IrModule,
    spec: &MethodSpec,
) -> Result<u32, CompilerError> {
    let (impl_index, method_index, type_args) = spec;
    let missing = || CompilerError::InternalError {
        detail: format!("monomorphise: missing generic method {method_index} of impl {impl_index}"),
        span: Span::default(),
    };
    let imp = module
        .impls
        .get_mut(*impl_index as usize)
        .ok_or_else(missing)?;
    let mut copy = imp
        .functions
        .get(*method_index as usize)
        .cloned()
        .ok_or_else(missing)?;
    let subs: HashMap<String, ResolvedType> = copy
        .generic_params
        .iter()
        .zip(type_args)
        .map(|(p, a)| (p.name.clone(), a.clone()))
        .collect();
    let mut name = copy.name.clone();
    for arg in type_args {
        name.push_str("__");
        type_suffix(arg, &mut name);
    }
    let base = name.clone();
    let mut n: u32 = 2;
    while imp.functions.iter().any(|f| f.name == name) {
        name = format!("{base}#{n}");
        n = n.saturating_add(1);
    }
    copy.name = name;
    copy.generic_params.clear();
    for param in &mut copy.params {
        if let Some(ty) = &mut param.ty {
            substitute_type(ty, &subs);
        }
        if let Some(default) = &mut param.default {
            substitute_expr_types(default, &subs);
        }
    }
    if let Some(ty) = &mut copy.return_type {
        substitute_type(ty, &subs);
    }
    if let Some(body) = &mut copy.body {
        substitute_expr_types(body, &subs);
    }
    let index =
        u32::try_from(imp.functions.len()).map_err(|_| CompilerError::TooManyDefinitions {
            kind: "method",
            span: Span::default(),
        })?;
    imp.functions.push(copy);
    Ok(index)
}

/// Point each call of a generic method at the copy for its types, then
/// drop the generic originals and renumber the calls into their impl
/// blocks.
///
/// A call whose arguments still hold a type parameter sits in a generic
/// body that is itself dropped; it is left alone. A call with concrete
/// arguments and no copy is a defect of this pass, and is reported.
pub(super) fn rewrite_method_calls(
    module: &mut IrModule,
    mapping: &HashMap<MethodSpec, u32>,
) -> Result<(), Vec<CompilerError>> {
    if !any_template(module) {
        return Ok(());
    }
    // Read the templates and the copies' names from a snapshot of the
    // impl blocks, so the walk below may change every body in the
    // module.
    let snapshot = module.impls.clone();
    let mut errors = Vec::new();
    for_each_module_expr_mut(module, &mut |expr| {
        let spec = method_call_spec(&snapshot, expr);
        let IrExpr::MethodCall {
            dispatch: DispatchKind::Static { impl_id },
            method,
            method_idx,
            args,
            ..
        } = expr
        else {
            return;
        };
        let Some(template) = snapshot
            .get(impl_id.0 as usize)
            .and_then(|imp| imp.functions.get(method_idx.0 as usize))
            .filter(|f| is_template(f))
        else {
            return;
        };
        let target = spec.as_ref().and_then(|s| mapping.get(s));
        match target {
            Some(index) => {
                if let Some(copy) = snapshot
                    .get(impl_id.0 as usize)
                    .and_then(|imp| imp.functions.get(*index as usize))
                {
                    method.clone_from(&copy.name);
                }
                *method_idx = MethodIdx(*index);
            }
            None if !args.iter().any(|(_, a)| contains_type_param(a.ty())) => {
                errors.push(CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: no copy of generic method `{}` for the call's argument types",
                        template.name
                    ),
                    span: Span::default(),
                });
            }
            None => {}
        }
    });
    if !errors.is_empty() {
        return Err(errors);
    }
    drop_templates(module);
    Ok(())
}

/// Drop every generic method with a body, and renumber each call into
/// the impl blocks that lost one.
fn drop_templates(module: &mut IrModule) {
    // Per impl block: the new index of each method, `None` for a
    // dropped one.
    let remap: Vec<Vec<Option<u32>>> = module
        .impls
        .iter()
        .map(|imp| {
            let mut next: u32 = 0;
            imp.functions
                .iter()
                .map(|f| {
                    if is_template(f) {
                        None
                    } else {
                        let index = next;
                        next = next.saturating_add(1);
                        Some(index)
                    }
                })
                .collect()
        })
        .collect();
    for imp in &mut module.impls {
        imp.functions.retain(|f| !is_template(f));
    }
    for_each_module_expr_mut(module, &mut |expr| {
        if let IrExpr::MethodCall {
            dispatch: DispatchKind::Static { impl_id },
            method_idx,
            ..
        } = expr
        {
            // A call to a dropped original keeps its index: it sits in
            // a generic body that compaction drops.
            if let Some(Some(index)) = remap
                .get(impl_id.0 as usize)
                .and_then(|block| block.get(method_idx.0 as usize))
            {
                *method_idx = MethodIdx(*index);
            }
        }
    });
}
