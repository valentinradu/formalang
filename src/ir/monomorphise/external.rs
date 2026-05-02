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
use crate::ir::{
    DispatchKind, FunctionId, GenericBase, ImplId, ImplTarget, ImportedKind, IrBlockStatement,
    IrExpr, IrModule, IrTrait, ResolvedType,
};
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

/// Defence-in-depth cycle detection over the imported-module import
/// graph. Semantic analysis already rejects cyclic `use` chains, so
/// this should never fire in practice — but if it does, the IR layer
/// surfaces a clear `InternalError` rather than silently producing
/// miscompiled output.
///
/// Returns the first node on a detected cycle, or `None` if the graph
/// is acyclic.
pub(super) fn detect_import_cycle(
    imported_modules: &HashMap<Vec<String>, IrModule>,
) -> Option<Vec<String>> {
    let mut visited: HashSet<Vec<String>> = HashSet::new();
    let mut on_stack: Vec<Vec<String>> = Vec::new();
    for start in imported_modules.keys() {
        if visited.contains(start) {
            continue;
        }
        if let Some(cycle) = dfs_detect_cycle(start, imported_modules, &mut visited, &mut on_stack) {
            return Some(cycle);
        }
    }
    None
}

fn dfs_detect_cycle(
    node: &Vec<String>,
    imported_modules: &HashMap<Vec<String>, IrModule>,
    visited: &mut HashSet<Vec<String>>,
    on_stack: &mut Vec<Vec<String>>,
) -> Option<Vec<String>> {
    if on_stack.contains(node) {
        return Some(node.clone());
    }
    if visited.contains(node) {
        return None;
    }
    on_stack.push(node.clone());
    if let Some(module) = imported_modules.get(node) {
        for import in &module.imports {
            if let Some(cycle) =
                dfs_detect_cycle(&import.module_path, imported_modules, visited, on_stack)
            {
                return Some(cycle);
            }
        }
    }
    on_stack.pop();
    visited.insert(node.clone());
    None
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

/// Per-imported-module id translation table for the body-id remap pass.
#[derive(Default)]
pub(super) struct ItemMaps {
    /// imported function index → local FunctionId
    pub functions: HashMap<u32, FunctionId>,
    /// imported impl index → local impl index
    pub impls: HashMap<u32, u32>,
}

/// Build per-imported-module id translation tables from the final
/// state of `module` after Phases 1a-1d. For each imported module
/// path, records (imported_id → local_id) for functions and impls so
/// the body-id remap pass can translate ids in cloned bodies.
///
/// Functions look up their local id by qualified name. Impl
/// translation is fed in directly from Phase 1c (no name to look up
/// against; impls are positional).
pub(super) fn build_item_maps(
    module: &IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
    impl_clone_remap: &HashMap<(Vec<String>, u32), u32>,
) -> HashMap<Vec<String>, ItemMaps> {
    let mut out: HashMap<Vec<String>, ItemMaps> = HashMap::new();
    for (path, imported) in imported_modules {
        let mut maps = ItemMaps::default();
        for (i, f) in imported.functions.iter().enumerate() {
            let qualified = qualified_name(path, &f.name);
            if let Some(local_id) = module.function_id(&qualified) {
                maps.functions
                    .insert(u32::try_from(i).unwrap_or(u32::MAX), local_id);
            }
        }
        for ((p, imported_idx), local_idx) in impl_clone_remap {
            if p == path {
                maps.impls.insert(*imported_idx, *local_idx);
            }
        }
        out.insert(path.clone(), maps);
    }
    out
}

/// Identify which imported module a cloned item came from by stripping
/// the longest matching imported path prefix from its qualified name.
/// Returns `None` for entry-module-native items (no `::` or no matching
/// imported path).
fn imported_path_of(name: &str, imported_modules: &HashMap<Vec<String>, IrModule>) -> Option<Vec<String>> {
    if !name.contains("::") {
        return None;
    }
    // Match the longest imported path that is a prefix of `name`.
    let mut best: Option<&Vec<String>> = None;
    for path in imported_modules.keys() {
        let prefix = qualified_name(path, "");
        // qualified_name with empty `name` produces "p1::p2::". Items
        // cloned from this module are named "p1::p2::Foo".
        if name.starts_with(&prefix)
            && best.map_or(true, |b| {
                qualified_name(b, "").len() < prefix.len()
            })
        {
            best = Some(path);
        }
    }
    best.cloned()
}

/// Phase 1e: walk every cloned item's body and translate id references
/// from the imported module's id-space into the entry module's id-space.
///
/// - `FunctionCall.function_id: Some(N)` → translate via the relevant
///   imported module's function map.
/// - `DispatchKind::Static.impl_id: ImplId(N)` → translate via the
///   relevant imported module's impl map.
///
/// `Reference.target` is left as-is (lowering emitted Unresolved;
/// `ResolveReferencesPass` rebinds via the path + module-prefix
/// resolution updated to use the qualified-name path).
///
/// Items not recognised as clones (no qualified-name prefix matching
/// any imported module path) are skipped — they're entry-module
/// native, their ids are valid as-is.
pub(super) fn remap_imported_body_ids(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
    impl_clone_remap: &HashMap<(Vec<String>, u32), u32>,
) {
    if imported_modules.is_empty() {
        return;
    }
    let item_maps = build_item_maps(module, imported_modules, impl_clone_remap);

    // Functions
    for func in &mut module.functions {
        let Some(path) = imported_path_of(&func.name, imported_modules) else {
            continue;
        };
        let Some(maps) = item_maps.get(&path) else {
            continue;
        };
        if let Some(body) = &mut func.body {
            remap_expr_ids(body, maps);
        }
    }

    // Impl methods. Identify which impls are clones via their target's
    // qualified name.
    let impl_paths: Vec<Option<Vec<String>>> = module
        .impls
        .iter()
        .map(|impl_block| match impl_block.target {
            ImplTarget::Struct(id) => module
                .get_struct(id)
                .and_then(|s| imported_path_of(&s.name, imported_modules)),
            ImplTarget::Enum(id) => module
                .get_enum(id)
                .and_then(|e| imported_path_of(&e.name, imported_modules)),
        })
        .collect();
    for (impl_block, path_opt) in module.impls.iter_mut().zip(impl_paths.iter()) {
        let Some(path) = path_opt else { continue };
        let Some(maps) = item_maps.get(path) else {
            continue;
        };
        for method in &mut impl_block.functions {
            if let Some(body) = &mut method.body {
                remap_expr_ids(body, maps);
            }
        }
    }

    // Lets
    for let_binding in &mut module.lets {
        let Some(path) = imported_path_of(&let_binding.name, imported_modules) else {
            continue;
        };
        let Some(maps) = item_maps.get(&path) else {
            continue;
        };
        remap_expr_ids(&mut let_binding.value, maps);
    }
}

/// Phase 1f: qualify bare-name paths in `FunctionCall` and `Reference`
/// expressions, with a per-item context derived from each cloned (or
/// entry-native) item's source-module import set.
///
/// At lowering time, an entry-module call `greet()` after `use
/// helper::greet` produces `FunctionCall { path: ["greet"],
/// function_id: None }`. After Phases 1b–1e the cloned function is
/// named `"helper::greet"` in the entry module; the bare path
/// `["greet"]` doesn't resolve through the joined-name lookup. This
/// pass rewrites bare paths to the qualified form using the right
/// context for each item:
///
/// - **Cloned imported items** use their source module's own
///   definitions + that module's `IrImport` list. So a body cloned
///   from `helper.fv` (with `use other::compute`) referencing
///   `compute` resolves to `["other", "compute"]`.
/// - **Entry-native items** use the entry module's `IrImport` list.
///   Bare references to imports like `use helper::greet` resolve to
///   `["helper", "greet"]`. The entry's own definitions are left
///   bare — `ResolveReferencesPass`'s `module_prefix` fallback
///   handles them.
pub(super) fn qualify_imported_paths(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    if imported_modules.is_empty() {
        return;
    }
    // Snapshot the entry module's imports for entry-native item bodies.
    let entry_imports = module.imports.clone();

    // Pre-compute the source module for each item (functions, impl
    // methods, lets) so the body walk doesn't need to look up while
    // borrowing module mutably.
    let func_sources: Vec<Option<Vec<String>>> = module
        .functions
        .iter()
        .map(|f| imported_path_of(&f.name, imported_modules))
        .collect();
    let impl_method_sources: Vec<Vec<Option<Vec<String>>>> = module
        .impls
        .iter()
        .map(|imp| {
            let impl_source = match imp.target {
                crate::ir::ImplTarget::Struct(id) => module
                    .get_struct(id)
                    .and_then(|s| imported_path_of(&s.name, imported_modules)),
                crate::ir::ImplTarget::Enum(id) => module
                    .get_enum(id)
                    .and_then(|e| imported_path_of(&e.name, imported_modules)),
            };
            imp.functions
                .iter()
                .map(|m| {
                    impl_source
                        .clone()
                        .or_else(|| imported_path_of(&m.name, imported_modules))
                })
                .collect()
        })
        .collect();
    let let_sources: Vec<Option<Vec<String>>> = module
        .lets
        .iter()
        .map(|l| imported_path_of(&l.name, imported_modules))
        .collect();

    // Walk each body with its precomputed source.
    for (idx, func) in module.functions.iter_mut().enumerate() {
        if let Some(body) = &mut func.body {
            let context = build_qualification_context(
                func_sources[idx].as_deref(),
                &entry_imports,
                imported_modules,
            );
            if !context.is_empty() {
                qualify_paths_in_expr(body, &context);
            }
        }
    }
    for (impl_idx, impl_block) in module.impls.iter_mut().enumerate() {
        for (m_idx, method) in impl_block.functions.iter_mut().enumerate() {
            if let Some(body) = &mut method.body {
                let source = impl_method_sources
                    .get(impl_idx)
                    .and_then(|v| v.get(m_idx))
                    .and_then(Option::as_deref);
                let context =
                    build_qualification_context(source, &entry_imports, imported_modules);
                if !context.is_empty() {
                    qualify_paths_in_expr(body, &context);
                }
            }
        }
    }
    for (idx, let_binding) in module.lets.iter_mut().enumerate() {
        let context = build_qualification_context(
            let_sources[idx].as_deref(),
            &entry_imports,
            imported_modules,
        );
        if !context.is_empty() {
            qualify_paths_in_expr(&mut let_binding.value, &context);
        }
    }
}

/// Build the bare-name → qualified-path map for a single item's body.
///
/// `source` is the imported module path the cloned item came from, or
/// `None` for entry-native items. `entry_imports` is the entry
/// module's `IrImport` list (used when source is None).
fn build_qualification_context(
    source: Option<&[String]>,
    entry_imports: &[crate::ir::IrImport],
    imported_modules: &HashMap<Vec<String>, IrModule>,
) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();

    if let Some(helper_path) = source {
        // Cloned item — use the source module's own items + imports.
        let Some(helper) = imported_modules.get(helper_path) else {
            return map;
        };
        // Helper's own items: cloned to entry under qualified name.
        // `helper.fv: pub fn helper_b()` cloned as `"helper::helper_b"`.
        let prefix = qualified_name(helper_path, "");
        for f in &helper.functions {
            map.entry(f.name.clone())
                .or_insert_with(|| format!("{prefix}{}", f.name));
        }
        for s in &helper.structs {
            map.entry(s.name.clone())
                .or_insert_with(|| format!("{prefix}{}", s.name));
        }
        for e in &helper.enums {
            map.entry(e.name.clone())
                .or_insert_with(|| format!("{prefix}{}", e.name));
        }
        for t in &helper.traits {
            map.entry(t.name.clone())
                .or_insert_with(|| format!("{prefix}{}", t.name));
        }
        for l in &helper.lets {
            map.entry(l.name.clone())
                .or_insert_with(|| format!("{prefix}{}", l.name));
        }
        // Helper's imports: each item bare-name maps to its declaring
        // module's qualified form.
        register_imports_into(&helper.imports, &mut map);
    } else {
        // Entry-native item — use entry's IrImport list. Entry's own
        // definitions are left bare (ResolveReferencesPass handles
        // them via module_prefix or by_name lookup).
        register_imports_into(entry_imports, &mut map);
    }

    map
}

