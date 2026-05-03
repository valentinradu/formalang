//! Phase 1a: clone each imported generic `External` into the current
//! module under a fresh local id, with substituted type arguments.
//!
//! The mapping returned by [`specialise_external_instantiations`] is fed
//! into [`super::rewrite::rewrite_external_references`] in Phase 2 so
//! every callsite that still names the type via
//! `(module_path, name, type_args)` is rewritten to point at the cloned
//! local definition.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::PrimitiveType;
use crate::error::CompilerError;
use crate::ir::{IrModule, IrTrait, ResolvedType};
use crate::location::Span;

use super::super::specialise::{substitute_type, type_suffix};
use super::super::walkers::walk_expr_types_mut;
use super::externalise::externalise_imported_refs;
use super::naming::qualified_capacity;

/// External generic instantiation key: `(module_path, name, type_args)`.
/// Populated from every `External { type_args, .. }` whose `type_args`
/// is non-empty.
pub(super) type ExternalInstantiation = (Vec<String>, String, Vec<ResolvedType>);

/// Walk the module and collect every external generic instantiation.
fn collect_external_instantiations(module: &IrModule) -> HashSet<ExternalInstantiation> {
    let mut out = HashSet::new();
    super::super::walkers::walk_module_types(module, &mut |ty| {
        collect_external_from_type(ty, &mut out);
    });
    out
}

pub(super) fn collect_external_from_type(
    ty: &ResolvedType,
    out: &mut HashSet<ExternalInstantiation>,
) {
    match ty {
        ResolvedType::External {
            module_path,
            name,
            type_args,
            ..
        } => {
            for a in type_args {
                collect_external_from_type(a, out);
            }
            // Both generic instantiations (non-empty type_args) and
            // non-generic references (empty type_args) are collected so
            // each gets cloned into the local module under a qualified
            // name. Cross-module Direction A: External is transient and
            // never reaches the backend.
            out.insert((module_path.clone(), name.clone(), type_args.clone()));
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            collect_external_from_type(inner, out);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                collect_external_from_type(t, out);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            collect_external_from_type(key_ty, out);
            collect_external_from_type(value_ty, out);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                collect_external_from_type(t, out);
            }
            collect_external_from_type(return_ty, out);
        }
        ResolvedType::Generic { args, .. } => {
            for a in args {
                collect_external_from_type(a, out);
            }
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::TypeParam(_)
        | ResolvedType::Error => {}
    }
}

/// Clone every external generic instantiation into the main module with
/// substituted type arguments. Returns a map from each instantiation to
/// its new local id so Phase 2 can rewrite the External references.
pub(in crate::ir::monomorphise) fn specialise_external_instantiations(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) -> Result<HashMap<ExternalInstantiation, ResolvedType>, Vec<CompilerError>> {
    let mut errors = Vec::new();
    let mut mapping: HashMap<ExternalInstantiation, ResolvedType> = HashMap::new();
    let initial = collect_external_instantiations(module);
    let mut worklist: VecDeque<ExternalInstantiation> = initial.into_iter().collect();

    while let Some(inst) = worklist.pop_front() {
        if mapping.contains_key(&inst) {
            continue;
        }
        let (ref module_path, ref name, ref args) = inst;
        let Some(imported) = imported_modules.get(module_path) else {
            // No IR available for this module — leave the External
            // unspecialised (preserves the prior behaviour for callers
            // who don't supply a complete imports map).
            continue;
        };
        match specialise_external(module, imported, module_path, name, args) {
            Ok((new_ty, more)) => {
                mapping.insert(inst, new_ty);
                worklist.extend(more);
            }
            Err(e) => {
                errors.push(e);
                // Sentinel so we don't keep retrying.
                mapping.insert(inst, ResolvedType::Primitive(PrimitiveType::Never));
            }
        }
    }

    if errors.is_empty() {
        Ok(mapping)
    } else {
        Err(errors)
    }
}

