//! Cross-module import metadata: backends use these to emit import
//! statements in the target language.

/// Kind of external item reference. Distinguishes definition kinds
/// when referencing items from other modules.
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ImportedKind {
    Struct,
    Trait,
    Enum,
    /// A standalone function imported via `use other::compute`. The
    /// linker puts the function in `IrModule.functions` under its
    /// qualified name, such as `other::compute`.
    Function,
    /// A module-level `pub let` imported via `use other::CONST`.
    /// The linker puts it in `IrModule.lets` under its qualified name.
    ModuleLet,
}

/// An import from another module.
///
/// Records the names that a `use` imports, so codegen can emit import
/// statements. The items themselves are in the module: the linker
/// copies each imported module into the module that imports it.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrImport {
    /// Logical module path (e.g., `["utils", "helpers"]`)
    pub module_path: Vec<String>,
    /// Items imported from this module
    pub items: Vec<IrImportItem>,
    /// Filesystem path to the source module file. Populated from the
    /// symbol table's `module_origins` during IR lowering.
    pub source_file: std::path::PathBuf,
}

/// A single imported item from a module.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrImportItem {
    /// Name of the imported item, as the `use` wrote it
    pub name: String,
    /// Kind of the item
    pub kind: ImportedKind,
}
