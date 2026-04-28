//! `IrModule` — the root IR node holding every definition for a
//! compilation unit, plus the name→id index maps that make lookups
//! cheap during lowering.

use std::collections::HashMap;

use crate::error::CompilerError;
use crate::location::Span;

use super::types::{IrEnum, IrFunction, IrImpl, IrLet, IrStruct, IrTrait};
use super::{EnumId, FunctionId, ImplId, IrImport, StructId, TraitId};

/// The root IR node containing all definitions.
///
/// Definitions are stored in vectors, indexed by their respective ID types.
/// For example, `StructId(0)` refers to `structs[0]`.
///
/// # Example
///
/// ```
/// use formalang::{compile_to_ir, StructId};
///
/// let source = "pub struct User { name: String }";
/// let module = compile_to_ir(source).unwrap();
/// let struct_id = StructId(0);
///
/// // Look up a struct by ID (direct indexing)
/// let struct_def = &module.structs[struct_id.0 as usize];
/// assert_eq!(struct_def.name, "User");
///
/// // Or use the helper method
/// let struct_def = module.get_struct(struct_id).expect("struct exists");
/// assert_eq!(struct_def.name, "User");
/// ```
/// **Serde note:** the private name→id index maps (`struct_names`,
/// `trait_names`, `enum_names`, `function_names`, `let_names`) are marked
/// `#[serde(skip)]` so round-tripped modules don't carry stale entries.
/// After deserialising, callers must call [`IrModule::rebuild_indices`]
/// before any `struct_id` / `trait_id` / `get_function` lookups, or those
/// helpers will return `None`.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct IrModule {
    /// All struct definitions, indexed by `StructId`
    pub structs: Vec<IrStruct>,

    /// All trait definitions, indexed by `TraitId`
    pub traits: Vec<IrTrait>,

    /// All enum definitions, indexed by `EnumId`
    pub enums: Vec<IrEnum>,

    /// All impl blocks
    pub impls: Vec<IrImpl>,

    /// Module-level let bindings (theme colours, fonts, shared config).
    pub lets: Vec<IrLet>,

    /// Standalone function definitions (outside impl blocks).
    pub functions: Vec<IrFunction>,

    /// Imports from other modules — drives codegen's import-statement
    /// emission.
    pub imports: Vec<IrImport>,

    /// Top-level nested modules declared in source (`mod foo { ... }`).
    /// Each [`IrModuleNode`] lists the IDs of its directly-contained
    /// structs, traits, enums, and functions, plus its own nested
    /// modules. The flat per-type vectors (`structs`, `traits`, etc.)
    /// remain authoritative — this tree is an *index* on top of them
    /// for backends that need to preserve module hierarchy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<IrModuleNode>,

    /// Mapping from struct names to IDs for lookup during lowering.
    /// Skipped during serde round-trips; rebuilt on load via
    /// `rebuild_indices`.
    #[serde(skip)]
    struct_names: HashMap<String, StructId>,

    #[serde(skip)]
    trait_names: HashMap<String, TraitId>,

    #[serde(skip)]
    enum_names: HashMap<String, EnumId>,

    #[serde(skip)]
    function_names: HashMap<String, FunctionId>,

    #[serde(skip)]
    let_names: HashMap<String, usize>,
}

/// One node of the module-hierarchy tree on [`IrModule::modules`].
///
/// `FormaLang` flattens nested-module type names during lowering
/// (`outer::inner::Type`) so the per-type IR vectors are flat. This
/// node lets backends that emit code into nested namespaces (e.g.
/// JS `export * from`, Swift nested types) reconstruct the source
/// module hierarchy without re-parsing qualified names.
///
/// IDs reference the corresponding flat vectors on [`IrModule`]; the
/// strings on those records are the qualified names.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct IrModuleNode {
    /// Module name as written in source (the unqualified segment, e.g.
    /// `"shapes"` for `mod shapes { ... }`).
    pub name: String,

    /// IDs of structs declared directly in this module (not in nested
    /// sub-modules).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structs: Vec<StructId>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<TraitId>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enums: Vec<EnumId>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<FunctionId>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<Self>,
}

