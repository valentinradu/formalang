//! Dead code elimination pass for IR optimization.
//!
//! This module removes code that doesn't affect program output:
//! - Unreachable branches (constant false conditions)
//! - Unused struct definitions
//! - Unused let bindings, when their value can have no effect and no
//!   fault
//!
//! # Example
//!
//! ```formalang
//! pub struct Used { value: I32 }
//! struct Unused { data: String }  // removed: nothing refers to it
//! let unused_value: I32 = 5       // removed: private, and nothing reads it
//! pub fn make() -> Used { Used(value: 1) }
//! ```

mod expr;
mod filtering;
mod lets;
mod reachability;
mod remap;

#[cfg(test)]
mod tests;

use crate::ir::{EnumId, IrModule, StructId, TraitId};
use std::collections::HashSet;

pub use expr::eliminate_dead_code_expr;
use remap::remove_unused_definitions;

/// Dead code eliminator that removes unreachable and unused code.
#[derive(Debug)]
pub struct DeadCodeEliminator<'a> {
    pub(super) module: &'a IrModule,
    /// Structs that are actually used
    pub(super) used_structs: HashSet<StructId>,
    /// Traits that are actually used (including those referenced only as
    /// trait constraints on generic parameters).
    pub(super) used_traits: HashSet<TraitId>,
    /// Enums that are actually used (including those referenced only in
    /// field types or variant constructions).
    pub(super) used_enums: HashSet<EnumId>,
}

impl<'a> DeadCodeEliminator<'a> {
    /// Create a new dead code eliminator.
    #[must_use]
    pub fn new(module: &'a IrModule) -> Self {
        Self {
            module,
            used_structs: HashSet::new(),
            used_traits: HashSet::new(),
            used_enums: HashSet::new(),
        }
    }

    /// Analyze the module to find all used definitions.
    pub fn analyze(&mut self) {
        // A public definition is the module's contract with whatever
        // links against it, so nothing inside the module needs to
        // reference it for it to be live.
        //
        // Without this the pass removed an unreferenced `pub struct`
        // while keeping every function, public or not — it never
        // removes functions at all. A library whose types were only
        // named by its callers came out of the pipeline with its API
        // stripped, and the backend emitted nothing a consumer could
        // use.
        self.mark_public_definitions();

        // Walk impl-method bodies for references. Do NOT mark the impl's
        // target type as used purely because an impl block exists; an impl
        // is dead code if nothing else references its target. Impls whose
        // target is removed are pruned by `remove_unused_definitions`.
        for impl_block in &self.module.impls {
            for func in &impl_block.functions {
                if let Some(body) = &func.body {
                    self.mark_used_in_expr(body);
                }
                for p in &func.params {
                    if let Some(ty) = &p.ty {
                        self.mark_used_in_type(ty);
                    }
                }
                if let Some(ret) = &func.return_type {
                    self.mark_used_in_type(ret);
                }
            }
        }

        // Mark structs used in standalone function bodies
        for func in &self.module.functions {
            if let Some(body) = &func.body {
                self.mark_used_in_expr(body);
            }
            // Trait constraints on generic parameters keep the trait alive
            for gp in &func.params {
                if let Some(ty) = &gp.ty {
                    self.mark_used_in_type(ty);
                }
            }
        }

        // Mark structs used in let bindings
        for let_binding in &self.module.lets {
            self.mark_used_in_expr(&let_binding.value);
            self.mark_used_in_type(&let_binding.ty);
        }

        // Mark structs referenced in struct fields and trait constraints on
        // their generic parameters.
        for s in &self.module.structs {
            for field in &s.fields {
                self.mark_used_in_type(&field.ty);
            }
            for trait_ref in &s.traits {
                self.used_traits.insert(trait_ref.trait_id);
                for arg in &trait_ref.args {
                    self.mark_used_in_type(arg);
                }
            }
            for gp in &s.generic_params {
                for constraint in &gp.constraints {
                    self.used_traits.insert(constraint.trait_id);
                    for arg in &constraint.args {
                        self.mark_used_in_type(arg);
                    }
                }
            }
        }

        // Mark trait constraints referenced by trait generic parameters and
        // through trait composition. A trait's own methods may also mention
        // types in their signatures.
        for t in &self.module.traits {
            for composed in &t.composed_traits {
                self.used_traits.insert(*composed);
            }
            for gp in &t.generic_params {
                for constraint in &gp.constraints {
                    self.used_traits.insert(constraint.trait_id);
                    for arg in &constraint.args {
                        self.mark_used_in_type(arg);
                    }
                }
            }
            for field in &t.fields {
                self.mark_used_in_type(&field.ty);
            }
            for method in &t.methods {
                for p in &method.params {
                    if let Some(ty) = &p.ty {
                        self.mark_used_in_type(ty);
                    }
                }
                if let Some(ret) = &method.return_type {
                    self.mark_used_in_type(ret);
                }
            }
        }

        // Mark trait constraints referenced by function generic parameters
        // and types mentioned in their signatures.
        for f in &self.module.functions {
            for p in &f.params {
                if let Some(ty) = &p.ty {
                    self.mark_used_in_type(ty);
                }
            }
            if let Some(ret) = &f.return_type {
                self.mark_used_in_type(ret);
            }
        }

        // A default can build a struct or name an enum. See the method.
        self.mark_used_in_defaults();

        // Enum variant fields can also reference types.
        self.mark_used_in_enums();

        // The trait of a live impl stays alive, so the impl never names
        // a removed trait. It is the only reference to a trait instance
        // such as `Container<I32>` after devirtualisation.
        self.mark_used_in_impl_traits();
    }

