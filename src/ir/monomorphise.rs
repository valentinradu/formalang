//! Monomorphisation pass.
//!
//! `FormaLang`'s IR preserves generics after lowering: `ResolvedType::Generic`
//! wraps a [`GenericBase`] (a struct or enum id) with concrete type
//! arguments, and `ResolvedType::TypeParam` appears inside the body of a
//! generic definition where the parameter has not yet been substituted.
//! Most statically-typed code-generation targets (C, WGSL, `TypeScript`
//! with typed emission, Swift, Kotlin) cannot emit parametric types
//! directly — they need one concrete specialisation per instantiation.
//!
//! The pass walks the IR, collects every unique `(base, type_args)` tuple
//! in use, clones each generic struct or enum once per unique tuple while
//! substituting [`ResolvedType::TypeParam`] references with the concrete
//! argument, rewrites every [`ResolvedType::Generic`] in the module to
//! point at the specialised copy, then removes the original generic
//! definitions (and rebuilds name indices).
//!
//! After the pass runs, no `ResolvedType::Generic` references remain in
//! the IR.
//!
//! # Limitations
//!
//! - Generic **traits** are not supported — the IR has no way to
//!   instantiate a generic trait today. A trait definition with
//!   non-empty `generic_params` that survives the pass is reported as
//!   an `InternalError`.
//! - External generic instantiations (`ResolvedType::External` with a
//!   non-empty `type_args`) are specialised when the pass is built via
//!   [`MonomorphisePass::with_imports`] and the imported module's IR is
//!   present in the supplied map. When no imports map is supplied, the
//!   pass leaves External references alone for backwards compatibility.
//!
//! # Usage
//!
//! ```no_run
//! use formalang::{compile_to_ir, Pipeline};
//! use formalang::ir::MonomorphisePass;
//!
//! let source = "pub struct Box<T> { value: T }\npub let b: Box<Number> = Box(value: 1)";
//! let module = compile_to_ir(source).unwrap();
//! let result = Pipeline::new().pass(MonomorphisePass::default()).run(module);
//! assert!(result.is_ok());
//! ```

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::CompilerError;
use crate::ir::{
    EnumId, GenericBase, ImportedKind, IrBlockStatement, IrEnum, IrExpr, IrField, IrFunction,
    IrImpl, IrModule, IrStruct, IrTrait, PrimitiveType, ResolvedType, StructId,
};
use crate::location::Span;
use crate::pipeline::IrPass;

/// Monomorphisation pass.
///
/// See the module-level documentation in `src/ir/monomorphise.rs` for the
/// full algorithm and limitations.
#[expect(
    clippy::exhaustive_structs,
    reason = "single optional field for imported module IRs; no further fields planned"
)]
#[derive(Debug, Clone, Default)]
pub struct MonomorphisePass {
    /// Imported module IRs keyed by their logical module path. When
    /// supplied, the pass specialises generic types referenced via
    /// `ResolvedType::External { type_args, .. }` by cloning the imported
    /// definition into the main module with substituted type arguments
    /// (audit finding #45). When empty, External-typed generic
    /// instantiations are left alone, preserving previous behaviour.
    pub imported_modules: HashMap<Vec<String>, IrModule>,
}

impl MonomorphisePass {
    /// Configure the pass with imported module IRs (audit finding #45).
    ///
    /// The map is keyed by logical module path (matching
    /// `ResolvedType::External::module_path`) and lets the pass clone
    /// imported generic definitions into the main module with substituted
    /// type arguments instead of leaving the External reference
    /// unspecialised.
    #[must_use]
    pub fn with_imports(mut self, imported_modules: HashMap<Vec<String>, IrModule>) -> Self {
        self.imported_modules = imported_modules;
        self
    }
}