fn register_imports_into(imports: &[crate::ir::IrImport], map: &mut HashMap<String, String>) {
    for imp in imports {
        let prefix = qualified_name(&imp.module_path, "");
        for item in &imp.items {
            map.entry(item.name.clone())
                .or_insert_with(|| format!("{prefix}{}", item.name));
        }
    }
}

fn qualify_paths_in_expr(expr: &mut IrExpr, bare_to_qualified: &HashMap<String, String>) {
    match expr {
        IrExpr::FunctionCall {
            path, function_id, ..
        } => {
            if function_id.is_none() && path.len() == 1 {
                if let Some(qualified) = bare_to_qualified.get(&path[0]) {
                    *path = qualified.split("::").map(String::from).collect();
                }
            }
        }
        IrExpr::Reference { path, target, .. } => {
            // Only qualify when target is still Unresolved — preserve
            // any explicitly-set target (locals, params, already-resolved
            // items).
            if matches!(target, crate::ir::ReferenceTarget::Unresolved) && path.len() == 1 {
                if let Some(qualified) = bare_to_qualified.get(&path[0]) {
                    *path = qualified.split("::").map(String::from).collect();
                }
            }
        }
        _ => {}
    }
    // Recurse into children using the same descent as remap_expr_ids.
    match expr {
        IrExpr::BinaryOp { left, right, .. } => {
            qualify_paths_in_expr(left, bare_to_qualified);
            qualify_paths_in_expr(right, bare_to_qualified);
        }
        IrExpr::UnaryOp { operand, .. } => qualify_paths_in_expr(operand, bare_to_qualified),
        IrExpr::Array { elements, .. } => {
            for e in elements {
                qualify_paths_in_expr(e, bare_to_qualified);
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                qualify_paths_in_expr(k, bare_to_qualified);
                qualify_paths_in_expr(v, bare_to_qualified);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            qualify_paths_in_expr(dict, bare_to_qualified);
            qualify_paths_in_expr(key, bare_to_qualified);
        }
        IrExpr::FieldAccess { object, .. } => qualify_paths_in_expr(object, bare_to_qualified),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            qualify_paths_in_expr(condition, bare_to_qualified);
            qualify_paths_in_expr(then_branch, bare_to_qualified);
            if let Some(eb) = else_branch {
                qualify_paths_in_expr(eb, bare_to_qualified);
            }
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            qualify_paths_in_expr(scrutinee, bare_to_qualified);
            for arm in arms {
                qualify_paths_in_expr(&mut arm.body, bare_to_qualified);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            qualify_paths_in_expr(collection, bare_to_qualified);
            qualify_paths_in_expr(body, bare_to_qualified);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                match stmt {
                    IrBlockStatement::Let { value, .. } => {
                        qualify_paths_in_expr(value, bare_to_qualified)
                    }
                    IrBlockStatement::Assign { target, value, .. } => {
                        qualify_paths_in_expr(target, bare_to_qualified);
                        qualify_paths_in_expr(value, bare_to_qualified);
                    }
                    IrBlockStatement::Expr(e) => qualify_paths_in_expr(e, bare_to_qualified),
                }
            }
            qualify_paths_in_expr(result, bare_to_qualified);
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                qualify_paths_in_expr(e, bare_to_qualified);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            qualify_paths_in_expr(closure, bare_to_qualified);
            for (_, e) in args {
                qualify_paths_in_expr(e, bare_to_qualified);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            qualify_paths_in_expr(receiver, bare_to_qualified);
            for (_, e) in args {
                qualify_paths_in_expr(e, bare_to_qualified);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                qualify_paths_in_expr(e, bare_to_qualified);
            }
        }
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            for (_, _, e) in fields {
                qualify_paths_in_expr(e, bare_to_qualified);
            }
        }
        IrExpr::Closure { body, .. } => qualify_paths_in_expr(body, bare_to_qualified),
        IrExpr::ClosureRef { env_struct, .. } => {
            qualify_paths_in_expr(env_struct, bare_to_qualified)
        }
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
    }
}

