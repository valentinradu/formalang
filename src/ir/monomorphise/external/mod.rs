//! Cross-module monomorphisation: clone every imported item into the entry
//! module under a qualified name and rewrite every cross-module reference
//! to point at the local clone.
//!
//! Phase ordering (driven by [`super::mod::Monomorphiser::run`]):
//! - **1a** [`specialise::specialise_external_instantiations`] — clone each
//!   imported generic `External` type with substituted type arguments.
//! - **1b** [`inline::inline_imported_functions`] — copy every imported
//!   function under a qualified name.
//! - **1c** [`inline::inline_imported_impls`] — copy every imported impl
//!   block whose target type is now local.
//! - **1d** [`inline::inline_imported_lets`] — copy every imported `pub`
//!   `let` under a qualified name.
//! - **1e** [`id_remap::remap_imported_body_ids`] — rewrite cloned body
//!   id-references from imported id-space to the entry module.
//! - **1f** [`qualify::qualify_imported_paths`] — qualify bare-name paths
//!   in cloned (and entry-native) bodies using their source module's
//!   import set.
//! - **2** [`rewrite::rewrite_external_references`] — rewrite every
//!   leftover `External` ref to its mapped local clone.
//! - **2a** [`module_tree::merge_imported_module_trees`] — splice the
//!   imported items into the entry module's `IrModuleNode` tree.
//! - **2b** [`file_ids::remap_imported_file_ids`] — register every
//!   imported file in the entry's `file_table` and remap every cloned
//!   span to the entry's id-space.

mod externalise;
mod file_ids;
mod id_remap;
mod inline;
mod module_tree;
mod naming;
mod qualify;
mod rewrite;
mod specialise;

pub(super) use file_ids::remap_imported_file_ids;
pub(super) use id_remap::remap_imported_body_ids;
pub(super) use inline::{inline_imported_functions, inline_imported_impls, inline_imported_lets};
pub(super) use module_tree::{detect_import_cycle, merge_imported_module_trees};
pub(super) use qualify::qualify_imported_paths;
pub(super) use rewrite::rewrite_external_references;
pub(super) use specialise::specialise_external_instantiations;
