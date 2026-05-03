//! Shared name helpers for cross-module clones: building the
//! `module::path::name` qualified form and recovering the source module
//! from a cloned item's qualified name.

use std::collections::HashMap;

use crate::ir::IrModule;

/// String-capacity hint for `module_path::...::name`: total length of every
/// path segment plus two bytes per `"::"` separator plus the trailing name.
/// Saturating throughout so an absurdly long path never wraps the usize
/// hint into something dangerous.
pub(super) fn qualified_capacity(module_path: &[String], name_len: usize) -> usize {
    module_path
        .iter()
        .map(String::len)
        .sum::<usize>()
        .saturating_add(module_path.len().saturating_mul(2))
        .saturating_add(name_len)
}

/// Build the qualified `module::path::name` form for cross-module clones.
pub(super) fn qualified_name(module_path: &[String], name: &str) -> String {
    let mut out = String::with_capacity(qualified_capacity(module_path, name.len()));
    for seg in module_path {
        out.push_str(seg);
        out.push_str("::");
    }
    out.push_str(name);
    out
}

/// Identify which imported module a cloned item came from by stripping
/// the longest matching imported path prefix from its qualified name.
/// Returns `None` for entry-module-native items (no `::` or no matching
/// imported path).
pub(super) fn imported_path_of(
    name: &str,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) -> Option<Vec<String>> {
    if !name.contains("::") {
        return None;
    }
    // Match the longest imported path that is a prefix of `name`.
    let mut best: Option<&Vec<String>> = None;
    for path in imported_modules.keys() {
        let prefix = qualified_name(path, "");
        // qualified_name with empty `name` produces "p1::p2::". Items
        // cloned from this module are named "p1::p2::Foo".
        if name.starts_with(&prefix)
            && best.is_none_or(|b| qualified_name(b, "").len() < prefix.len())
        {
            best = Some(path);
        }
    }
    best.cloned()
}