/// Walk an expression tree and apply id remaps for FunctionCall.function_id
/// and DispatchKind::Static.impl_id.
fn remap_expr_ids(expr: &mut IrExpr, maps: &ItemMaps) {
    match expr {
        IrExpr::FunctionCall { function_id, .. } => {
            if let Some(old) = function_id {
                if let Some(new) = maps.functions.get(&old.0) {
                    *function_id = Some(*new);
                }
            }
        }
        IrExpr::MethodCall { dispatch, .. } => {
            if let DispatchKind::Static { impl_id } = dispatch {
                if let Some(new) = maps.impls.get(&impl_id.0) {
                    *impl_id = ImplId(*new);
                }
            }
        }
        _ => {}
    }
    // Recurse into children. Block statements have a different shape;
    // handle them via their own descent.
    match expr {
        IrExpr::BinaryOp { left, right, .. } => {
            remap_expr_ids(left, maps);
            remap_expr_ids(right, maps);
        }
        IrExpr::UnaryOp { operand, .. } => remap_expr_ids(operand, maps),
        IrExpr::Array { elements, .. } => {
            for e in elements {
                remap_expr_ids(e, maps);
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                remap_expr_ids(k, maps);
                remap_expr_ids(v, maps);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            remap_expr_ids(dict, maps);
            remap_expr_ids(key, maps);
        }
        IrExpr::FieldAccess { object, .. } => remap_expr_ids(object, maps),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            remap_expr_ids(condition, maps);
            remap_expr_ids(then_branch, maps);
            if let Some(eb) = else_branch {
                remap_expr_ids(eb, maps);
            }
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            remap_expr_ids(scrutinee, maps);
            for arm in arms {
                remap_expr_ids(&mut arm.body, maps);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            remap_expr_ids(collection, maps);
            remap_expr_ids(body, maps);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                match stmt {
                    IrBlockStatement::Let { value, .. } => remap_expr_ids(value, maps),
                    IrBlockStatement::Assign { target, value, .. } => {
                        remap_expr_ids(target, maps);
                        remap_expr_ids(value, maps);
                    }
                    IrBlockStatement::Expr(e) => remap_expr_ids(e, maps),
                }
            }
            remap_expr_ids(result, maps);
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                remap_expr_ids(e, maps);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            remap_expr_ids(closure, maps);
            for (_, e) in args {
                remap_expr_ids(e, maps);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            remap_expr_ids(receiver, maps);
            for (_, e) in args {
                remap_expr_ids(e, maps);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                remap_expr_ids(e, maps);
            }
        }
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            for (_, _, e) in fields {
                remap_expr_ids(e, maps);
            }
        }
        IrExpr::Closure { body, .. } => remap_expr_ids(body, maps),
        IrExpr::ClosureRef { env_struct, .. } => remap_expr_ids(env_struct, maps),
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
    }
}

