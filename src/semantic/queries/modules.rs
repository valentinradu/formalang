//! Walking the nested-module tree.
//!
//! Every editor query — hover, go-to-definition, completion — used to
//! read only the file's own top-level symbol maps, so anything
//! declared inside `pub mod geometry { ... }` was invisible. These
//! helpers descend into the tree, and each caller decides what to do
//! with what it finds.

use crate::semantic::symbol_table::SymbolTable;

/// Visit every nested module reachable from `symbols`, deepest last.
///
/// `prefix` is the qualified path of `symbols` itself — empty for the
/// file's own table — so each call receives the module's qualified
/// name, e.g. `"geometry"` and then `"geometry::inner"`.
///
/// The editor queries all used to read only the top-level maps, so
/// anything declared inside `pub mod geometry { ... }` was invisible:
/// no hover, no go-to-definition, no completion.
pub(super) fn for_each_module<'s>(
    prefix: &str,
    symbols: &'s SymbolTable,
    visit: &mut impl FnMut(&str, &'s SymbolTable),
) {
    // Sorted so the answer does not depend on `HashMap` order.
    let mut names: Vec<&String> = symbols.modules.keys().collect();
    names.sort();
    for name in names {
        let Some(info) = symbols.modules.get(name) else {
            continue;
        };
        let qualified = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}::{name}")
        };
        visit(&qualified, &info.symbols);
        for_each_module(&qualified, &info.symbols, visit);
    }
}

/// Split a possibly-qualified name into its module path and its last
/// segment: `"geometry::Point"` becomes `("geometry", "Point")`.
pub(super) fn split_qualified(name: &str) -> (&str, &str) {
    name.rsplit_once("::")
        .map_or(("", name), |(path, last)| (path, last))
}

/// Visit every nested module with its [`ModuleInfo`], so a caller can
/// read the module's own span.
pub(super) fn visit_modules(
    symbols: &SymbolTable,
    visit: &mut impl FnMut(&str, &crate::semantic::symbol_table::ModuleInfo),
) {
    fn walk(
        prefix: &str,
        symbols: &SymbolTable,
        visit: &mut impl FnMut(&str, &crate::semantic::symbol_table::ModuleInfo),
    ) {
        let mut names: Vec<&String> = symbols.modules.keys().collect();
        names.sort();
        for name in names {
            let Some(info) = symbols.modules.get(name) else {
                continue;
            };
            let qualified = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}::{name}")
            };
            visit(&qualified, info);
            walk(&qualified, &info.symbols, visit);
        }
    }
    walk("", symbols, visit);
}
