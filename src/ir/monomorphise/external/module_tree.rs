//! Phase 2a: merge each imported module's `IrModuleNode` tree into the
//! entry module's `modules` tree under the import's `module_path`.
//! Also exposes a defence-in-depth import-cycle check that fires before
//! any phase touches the imported-module set.

use std::collections::{HashMap, HashSet};

use crate::ir::{IrModule, IrModuleNode};

use super::naming::qualified_name;

/// Phase 2a: merge each imported module's `IrModuleNode` tree into
/// the entry module's `modules` tree under the import's `module_path`.
///
/// For each imported module at path `["a", "b"]`, walks/creates
/// `entry.modules` to find or create node `"a"`, then under it node
/// `"b"`. Populates the deepest node with id references to the
/// cloned items: walks `imported.{structs, traits, enums, functions}`
/// and looks up each by qualified name in the entry module to get
/// the local id.
///
/// `IrLet` ids aren't included in `IrModuleNode` (no field exists for
/// lets in the tree node), so they're tracked only via the flat
/// `IrModule.lets` after inlining.
///
/// Existing tree nodes along the path get merged with the imported
/// items rather than replaced — entries from local `mod foo {...}`
/// declarations and from imported modules sharing a name coexist.
///
/// Codegen ignores this tree (the flat per-type vectors remain
/// authoritative); source-introspection tools consume it for the
/// complete cross-module picture.
pub(in crate::ir::monomorphise) fn merge_imported_module_trees(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    for (path, imported) in super::sorted_imports(imported_modules) {
        if path.is_empty() {
            continue;
        }
        let prefix = qualified_name(path, "");

        // Build a leaf node carrying ids for the imported module's
        // top-level items, looked up by qualified name in entry.
        let mut leaf = IrModuleNode::default();
        for s in &imported.structs {
            let q = format!("{prefix}{}", s.name);
            if let Some(id) = module.struct_id(&q) {
                leaf.structs.push(id);
            }
        }
        for t in &imported.traits {
            let q = format!("{prefix}{}", t.name);
            if let Some(id) = module.trait_id(&q) {
                leaf.traits.push(id);
            }
        }
        for e in &imported.enums {
            let q = format!("{prefix}{}", e.name);
            if let Some(id) = module.enum_id(&q) {
                leaf.enums.push(id);
            }
        }
        for f in &imported.functions {
            let q = format!("{prefix}{}", f.name);
            if let Some(id) = module.function_id(&q) {
                leaf.functions.push(id);
            }
        }

        // Splice the leaf under `path` in entry's modules tree,
        // creating intermediate nodes and merging on name overlap.
        splice_module_node(&mut module.modules, path, leaf);
    }
}

fn splice_module_node(parent_nodes: &mut Vec<IrModuleNode>, path: &[String], leaf: IrModuleNode) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    if rest.is_empty() {
        // Last segment: find or create node with this name; merge.
        if let Some(existing) = parent_nodes.iter_mut().find(|n| &n.name == head) {
            existing.structs.extend(leaf.structs);
            existing.traits.extend(leaf.traits);
            existing.enums.extend(leaf.enums);
            existing.functions.extend(leaf.functions);
            existing.modules.extend(leaf.modules);
        } else {
            let mut node = leaf;
            node.name.clone_from(head);
            parent_nodes.push(node);
        }
        return;
    }
    // Intermediate segment: find or create node, recurse.
    let idx = parent_nodes
        .iter()
        .position(|n| &n.name == head)
        .unwrap_or_else(|| {
            parent_nodes.push(IrModuleNode {
                name: head.clone(),
                ..Default::default()
            });
            parent_nodes.len().saturating_sub(1)
        });
    let Some(node) = parent_nodes.get_mut(idx) else {
        return;
    };
    splice_module_node(&mut node.modules, rest, leaf);
}

/// Defence-in-depth cycle detection over the imported-module import
/// graph. Semantic analysis already rejects cyclic `use` chains, so
/// this should never fire in practice — but if it does, the IR layer
/// surfaces a clear `InternalError` rather than silently producing
/// miscompiled output.
///
/// Returns the first node on a detected cycle, or `None` if the graph
/// is acyclic.
pub(in crate::ir::monomorphise) fn detect_import_cycle(
    imported_modules: &HashMap<Vec<String>, IrModule>,
) -> Option<Vec<String>> {
    let mut visited: HashSet<Vec<String>> = HashSet::new();
    let mut on_stack: Vec<Vec<String>> = Vec::new();
    for start in imported_modules.keys() {
        if visited.contains(start) {
            continue;
        }
        if let Some(cycle) = dfs_detect_cycle(start, imported_modules, &mut visited, &mut on_stack)
        {
            return Some(cycle);
        }
    }
    None
}

fn dfs_detect_cycle(
    node: &Vec<String>,
    imported_modules: &HashMap<Vec<String>, IrModule>,
    visited: &mut HashSet<Vec<String>>,
    on_stack: &mut Vec<Vec<String>>,
) -> Option<Vec<String>> {
    if on_stack.contains(node) {
        return Some(node.clone());
    }
    if visited.contains(node) {
        return None;
    }
    on_stack.push(node.clone());
    if let Some(module) = imported_modules.get(node) {
        for import in &module.imports {
            if let Some(cycle) =
                dfs_detect_cycle(&import.module_path, imported_modules, visited, on_stack)
            {
                return Some(cycle);
            }
        }
    }
    on_stack.pop();
    visited.insert(node.clone());
    None
}