/// Phase 1d: inline every imported pub `let` into the current module
/// under a qualified name. The clone has its `ty` and the `value`
/// expression's embedded ResolvedTypes externalised so the next
/// worklist iteration of [`specialise_external_instantiations`] picks
/// up any types they reference.
///
/// `IrLet` carries explicit visibility; non-public lets are skipped
/// (cross-module access to them is rejected at semantic time anyway).
///
/// Body **id-references** in the `value` expression stay in the imported
/// id-space. A follow-up commit walks all cloned bodies and remaps.
pub(super) fn inline_imported_lets(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    for (module_path, imported) in imported_modules {
        for let_binding in &imported.lets {
            if !let_binding.visibility.is_public() {
                continue;
            }
            let qualified = qualified_name(module_path, &let_binding.name);
            if module.has_let(&qualified) {
                continue;
            }

            let mut clone = let_binding.clone();
            clone.name.clone_from(&qualified);
            externalise_imported_refs(&mut clone.ty, imported, module_path);
            walk_expr_types_mut(&mut clone.value, &mut |ty| {
                externalise_imported_refs(ty, imported, module_path);
            });
            module.add_let(clone);
        }
    }
}

/// Phase 1c: inline every imported impl block whose target type is now
/// present in the local module (its struct or enum has already been
/// cloned by Phase 1a).
///
/// The clone has its `target` and `trait_ref.trait_id` translated from
/// the imported module's id-space to the local clone's id (looked up by
/// qualified name). Method signatures and bodies have ResolvedType
/// references externalised so subsequent worklist iterations of
/// [`specialise_external_instantiations`] pick up any new types.
///
/// Method body **id-references** (`DispatchKind::Static.impl_id`,
/// `Reference::Function/Struct/...` ids in method bodies) stay in the
/// imported module's id-space and are stale — same limitation as
/// [`inline_imported_functions`]. A follow-up commit walks bodies and
/// remaps those ids.
///
/// Impls whose target struct/enum was *not* cloned are skipped (they
/// would have nowhere to attach in the local module).
///
/// Records the (imported_module_path, imported_impl_idx) →
/// local_impl_idx mapping into `impl_remap` so the body-id remapping
/// pass can rewrite `DispatchKind::Static.impl_id` references in
/// cloned bodies.
pub(super) fn inline_imported_impls(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
    impl_remap: &mut HashMap<(Vec<String>, u32), u32>,
) {
    for (module_path, imported) in imported_modules {
        for (imported_idx, impl_block) in imported.impls.iter().enumerate() {
            // Translate the impl's target id to the local clone's id.
            let new_target = match impl_block.target {
                ImplTarget::Struct(imported_id) => {
                    let Some(s) = imported.structs.get(imported_id.0 as usize) else {
                        continue;
                    };
                    let qualified = qualified_name(module_path, &s.name);
                    let Some(local_id) = module.struct_id(&qualified) else {
                        continue;
                    };
                    ImplTarget::Struct(local_id)
                }
                ImplTarget::Enum(imported_id) => {
                    let Some(e) = imported.enums.get(imported_id.0 as usize) else {
                        continue;
                    };
                    let qualified = qualified_name(module_path, &e.name);
                    let Some(local_id) = module.enum_id(&qualified) else {
                        continue;
                    };
                    ImplTarget::Enum(local_id)
                }
            };

            let mut clone = impl_block.clone();
            clone.target = new_target;

            // Translate trait_ref.trait_id by qualified-name lookup if
            // the trait was cloned. If not, the trait_ref still points
            // at the imported id-space — leave the original id and let
            // a leftover-scanner catch it later if it matters.
            if let Some(tref) = &mut clone.trait_ref {
                if let Some(t) = imported.traits.get(tref.trait_id.0 as usize) {
                    let qualified = qualified_name(module_path, &t.name);
                    if let Some(local_id) = module.trait_id(&qualified) {
                        tref.trait_id = local_id;
                    }
                }
            }

            // Externalise types in each method's signature + body. The
            // worklist re-runs Phase 1a and clones any types these
            // methods reference for the first time.
            for method in &mut clone.functions {
                walk_function_types_mut(method, &mut |ty| {
                    externalise_imported_refs(ty, imported, module_path);
                });
            }

            // Record the index mapping for body-id remap: the new
            // local impl is appended to module.impls; its index is the
            // current length minus 1 after the push.
            let local_idx = u32::try_from(module.impls.len()).unwrap_or(u32::MAX);
            module.impls.push(clone);
            impl_remap.insert(
                (module_path.clone(), u32::try_from(imported_idx).unwrap_or(u32::MAX)),
                local_idx,
            );
        }
    }
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