impl IrModule {
    /// Create a new empty IR module.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a struct by ID. Returns `None` if the ID is out of bounds.
    #[must_use]
    pub fn get_struct(&self, id: StructId) -> Option<&IrStruct> {
        self.structs.get(id.0 as usize)
    }

    /// Look up a trait by ID. Returns `None` if the ID is out of bounds.
    #[must_use]
    pub fn get_trait(&self, id: TraitId) -> Option<&IrTrait> {
        self.traits.get(id.0 as usize)
    }

    /// Look up an enum by ID. Returns `None` if the ID is out of bounds.
    #[must_use]
    pub fn get_enum(&self, id: EnumId) -> Option<&IrEnum> {
        self.enums.get(id.0 as usize)
    }

    /// Look up a struct ID by name.
    #[must_use]
    pub fn struct_id(&self, name: &str) -> Option<StructId> {
        self.struct_names.get(name).copied()
    }

    /// Look up a trait ID by name.
    #[must_use]
    pub fn trait_id(&self, name: &str) -> Option<TraitId> {
        self.trait_names.get(name).copied()
    }

    /// Look up an enum ID by name.
    #[must_use]
    pub fn enum_id(&self, name: &str) -> Option<EnumId> {
        self.enum_names.get(name).copied()
    }

    /// Add a struct and return its ID.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; callers push errors into a Vec so allocation is bounded"
    )]
    pub(crate) fn add_struct(
        &mut self,
        name: String,
        s: IrStruct,
    ) -> Result<StructId, CompilerError> {
        let id = u32::try_from(self.structs.len())
            .map(StructId)
            .map_err(|_| CompilerError::TooManyDefinitions {
                kind: "struct",
                span: Span::default(),
            })?;
        self.struct_names.insert(name, id);
        self.structs.push(s);
        Ok(id)
    }

    /// Add a trait and return its ID.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; callers push errors into a Vec so allocation is bounded"
    )]
    pub(crate) fn add_trait(&mut self, name: String, t: IrTrait) -> Result<TraitId, CompilerError> {
        let id = u32::try_from(self.traits.len()).map(TraitId).map_err(|_| {
            CompilerError::TooManyDefinitions {
                kind: "trait",
                span: Span::default(),
            }
        })?;
        self.trait_names.insert(name, id);
        self.traits.push(t);
        Ok(id)
    }

    /// Add an enum and return its ID.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; callers push errors into a Vec so allocation is bounded"
    )]
    pub(crate) fn add_enum(&mut self, name: String, e: IrEnum) -> Result<EnumId, CompilerError> {
        let id = u32::try_from(self.enums.len()).map(EnumId).map_err(|_| {
            CompilerError::TooManyDefinitions {
                kind: "enum",
                span: Span::default(),
            }
        })?;
        self.enum_names.insert(name, id);
        self.enums.push(e);
        Ok(id)
    }

    /// Look up a mutable reference to a struct by its ID.
    /// `None` on out-of-bounds — callers should treat as a compiler
    /// invariant violation (IDs from [`Self::struct_id`] are always
    /// valid unless the underlying `Vec` was mutated externally).
    pub(crate) fn struct_mut(&mut self, id: StructId) -> Option<&mut IrStruct> {
        self.structs.get_mut(id.0 as usize)
    }

    pub(crate) fn trait_mut(&mut self, id: TraitId) -> Option<&mut IrTrait> {
        self.traits.get_mut(id.0 as usize)
    }

    pub(crate) fn enum_mut(&mut self, id: EnumId) -> Option<&mut IrEnum> {
        self.enums.get_mut(id.0 as usize)
    }

    /// Add an impl block and return its ID.
    ///
    /// # Errors
    ///
    /// Returns [`CompilerError::TooManyDefinitions`] if the impl count exceeds `u32::MAX`.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; callers push errors into a Vec so allocation is bounded"
    )]
    pub(crate) fn add_impl(&mut self, i: IrImpl) -> Result<ImplId, CompilerError> {
        let id = u32::try_from(self.impls.len()).map(ImplId).map_err(|_| {
            CompilerError::TooManyDefinitions {
                kind: "impl",
                span: Span::default(),
            }
        })?;
        self.impls.push(i);
        Ok(id)
    }

    /// Return the `ImplId` that the next [`Self::add_impl`] call will
    /// produce, without mutating. `None` if the impl count has already
    /// reached `u32::MAX`.
    #[must_use]
    pub(crate) fn next_impl_id(&self) -> Option<ImplId> {
        u32::try_from(self.impls.len()).ok().map(ImplId)
    }

    /// Look up a let binding by name.
    #[must_use]
    pub fn get_let(&self, name: &str) -> Option<&IrLet> {
        self.let_names.get(name).and_then(|&idx| self.lets.get(idx))
    }

    /// Check if a let binding exists.
    #[must_use]
    pub fn has_let(&self, name: &str) -> bool {
        self.let_names.contains_key(name)
    }

    /// Add a let binding.
    pub(crate) fn add_let(&mut self, l: IrLet) {
        let idx = self.lets.len();
        self.let_names.insert(l.name.clone(), idx);
        self.lets.push(l);
    }

    /// Look up a function by ID. Returns `None` if the ID is out of bounds.
    #[must_use]
    pub fn get_function(&self, id: FunctionId) -> Option<&IrFunction> {
        self.functions.get(id.0 as usize)
    }

    /// Look up a function ID by name.
    #[must_use]
    pub fn function_id(&self, name: &str) -> Option<FunctionId> {
        self.function_names.get(name).copied()
    }

    /// Add a standalone function and return its ID.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; callers push errors into a Vec so allocation is bounded"
    )]
    pub(crate) fn add_function(
        &mut self,
        name: String,
        f: IrFunction,
    ) -> Result<FunctionId, CompilerError> {
        let id = u32::try_from(self.functions.len())
            .map(FunctionId)
            .map_err(|_| CompilerError::TooManyDefinitions {
                kind: "function",
                span: Span::default(),
            })?;
        self.function_names.insert(name, id);
        self.functions.push(f);
        Ok(id)
    }

    /// Rebuild the name-to-ID index maps from the current definition lists.
    ///
    /// Call this after any [`IrPass`] that adds, removes, or reorders
    /// definitions in `structs`, `traits`, `enums`, `functions`, or `lets`.
    /// Passes that only mutate fields within existing definitions do not need
    /// to call this.
    ///
    /// [`IrPass`]: crate::pipeline::IrPass
    pub fn rebuild_indices(&mut self) {
        self.struct_names.clear();
        for (idx, s) in self.structs.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked by add_struct which errors before len reaches u32::MAX"
            )]
            let prev = self
                .struct_names
                .insert(s.name.clone(), StructId(idx as u32));
            debug_assert!(
                prev.is_none(),
                "duplicate struct name `{}` in module; rebuild_indices requires unique names",
                s.name
            );
        }

        self.trait_names.clear();
        for (idx, t) in self.traits.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked by add_trait which errors before len reaches u32::MAX"
            )]
            let prev = self.trait_names.insert(t.name.clone(), TraitId(idx as u32));
            debug_assert!(
                prev.is_none(),
                "duplicate trait name `{}` in module; rebuild_indices requires unique names",
                t.name
            );
        }

        self.enum_names.clear();
        for (idx, e) in self.enums.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked by add_enum which errors before len reaches u32::MAX"
            )]
            let prev = self.enum_names.insert(e.name.clone(), EnumId(idx as u32));
            debug_assert!(
                prev.is_none(),
                "duplicate enum name `{}` in module; rebuild_indices requires unique names",
                e.name
            );
        }

        self.function_names.clear();
        for (idx, f) in self.functions.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked by add_function which errors before len reaches u32::MAX"
            )]
            let prev = self
                .function_names
                .insert(f.name.clone(), FunctionId(idx as u32));
            // Functions may share names (overloaded dispatch); a
            // debug-only trace keeps the invariant visible without
            // breaking consumers that exploit overload resolution.
            debug_assert!(
                prev.is_none() || cfg!(test),
                "duplicate function name `{}` in module; rebuild_indices will shadow earlier entries",
                f.name
            );
        }

        self.let_names.clear();
        for (idx, l) in self.lets.iter().enumerate() {
            let prev = self.let_names.insert(l.name.clone(), idx);
            debug_assert!(
                prev.is_none(),
                "duplicate let name `{}` in module; rebuild_indices requires unique names",
                l.name
            );
        }
    }
}