#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are aggregated at the pass boundary"
)]
fn specialise_external(
    module: &mut IrModule,
    imported: &IrModule,
    module_path: &[String],
    name: &str,
    args: &[ResolvedType],
) -> Result<(ResolvedType, Vec<ExternalInstantiation>), CompilerError> {
    if let Some(source) = imported.structs.iter().find(|s| s.name == *name) {
        if source.generic_params.len() != args.len() {
            return Err(CompilerError::GenericArityMismatch {
                name: name.to_string(),
                expected: source.generic_params.len(),
                actual: args.len(),
                span: Span::default(),
            });
        }
        // Reuse an existing clone for the same (module_path, name, args)
        // tuple so a re-run of phase 1a (after inlining adds new External
        // refs) doesn't mint a duplicate `#2` clone alongside the canonical
        // one. The canonical qualified name can't collide with a
        // user-defined struct: `::` is not a legal identifier character.
        if let Some(existing) = canonical_qualified_name(name, args, module_path, module) {
            return Ok((ResolvedType::Struct(existing), Vec::new()));
        }
        let subs: HashMap<String, ResolvedType> = source
            .generic_params
            .iter()
            .zip(args.iter())
            .map(|(p, a)| (p.name.clone(), a.clone()))
            .collect();
        let mangled = mangle_external_name(name, args, module_path, module);
        let mut spec = source.clone();
        spec.name.clone_from(&mangled);
        spec.generic_params.clear();
        for field in &mut spec.fields {
            externalise_imported_refs(&mut field.ty, imported, module_path);
            substitute_type(&mut field.ty, &subs);
            if let Some(expr) = &mut field.default {
                walk_expr_types_mut(expr, &mut |ty| {
                    externalise_imported_refs(ty, imported, module_path);
                    substitute_type(ty, &subs);
                });
            }
        }
        let mut discovered: HashSet<ExternalInstantiation> = HashSet::new();
        for field in &spec.fields {
            collect_external_from_type(&field.ty, &mut discovered);
        }
        let new_id = module.add_struct(mangled, spec)?;
        Ok((
            ResolvedType::Struct(new_id),
            discovered.into_iter().collect(),
        ))
    } else if let Some(source) = imported.enums.iter().find(|e| e.name == *name) {
        if source.generic_params.len() != args.len() {
            return Err(CompilerError::GenericArityMismatch {
                name: name.to_string(),
                expected: source.generic_params.len(),
                actual: args.len(),
                span: Span::default(),
            });
        }
        if let Some(existing) = canonical_qualified_enum(name, args, module_path, module) {
            return Ok((ResolvedType::Enum(existing), Vec::new()));
        }
        let subs: HashMap<String, ResolvedType> = source
            .generic_params
            .iter()
            .zip(args.iter())
            .map(|(p, a)| (p.name.clone(), a.clone()))
            .collect();
        let mangled = mangle_external_name(name, args, module_path, module);
        let mut spec = source.clone();
        spec.name.clone_from(&mangled);
        spec.generic_params.clear();
        for variant in &mut spec.variants {
            for field in &mut variant.fields {
                externalise_imported_refs(&mut field.ty, imported, module_path);
                substitute_type(&mut field.ty, &subs);
                if let Some(expr) = &mut field.default {
                    walk_expr_types_mut(expr, &mut |ty| {
                        externalise_imported_refs(ty, imported, module_path);
                        substitute_type(ty, &subs);
                    });
                }
            }
        }
        let mut discovered: HashSet<ExternalInstantiation> = HashSet::new();
        for variant in &spec.variants {
            for field in &variant.fields {
                collect_external_from_type(&field.ty, &mut discovered);
            }
        }
        let new_id = module.add_enum(mangled, spec)?;
        Ok((ResolvedType::Enum(new_id), discovered.into_iter().collect()))
    } else if let Some(source) = imported.traits.iter().find(|t| t.name == *name) {
        specialise_external_trait(module, imported, source, module_path, name, args)
    } else {
        Err(CompilerError::InternalError {
            detail: format!(
                "monomorphise: imported module {module_path:?} has no type named `{name}` to specialise"
            ),
            span: Span::default(),
        })
    }
}