impl IrPass for MonomorphisePass {
    fn name(&self) -> &'static str {
        "monomorphise"
    }

    fn run(&mut self, mut module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        let mut errors = Vec::new();

        // Phase 1a: specialise external generic instantiations (audit #45).
        // For each `External { module_path, name, type_args }` with
        // non-empty type_args, look up the imported module's IR, clone the
        // generic definition into the current module with substituted
        // arguments, and remember the mapping so Phase 2 can rewrite
        // External references to the new local Struct/Enum.
        let external_mapping = if self.imported_modules.is_empty() {
            HashMap::new()
        } else {
            match specialise_external_instantiations(&mut module, &self.imported_modules) {
                Ok(map) => map,
                Err(mut e) => {
                    errors.append(&mut e);
                    HashMap::new()
                }
            }
        };

        // Phase 1: collect every `Generic { base, args }` instantiation in
        // the module. The worklist processes args recursively when a
        // specialisation itself references more generics.
        let initial = collect_all_instantiations(&module);
        let mut worklist: VecDeque<Instantiation> = initial.into_iter().collect();
        let mut mapping: HashMap<Instantiation, GenericBase> = HashMap::new();

        while let Some(inst) = worklist.pop_front() {
            if mapping.contains_key(&inst) {
                continue;
            }
            match specialise(&mut module, &inst) {
                Ok((spec_base, more)) => {
                    mapping.insert(inst, spec_base);
                    worklist.extend(more);
                }
                Err(e) => {
                    errors.push(e);
                    // Record a sentinel so we don't keep retrying the same
                    // broken instantiation.
                    mapping.insert(inst, GenericBase::Struct(StructId(u32::MAX)));
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        // Phase 2: rewrite every Generic reference to its specialisation,
        // and every External-with-type-args reference to its cloned
        // local specialisation (when an `imported_modules` map was
        // provided).
        rewrite_module(&mut module, &mapping);
        if !external_mapping.is_empty() {
            rewrite_external_references(&mut module, &external_mapping);
        }

        // Phase 2b: clone each impl block once per specialisation of its
        // target generic struct/enum, substituting the impl body with the
        // concrete type arguments. Returns a reverse map from original
        // impl index to the list of `(specialised target, new impl index)`
        // clones — Phase 2c uses it to rewrite dispatch sites.
        let impl_remap = specialise_impls(&mut module, &mapping);

        // Phase 2c: rewrite `DispatchKind::Static { impl_id }` at every
        // method-call site. A call on a specialised receiver should
        // dispatch to the cloned impl, not the original generic-impl
        // slot. Audit finding #5b.
        rewrite_dispatch_impl_ids(&mut module, &impl_remap);

        // Phase 3: compact — drop the original generic structs, enums, and
        // the generic impls that were expanded in Phase 2b; then remap
        // surviving IDs for each kind. Order matters: drop generic-targeted
        // impls before `apply_remaps` rewrites ids, because the retain
        // predicate below indexes into the pre-compaction remap tables.
        let struct_remap = build_struct_remap(&module);
        let enum_remap = build_enum_remap(&module);
        let impl_index_remap =
            drop_specialised_generic_impls(&mut module, &struct_remap, &enum_remap);
        apply_remaps(&mut module, &struct_remap, &enum_remap)?;
        apply_impl_index_remap(&mut module, &impl_index_remap);
        module.structs.retain(|s| s.generic_params.is_empty());
        module.enums.retain(|e| e.generic_params.is_empty());
        module.rebuild_indices();

        // Phase 4: sanity — no Generic should remain anywhere.
        let mut leftovers = LeftoverScanner::default();
        leftovers.scan(&module);
        if let Some(detail) = leftovers.first_error() {
            return Err(vec![CompilerError::InternalError {
                detail,
                span: Span::default(),
            }]);
        }

        Ok(module)
    }
}

// =============================================================================
// Phase 1: collection
// =============================================================================

fn collect_all_instantiations(module: &IrModule) -> HashSet<Instantiation> {
    let mut out = HashSet::new();
    let mut collector = |ty: &ResolvedType| collect_from_type(ty, &mut out);
    walk_module_types(module, &mut collector);
    out
}

fn collect_from_type(ty: &ResolvedType, out: &mut HashSet<Instantiation>) {
    match ty {
        ResolvedType::Generic { base, args } => {
            for a in args {
                collect_from_type(a, out);
            }
            out.insert((*base, args.clone()));
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            collect_from_type(inner, out);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                collect_from_type(t, out);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            collect_from_type(key_ty, out);
            collect_from_type(value_ty, out);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                collect_from_type(t, out);
            }
            collect_from_type(return_ty, out);
        }
        ResolvedType::External { type_args, .. } => {
            for t in type_args {
                collect_from_type(t, out);
            }
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::TypeParam(_) => {}
    }
}

// =============================================================================
// Phase 1a: external generic specialisation (audit #45)
// =============================================================================

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
            if !type_args.is_empty() {
                out.insert((module_path.clone(), name.clone(), type_args.clone()));
            }
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
        | ResolvedType::TypeParam(_) => {}
    }
}

/// Specialise every external generic instantiation by cloning the
/// imported definition into the main module with substituted type
/// arguments. Returns a map from external instantiation to the new
/// local `(StructId | EnumId)` so Phase 2 can rewrite the External
/// references. Audit finding #45.
fn specialise_external_instantiations(
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
        let mangled = mangle_external_name(name, args, module);
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
        let mangled = mangle_external_name(name, args, module);
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
    } else {
        Err(CompilerError::InternalError {
            detail: format!(
                "monomorphise: imported module {module_path:?} has no type named `{name}` to specialise"
            ),
            span: Span::default(),
        })
    }
}

