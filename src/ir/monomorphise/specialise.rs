//! Phase 1b / 1c: clone each generic struct, enum, and trait once per
//! distinct `(base, args)` instantiation, and provide the substitution and
//! name-mangling helpers reused by every other phase.

use std::collections::{HashMap, HashSet};

use crate::ast::PrimitiveType;
use crate::error::CompilerError;
use crate::ir::{EnumId, GenericBase, IrExpr, IrModule, ResolvedType, StructId, TraitId};
use crate::location::Span;

use super::collect::collect_from_type;
use super::walkers::walk_expr_types_mut;

/// A single generic instantiation key: `(base, type_args)`.
pub(super) type Instantiation = (GenericBase, Vec<ResolvedType>);

/// Outcome of specialising a single generic instantiation.
pub(super) type SpecialiseOk = (GenericBase, Vec<Instantiation>);

/// Specialise a single generic instantiation (struct, enum, or trait),
/// appending the clone to the module and returning its new base plus any
/// further instantiations introduced by the clone's field types.
#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are bounded to a Vec<CompilerError> at the pass boundary"
)]
pub(super) fn specialise(
    module: &mut IrModule,
    (base, args): &Instantiation,
) -> Result<SpecialiseOk, CompilerError> {
    match base {
        GenericBase::Struct(id) => specialise_struct(module, *id, args),
        GenericBase::Enum(id) => specialise_enum(module, *id, args),
        GenericBase::Trait(id) => specialise_trait(module, *id, args),
    }
}