#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are aggregated at the pass boundary"
)]
fn specialise_external_trait(
    module: &mut IrModule,
    imported: &IrModule,
    source: &IrTrait,
    module_path: &[String],
    name: &str,
    args: &[ResolvedType],
) -> Result<(ResolvedType, Vec<ExternalInstantiation>), CompilerError> {
    if source.generic_params.len() != args.len() {
        return Err(CompilerError::GenericArityMismatch {
            name: name.to_string(),
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
    let mangled = mangle_external_name(name, args, module_path, module);
    let mut spec = source.clone();
    spec.name.clone_from(&mangled);
    spec.generic_params.clear();
    // Trait field types (associated constants / consts on traits) and
    // method signature types both need their imported-id refs externalised
    // and any type-param refs substituted, mirroring the struct/enum loops.
    for field in &mut spec.fields {
        externalise_imported_refs(&mut field.ty, imported, module_path);
        substitute_type(&mut field.ty, &subs);
        if let Some(expr) = &mut field.default {
            walk_expr_types_mut(expr, &mut |ty| {
                externalise_imported_refs(ty, imported, module_path);
                substitute_type(ty, &subs);
            });
        }
    }
    for sig in &mut spec.methods {
        for param in &mut sig.params {
            // `self` params have `ty: None` and inherit from the impl
            // block; they need no rewriting here.
            if let Some(ty) = &mut param.ty {
                externalise_imported_refs(ty, imported, module_path);
                substitute_type(ty, &subs);
            }
        }
        if let Some(rt) = &mut sig.return_type {
            externalise_imported_refs(rt, imported, module_path);
            substitute_type(rt, &subs);
        }
    }
    let mut discovered: HashSet<ExternalInstantiation> = HashSet::new();
    for field in &spec.fields {
        collect_external_from_type(&field.ty, &mut discovered);
    }
    for sig in &spec.methods {
        for param in &sig.params {
            if let Some(ty) = &param.ty {
                collect_external_from_type(ty, &mut discovered);
            }
        }
        if let Some(rt) = &sig.return_type {
            collect_external_from_type(rt, &mut discovered);
        }
    }
    let new_id = module.add_trait(mangled, spec)?;
    Ok((
        ResolvedType::Trait(new_id),
        discovered.into_iter().collect(),
    ))
}

/// Compute the un-deduplicated name for an external specialisation.
/// Mirrors the prefix construction in [`mangle_external_name`] but stops
/// before the `#N` collision-avoidance suffix so callers can probe the
/// module for an existing canonical clone.
fn canonical_external_name(name: &str, args: &[ResolvedType], module_path: &[String]) -> String {
    if args.is_empty() {
        let mut qualified = String::with_capacity(qualified_capacity(module_path, name.len()));
        for segment in module_path {
            qualified.push_str(segment);
            qualified.push_str("::");
        }
        qualified.push_str(name);
        qualified
    } else {
        let mut s = name.to_string();
        for a in args {
            s.push_str("__");
            type_suffix(a, &mut s);
        }
        s
    }
}

/// If a struct under the canonical qualified name already exists, return
/// its id so callers can reuse it instead of cloning a duplicate.
fn canonical_qualified_name(
    name: &str,
    args: &[ResolvedType],
    module_path: &[String],
    module: &IrModule,
) -> Option<crate::ir::StructId> {
    module.struct_id(&canonical_external_name(name, args, module_path))
}

/// Enum variant of [`canonical_qualified_name`].
fn canonical_qualified_enum(
    name: &str,
    args: &[ResolvedType],
    module_path: &[String],
    module: &IrModule,
) -> Option<crate::ir::EnumId> {
    module.enum_id(&canonical_external_name(name, args, module_path))
}

/// Build a unique mangled name for an external specialisation.
///
/// - **Generic** instantiations keep the historical `Name__TypeArgs` shape so
///   existing snapshots / consumers (`Helper__I32`, etc.) are unchanged.
/// - **Non-generic** clones use the qualified `module::path::Name` form to
///   avoid colliding with user-chosen local names. The lexer rejects `::`
///   inside identifiers, so this form is unreachable from source.
fn mangle_external_name(
    name: &str,
    args: &[ResolvedType],
    module_path: &[String],
    module: &IrModule,
) -> String {
    let mut out = if args.is_empty() {
        let mut qualified = String::with_capacity(qualified_capacity(module_path, name.len()));
        for segment in module_path {
            qualified.push_str(segment);
            qualified.push_str("::");
        }
        qualified.push_str(name);
        qualified
    } else {
        let mut s = name.to_string();
        for a in args {
            s.push_str("__");
            type_suffix(a, &mut s);
        }
        s
    };
    if module.struct_id(&out).is_none()
        && module.enum_id(&out).is_none()
        && module.trait_id(&out).is_none()
    {
        return out;
    }
    let base = std::mem::take(&mut out);
    let mut n: u32 = 2;
    loop {
        let candidate = format!("{base}#{n}");
        if module.struct_id(&candidate).is_none()
            && module.enum_id(&candidate).is_none()
            && module.trait_id(&candidate).is_none()
        {
            return candidate;
        }
        n = n.saturating_add(1);
        if n == u32::MAX {
            return candidate;
        }
    }
}