/// Build a unique mangled name for an external specialisation. Mirrors
/// `mangle_name` but tags the source module so cross-module collisions
/// stay distinct.
fn mangle_external_name(name: &str, args: &[ResolvedType], module: &IrModule) -> String {
    let mut out = name.to_string();
    for a in args {
        out.push_str("__");
        type_suffix(a, &mut out);
    }
    if module.struct_id(&out).is_none() && module.enum_id(&out).is_none() {
        return out;
    }
    let mut n: u32 = 2;
    loop {
        let candidate = format!("{out}#{n}");
        if module.struct_id(&candidate).is_none() && module.enum_id(&candidate).is_none() {
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
        ResolvedType::Primitive(_) | ResolvedType::TypeParam(_) => {}
    }
}

/// Phase 2 helper: rewrite every `External { type_args, .. }` whose
/// `(module_path, name, type_args)` was specialised to the cloned
/// local Struct/Enum.
fn rewrite_external_references(
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
            if !type_args.is_empty() {
                let key = (module_path.clone(), name.clone(), type_args.clone());
                if let Some(new_ty) = mapping.get(&key) {
                    *ty = new_ty.clone();
                }
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
        | ResolvedType::TypeParam(_) => {}
    }
}

// =============================================================================
// Phase 1b: specialisation
// =============================================================================

/// A single generic instantiation key: `(base, type_args)`.
type Instantiation = (GenericBase, Vec<ResolvedType>);

/// Outcome of specialising a single generic instantiation.
type SpecialiseOk = (GenericBase, Vec<Instantiation>);

/// Map from `(original impl index, specialised target)` to the new
/// impl index in `module.impls` after Phase 2b.
type ImplRemap = HashMap<(usize, GenericBase), usize>;

/// Specialise a single generic instantiation (struct or enum), appending
/// the clone to the module and returning its new base plus any further
/// instantiations introduced by the clone's field types.
#[expect(
    clippy::result_large_err,
    reason = "CompilerError is large by design; errors are bounded to a Vec<CompilerError> at the pass boundary"
)]
fn specialise(
    module: &mut IrModule,
    (base, args): &Instantiation,
) -> Result<SpecialiseOk, CompilerError> {
    match base {
        GenericBase::Struct(id) => specialise_struct(module, *id, args),
        GenericBase::Enum(id) => specialise_enum(module, *id, args),
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

/// Build a stable mangled name for a specialisation. Collisions with
/// existing names would break `rebuild_indices`, so on the off chance a
/// user-written struct already has the mangled name, we append an
/// incrementing suffix.
fn mangle_name(base: &str, args: &[ResolvedType], module: &IrModule) -> String {
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

fn type_suffix(ty: &ResolvedType, out: &mut String) {
    match ty {
        ResolvedType::Primitive(p) => out.push_str(match p {
            PrimitiveType::String => "String",
            PrimitiveType::Number => "Number",
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
    }
}

fn write_usize(out: &mut String, prefix: &str, n: usize) -> core::fmt::Result {
    use core::fmt::Write;
    write!(out, "{prefix}{n}")
}

// =============================================================================
// Phase 1c: substitution helpers
// =============================================================================

fn substitute_type(ty: &mut ResolvedType, subs: &HashMap<String, ResolvedType>) {
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
        | ResolvedType::Enum(_) => {}
    }
}

fn substitute_expr_types(expr: &mut IrExpr, subs: &HashMap<String, ResolvedType>) {
    walk_expr_types_mut(expr, &mut |ty| substitute_type(ty, subs));
}

// =============================================================================
// Phase 2: rewrite Generic → Struct(spec)
// =============================================================================

fn rewrite_module(module: &mut IrModule, mapping: &HashMap<Instantiation, GenericBase>) {
    let rewrite = |ty: &mut ResolvedType| rewrite_type(ty, mapping);
    walk_module_types_mut(module, rewrite);
}

fn rewrite_type(ty: &mut ResolvedType, mapping: &HashMap<Instantiation, GenericBase>) {
    // Recurse first so nested generics inside args are resolved before we
    // try to look up the outer key (the mapping keys hold fully-rewritten
    // inner types, so we must rewrite inner before outer lookup).
    match ty {
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            rewrite_type(inner, mapping);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                rewrite_type(t, mapping);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            rewrite_type(key_ty, mapping);
            rewrite_type(value_ty, mapping);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                rewrite_type(t, mapping);
            }
            rewrite_type(return_ty, mapping);
        }
        ResolvedType::Generic { base, args } => {
            for a in args.iter_mut() {
                rewrite_type(a, mapping);
            }
            if let Some(&spec) = mapping.get(&(*base, args.clone())) {
                *ty = match spec {
                    GenericBase::Struct(id) => ResolvedType::Struct(id),
                    GenericBase::Enum(id) => ResolvedType::Enum(id),
                };
            }
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                rewrite_type(a, mapping);
            }
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::TypeParam(_) => {}
    }
}

// =============================================================================
// Phase 2b: specialise impl blocks targeting generic structs/enums
// =============================================================================

/// For each impl block whose target is a generic struct/enum, append one
/// cloned impl per specialisation of that target (with `TypeParam`s
/// substituted for the concrete type args of that specialisation). The
/// originals are retained in `module.impls` for now; they are dropped in
/// Phase 3 by `drop_specialised_generic_impls`.
///
/// Dispatch sites (`DispatchKind::Static { impl_id }`) still reference the
/// original generic-impl slot after this runs. Backends that iterate
/// `module.impls` to locate methods on a specialised type will find them
/// correctly here; Phase 2c (`rewrite_dispatch_impl_ids`) uses the
/// returned [`ImplRemap`] to retarget `DispatchKind::Static { impl_id }`
/// sites onto the cloned impl for each specialisation.
fn specialise_impls(
    module: &mut IrModule,
    mapping: &HashMap<Instantiation, GenericBase>,
) -> ImplRemap {
    // Group specialisations by original generic base.
    type Spec = (Vec<ResolvedType>, GenericBase);
    let mut by_base: HashMap<GenericBase, Vec<Spec>> = HashMap::new();
    for ((orig_base, args), spec_base) in mapping {
        by_base
            .entry(*orig_base)
            .or_default()
            .push((args.clone(), *spec_base));
    }
    let mut new_impls: Vec<IrImpl> = Vec::new();
    let mut impl_remap: ImplRemap = HashMap::new();

    for (orig_idx, imp) in module.impls.iter().enumerate() {
        let base = match imp.target {
            crate::ir::ImplTarget::Struct(id) => GenericBase::Struct(id),
            crate::ir::ImplTarget::Enum(id) => GenericBase::Enum(id),
        };
        let Some(specs) = by_base.get(&base) else {
            continue;
        };
        let generic_param_names: Vec<String> = match base {
            GenericBase::Struct(sid) => module
                .get_struct(sid)
                .map(|s| s.generic_params.iter().map(|p| p.name.clone()).collect())
                .unwrap_or_default(),
            GenericBase::Enum(eid) => module
                .get_enum(eid)
                .map(|e| e.generic_params.iter().map(|p| p.name.clone()).collect())
                .unwrap_or_default(),
        };
        if generic_param_names.is_empty() {
            continue;
        }
        for (args, spec_base) in specs {
            if generic_param_names.len() != args.len() {
                continue;
            }
            let subs: HashMap<String, ResolvedType> = generic_param_names
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect();
            let mut clone = imp.clone();
            clone.target = match spec_base {
                GenericBase::Struct(id) => crate::ir::ImplTarget::Struct(*id),
                GenericBase::Enum(id) => crate::ir::ImplTarget::Enum(*id),
            };
            for func in &mut clone.functions {
                for param in &mut func.params {
                    if let Some(ty) = &mut param.ty {
                        substitute_type(ty, &subs);
                    }
                    if let Some(default) = &mut param.default {
                        substitute_expr_types(default, &subs);
                    }
                }
                if let Some(ret_ty) = &mut func.return_type {
                    substitute_type(ret_ty, &subs);
                }
                if let Some(body) = &mut func.body {
                    substitute_expr_types(body, &subs);
                }
            }
            walk_impl_types_mut(&mut clone, &mut |ty| rewrite_type(ty, mapping));
            // Record the (orig_idx, spec_target) → new_idx mapping so
            // dispatch-site rewriting can find the right clone.
            let new_idx = module.impls.len().saturating_add(new_impls.len());
            impl_remap.insert((orig_idx, *spec_base), new_idx);
            new_impls.push(clone);
        }
    }

    module.impls.extend(new_impls);
    impl_remap
}

/// `ImplRemap`-aware type-to-base extraction. Returns the
/// `GenericBase` of a concrete struct/enum receiver type (post Phase 2
/// rewrite). Returns `None` for non-nominal types.
fn receiver_to_base(ty: &ResolvedType) -> Option<GenericBase> {
    match ty {
        ResolvedType::Struct(id) => Some(GenericBase::Struct(*id)),
        ResolvedType::Enum(id) => Some(GenericBase::Enum(*id)),
        ResolvedType::Optional(inner) => receiver_to_base(inner),
        ResolvedType::Primitive(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Array(_)
        | ResolvedType::Range(_)
        | ResolvedType::Tuple(_)
        | ResolvedType::Generic { .. }
        | ResolvedType::TypeParam(_)
        | ResolvedType::External { .. }
        | ResolvedType::Dictionary { .. }
        | ResolvedType::Closure { .. } => None,
    }
}

/// Rewrite `DispatchKind::Static { impl_id }` at every method-call
/// site so the id points at the per-specialisation clone created in
/// Phase 2b. Walks every expression in the module.
fn dispatch_rewrite_expr(expr: &mut IrExpr, impl_remap: &ImplRemap) {
    use crate::ir::{DispatchKind, ImplId};
    // Recurse first so nested method calls are rewritten too.
    for child in iter_expr_children_mut(expr) {
        dispatch_rewrite_expr(child, impl_remap);
    }
    if let IrExpr::MethodCall {
        receiver,
        dispatch: DispatchKind::Static { impl_id },
        ..
    } = expr
    {
        let old_idx = impl_id.0 as usize;
        if let Some(target_base) = receiver_to_base(receiver.ty()) {
            if let Some(&new_idx) = impl_remap.get(&(old_idx, target_base)) {
                *impl_id = ImplId(u32::try_from(new_idx).unwrap_or(u32::MAX));
            }
        }
    }
}

fn rewrite_dispatch_impl_ids(module: &mut IrModule, impl_remap: &ImplRemap) {
    if impl_remap.is_empty() {
        return;
    }
    // Walk every expression in the module.
    for func in &mut module.functions {
        if let Some(body) = &mut func.body {
            dispatch_rewrite_expr(body, impl_remap);
        }
        for param in &mut func.params {
            if let Some(default) = &mut param.default {
                dispatch_rewrite_expr(default, impl_remap);
            }
        }
    }
    for imp in &mut module.impls {
        for func in &mut imp.functions {
            if let Some(body) = &mut func.body {
                dispatch_rewrite_expr(body, impl_remap);
            }
            for param in &mut func.params {
                if let Some(default) = &mut param.default {
                    dispatch_rewrite_expr(default, impl_remap);
                }
            }
        }
    }
    for s in &mut module.structs {
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                dispatch_rewrite_expr(default, impl_remap);
            }
        }
    }
    for e in &mut module.enums {
        for variant in &mut e.variants {
            for field in &mut variant.fields {
                if let Some(default) = &mut field.default {
                    dispatch_rewrite_expr(default, impl_remap);
                }
            }
        }
    }
    for l in &mut module.lets {
        dispatch_rewrite_expr(&mut l.value, impl_remap);
    }
}

/// Mutable iterator over a single expression's direct child expressions.
/// Used by Phase 2c so dispatch rewriting can recurse without spinning
/// up a full visitor.
fn iter_expr_children_mut(expr: &mut IrExpr) -> Vec<&mut IrExpr> {
    let mut out: Vec<&mut IrExpr> = Vec::new();
    match expr {
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
        IrExpr::BinaryOp { left, right, .. } => {
            out.push(left.as_mut());
            out.push(right.as_mut());
        }
        IrExpr::UnaryOp { operand, .. } => out.push(operand.as_mut()),
        IrExpr::Array { elements, .. } => out.extend(elements.iter_mut()),
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                out.push(k);
                out.push(v);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            out.push(dict.as_mut());
            out.push(key.as_mut());
        }
        IrExpr::FieldAccess { object, .. } => out.push(object.as_mut()),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            out.push(condition.as_mut());
            out.push(then_branch.as_mut());
            if let Some(eb) = else_branch {
                out.push(eb.as_mut());
            }
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            out.push(scrutinee.as_mut());
            for arm in arms {
                out.push(&mut arm.body);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            out.push(collection.as_mut());
            out.push(body.as_mut());
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                match stmt {
                    IrBlockStatement::Let { value, .. } => out.push(value),
                    IrBlockStatement::Assign { target, value, .. } => {
                        out.push(target);
                        out.push(value);
                    }
                    IrBlockStatement::Expr(e) => out.push(e),
                }
            }
            out.push(result.as_mut());
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                out.push(e);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            out.push(receiver.as_mut());
            for (_, e) in args {
                out.push(e);
            }
        }
        IrExpr::StructInst { fields, .. }
        | IrExpr::EnumInst { fields, .. }
        | IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                out.push(e);
            }
        }
        IrExpr::Closure { body, .. } => out.push(body.as_mut()),
    }
    out
}

