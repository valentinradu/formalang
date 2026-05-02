//! Phase 1a: clone each imported generic `External` into the current
//! module under a fresh local id, with substituted type arguments.
//!
//! The mapping returned by [`specialise_external_instantiations`] is fed
//! into [`rewrite_external_references`] in Phase 2 so every callsite that
//! still names the type via `(module_path, name, type_args)` is rewritten
//! to point at the cloned local definition.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::PrimitiveType;
use crate::error::CompilerError;
use crate::ir::{GenericBase, ImportedKind, IrModule, IrTrait, ResolvedType};
use crate::location::Span;

use super::specialise::{substitute_type, type_suffix};
use super::walkers::{
    walk_expr_types_mut, walk_function_types_mut, walk_module_types, walk_module_types_mut,
};

/// External generic instantiation key: `(module_path, name, type_args)`.
/// Populated from every `External { type_args, .. }` whose `type_args`
/// is non-empty.
type ExternalInstantiation = (Vec<String>, String, Vec<ResolvedType>);

/// Walk the module and collect every external generic instantiation.
fn collect_external_instantiations(module: &IrModule) -> HashSet<ExternalInstantiation> {
    let mut out = HashSet::new();
    walk_module_types(module, &mut |ty| collect_external_from_type(ty, &mut out));
    out
}

fn collect_external_from_type(ty: &ResolvedType, out: &mut HashSet<ExternalInstantiation>) {
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
pub(super) fn specialise_external_instantiations(
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
    Ok((ResolvedType::Trait(new_id), discovered.into_iter().collect()))
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
        let mut qualified = String::with_capacity(
            module_path.iter().map(String::len).sum::<usize>()
                + module_path.len() * 2
                + name.len(),
        );
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

/// Translate references inside a cloned imported definition: any
/// `Struct/Trait/Enum` ID points into the imported module's index space
/// and is invalid in the main module. Replace those with `External`
/// references that name the same type via its module path so later
/// resolution remains logical, not positional.
fn externalise_imported_refs(ty: &mut ResolvedType, imported: &IrModule, module_path: &[String]) {
    match ty {
        ResolvedType::Struct(id) => {
            if let Some(s) = imported.get_struct(*id) {
                *ty = ResolvedType::External {
                    module_path: module_path.to_vec(),
                    name: s.name.clone(),
                    kind: ImportedKind::Struct,
                    type_args: vec![],
                };
            }
        }
        ResolvedType::Enum(id) => {
            if let Some(e) = imported.get_enum(*id) {
                *ty = ResolvedType::External {
                    module_path: module_path.to_vec(),
                    name: e.name.clone(),
                    kind: ImportedKind::Enum,
                    type_args: vec![],
                };
            }
        }
        ResolvedType::Trait(id) => {
            if let Some(t) = imported.get_trait(*id) {
                *ty = ResolvedType::External {
                    module_path: module_path.to_vec(),
                    name: t.name.clone(),
                    kind: ImportedKind::Trait,
                    type_args: vec![],
                };
            }
        }
        ResolvedType::Generic { base, args } => {
            // Translate the base ID to a logical name, and externalise
            // each generic argument too. Remains an instantiation so
            // collect_external_instantiations re-discovers it.
            let (base_name, kind) = match base {
                GenericBase::Struct(id) => imported
                    .get_struct(*id)
                    .map(|s| (s.name.clone(), ImportedKind::Struct)),
                GenericBase::Enum(id) => imported
                    .get_enum(*id)
                    .map(|e| (e.name.clone(), ImportedKind::Enum)),
                GenericBase::Trait(id) => imported
                    .get_trait(*id)
                    .map(|t| (t.name.clone(), ImportedKind::Trait)),
            }
            .unwrap_or_else(|| (String::new(), ImportedKind::Struct));
            for a in args.iter_mut() {
                externalise_imported_refs(a, imported, module_path);
            }
            *ty = ResolvedType::External {
                module_path: module_path.to_vec(),
                name: base_name,
                kind,
                type_args: std::mem::take(args),
            };
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            externalise_imported_refs(inner, imported, module_path);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                externalise_imported_refs(t, imported, module_path);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            externalise_imported_refs(key_ty, imported, module_path);
            externalise_imported_refs(value_ty, imported, module_path);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                externalise_imported_refs(t, imported, module_path);
            }
            externalise_imported_refs(return_ty, imported, module_path);
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                externalise_imported_refs(a, imported, module_path);
            }
        }
        ResolvedType::Primitive(_) | ResolvedType::TypeParam(_) | ResolvedType::Error => {}
    }
}