    /// Mark the trait, and the types in its arguments, of each impl
    /// whose target is live.
    fn mark_used_in_impl_traits(&mut self) {
        for imp in &self.module.impls {
            let live = match imp.target {
                crate::ir::ImplTarget::Struct(id) => self.used_structs.contains(&id),
                crate::ir::ImplTarget::Enum(id) => self.used_enums.contains(&id),
                crate::ir::ImplTarget::Primitive(_) => true,
            };
            if let (true, Some(trait_ref)) = (live, &imp.trait_ref) {
                self.used_traits.insert(trait_ref.trait_id);
                for arg in &trait_ref.args {
                    self.mark_used_in_type(arg);
                }
            }
        }
    }

    /// Mark the types in each enum variant field, and the traits in
    /// each constraint on an enum's type parameters.
    fn mark_used_in_enums(&mut self) {
        for e in &self.module.enums {
            for variant in &e.variants {
                for field in &variant.fields {
                    self.mark_used_in_type(&field.ty);
                }
            }
            for gp in &e.generic_params {
                for constraint in &gp.constraints {
                    self.used_traits.insert(constraint.trait_id);
                    for arg in &constraint.args {
                        self.mark_used_in_type(arg);
                    }
                }
            }
        }
    }

    /// Mark what every parameter default and field default uses. A
    /// default is an expression like any other, and what it builds or
    /// names stays alive. Without this, a private struct that only a
    /// parameter default built was removed, and the default kept its
    /// stale id.
    fn mark_used_in_defaults(&mut self) {
        let module = self.module;
        let params = module
            .functions
            .iter()
            .chain(module.impls.iter().flat_map(|i| i.functions.iter()))
            .flat_map(|f| f.params.iter())
            .filter_map(|p| p.default.as_ref());
        let fields = module
            .structs
            .iter()
            .flat_map(|s| s.fields.iter())
            .chain(
                module
                    .enums
                    .iter()
                    .flat_map(|e| e.variants.iter().flat_map(|v| v.fields.iter())),
            )
            .chain(module.traits.iter().flat_map(|t| t.fields.iter()))
            .filter_map(|f| f.default.as_ref());
        for default in params.chain(fields) {
            self.mark_used_in_expr(default);
        }
    }

    /// Seed the used sets with every `pub` struct, enum and trait.
    fn mark_public_definitions(&mut self) {
        use crate::ast::Visibility;

        for (index, s) in self.module.structs.iter().enumerate() {
            if s.visibility == Visibility::Public {
                if let Ok(id) = u32::try_from(index) {
                    self.used_structs.insert(StructId(id));
                }
            }
        }
        for (index, t) in self.module.traits.iter().enumerate() {
            if t.visibility == Visibility::Public {
                if let Ok(id) = u32::try_from(index) {
                    self.used_traits.insert(TraitId(id));
                }
            }
        }
        for (index, e) in self.module.enums.iter().enumerate() {
            if e.visibility == Visibility::Public {
                if let Ok(id) = u32::try_from(index) {
                    self.used_enums.insert(EnumId(id));
                }
            }
        }
    }

    /// Check if a struct is used.
    #[must_use]
    pub fn is_struct_used(&self, id: StructId) -> bool {
        self.used_structs.contains(&id)
    }

    /// Check if a trait is used.
    ///
    /// A trait is considered used when it appears as a type, as a trait
    /// constraint on a generic parameter, or as a trait composed into
    /// another live trait.
    #[must_use]
    pub fn is_trait_used(&self, id: TraitId) -> bool {
        self.used_traits.contains(&id)
    }

    /// Get the set of used struct IDs.
    #[must_use]
    pub const fn used_structs(&self) -> &HashSet<StructId> {
        &self.used_structs
    }