/// Drop impls whose target is a generic struct or enum that got specialised
/// (and therefore survives in `module.impls` only through its Phase-2b
/// clones). Returns the old-index → new-index mapping for surviving
/// impls so callers can rewrite `DispatchKind::Static { impl_id }`
/// references to match the compacted vector.
fn drop_specialised_generic_impls(
    module: &mut IrModule,
    struct_remap: &[Option<StructId>],
    enum_remap: &[Option<EnumId>],
) -> Vec<Option<usize>> {
    let keep: Vec<bool> = module
        .impls
        .iter()
        .map(|imp| match imp.target {
            crate::ir::ImplTarget::Struct(id) => struct_remap
                .get(id.0 as usize)
                .copied()
                .is_none_or(|slot| slot.is_some()),
            crate::ir::ImplTarget::Enum(id) => enum_remap
                .get(id.0 as usize)
                .copied()
                .is_none_or(|slot| slot.is_some()),
        })
        .collect();
    let mut new_index: Vec<Option<usize>> = Vec::with_capacity(keep.len());
    let mut next: usize = 0;
    for &k in &keep {
        if k {
            new_index.push(Some(next));
            next = next.saturating_add(1);
        } else {
            new_index.push(None);
        }
    }
    let mut idx = 0;
    module.impls.retain(|_| {
        let k = keep.get(idx).copied().unwrap_or(false);
        idx = idx.saturating_add(1);
        k
    });
    new_index
}

