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
//! - The pass does not yet chase through external module imports
//!   (`ResolvedType::External`) — generic arguments on imported types
//!   are walked but not specialised.
//!
//! # Usage
//!
//! ```no_run
//! use formalang::{compile_to_ir, Pipeline};
//! use formalang::ir::MonomorphisePass;
//!
//! let source = "pub struct Box<T> { value: T }\npub let b: Box<Number> = Box(value: 1)";
//! let module = compile_to_ir(source).unwrap();
//! let result = Pipeline::new().pass(MonomorphisePass).run(module);
//! assert!(result.is_ok());
//! ```

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::CompilerError;
use crate::ir::{
    EnumId, GenericBase, IrBlockStatement, IrEnum, IrExpr, IrField, IrFunction, IrImpl, IrModule,
    IrStruct, IrTrait, PrimitiveType, ResolvedType, StructId,
};
use crate::location::Span;
use crate::pipeline::IrPass;

/// Monomorphisation pass.
///
/// See the module-level documentation in `src/ir/monomorphise.rs` for the
/// full algorithm and limitations.
#[expect(
    clippy::exhaustive_structs,
    reason = "zero-sized marker type; no fields to add"
)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MonomorphisePass;

impl IrPass for MonomorphisePass {
    fn name(&self) -> &'static str {
        "monomorphise"
    }

    fn run(&mut self, mut module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        let mut errors = Vec::new();

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

        // Phase 2: rewrite every Generic reference to its specialisation.
        rewrite_module(&mut module, &mapping);

        // Phase 3: compact — drop the original generic structs and enums,
        // remap surviving IDs for each kind.
        let struct_remap = build_struct_remap(&module);
        let enum_remap = build_enum_remap(&module);
        apply_remaps(&mut module, &struct_remap, &enum_remap);
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
        ResolvedType::Array(inner) | ResolvedType::Optional(inner) => {
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
// Phase 1b: specialisation
// =============================================================================

/// A single generic instantiation key: `(base, type_args)`.
type Instantiation = (GenericBase, Vec<ResolvedType>);

/// Outcome of specialising a single generic instantiation.
type SpecialiseOk = (GenericBase, Vec<Instantiation>);

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
        ResolvedType::Array(inner) | ResolvedType::Optional(inner) => {
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
        ResolvedType::Array(inner) | ResolvedType::Optional(inner) => {
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

fn apply_remaps(
    module: &mut IrModule,
    struct_remap: &[Option<StructId>],
    enum_remap: &[Option<EnumId>],
) {
    walk_module_types_mut(module, |ty| remap_type(ty, struct_remap, enum_remap));
    for imp in &mut module.impls {
        match &mut imp.target {
            crate::ir::ImplTarget::Struct(id) => {
                if let Some(Some(new)) = struct_remap.get(id.0 as usize).copied() {
                    *id = new;
                }
            }
            crate::ir::ImplTarget::Enum(id) => {
                if let Some(Some(new)) = enum_remap.get(id.0 as usize).copied() {
                    *id = new;
                }
            }
        }
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
        ResolvedType::Array(inner) | ResolvedType::Optional(inner) => {
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
    // NOTE: `TypeParam(name)` is *not* treated as a leftover here. The IR
    // lowering layer currently emits `TypeParam` as a placeholder for
    // unresolved field-access receivers and similar best-effort shapes
    // (tracked as a separate follow-up). Once that is tightened, the
    // `TypeParam` arm below should be re-enabled.
    match ty {
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
        ResolvedType::Array(inner) | ResolvedType::Optional(inner) => first_leftover(inner),
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
        | ResolvedType::Enum(_)
        | ResolvedType::TypeParam(_) => None,
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
            for (_, ty) in captures {
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
            for (_, ty) in captures {
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
