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
//! After the pass runs, the only `ResolvedType::Generic` references
//! left in the IR are the five built-in carriers of the prelude:
//! `Array`, `Seq`, `Dictionary`, `Range` and `Optional`. They are the
//! types `[T]`, a sequence, `[K: V]`, `a..b` and `T?`, and
//! `Generic { base, args }` is their final shape: a backend reads the
//! element type from `args`. Every other generic struct, enum, trait and
//! function is replaced by one concrete copy per use.
//!
//! Generic traits take the same path: each `(trait, args)` pair in a
//! constraint or an impl header gets a concrete copy of the trait,
//! and the generic original goes.
//!
//! A generic function, and a method with its own type parameters and a
//! body, is copied once for each tuple of type arguments that a call
//! gives it (see `functions.rs` and `methods.rs`). The pass finds the
//! tuple by unification of the declared parameter types with the
//! argument types (`unify.rs`). After Phase 2 an argument can already
//! have a specialised type, such as `Struct(Box__I32)`; `origins.rs`
//! keeps the instantiation behind each specialisation, so a parameter
//! of type `Box<T>` still binds `T` to `I32`.
//!
//! # Depth limit
//!
//! A generic that uses itself with a larger type, such as
//! `fn grow<T>(x: T) { grow(x: Box(value: x)) }`, needs a new copy at
//! each level, so the worklist never empties. The pass stops at a
//! nesting depth of 32 type arguments and reports
//! [`CompilerError::InstantiationDepthExceeded`] (E148). Its field
//! `written` is true when the deep type is in the program text, such
//! as `Box<Box<...>>`, and false when a generic makes it by a call of
//! itself.
//!
//! # Ids after the pass
//!
//! Compaction renumbers the structs, enums, traits, functions and
//! impls. The pass then makes each id agree with the definition that
//! its path or type names: the `function_id` of each call
//! (`call_ids.rs`), and the `struct_id` or `enum_id` of each instance
//! (`instance_ids.rs`).
//!
//! # Imported modules
//!
//! The public entry points link each imported module into the module
//! that they give (see `crate::ir::link`), so the module holds no
//! `ResolvedType::External` and the pass needs no import map. A caller
//! that builds a module with `External` types, for example with
//! [`crate::ir::lower_to_ir`], can supply the IR of each imported
//! module with [`MonomorphisePass::with_imports`]. The pass then
//! copies each imported definition into the module under a qualified
//! name, and rewrites each `External` to the copy. Without an import
//! map, the pass leaves `External` references alone.
//!
//! # Usage
//!
//! ```no_run
//! use formalang::{compile_to_ir, Pipeline};
//! use formalang::ir::MonomorphisePass;
//!
//! let source = "pub struct Box<T> { value: T }\npub let b: Box<I32> = Box(value: 1)";
//! let module = compile_to_ir(source).unwrap();
//! let result = Pipeline::new().pass(MonomorphisePass::default()).run(module);
//! assert!(result.is_ok());
//! ```

use std::collections::{HashMap, VecDeque};

use crate::error::CompilerError;
use crate::ir::{GenericBase, IrModule, ResolvedType, StructId};
use crate::location::Span;
use crate::pipeline::IrPass;

mod call_ids;
mod collect;
mod compact;
mod expr_walk;
mod external;
mod functions;
mod instance_ids;
mod leftover;
mod methods;
mod origins;
mod rewrite;
pub(in crate::ir) mod specialise;
mod unify;
pub(super) mod walkers;

use call_ids::resync_call_ids;
use collect::collect_all_instantiations;
use compact::{
    apply_impl_index_remap, apply_remaps, build_enum_remap, build_struct_remap, build_trait_remap,
    drop_specialised_generic_impls,
};
use external::{
    detect_import_cycle, inline_imported_functions, inline_imported_impls, inline_imported_lets,
    merge_imported_module_trees, qualify_imported_paths, remap_imported_body_ids,
    remap_imported_file_ids, rewrite_external_references, specialise_external_instantiations,
};
use functions::specialise_generic_functions;
use leftover::LeftoverScanner;
use rewrite::{
    devirtualise_concrete_receivers, rewrite_dispatch_impl_ids, rewrite_module, specialise_impls,
};
use specialise::{specialise, Instantiation};