/// Rewrite every `DispatchKind::Static { impl_id }` so it points at the
/// compacted impl index. Called after `drop_specialised_generic_impls`.
fn impl_index_rewrite_expr(expr: &mut IrExpr, remap: &[Option<usize>]) {
    use crate::ir::{DispatchKind, ImplId};
    for child in iter_expr_children_mut(expr) {
        impl_index_rewrite_expr(child, remap);
    }
    if let IrExpr::MethodCall {
        dispatch: DispatchKind::Static { impl_id },
        ..
    } = expr
    {
        if let Some(Some(new)) = remap.get(impl_id.0 as usize).copied() {
            *impl_id = ImplId(u32::try_from(new).unwrap_or(u32::MAX));
        }
    }
}

fn apply_impl_index_remap(module: &mut IrModule, remap: &[Option<usize>]) {
    let identity = remap
        .iter()
        .enumerate()
        .all(|(i, s)| matches!(s, Some(j) if *j == i));
    if identity {
        return;
    }
    for func in &mut module.functions {
        if let Some(body) = &mut func.body {
            impl_index_rewrite_expr(body, remap);
        }
    }
    for imp in &mut module.impls {
        for func in &mut imp.functions {
            if let Some(body) = &mut func.body {
                impl_index_rewrite_expr(body, remap);
            }
        }
    }
    for s in &mut module.structs {
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                impl_index_rewrite_expr(default, remap);
            }
        }
    }
    for l in &mut module.lets {
        impl_index_rewrite_expr(&mut l.value, remap);
    }
}

fn walk_impl_types_mut(imp: &mut IrImpl, visit: &mut impl FnMut(&mut ResolvedType)) {
    for f in &mut imp.functions {
        walk_function_types_mut(f, visit);
    }
}

// =============================================================================
// Phase 3: compact — drop original generic definitions, remap IDs
// =============================================================================

/// Build an old-id → new-id remap table for structs. Structs with non-empty
/// `generic_params` become `None` (they will be dropped on compaction);
/// surviving structs map to their new post-compaction position.
fn build_struct_remap(module: &IrModule) -> Vec<Option<StructId>> {
    let mut out = Vec::with_capacity(module.structs.len());
    let mut next: u32 = 0;
    for s in &module.structs {
        if s.generic_params.is_empty() {
            out.push(Some(StructId(next)));
            next = next.saturating_add(1);
        } else {
            out.push(None);
        }
    }
    out
}

/// Matching remap for enums.
fn build_enum_remap(module: &IrModule) -> Vec<Option<EnumId>> {
    let mut out = Vec::with_capacity(module.enums.len());
    let mut next: u32 = 0;
    for e in &module.enums {
        if e.generic_params.is_empty() {
            out.push(Some(EnumId(next)));
            next = next.saturating_add(1);
        } else {
            out.push(None);
        }
    }
    out
}

