//! Cross-module import metadata: backends use these to emit import
//! statements in the target language.

/// Kind of external type reference. Distinguishes the three definition
/// kinds when referencing types from other modules.
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ImportedKind {
    Struct,
    Trait,
    Enum,
}

/// An import from another module. Tracks which types were imported from
/// external modules so codegen can emit proper import statements.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IrImport {
    /// Logical module path (e.g., `["utils", "helpers"]`)
    pub module_path: Vec<String>,
    /// Items imported from this module
    pub items: Vec<IrImportItem>,
    /// Filesystem path to the source module file. Used by codegen to look
    /// up the cached `IrModule` for generating impl blocks from imported
    /// types. Populated from the symbol table's `module_origins` during
    /// IR lowering.
    pub source_file: std::path::PathBuf,
}

/// A single imported item from a module.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IrImportItem {
    /// Name of the imported type
    pub name: String,
    /// Kind of type (struct, trait, or enum)
    pub kind: ImportedKind,
}