/// Monomorphisation pass. See the module documentation for the algorithm.
#[expect(
    clippy::exhaustive_structs,
    reason = "single optional field for imported module IRs; no further fields planned"
)]
#[derive(Debug, Clone, Default)]
pub struct MonomorphisePass {
    /// Imported IRs keyed by logical module path. When present, each
    /// `External` type is replaced with a local copy of its definition;
    /// when empty, `External` types stay as they are. No public entry
    /// point fills it: their modules hold no `External` type.
    pub imported_modules: HashMap<Vec<String>, IrModule>,
}

impl MonomorphisePass {
    /// Configure the pass with imported module IRs, keyed by logical module
    /// path (matching `ResolvedType::External::module_path`).
    ///
    /// Only a module that holds `External` types needs this, such as the
    /// result of [`crate::ir::lower_to_ir`] for a file with a `use`. The
    /// public entry points link the imported modules during the lowering
    /// and do not call it.
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

    #[expect(
        clippy::too_many_lines,
        reason = "the pass is a single ordered phase pipeline; phase boundaries are commented inline"
    )]
    #[expect(
        clippy::items_after_statements,
        reason = "the inline `args_have_type_param` / `contains_type_param` helpers belong next to the closure that uses them"
    )]
    fn run(&mut self, mut module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        let mut errors = Vec::new();
        // Tracks (imported_module_path, imported_impl_idx) → local_impl_idx
        // for the body-id remap pass. Populated by Phase 1c.
        let mut impl_clone_remap: HashMap<(Vec<String>, u32), u32> = HashMap::new();

        // Phase 0: defence-in-depth import-cycle check. Semantic
        // analysis already rejects cyclic `use` chains; surface a
        // clear diagnostic if one slipped through to the IR layer.
        if !self.imported_modules.is_empty() {
            if let Some(cycle_node) = detect_import_cycle(&self.imported_modules) {
                return Err(vec![CompilerError::InternalError {
                    detail: format!(
                        "monomorphise: cyclic import involving module {cycle_node:?} reached the IR layer; semantic analysis should have rejected this"
                    ),
                    span: Span::default(),
                }]);
            }
        }

        // Phase 1a: clone each imported `External` (generic or non-generic)
        // into the current module under a qualified name. Phase 2 uses the
        // returned map to rewrite the External references.
        let mut external_mapping = if self.imported_modules.is_empty() {
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

        // Phase 1b: inline imported functions into the current module
        // under qualified names. Each clone's signature + body has its
        // ResolvedType references externalised so later phases can
        // rewrite them to local clones. After this runs, the entry
        // module's `functions` includes every imported function;
        // call sites resolve via path-based lookup, and DCE prunes
        // unused clones in the codegen pipeline.
        if !self.imported_modules.is_empty() {
            if let Err(mut e) = inline_imported_functions(&mut module, &self.imported_modules) {
                errors.append(&mut e);
            }
            // Phase 1c: inline imported impl blocks whose target type
            // is now in the local module. Method signatures and bodies
            // have their types externalised the same way as functions.
            if let Err(mut e) =
                inline_imported_impls(&mut module, &self.imported_modules, &mut impl_clone_remap)
            {
                errors.append(&mut e);
            }
            // Phase 1d: inline imported pub `let`s under qualified
            // names. Initialiser expressions have their types
            // externalised the same way as function bodies.
            inline_imported_lets(&mut module, &self.imported_modules);
            // Re-run Phase 1a so any External references introduced by
            // the inlined function / impl-method bodies (types they
            // reference that the entry module didn't directly mention)
            // are also specialised. Existing entries in
            // `external_mapping` are deduplicated by the worklist; new
            // entries are merged in.
            match specialise_external_instantiations(&mut module, &self.imported_modules) {
                Ok(more) => external_mapping.extend(more),
                Err(mut e) => errors.append(&mut e),
            }
            // Phase 1e: walk every cloned item's body and translate
            // `FunctionCall.function_id` and
            // `DispatchKind::Static.impl_id` references from each
            // imported module's id-space into the entry module's
            // id-space. `Reference.target` stays Unresolved (lowering
            // emitted it that way; ResolveReferencesPass will rebind
            // via the qualified-name path).
            remap_imported_body_ids(&mut module, &self.imported_modules, &impl_clone_remap);
            // Phase 1f: qualify single-segment paths in entry-side
            // FunctionCall/Reference expressions where the bare name
            // matches a cloned imported item. Lets `use helper::greet;
            // greet()` resolve correctly to the cloned `helper::greet`
            // via the qualified-name lookup in ResolveReferencesPass.
            qualify_imported_paths(&mut module, &self.imported_modules);
            // Phase 2a: splice each imported module's IrModuleNode
            // tree into the entry's modules tree under the import's
            // module_path, populating with translated local ids.
            merge_imported_module_trees(&mut module, &self.imported_modules);
            // Phase 2b: register each imported source file in the
            // entry's `file_table` and remap every cloned item's
            // `IrSpan.file` from the imported id-space into the entry
            // id-space. After this pass, every span resolves through
            // `IrModule.file_path` on the entry — backends emitting
            // DWARF / source maps don't need access to the imported
            // modules' IR.
            remap_imported_file_ids(&mut module, &self.imported_modules);
        }

        // Phase 1: collect every `Generic { base, args }` instantiation in
        // the module. The worklist processes args recursively when a
        // specialisation itself references more generics. Prelude-shipped
        // built-ins (`Optional`, `Array`, `Seq`, `Dictionary`, `Range`)
        // are type carriers, not specialisable templates, so filter them
        // out up front. Any nested args inside them stay on the worklist.
        let prelude_skip = compact::prelude_carrier_bases(&module);
        // Filter out:
        //   - prelude built-ins (carriers, never specialised);
        //   - instantiations whose args still contain `TypeParam`
        //     (these come from inside generic-function bodies; the
        //     containing function will be cloned by Phase 2d with
        //     concrete args, and those concrete instantiations get
        //     picked up at that point).
        fn args_have_type_param(args: &[ResolvedType]) -> bool {
            args.iter().any(contains_type_param)
        }
        fn contains_type_param(ty: &ResolvedType) -> bool {
            match ty {
                ResolvedType::TypeParam(_) => true,
                ResolvedType::Tuple(fields) => fields.iter().any(|(_, t)| contains_type_param(t)),
                ResolvedType::Closure {
                    param_tys,
                    return_ty,
                } => {
                    param_tys.iter().any(|(_, t)| contains_type_param(t))
                        || contains_type_param(return_ty)
                }
                ResolvedType::Generic { args, .. } => args.iter().any(contains_type_param),
                ResolvedType::External { type_args, .. } => {
                    type_args.iter().any(contains_type_param)
                }
                ResolvedType::Primitive(_)
                | ResolvedType::Struct(_)
                | ResolvedType::Trait(_)
                | ResolvedType::Enum(_)
                | ResolvedType::Error => false,
            }
        }
        let initial = collect_all_instantiations(&module);
        let mut worklist: VecDeque<Instantiation> = initial
            .into_iter()
            .filter(|(base, args)| !prelude_skip.contains(base) && !args_have_type_param(args))
            .collect();
        let mut mapping: HashMap<Instantiation, GenericBase> = HashMap::new();
        let mut origins = origins::Origins::default();

        while let Some(inst) = worklist.pop_front() {
            if mapping.contains_key(&inst) {
                continue;
            }
            if let Some(error) = origins.too_deep(&inst, &module, true) {
                // Each deeper level of one written type is its own
                // instantiation. One report for each generic is enough.
                if !errors.contains(&error) {
                    errors.push(error);
                }
                mapping.insert(inst, GenericBase::Struct(StructId(u32::MAX)));
                continue;
            }
            match specialise(&mut module, &inst) {
                Ok((spec_base, more)) => {
                    origins.insert(inst.clone(), spec_base);
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
        let impl_remap = specialise_impls(&mut module, &mapping, |_| true);

        // Phase 2c: rewrite `DispatchKind::Static { impl_id }` so calls on
        // specialised receivers dispatch to the cloned impl, not the
        // original generic slot.
        rewrite_dispatch_impl_ids(&mut module, &impl_remap);

        // Phase 2d: specialise generic functions. Walks every
        // `IrExpr::FunctionCall` whose path resolves to a function
        // with non-empty `generic_params`, infers the type-arg tuple
        // by unifying the function's declared parameter types against
        // the call site's argument types, and clones the function per
        // unique tuple with `TypeParam` substituted in params, return,
        // and body. Call sites are rewritten to point at the
        // specialised name; original generic functions are dropped in
        // Phase 3 (`functions.retain(|f| f.generic_params.is_empty())`).
        // Tier-1 follow-up to item E2: this also gives Phase 2e
        // (devirtualisation) source-level reachability — substituting
        // `TypeParam(T)` for the concrete receiver type inside a
        // generic body's method call is what makes the dispatch
        // resolvable.
        if let Err(e) = specialise_generic_functions(&mut module, &origins) {
            errors.extend(e);
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        // Phase 2d-bis: cloned function bodies may contain fresh
        // `Generic { base, args }` instantiations that weren't in the
        // original Phase 1 worklist (a generic body that constructs
        // `Pair<T, T>` becomes `Pair<I32, I32>` after the clone for
        // T=I32). Re-collect, specialise the new arrivals, and rewrite
        // so those references reach concrete struct/enum ids.
        let before_post: std::collections::HashSet<Instantiation> =
            mapping.keys().cloned().collect();
        let post_clone_initial = collect_all_instantiations(&module);
        let mut post_worklist: VecDeque<Instantiation> = post_clone_initial
            .into_iter()
            .filter(|(base, args)| {
                !prelude_skip.contains(base)
                    && !args_have_type_param(args)
                    && !mapping.contains_key(&(*base, args.clone()))
            })
            .collect();
        while let Some(inst) = post_worklist.pop_front() {
            if mapping.contains_key(&inst) {
                continue;
            }
            if let Some(error) = origins.too_deep(&inst, &module, false) {
                errors.push(error);
                mapping.insert(inst, GenericBase::Struct(StructId(u32::MAX)));
                continue;
            }
            match specialise(&mut module, &inst) {
                Ok((spec_base, more)) => {
                    origins.insert(inst.clone(), spec_base);
                    mapping.insert(inst, spec_base);
                    post_worklist
                        .extend(more.into_iter().filter(|(b, a)| {
                            !prelude_skip.contains(b) && !args_have_type_param(a)
                        }));
                }
                Err(e) => {
                    errors.push(e);
                    mapping.insert(inst, GenericBase::Struct(StructId(u32::MAX)));
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        rewrite_module(&mut module, &mapping);
        let post_impl_remap =
            specialise_impls(&mut module, &mapping, |inst| !before_post.contains(inst));
        rewrite_dispatch_impl_ids(&mut module, &post_impl_remap);

        // Phase 2e: devirtualise. FormaLang has no dynamic dispatch
        // (Tier-1 item E2 bans trait values at semantic time), so any
        // `DispatchKind::Virtual` whose receiver became concrete after
        // Phases 2 and 2d must be resolved to `Static` here. Calls
        // inside a generic body that was never instantiated still
        // carry a `TypeParam` receiver; those stay `Virtual` and are
        // tolerated by the leftover scanner since the function itself
        // is dropped during compaction.
        devirtualise_concrete_receivers(&mut module, &mapping);

        // Phase 3: compact — drop the original generic structs, enums, and
        // the generic impls that were expanded in Phase 2b; then remap
        // surviving IDs for each kind. Order matters: drop generic-targeted
        // impls before `apply_remaps` rewrites ids, because the retain
        // predicate below indexes into the pre-compaction remap tables.
        let struct_remap = build_struct_remap(&module);
        let enum_remap = build_enum_remap(&module);
        let trait_remap = build_trait_remap(&module);
        let impl_index_remap =
            drop_specialised_generic_impls(&mut module, &struct_remap, &enum_remap);
        // Tier-1 Phase 2e: generic-function compaction mirrors the
        // struct/enum rules. Originals with non-empty `generic_params`
        // were either cloned per call-site arg-tuple (and the clones
        // have empty `generic_params`) or never instantiated — either
        // way they have no surviving callers and are dropped here.
        // They go before the remaps: a function that no call
        // instantiates can still dispatch through a generic trait that
        // compaction drops, and nothing can remap that dispatch.
        module.functions.retain(|f| f.generic_params.is_empty());
        apply_remaps(&mut module, &struct_remap, &enum_remap, &trait_remap)?;
        apply_impl_index_remap(&mut module, &impl_index_remap);
        // Compaction normally drops every generic template (its uses
        // were specialised in Phase 2). The five prelude-shipped
        // built-in carriers (`Optional`, `Array`, `Seq`, `Dictionary`,
        // `Range`) are exempt: they're never specialised — `Generic { base, args }`
        // is their canonical post-pass shape — so they have to survive
        // for dispatch and lookup to keep working.
        let is_prelude_builtin =
            |name: &str| compact::is_prelude_struct_name(name) || name == "Optional";
        module
            .structs
            .retain(|s| s.generic_params.is_empty() || is_prelude_builtin(&s.name));
        module
            .enums
            .retain(|e| e.generic_params.is_empty() || is_prelude_builtin(&e.name));
        module.traits.retain(|t| t.generic_params.is_empty());
        module.rebuild_indices();

        // Phase 3b: a call's `function_id` has to agree with its path.
        // Specialisation rewrote the paths and compaction renumbered
        // the functions; neither touched the ids.
        resync_call_ids(&mut module);
        instance_ids::resync_instance_ids(&mut module);

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
