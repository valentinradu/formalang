//! Type-parameter substitution and unification over [`ResolvedType`].
//!
//! Split out of `helpers.rs` to keep each file under the line ceiling
//! that `scripts/check_file_sizes.sh` enforces.

use crate::ir::ResolvedType;
use std::collections::HashMap;

/// Substitute `TypeParam(name)` references inside `ty` using `subs`.
/// Used by `resolve_method_return_type` when the receiver is a
/// `Generic { base, args }` so the impl method's return type
/// (declared in terms of the struct's generic params) gets the
/// concrete instantiation's type arguments.
pub(in crate::ir::lower::expr) fn substitute_typeparam_in_resolved(
    ty: &mut ResolvedType,
    subs: &HashMap<String, ResolvedType>,
) {
    match ty {
        ResolvedType::TypeParam(name) => {
            if let Some(concrete) = subs.get(name) {
                *ty = concrete.clone();
            }
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                substitute_typeparam_in_resolved(t, subs);
            }
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

/// Walk a declared type and a concrete type in parallel, binding each
/// `TypeParam(P)` on the declared side to what stands in its place.
///
/// This is the lowering-time twin of the unifier in
/// `ir::monomorphise::functions`. Lowering needs its own because the
/// type it writes into a call expression becomes the type of the `let`
/// that binds the result, and that happens long before
/// monomorphisation runs. A first binding wins: a conflict means the
/// same parameter took two types, which the semantic pass reports.
pub(in crate::ir::lower::expr) fn unify_typeparam_in_resolved(
    declared: &ResolvedType,
    concrete: &ResolvedType,
    subs: &mut HashMap<String, ResolvedType>,
) {
    match (declared, concrete) {
        (ResolvedType::TypeParam(name), found) => {
            subs.entry(name.clone()).or_insert_with(|| found.clone());
        }
        (ResolvedType::Tuple(declared_fields), ResolvedType::Tuple(found_fields)) => {
            for ((_, d), (_, f)) in declared_fields.iter().zip(found_fields.iter()) {
                unify_typeparam_in_resolved(d, f, subs);
            }
        }
        (
            ResolvedType::Closure {
                param_tys: declared_params,
                return_ty: declared_ret,
            },
            ResolvedType::Closure {
                param_tys: found_params,
                return_ty: found_ret,
            },
        ) => {
            for ((_, d), (_, f)) in declared_params.iter().zip(found_params.iter()) {
                unify_typeparam_in_resolved(d, f, subs);
            }
            unify_typeparam_in_resolved(declared_ret, found_ret, subs);
        }
        (
            ResolvedType::Generic {
                args: declared_args,
                ..
            },
            ResolvedType::Generic {
                args: found_args, ..
            },
        )
        | (
            ResolvedType::External {
                type_args: declared_args,
                ..
            },
            ResolvedType::External {
                type_args: found_args,
                ..
            },
        ) => {
            for (d, f) in declared_args.iter().zip(found_args.iter()) {
                unify_typeparam_in_resolved(d, f, subs);
            }
        }
        _ => {}
    }
}

/// Whether `ty` still mentions a type parameter anywhere inside it.
pub(in crate::ir::lower::expr) fn holds_a_type_param(ty: &ResolvedType) -> bool {
    match ty {
        ResolvedType::TypeParam(_) => true,
        ResolvedType::Tuple(fields) => fields.iter().any(|(_, t)| holds_a_type_param(t)),
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => param_tys.iter().any(|(_, t)| holds_a_type_param(t)) || holds_a_type_param(return_ty),
        ResolvedType::Generic { args, .. }
        | ResolvedType::External {
            type_args: args, ..
        } => args.iter().any(holds_a_type_param),
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Error => false,
    }
}
