//! The final names of the imported items, and the module tree that
//! holds them.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ir::{EnumId, FunctionId, IrModule, IrModuleNode, StructId, TraitId};

use super::rename::rename_prefixes;
use super::{hidden_prefix, LinkedModule};

/// The ids of the modules whose items have hidden names in `module`.
fn hidden_modules(module: &IrModule) -> BTreeSet<u32> {
    let names = module
        .structs
        .iter()
        .map(|s| &s.name)
        .chain(module.enums.iter().map(|e| &e.name))
        .chain(module.traits.iter().map(|t| &t.name))
        .chain(module.functions.iter().map(|f| &f.name))
        .chain(module.lets.iter().map(|l| &l.name));
    names
        .filter_map(|name| {
            name.strip_prefix('@')
                .and_then(|rest| rest.split_once("::"))
                .and_then(|(id, _)| id.parse::<u32>().ok())
        })
        .collect()
}

/// Choose the final path of each module in `module`.
///
/// A module takes the path that the source wrote in its `use`. Two
/// modules cannot share a path, and a path cannot also be the prefix
/// of a name that the entry module defines itself: then the module
/// takes its id as a suffix, `base#2`. The module `own`, when given,
/// takes the empty path, so its items keep their short names.
fn choose_prefixes(
    module: &IrModule,
    logical: &[Vec<String>],
    own: Option<u32>,
) -> HashMap<String, String> {
    let taken_by_source: HashSet<&str> = module
        .structs
        .iter()
        .map(|s| s.name.as_str())
        .chain(module.enums.iter().map(|e| e.name.as_str()))
        .chain(module.traits.iter().map(|t| t.name.as_str()))
        .chain(module.functions.iter().map(|f| f.name.as_str()))
        .chain(module.lets.iter().map(|l| l.name.as_str()))
        .filter(|name| !name.starts_with('@'))
        .filter_map(|name| name.split_once("::").map(|(head, _)| head))
        .collect();
    let mut used: HashSet<String> = HashSet::new();
    let mut prefixes = HashMap::new();
    for id in hidden_modules(module) {
        if Some(id) == own {
            prefixes.insert(hidden_prefix(id), String::new());
            continue;
        }
        let written = usize::try_from(id)
            .ok()
            .and_then(|i| logical.get(i))
            .map(|path| path.join("::"))
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| format!("module{id}"));
        let head = written.split("::").next().unwrap_or_default();
        let path = if used.contains(&written) || taken_by_source.contains(head) {
            format!("{written}#{id}")
        } else {
            written
        };
        used.insert(path.clone());
        prefixes.insert(hidden_prefix(id), path);
    }
    prefixes
}

/// Give each imported item of the entry module its final name, and
/// put each one in the module tree under the path of its module.
///
/// `logical` holds the module path of each module id, as the source
/// wrote it in a `use`.
pub(crate) fn finish_entry(module: &mut IrModule, logical: &[Vec<String>]) {
    let prefixes = choose_prefixes(module, logical, None);
    if prefixes.is_empty() {
        return;
    }
    let owners = item_owners(module, &prefixes);
    rename_prefixes(module, &prefixes);
    for (path, item) in owners {
        let node = node_at(&mut module.modules, &path);
        match item {
            Item::Struct(id) => node.structs.push(id),
            Item::Enum(id) => node.enums.push(id),
            Item::Trait(id) => node.traits.push(id),
            Item::Function(id) => node.functions.push(id),
        }
    }
}

/// The IR of an imported module, as a module on its own: its own items
/// have their short names, and each item that it imports has the path
/// of its module.
pub(crate) fn module_view(linked: &LinkedModule, own: u32, logical: &[Vec<String>]) -> IrModule {
    let mut module = linked.ir.clone();
    let prefixes = choose_prefixes(&module, logical, Some(own));
    rename_prefixes(&mut module, &prefixes);
    module
}

/// An item that a node of the module tree lists.
enum Item {
    Struct(StructId),
    Enum(EnumId),
    Trait(TraitId),
    Function(FunctionId),
}

/// The module path of each imported item, with its final prefix: the
/// path of its module, then the inline modules around it.
fn item_owners(module: &IrModule, prefixes: &HashMap<String, String>) -> Vec<(Vec<String>, Item)> {
    let path_of = |name: &str| -> Option<Vec<String>> {
        let (head, rest) = name.split_once("::")?;
        let prefix = prefixes.get(head)?;
        let mut path: Vec<String> = prefix.split("::").map(String::from).collect();
        if let Some((inner, _)) = rest.rsplit_once("::") {
            path.extend(inner.split("::").map(String::from));
        }
        Some(path)
    };
    let mut out = Vec::new();
    for (i, s) in module.structs.iter().enumerate() {
        if let (Some(path), Ok(id)) = (path_of(&s.name), u32::try_from(i)) {
            out.push((path, Item::Struct(StructId(id))));
        }
    }
    for (i, e) in module.enums.iter().enumerate() {
        if let (Some(path), Ok(id)) = (path_of(&e.name), u32::try_from(i)) {
            out.push((path, Item::Enum(EnumId(id))));
        }
    }
    for (i, t) in module.traits.iter().enumerate() {
        if let (Some(path), Ok(id)) = (path_of(&t.name), u32::try_from(i)) {
            out.push((path, Item::Trait(TraitId(id))));
        }
    }
    for (i, f) in module.functions.iter().enumerate() {
        if let (Some(path), Ok(id)) = (path_of(&f.name), u32::try_from(i)) {
            out.push((path, Item::Function(FunctionId(id))));
        }
    }
    out
}

/// The node at `path` in the tree `nodes`. The nodes on the way are
/// made when they are not there yet.
fn node_at<'a>(nodes: &'a mut Vec<IrModuleNode>, path: &[String]) -> &'a mut IrModuleNode {
    let (first, rest) = match path.split_first() {
        Some((first, rest)) => (first.as_str(), rest),
        None => ("", path),
    };
    let index = nodes
        .iter()
        .position(|n| n.name == first)
        .unwrap_or_else(|| {
            nodes.push(IrModuleNode {
                name: first.to_string(),
                ..IrModuleNode::default()
            });
            nodes.len().saturating_sub(1)
        });
    // The index is valid: it was found in `nodes`, or it is the node
    // that was pushed just now.
    #[expect(
        clippy::indexing_slicing,
        reason = "the index comes from `position` or from the push above"
    )]
    let node = &mut nodes[index];
    if rest.is_empty() {
        node
    } else {
        node_at(&mut node.modules, rest)
    }
}