    /// Get the set of used trait IDs.
    #[must_use]
    pub const fn used_traits(&self) -> &HashSet<TraitId> {
        &self.used_traits
    }

    /// Get the set of used enum IDs.
    #[must_use]
    pub const fn used_enums_set(&self) -> &HashSet<EnumId> {
        &self.used_enums
    }

    /// Check if an enum is used.
    #[must_use]
    pub fn is_enum_used(&self, id: EnumId) -> bool {
        self.used_enums.contains(&id)
    }
}

/// Eliminate dead code from an entire module.
///
/// This removes:
/// - Unreachable branches in expressions
/// - Unused local `let` bindings whose value has no effect
/// - Unused struct, trait and enum definitions, and unused private
///   module `let` bindings whose value has no effect (when
///   `remove_unused_structs` is true)
#[must_use]
pub fn eliminate_dead_code(module: &IrModule, remove_unused_structs: bool) -> IrModule {
    let mut result = module.clone();

    // Process expressions in impl blocks
    for impl_block in &mut result.impls {
        for func in &mut impl_block.functions {
            func.body = func.body.take().map(eliminate_dead_code_expr);
        }
    }

    // Process standalone functions
    for func in &mut result.functions {
        func.body = func.body.take().map(eliminate_dead_code_expr);
    }

    // Process let bindings
    for let_binding in &mut result.lets {
        let_binding.value = eliminate_dead_code_expr(let_binding.value.clone());
    }

    // Process struct field defaults
    for struct_def in &mut result.structs {
        for field in &mut struct_def.fields {
            if let Some(default) = &mut field.default {
                *default = eliminate_dead_code_expr(default.clone());
            }
        }
    }

    // Remove the bindings that nothing reads. This runs before the
    // definitions go, so a struct that only an unused binding named goes
    // too.
    lets::remove_unused_lets(&mut result, remove_unused_structs);

    // Physically remove unused structs/traits/enums, then rewrite every ID
    // reference so the module stays internally consistent.
    //
    // Removing a definition can make another one unreachable: a trait
    // whose method signature mentions `Dictionary` keeps `Dictionary`
    // alive, so dropping the trait has to be followed by another look
    // at `Dictionary`. One round is therefore not enough, and the pass
    // used to leave that second definition behind — which also made it
    // non-idempotent, since a second `run` removed what the first one
    // missed. Repeat until nothing more goes.
    //
    // Each round removes at least one definition or stops, so the loop
    // runs at most once per definition. The bound below is belt and
    // braces against a future change that could make a round remove
    // nothing while still reporting a different count.
    if remove_unused_structs {
        let rounds = result
            .structs
            .len()
            .saturating_add(result.traits.len())
            .saturating_add(result.enums.len())
            .saturating_add(1);
        for _ in 0..rounds {
            let before = (
                result.structs.len(),
                result.traits.len(),
                result.enums.len(),
            );

            let mut eliminator = DeadCodeEliminator::new(&result);
            eliminator.analyze();
            let used_structs = eliminator.used_structs.clone();
            let used_traits = eliminator.used_traits.clone();
            let used_enums = eliminator.used_enums.clone();
            drop(eliminator);
            remove_unused_definitions(&mut result, &used_structs, &used_traits, &used_enums);

            let after = (
                result.structs.len(),
                result.traits.len(),
                result.enums.len(),
            );
            if before == after {
                break;
            }
        }
    }

    result
}

/// An [`IrPass`] that removes dead code from the module.
///
/// Wraps [`eliminate_dead_code`] for use in a [`Pipeline`].
///
/// [`IrPass`]: crate::pipeline::IrPass
/// [`Pipeline`]: crate::pipeline::Pipeline
#[derive(Debug)]
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
pub struct DeadCodeEliminationPass {
    /// When `true`, the definitions that nothing references are removed:
    /// structs, traits, enums, and private module `let` bindings whose
    /// value has no effect.
    pub remove_unused_structs: bool,
}

impl DeadCodeEliminationPass {
    /// Create a new dead-code elimination pass.
    ///
    /// Physically removes unused struct, trait, and enum definitions (and any
    /// impl blocks whose target is removed), then rewrites every surviving ID
    /// reference across the module and rebuilds the name-to-ID indices.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            remove_unused_structs: true,
        }
    }
}

impl Default for DeadCodeEliminationPass {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::pipeline::IrPass for DeadCodeEliminationPass {
    fn name(&self) -> &'static str {
        "dead-code-elimination"
    }

    fn run(&mut self, module: IrModule) -> Result<IrModule, Vec<crate::error::CompilerError>> {
        Ok(eliminate_dead_code(&module, self.remove_unused_structs))
    }
}
