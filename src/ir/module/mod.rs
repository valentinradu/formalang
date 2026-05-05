//! `IrModule` — the root IR node holding every definition for a
//! compilation unit, plus the name→id index maps that make lookups
//! cheap during lowering.

use std::collections::HashMap;

use super::types::{IrEnum, IrFunction, IrImpl, IrLet, IrStruct, IrTrait};
use super::IrImport;
use super::{EnumId, FunctionId, StructId, TraitId};

mod core;
mod prelude;

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

    /// Imports from other modules; drives codegen's import-statement
    /// emission.
    pub imports: Vec<IrImport>,

    /// Top-level nested modules declared in source (`mod foo { ... }`).
    /// Each [`IrModuleNode`] lists the IDs of its directly-contained
    /// structs, traits, enums, and functions, plus its own nested
    /// modules. The flat per-type vectors (`structs`, `traits`, etc.)
    /// remain authoritative; this tree is an *index* on top of them
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