/// Remap struct/enum IDs across the module after compaction.
///
/// Returns `Err` if an impl-target lookup hits an out-of-bounds index
/// or a `None` slot (a "dropped" struct/enum that should have been
/// removed alongside its impl by [`drop_specialised_generic_impls`]).
/// Audit2 B22: previously these cases silently no-op'd, leaving
/// dangling target IDs in the IR.
fn apply_remaps(
    module: &mut IrModule,
    struct_remap: &[Option<StructId>],
    enum_remap: &[Option<EnumId>],
) -> Result<(), Vec<CompilerError>> {
    walk_module_types_mut(module, |ty| remap_type(ty, struct_remap, enum_remap));
    let mut errors: Vec<CompilerError> = Vec::new();
    for imp in &mut module.impls {
        match &mut imp.target {
            crate::ir::ImplTarget::Struct(id) => match struct_remap.get(id.0 as usize).copied() {
                Some(Some(new)) => *id = new,
                Some(None) => errors.push(CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: impl block targets struct id {} which was dropped during compaction (drop_specialised_generic_impls missed it)",
                        id.0
                    ),
                    span: Span::default(),
                }),
                None => errors.push(CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: impl block targets struct id {} which is out of bounds for the remap table (len {})",
                        id.0,
                        struct_remap.len()
                    ),
                    span: Span::default(),
                }),
            },
            crate::ir::ImplTarget::Enum(id) => match enum_remap.get(id.0 as usize).copied() {
                Some(Some(new)) => *id = new,
                Some(None) => errors.push(CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: impl block targets enum id {} which was dropped during compaction (drop_specialised_generic_impls missed it)",
                        id.0
                    ),
                    span: Span::default(),
                }),
                None => errors.push(CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: impl block targets enum id {} which is out of bounds for the remap table (len {})",
                        id.0,
                        enum_remap.len()
                    ),
                    span: Span::default(),
                }),
            },
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn remap_type(
    ty: &mut ResolvedType,
    struct_remap: &[Option<StructId>],
    enum_remap: &[Option<EnumId>],
) {
    match ty {
        ResolvedType::Struct(id) => {
            if let Some(Some(new)) = struct_remap.get(id.0 as usize).copied() {
                *id = new;
            }
        }
        ResolvedType::Enum(id) => {
            if let Some(Some(new)) = enum_remap.get(id.0 as usize).copied() {
                *id = new;
            }
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            remap_type(inner, struct_remap, enum_remap);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                remap_type(t, struct_remap, enum_remap);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            remap_type(key_ty, struct_remap, enum_remap);
            remap_type(value_ty, struct_remap, enum_remap);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                remap_type(t, struct_remap, enum_remap);
            }
            remap_type(return_ty, struct_remap, enum_remap);
        }
        ResolvedType::Generic { base, args } => {
            // Defensive: by Phase 3 every Generic should have been
            // rewritten to a concrete Struct/Enum, but remap the base just
            // in case a caller is inspecting state mid-pass.
            match base {
                GenericBase::Struct(id) => {
                    if let Some(Some(new)) = struct_remap.get(id.0 as usize).copied() {
                        *id = new;
                    }
                }
                GenericBase::Enum(id) => {
                    if let Some(Some(new)) = enum_remap.get(id.0 as usize).copied() {
                        *id = new;
                    }
                }
            }
            for a in args {
                remap_type(a, struct_remap, enum_remap);
            }
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                remap_type(a, struct_remap, enum_remap);
            }
        }
        ResolvedType::Primitive(_) | ResolvedType::Trait(_) | ResolvedType::TypeParam(_) => {}
    }
}

// =============================================================================
// Phase 4: leftover detection (sanity check)
// =============================================================================

#[derive(Default)]
struct LeftoverScanner {
    first: Option<String>,
}

impl LeftoverScanner {
    fn note(&mut self, detail: String) {
        if self.first.is_none() {
            self.first = Some(detail);
        }
    }

    fn first_error(self) -> Option<String> {
        self.first
            .map(|s| format!("monomorphise: leftover after pass — {s}"))
    }

    fn scan(&mut self, module: &IrModule) {
        // Traits with generic_params are still unsupported — the IR has no
        // way to instantiate a generic trait today. Generic structs and
        // enums are compacted in Phase 3.
        for t in &module.traits {
            if !t.generic_params.is_empty() {
                self.note(format!(
                    "generic trait `{}` remains (generic traits are not supported)",
                    t.name
                ));
            }
        }

        let mut check = |ty: &ResolvedType| {
            if let Some(sample) = first_leftover(ty) {
                self.note(sample);
            }
        };
        walk_module_types(module, &mut check);
    }
}

fn first_leftover(ty: &ResolvedType) -> Option<String> {
    // After audit findings #4, #8, and #27, IR lowering no longer emits
    // `TypeParam` as a "best-effort placeholder" — every reachable lowering
    // path either resolves to a concrete type or pushes an
    // `InternalError`. A `TypeParam` survival here therefore means a real
    // monomorphisation gap (a type parameter that wasn't substituted) and
    // should be reported.
    match ty {
        ResolvedType::TypeParam(name) => Some(format!("unresolved TypeParam(`{name}`)")),
        ResolvedType::Generic { base, args } => {
            let (kind, id) = match base {
                GenericBase::Struct(s) => ("struct", s.0),
                GenericBase::Enum(e) => ("enum", e.0),
            };
            Some(format!(
                "unresolved Generic(base={kind}_id={id}, {} args)",
                args.len()
            ))
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            first_leftover(inner)
        }
        ResolvedType::Tuple(fields) => fields.iter().find_map(|(_, t)| first_leftover(t)),
        ResolvedType::Dictionary { key_ty, value_ty } => {
            first_leftover(key_ty).or_else(|| first_leftover(value_ty))
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => param_tys
            .iter()
            .find_map(|(_, t)| first_leftover(t))
            .or_else(|| first_leftover(return_ty)),
        ResolvedType::External { type_args, .. } => type_args.iter().find_map(first_leftover),
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_) => None,
    }
}

