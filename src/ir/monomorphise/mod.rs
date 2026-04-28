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
//! let source = "pub struct Box<T> { value: T }\npub let b: Box<I32> = Box(value: 1)";
//! let module = compile_to_ir(source).unwrap();
//! let result = Pipeline::new().pass(MonomorphisePass::default()).run(module);
//! assert!(result.is_ok());
//! ```

use std::collections::{HashMap, VecDeque};

use crate::error::CompilerError;
use crate::ir::{GenericBase, IrModule, StructId};
use crate::location::Span;
use crate::pipeline::IrPass;

mod collect;
mod compact;
mod expr_walk;
mod external;
mod functions;
mod leftover;
mod rewrite;
mod specialise;
mod walkers;

use collect::collect_all_instantiations;
use compact::{
    apply_impl_index_remap, apply_remaps, build_enum_remap, build_struct_remap, build_trait_remap,
    drop_specialised_generic_impls,
};
use external::{rewrite_external_references, specialise_external_instantiations};
use functions::specialise_generic_functions;
use leftover::LeftoverScanner;
use rewrite::{
    devirtualise_concrete_receivers, rewrite_dispatch_impl_ids, rewrite_module, specialise_impls,
};
use specialise::{specialise, Instantiation};

/// Monomorphisation pass. See module docs for the algorithm.
#[expect(
    clippy::exhaustive_structs,
    reason = "single optional field for imported module IRs; no further fields planned"
)]
#[derive(Debug, Clone, Default)]
pub struct MonomorphisePass {
    /// Imported IRs keyed by logical module path. When present, generic
    /// `External` types are specialised locally; when empty they are left
    /// as-is.
    pub imported_modules: HashMap<Vec<String>, IrModule>,
}

impl MonomorphisePass {
    /// Configure the pass with imported module IRs, keyed by logical module
    /// path (matching `ResolvedType::External::module_path`).
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

        // Phase 1a: clone each imported generic `External` into the current
        // module under a fresh local id. Phase 2 uses the returned map to
        // rewrite the External references.
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
        if let Err(e) = specialise_generic_functions(&mut module) {
            errors.extend(e);
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        // Phase 2e: devirtualise. FormaLang has no dynamic dispatch
        // (Tier-1 item E2 bans trait values at semantic time), so any
        // `DispatchKind::Virtual` whose receiver became concrete after
        // Phases 2 and 2d must be resolved to `Static` here. Calls
        // inside a generic body that was never instantiated still
        // carry a `TypeParam` receiver; those stay `Virtual` and are
        // tolerated by the leftover scanner since the function itself
        // is dropped during compaction.
        devirtualise_concrete_receivers(&mut module);

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
        apply_remaps(&mut module, &struct_remap, &enum_remap, &trait_remap)?;
        apply_impl_index_remap(&mut module, &impl_index_remap);
        module.structs.retain(|s| s.generic_params.is_empty());
        module.enums.retain(|e| e.generic_params.is_empty());
        module.traits.retain(|t| t.generic_params.is_empty());
        // Tier-1 Phase 2e: generic-function compaction mirrors the
        // struct/enum rules. Originals with non-empty `generic_params`
        // were either cloned per call-site arg-tuple (and the clones
        // have empty `generic_params`) or never instantiated — either
        // way they have no surviving callers and are dropped here.
        module.functions.retain(|f| f.generic_params.is_empty());
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