/// Phase 2 helper: rewrite every `External { type_args, .. }` whose
/// `(module_path, name, type_args)` was specialised to the cloned
/// local Struct/Enum.
pub(super) fn rewrite_external_references(
    module: &mut IrModule,
    mapping: &HashMap<ExternalInstantiation, ResolvedType>,
) {
    walk_module_types_mut(module, |ty| rewrite_external_type(ty, mapping));
}

fn rewrite_external_type(
    ty: &mut ResolvedType,
    mapping: &HashMap<ExternalInstantiation, ResolvedType>,
) {
    match ty {
        ResolvedType::External {
            module_path,
            name,
            type_args,
            ..
        } => {
            for a in type_args.iter_mut() {
                rewrite_external_type(a, mapping);
            }
            // Both generic and non-generic externals are eligible; the
            // collection step now enqueues both.
            let key = (module_path.clone(), name.clone(), type_args.clone());
            if let Some(new_ty) = mapping.get(&key) {
                *ty = new_ty.clone();
            }
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            rewrite_external_type(inner, mapping);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                rewrite_external_type(t, mapping);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            rewrite_external_type(key_ty, mapping);
            rewrite_external_type(value_ty, mapping);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                rewrite_external_type(t, mapping);
            }
            rewrite_external_type(return_ty, mapping);
        }
        ResolvedType::Generic { args, .. } => {
            for a in args {
                rewrite_external_type(a, mapping);
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

/// Build the qualified `module::path::name` form for cross-module clones.
fn qualified_name(module_path: &[String], name: &str) -> String {
    let mut out = String::with_capacity(
        module_path.iter().map(String::len).sum::<usize>() + module_path.len() * 2 + name.len(),
    );
    for seg in module_path {
        out.push_str(seg);
        out.push_str("::");
    }
    out.push_str(name);
    out
}

/// Phase 1b: inline every imported function into the current module under
/// a qualified name.
///
/// Each clone has its signature and body types externalised via
/// [`externalise_imported_refs`] so subsequent worklist iterations of
/// [`specialise_external_instantiations`] pick up any newly-introduced
/// type references and clone them too. Body id-references
/// (`ReferenceTarget::Function/Struct/Enum/Trait/ModuleLet`,
/// `FunctionCall.function_id`, `DispatchKind::Static.impl_id`) stay
/// in their imported-module id-space — correct for leaf functions whose
/// bodies only reference `Param` / `Local` / primitive types, but will
/// produce stale references if the body touches other module-level
/// items. A follow-up commit walks and remaps those id references.
///
/// FormaLang's IR doesn't carry function visibility today, so every
/// function in each imported module is cloned. Unused clones are removed
/// by `DeadCodeEliminationPass` in the codegen pipeline.
pub(super) fn inline_imported_functions(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) -> Result<(), Vec<CompilerError>> {
    let mut errors = Vec::new();

    for (module_path, imported) in imported_modules {
        for func in &imported.functions {
            let qualified = qualified_name(module_path, &func.name);

            // Skip if a clone with this name already exists. Defends
            // against double-processing when the same module appears
            // under multiple aliases or the pass is run twice.
            if module.function_id(&qualified).is_some() {
                continue;
            }

            let mut clone = func.clone();
            clone.name = qualified.clone();

            // Externalise types in signature + body. The next worklist
            // iteration of specialise_external_instantiations re-collects
            // the produced External references and clones them.
            walk_function_types_mut(&mut clone, &mut |ty| {
                externalise_imported_refs(ty, imported, module_path);
            });

            if let Err(e) = module.add_function(qualified, clone) {
                errors.push(e);
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