// =============================================================================
// Walkers
// =============================================================================

/// Read-only walk over every `ResolvedType` reachable from the module.
fn walk_module_types(module: &IrModule, visit: &mut impl FnMut(&ResolvedType)) {
    for s in &module.structs {
        walk_struct_types(s, visit);
    }
    for t in &module.traits {
        walk_trait_types(t, visit);
    }
    for e in &module.enums {
        walk_enum_types(e, visit);
    }
    for imp in &module.impls {
        walk_impl_types(imp, visit);
    }
    for f in &module.functions {
        walk_function_types(f, visit);
    }
    for l in &module.lets {
        visit(&l.ty);
        walk_expr_types(&l.value, visit);
    }
}

fn walk_struct_types(s: &IrStruct, visit: &mut impl FnMut(&ResolvedType)) {
    for f in &s.fields {
        walk_field_types(f, visit);
    }
}

fn walk_trait_types(t: &IrTrait, visit: &mut impl FnMut(&ResolvedType)) {
    for f in &t.fields {
        walk_field_types(f, visit);
    }
    for sig in &t.methods {
        for p in &sig.params {
            if let Some(ty) = &p.ty {
                visit(ty);
            }
            if let Some(d) = &p.default {
                walk_expr_types(d, visit);
            }
        }
        if let Some(ty) = &sig.return_type {
            visit(ty);
        }
    }
}

fn walk_enum_types(e: &IrEnum, visit: &mut impl FnMut(&ResolvedType)) {
    for v in &e.variants {
        for f in &v.fields {
            walk_field_types(f, visit);
        }
    }
}

fn walk_impl_types(imp: &IrImpl, visit: &mut impl FnMut(&ResolvedType)) {
    for f in &imp.functions {
        walk_function_types(f, visit);
    }
}

fn walk_function_types(f: &IrFunction, visit: &mut impl FnMut(&ResolvedType)) {
    for p in &f.params {
        if let Some(ty) = &p.ty {
            visit(ty);
        }
        if let Some(d) = &p.default {
            walk_expr_types(d, visit);
        }
    }
    if let Some(ty) = &f.return_type {
        visit(ty);
    }
    if let Some(body) = &f.body {
        walk_expr_types(body, visit);
    }
}

fn walk_field_types(f: &IrField, visit: &mut impl FnMut(&ResolvedType)) {
    visit(&f.ty);
    if let Some(d) = &f.default {
        walk_expr_types(d, visit);
    }
}

fn walk_expr_types(expr: &IrExpr, visit: &mut impl FnMut(&ResolvedType)) {
    visit(expr.ty());
    match expr {
        IrExpr::StructInst { fields, .. }
        | IrExpr::EnumInst { fields, .. }
        | IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                walk_expr_types(e, visit);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                walk_expr_types(e, visit);
            }
        }
        IrExpr::FieldAccess { object, .. } => walk_expr_types(object, visit),
        IrExpr::BinaryOp { left, right, .. } => {
            walk_expr_types(left, visit);
            walk_expr_types(right, visit);
        }
        IrExpr::UnaryOp { operand, .. } => walk_expr_types(operand, visit),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            walk_expr_types(condition, visit);
            walk_expr_types(then_branch, visit);
            if let Some(e) = else_branch {
                walk_expr_types(e, visit);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            walk_expr_types(collection, visit);
            walk_expr_types(body, visit);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr_types(scrutinee, visit);
            for arm in arms {
                walk_expr_types(&arm.body, visit);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, a) in args {
                walk_expr_types(a, visit);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            walk_expr_types(receiver, visit);
            for (_, a) in args {
                walk_expr_types(a, visit);
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr_types(k, visit);
                walk_expr_types(v, visit);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            walk_expr_types(dict, visit);
            walk_expr_types(key, visit);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                walk_block_stmt_types(stmt, visit);
            }
            walk_expr_types(result, visit);
        }
        IrExpr::Closure {
            params,
            captures,
            body,
            ..
        } => {
            for (_, _, ty) in params {
                visit(ty);
            }
            for (_, _, ty) in captures {
                visit(ty);
            }
            walk_expr_types(body, visit);
        }
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
    }
}

fn walk_block_stmt_types(
    stmt: &crate::ir::IrBlockStatement,
    visit: &mut impl FnMut(&ResolvedType),
) {
    match stmt {
        IrBlockStatement::Let { value, .. } => walk_expr_types(value, visit),
        IrBlockStatement::Assign { target, value } => {
            walk_expr_types(target, visit);
            walk_expr_types(value, visit);
        }
        IrBlockStatement::Expr(e) => walk_expr_types(e, visit),
    }
}

