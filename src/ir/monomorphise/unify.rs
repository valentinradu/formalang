//! Matching a declared parameter type against an argument type, to
//! find what each type parameter stands for.

use std::collections::HashMap;

use crate::ir::ResolvedType;

use super::origins::Origins;

/// Two concrete types that one type parameter must both stand for.
#[derive(Debug)]
pub(super) struct Conflict {
    pub(super) param: String,
    pub(super) first: ResolvedType,
    pub(super) second: ResolvedType,
}

impl Conflict {
    /// The text of the `InternalError` for a call of `callee`.
    pub(super) fn detail(&self, callee: &str) -> String {
        format!(
            "monomorphise: a call of `{callee}` binds the type parameter `{}` to both {:?} and {:?}",
            self.param, self.first, self.second
        )
    }
}

/// Structural unification: walk `param` and `arg` in parallel; when a
/// `TypeParam(P)` appears on the param side, bind `P → arg`.
///
/// # Errors
///
/// Returns a [`Conflict`] when `P` must stand for two different
/// concrete types. The semantic pass rejects such a call, so a conflict
/// here is a compiler defect. A type that holds a type parameter, the
/// type `Never` and the error type make no conflict: they can stand in
/// any place.
///
/// An argument can be a specialisation that Phase 2 already made, such
/// as `Struct(Box__I32)` for a parameter `Box<T>`. `origins` gives the
/// instantiation behind it, and the unification continues on that.
pub(super) fn unify_types(
    param: &ResolvedType,
    arg: &ResolvedType,
    subs: &mut HashMap<String, ResolvedType>,
    origins: &Origins,
) -> Result<(), Box<Conflict>> {
    if let (ResolvedType::Generic { .. }, Some((base, args))) = (param, origins.of(arg)) {
        let original = ResolvedType::Generic {
            base: *base,
            args: args.clone(),
        };
        return unify_types(param, &original, subs, origins);
    }
    match (param, arg) {
        (ResolvedType::TypeParam(name), concrete) => {
            let concrete = origins.canonical(concrete);
            match subs.get(name) {
                None => {
                    subs.insert(name.clone(), concrete);
                }
                Some(first) if *first != concrete && is_fixed(first) && is_fixed(&concrete) => {
                    return Err(Box::new(Conflict {
                        param: name.clone(),
                        first: first.clone(),
                        second: concrete,
                    }));
                }
                Some(_) => {}
            }
        }
        (ResolvedType::Tuple(ps), ResolvedType::Tuple(as_)) => {
            for ((_, p), (_, a)) in ps.iter().zip(as_.iter()) {
                unify_types(p, a, subs, origins)?;
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
                unify_types(p, a, subs, origins)?;
            }
            unify_types(pr, ar, subs, origins)?;
        }
        (
            ResolvedType::Generic { base: pb, args: pa },
            ResolvedType::Generic { base: ab, args: aa },
        ) if pb == ab => {
            for (p, a) in pa.iter().zip(aa.iter()) {
                unify_types(p, a, subs, origins)?;
            }
        }
        // Concrete-vs-concrete or shape-mismatch: nothing to bind.
        _ => {}
    }
    Ok(())
}

/// Whether `ty` is one fixed type: it holds no type parameter, and it
/// is not `Never` or the error type, which fit in any place.
fn is_fixed(ty: &ResolvedType) -> bool {
    !contains_type_param(ty)
        && !matches!(
            ty,
            ResolvedType::Error | ResolvedType::Primitive(crate::ast::PrimitiveType::Never)
        )
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
