//! `IrModule` — the root IR node holding every definition for a
//! compilation unit, plus the name→id index maps that make lookups
//! cheap during lowering.

use std::collections::HashMap;

use crate::error::CompilerError;
use crate::location::Span;

use super::types::{IrEnum, IrFunction, IrImpl, IrLet, IrStruct, IrTrait};
use super::{EnumId, FunctionId, ImplId, IrImport, ResolvedType, StructId, TraitId};

/// The root IR node containing all definitions.
///
/// Definitions are stored in vectors, indexed by their respective ID types.
/// For example, `StructId(0)` refers to `structs[0]`.
///
/// # Example
///
/// ```
/// use formalang::compile_to_ir;
///
/// let source = "pub struct User { name: String }";
/// let module = compile_to_ir(source).unwrap();
///
/// // Look up by name (skips prelude built-ins like Array, Dictionary, Range).
/// let struct_id = module.struct_id("User").expect("User exists");
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

    /// Source-file table indexed by [`crate::ir::FileId`]. Index 0 is
    /// reserved for synthetic / unknown nodes (closure-converted lift
    /// wrappers, monomorphised specialisations, hand-constructed test
    /// IR). Real source files start at index 1; the entry-point file
    /// is conventionally the first registered.
    ///
    /// Backends emit DWARF `DW_AT_decl_file` / source-map `sources`
    /// entries by walking this table; per-IR-node spans carry the
    /// `FileId` that indexes into it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_table: Vec<std::path::PathBuf>,

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

    /// Prelude-defined `Array<T>` struct id. The four built-in compound
    /// types live in `src/prelude.fv` as ordinary generic definitions;
    /// these accessors hand back the ids the lowering and IR walkers
    /// need to identify built-ins without hardcoding name strings.
    #[must_use]
    pub fn prelude_array_id(&self) -> Option<StructId> {
        self.struct_id("Array")
    }

    /// Prelude-defined `Dictionary<K, V>` struct id.
    #[must_use]
    pub fn prelude_dictionary_id(&self) -> Option<StructId> {
        self.struct_id("Dictionary")
    }

    /// Prelude-defined `Range<T>` struct id.
    #[must_use]
    pub fn prelude_range_id(&self) -> Option<StructId> {
        self.struct_id("Range")
    }

    /// Prelude-defined `Optional<T>` enum id.
    #[must_use]
    pub fn prelude_optional_id(&self) -> Option<EnumId> {
        self.enum_id("Optional")
    }

    /// True iff `id` points at a prelude-defined built-in struct
    /// (`Array`, `Dictionary`, `Range`).
    #[must_use]
    pub fn is_prelude_struct(&self, id: StructId) -> bool {
        Some(id) == self.prelude_array_id()
            || Some(id) == self.prelude_dictionary_id()
            || Some(id) == self.prelude_range_id()
    }

    /// True iff `id` points at the prelude-defined `Optional<T>` enum.
    #[must_use]
    pub fn is_prelude_enum(&self, id: EnumId) -> bool {
        Some(id) == self.prelude_optional_id()
    }

    /// Iterate over user-defined structs only, skipping the prelude
    /// built-ins (`Array`, `Dictionary`, `Range`). Use this when a test
    /// wants the user-authored structs without indexing past the
    /// prelude's leading slots.
    pub fn user_structs(&self) -> impl Iterator<Item = &IrStruct> {
        let array = self.prelude_array_id();
        let dict = self.prelude_dictionary_id();
        let range = self.prelude_range_id();
        self.structs.iter().enumerate().filter_map(move |(i, s)| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "struct count fits in u32 by construction (add_struct guards the cast)"
            )]
            let id = StructId(i as u32);
            if Some(id) == array || Some(id) == dict || Some(id) == range {
                None
            } else {
                Some(s)
            }
        })
    }

    /// Iterate over user-defined enums only, skipping the prelude-built-in
    /// `Optional<T>` enum.
    pub fn user_enums(&self) -> impl Iterator<Item = &IrEnum> {
        let optional = self.prelude_optional_id();
        self.enums.iter().enumerate().filter_map(move |(i, e)| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "enum count fits in u32 by construction (add_enum guards the cast)"
            )]
            let id = EnumId(i as u32);
            if Some(id) == optional {
                None
            } else {
                Some(e)
            }
        })
    }

    /// If `ty` is `Array<T>` (the prelude-defined struct), return `T`.
    /// Built-in compound types share the `Generic` variant with user-
    /// defined generics; these helpers let callers introspect by shape
    /// without needing prelude IDs themselves.
    #[must_use]
    pub fn array_element_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType> {
        let arr = self.prelude_array_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == arr && args.len() == 1 {
                return args.first();
            }
        }
        None
    }

    /// If `ty` is `Dictionary<K, V>`, return `(K, V)`.
    #[must_use]
    pub fn dictionary_kv_ty<'a>(
        &self,
        ty: &'a ResolvedType,
    ) -> Option<(&'a ResolvedType, &'a ResolvedType)> {
        let did = self.prelude_dictionary_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == did {
                if let [k, v] = args.as_slice() {
                    return Some((k, v));
                }
            }
        }
        None
    }

    /// If `ty` is `Range<T>`, return `T`.
    #[must_use]
    pub fn range_element_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType> {
        let rid = self.prelude_range_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Struct(id),
            args,
        } = ty
        {
            if *id == rid && args.len() == 1 {
                return args.first();
            }
        }
        None
    }

    /// If `ty` is `Optional<T>`, return `T`.
    #[must_use]
    pub fn optional_inner_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType> {
        let opt = self.prelude_optional_id()?;
        if let ResolvedType::Generic {
            base: crate::ir::GenericBase::Enum(id),
            args,
        } = ty
        {
            if *id == opt && args.len() == 1 {
                return args.first();
            }
        }
        None
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

    /// Look up a function ID by name. For overloaded names returns the id
    /// of the first registered overload; callers that need to enumerate
    /// every overload should walk [`Self::functions`] and filter by name.
    #[must_use]
    pub fn function_id(&self, name: &str) -> Option<FunctionId> {
        self.function_names.get(name).copied()
    }

    /// Look up the source path for a [`crate::ir::FileId`]. Returns
    /// `None` for `FileId::SYNTHETIC` (id 0) and for ids past the
    /// table's length.
    #[must_use]
    pub fn file_path(&self, file: crate::ir::FileId) -> Option<&std::path::PathBuf> {
        if file.is_synthetic() {
            return None;
        }
        // FileId(1) is the first real file; index into the table is
        // file.0 - 1 so synthetic id 0 doesn't consume a slot.
        let idx = (file.0.checked_sub(1))? as usize;
        self.file_table.get(idx)
    }

    /// Register a source file in the file table and return its
    /// [`crate::ir::FileId`]. If the path is already registered,
    /// returns the existing id. The first registered file gets
    /// `FileId(1)` (id 0 is reserved for synthetic nodes).
    pub fn register_file(&mut self, path: std::path::PathBuf) -> crate::ir::FileId {
        if let Some(idx) = self.file_table.iter().position(|p| p == &path) {
            // +1 because the table is offset to leave id 0 reserved.
            return crate::ir::FileId(u32::try_from(idx).unwrap_or(0).saturating_add(1));
        }
        self.file_table.push(path);
        crate::ir::FileId(u32::try_from(self.file_table.len()).unwrap_or(1))
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
        // Preserve the first registration for overloaded names; later
        // entries are still discoverable via the `functions` array.
        self.function_names.entry(name).or_insert(id);
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

        // Functions may legitimately share a name (overload resolution
        // dispatches by parameter signature, not by name alone). The
        // name-to-id index keeps only the first occurrence; callers that
        // need every overload walk `functions` directly. Identical-
        // signature duplicates are rejected upstream in semantic
        // analysis, so anything that reaches IR is a valid overload set.
        self.function_names.clear();
        for (idx, f) in self.functions.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked by add_function which errors before len reaches u32::MAX"
            )]
            self.function_names
                .entry(f.name.clone())
                .or_insert(FunctionId(idx as u32));
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