#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are bounded to a Vec<CompilerError> at the pass boundary"
)]
fn specialise_struct(
    module: &mut IrModule,
    base_id: StructId,
    args: &[ResolvedType],
) -> Result<SpecialiseOk, CompilerError> {
    let Some(source) = module.get_struct(base_id).cloned() else {
        return Err(CompilerError::InternalError {
            detail: format!(
                "monomorphise: missing struct id {} for instantiation",
                base_id.0
            ),
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

    let mangled = mangle_name(&source.name, args, module);
    let mut spec = source;
    spec.name.clone_from(&mangled);
    spec.generic_params.clear();
    for field in &mut spec.fields {
        substitute_type(&mut field.ty, &subs);
        if let Some(expr) = &mut field.default {
            substitute_expr_types(expr, &subs);
        }
    }

    let mut discovered: HashSet<Instantiation> = HashSet::new();
    for field in &spec.fields {
        collect_from_type(&field.ty, &mut discovered);
    }

    let new_id = module.add_struct(mangled, spec)?;
    Ok((
        GenericBase::Struct(new_id),
        discovered.into_iter().collect(),
    ))
}

#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are bounded to a Vec<CompilerError> at the pass boundary"
)]
fn specialise_enum(
    module: &mut IrModule,
    base_id: EnumId,
    args: &[ResolvedType],
) -> Result<SpecialiseOk, CompilerError> {
    let Some(source) = module.get_enum(base_id).cloned() else {
        return Err(CompilerError::InternalError {
            detail: format!(
                "monomorphise: missing enum id {} for instantiation",
                base_id.0
            ),
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

    let mangled = mangle_name(&source.name, args, module);
    let mut spec = source;
    spec.name.clone_from(&mangled);
    spec.generic_params.clear();
    for variant in &mut spec.variants {
        for field in &mut variant.fields {
            substitute_type(&mut field.ty, &subs);
            if let Some(expr) = &mut field.default {
                substitute_expr_types(expr, &subs);
            }
        }
    }

    let mut discovered: HashSet<Instantiation> = HashSet::new();
    for variant in &spec.variants {
        for field in &variant.fields {
            collect_from_type(&field.ty, &mut discovered);
        }
    }

    let new_id = module.add_enum(mangled, spec)?;
    Ok((GenericBase::Enum(new_id), discovered.into_iter().collect()))
}

#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are bounded to a Vec<CompilerError> at the pass boundary"
)]
fn specialise_trait(
    module: &mut IrModule,
    base_id: TraitId,
    args: &[ResolvedType],
) -> Result<SpecialiseOk, CompilerError> {
    let Some(source) = module.get_trait(base_id).cloned() else {
        return Err(CompilerError::InternalError {
            detail: format!(
                "monomorphise: missing trait id {} for instantiation",
                base_id.0
            ),
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

    let mangled = mangle_name(&source.name, args, module);
    let mut spec = source;
    spec.name.clone_from(&mangled);
    spec.generic_params.clear();
    for field in &mut spec.fields {
        substitute_type(&mut field.ty, &subs);
        if let Some(expr) = &mut field.default {
            substitute_expr_types(expr, &subs);
        }
    }
    for sig in &mut spec.methods {
        for param in &mut sig.params {
            if let Some(t) = &mut param.ty {
                substitute_type(t, &subs);
            }
        }
        if let Some(rt) = &mut sig.return_type {
            substitute_type(rt, &subs);
        }
    }

    let mut discovered: HashSet<Instantiation> = HashSet::new();
    for field in &spec.fields {
        collect_from_type(&field.ty, &mut discovered);
    }
    for sig in &spec.methods {
        for param in &sig.params {
            if let Some(t) = &param.ty {
                collect_from_type(t, &mut discovered);
            }
        }
        if let Some(rt) = &sig.return_type {
            collect_from_type(rt, &mut discovered);
        }
    }

    let new_id = module.add_trait(mangled, spec)?;
    Ok((GenericBase::Trait(new_id), discovered.into_iter().collect()))
}

/// Build a stable mangled name for a specialisation. Collisions with
/// existing names would break `rebuild_indices`, so on the off chance a
/// user-written struct already has the mangled name, we append an
/// incrementing suffix.
pub(super) fn mangle_name(base: &str, args: &[ResolvedType], module: &IrModule) -> String {
    let mut out = base.to_string();
    for a in args {
        out.push_str("__");
        type_suffix(a, &mut out);
    }
    if module.struct_id(&out).is_none() {
        return out;
    }
    let mut n: u32 = 2;
    loop {
        let candidate = format!("{out}#{n}");
        if module.struct_id(&candidate).is_none() {
            return candidate;
        }
        n = n.saturating_add(1);
        if n == u32::MAX {
            // Extraordinarily unlikely; return what we have and let
            // rebuild_indices' debug_assert catch any collision.
            return candidate;
        }
    }
}

pub(super) fn type_suffix(ty: &ResolvedType, out: &mut String) {
    match ty {
        ResolvedType::Primitive(p) => out.push_str(match p {
            PrimitiveType::String => "String",
            PrimitiveType::I32 => "I32",
            PrimitiveType::I64 => "I64",
            PrimitiveType::F32 => "F32",
            PrimitiveType::F64 => "F64",
            PrimitiveType::Boolean => "Boolean",
            PrimitiveType::Path => "Path",
            PrimitiveType::Regex => "Regex",
            PrimitiveType::Never => "Never",
        }),
        ResolvedType::Struct(id) => {
            let _ = write_usize(out, "S", usize::try_from(id.0).unwrap_or(0));
        }
        ResolvedType::Trait(id) => {
            let _ = write_usize(out, "T", usize::try_from(id.0).unwrap_or(0));
        }
        ResolvedType::Enum(id) => {
            let _ = write_usize(out, "E", usize::try_from(id.0).unwrap_or(0));
        }
        ResolvedType::Array(inner) => {
            out.push_str("Arr_");
            type_suffix(inner, out);
        }
        ResolvedType::Range(inner) => {
            out.push_str("Rng_");
            type_suffix(inner, out);
        }
        ResolvedType::Optional(inner) => {
            out.push_str("Opt_");
            type_suffix(inner, out);
        }
        ResolvedType::Tuple(fields) => {
            out.push_str("Tup");
            for (_, t) in fields {
                out.push('_');
                type_suffix(t, out);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            out.push_str("Dict_");
            type_suffix(key_ty, out);
            out.push('_');
            type_suffix(value_ty, out);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            out.push_str("Fn");
            for (_, t) in param_tys {
                out.push('_');
                type_suffix(t, out);
            }
            out.push_str("__ret_");
            type_suffix(return_ty, out);
        }
        ResolvedType::Generic { base, args } => {
            match base {
                GenericBase::Struct(id) => {
                    let _ = write_usize(out, "GS", usize::try_from(id.0).unwrap_or(0));
                }
                GenericBase::Enum(id) => {
                    let _ = write_usize(out, "GE", usize::try_from(id.0).unwrap_or(0));
                }
                GenericBase::Trait(id) => {
                    let _ = write_usize(out, "GT", usize::try_from(id.0).unwrap_or(0));
                }
            }
            for a in args {
                out.push('_');
                type_suffix(a, out);
            }
        }
        ResolvedType::External {
            module_path, name, ..
        } => {
            out.push_str("Ext_");
            for seg in module_path {
                out.push_str(seg);
                out.push('_');
            }
            out.push_str(name);
        }
        ResolvedType::TypeParam(name) => {
            out.push_str("TP_");
            out.push_str(name);
        }
        ResolvedType::Error => {
            out.push_str("Err");
        }
    }
}

fn write_usize(out: &mut String, prefix: &str, n: usize) -> core::fmt::Result {
    use core::fmt::Write;
    write!(out, "{prefix}{n}")
}

// Phase 1c: substitution helpers

pub(super) fn substitute_type(ty: &mut ResolvedType, subs: &HashMap<String, ResolvedType>) {
    match ty {
        ResolvedType::TypeParam(name) => {
            if let Some(concrete) = subs.get(name) {
                *ty = concrete.clone();
            }
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            substitute_type(inner, subs);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                substitute_type(t, subs);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            substitute_type(key_ty, subs);
            substitute_type(value_ty, subs);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                substitute_type(t, subs);
            }
            substitute_type(return_ty, subs);
        }
        ResolvedType::Generic { args, .. } => {
            for a in args {
                substitute_type(a, subs);
            }
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                substitute_type(a, subs);
            }
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Error => {}
    }
}

pub(super) fn substitute_expr_types(expr: &mut IrExpr, subs: &HashMap<String, ResolvedType>) {
    walk_expr_types_mut(expr, &mut |ty| substitute_type(ty, subs));
}