/// Mutable walk over every `ResolvedType` reachable from the module. The
/// closure is called once per type slot; mutations there are reflected
/// back into the IR.
fn walk_module_types_mut(module: &mut IrModule, mut visit: impl FnMut(&mut ResolvedType)) {
    for s in &mut module.structs {
        for f in &mut s.fields {
            visit(&mut f.ty);
            if let Some(d) = &mut f.default {
                walk_expr_types_mut(d, &mut visit);
            }
        }
    }
    for t in &mut module.traits {
        for f in &mut t.fields {
            visit(&mut f.ty);
            if let Some(d) = &mut f.default {
                walk_expr_types_mut(d, &mut visit);
            }
        }
        for sig in &mut t.methods {
            for p in &mut sig.params {
                if let Some(ty) = &mut p.ty {
                    visit(ty);
                }
                if let Some(d) = &mut p.default {
                    walk_expr_types_mut(d, &mut visit);
                }
            }
            if let Some(ty) = &mut sig.return_type {
                visit(ty);
            }
        }
    }
    for e in &mut module.enums {
        for v in &mut e.variants {
            for f in &mut v.fields {
                visit(&mut f.ty);
                if let Some(d) = &mut f.default {
                    walk_expr_types_mut(d, &mut visit);
                }
            }
        }
    }
    for imp in &mut module.impls {
        for f in &mut imp.functions {
            walk_function_types_mut(f, &mut visit);
        }
    }
    for f in &mut module.functions {
        walk_function_types_mut(f, &mut visit);
    }
    for l in &mut module.lets {
        visit(&mut l.ty);
        walk_expr_types_mut(&mut l.value, &mut visit);
    }
}

fn walk_function_types_mut(f: &mut IrFunction, visit: &mut impl FnMut(&mut ResolvedType)) {
    for p in &mut f.params {
        if let Some(ty) = &mut p.ty {
            visit(ty);
        }
        if let Some(d) = &mut p.default {
            walk_expr_types_mut(d, visit);
        }
    }
    if let Some(ty) = &mut f.return_type {
        visit(ty);
    }
    if let Some(body) = &mut f.body {
        walk_expr_types_mut(body, visit);
    }
}

fn walk_expr_types_mut(expr: &mut IrExpr, visit: &mut impl FnMut(&mut ResolvedType)) {
    walk_expr_types_mut_inner(expr, visit);
}

#[expect(
    clippy::too_many_lines,
    clippy::match_same_arms,
    reason = "exhaustive match over every IrExpr variant; merging similar arms would hide which variants have which children"
)]
fn walk_expr_types_mut_inner(expr: &mut IrExpr, visit: &mut impl FnMut(&mut ResolvedType)) {
    // Visit this node's type first, then descend into children.
    match expr {
        IrExpr::Literal { ty, .. } => visit(ty),
        IrExpr::StructInst { fields, ty, .. } => {
            visit(ty);
            for (_, e) in fields {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::EnumInst { fields, ty, .. } => {
            visit(ty);
            for (_, e) in fields {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::Tuple { fields, ty, .. } => {
            visit(ty);
            for (_, e) in fields {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::Array { elements, ty, .. } => {
            visit(ty);
            for e in elements {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::Reference { ty, .. }
        | IrExpr::SelfFieldRef { ty, .. }
        | IrExpr::LetRef { ty, .. } => visit(ty),
        IrExpr::FieldAccess { object, ty, .. } => {
            visit(ty);
            walk_expr_types_mut_inner(object, visit);
        }
        IrExpr::BinaryOp {
            left, right, ty, ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(left, visit);
            walk_expr_types_mut_inner(right, visit);
        }
        IrExpr::UnaryOp { operand, ty, .. } => {
            visit(ty);
            walk_expr_types_mut_inner(operand, visit);
        }
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ty,
            ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(condition, visit);
            walk_expr_types_mut_inner(then_branch, visit);
            if let Some(e) = else_branch {
                walk_expr_types_mut_inner(e, visit);
            }
        }
        IrExpr::For {
            collection,
            body,
            ty,
            ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(collection, visit);
            walk_expr_types_mut_inner(body, visit);
        }
        IrExpr::Match {
            scrutinee,
            arms,
            ty,
            ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(scrutinee, visit);
            for arm in arms {
                walk_expr_types_mut_inner(&mut arm.body, visit);
            }
        }
        IrExpr::FunctionCall { args, ty, .. } => {
            visit(ty);
            for (_, a) in args {
                walk_expr_types_mut_inner(a, visit);
            }
        }
        IrExpr::MethodCall {
            receiver, args, ty, ..
        } => {
            visit(ty);
            walk_expr_types_mut_inner(receiver, visit);
            for (_, a) in args {
                walk_expr_types_mut_inner(a, visit);
            }
        }
        IrExpr::DictLiteral { entries, ty, .. } => {
            visit(ty);
            for (k, v) in entries {
                walk_expr_types_mut_inner(k, visit);
                walk_expr_types_mut_inner(v, visit);
            }
        }
        IrExpr::DictAccess { dict, key, ty, .. } => {
            visit(ty);
            walk_expr_types_mut_inner(dict, visit);
            walk_expr_types_mut_inner(key, visit);
        }
        IrExpr::Block {
            statements,
            result,
            ty,
            ..
        } => {
            visit(ty);
            for stmt in statements {
                walk_block_stmt_types_mut(stmt, visit);
            }
            walk_expr_types_mut_inner(result, visit);
        }
        IrExpr::Closure {
            params,
            captures,
            body,
            ty,
            ..
        } => {
            visit(ty);
            for (_, _, ty) in params {
                visit(ty);
            }
            for (_, _, ty) in captures {
                visit(ty);
            }
            walk_expr_types_mut_inner(body, visit);
        }
    }
}

fn walk_block_stmt_types_mut(
    stmt: &mut IrBlockStatement,
    visit: &mut impl FnMut(&mut ResolvedType),
) {
    match stmt {
        IrBlockStatement::Let { value, .. } => walk_expr_types_mut_inner(value, visit),
        IrBlockStatement::Assign { target, value } => {
            walk_expr_types_mut_inner(target, visit);
            walk_expr_types_mut_inner(value, visit);
        }
        IrBlockStatement::Expr(e) => walk_expr_types_mut_inner(e, visit),
    }
}
