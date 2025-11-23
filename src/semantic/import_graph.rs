use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Import graph for detecting circular dependencies
///
/// Tracks which modules import which other modules.
/// Uses depth-first search to detect cycles.
pub struct ImportGraph {
    /// Map from module path to its direct imports
    edges: HashMap<PathBuf, HashSet<PathBuf>>,
}

impl ImportGraph {
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    /// Add an import edge from `from` module to `to` module
    pub fn add_import(&mut self, from: PathBuf, to: PathBuf) {
        self.edges.entry(from).or_default().insert(to);
    }

    /// Check if adding an import would create a cycle
    ///
    /// Returns the cycle path if one would be created, None otherwise
    pub fn would_create_cycle(&self, from: &PathBuf, to: &PathBuf) -> Option<Vec<PathBuf>> {
        // Check if there's already a path from `to` back to `from`
        // This would create a cycle: from -> to -> ... -> from
        let mut visited = HashSet::new();
        let mut path = vec![to.clone()];

        self.dfs_find_path(to, from, &mut visited, &mut path)
    }

    /// Depth-first search to find a path from `current` to `target`
    fn dfs_find_path(
        &self,
        current: &PathBuf,
        target: &PathBuf,
        visited: &mut HashSet<PathBuf>,
        path: &mut Vec<PathBuf>,
    ) -> Option<Vec<PathBuf>> {
        if current == target {
            // Found a path!
            return Some(path.clone());
        }

        if visited.contains(current) {
            return None;
        }

        visited.insert(current.clone());

        if let Some(neighbors) = self.edges.get(current) {
            for neighbor in neighbors {
                path.push(neighbor.clone());
                if let Some(cycle) = self.dfs_find_path(neighbor, target, visited, path) {
                    return Some(cycle);
                }
                path.pop();
            }
        }

        None
    }
}

impl Default for ImportGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_graph_is_empty() {
        let graph = ImportGraph::new();
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn test_default_creates_empty_graph() {
        let graph = ImportGraph::default();
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn test_add_import_creates_edge() {
        let mut graph = ImportGraph::new();
        let from = PathBuf::from("a.forma");
        let to = PathBuf::from("b.forma");

        graph.add_import(from.clone(), to.clone());

        assert!(graph.edges.contains_key(&from));
        assert!(graph.edges.get(&from).unwrap().contains(&to));
    }

    #[test]
    fn test_add_multiple_imports_from_same_module() {
        let mut graph = ImportGraph::new();
        let from = PathBuf::from("main.forma");
        let to1 = PathBuf::from("utils.forma");
        let to2 = PathBuf::from("helpers.forma");

        graph.add_import(from.clone(), to1.clone());
        graph.add_import(from.clone(), to2.clone());

        let imports = graph.edges.get(&from).unwrap();
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&to1));
        assert!(imports.contains(&to2));
    }

    #[test]
    fn test_no_cycle_simple() {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");

        graph.add_import(a.clone(), b.clone());

        // Adding c -> a should not create a cycle
        let c = PathBuf::from("c.forma");
        assert!(graph.would_create_cycle(&c, &a).is_none());
    }

    #[test]
    fn test_detects_direct_cycle() {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");

        // a imports b
        graph.add_import(a.clone(), b.clone());

        // b importing a would create cycle: a -> b -> a
        let cycle = graph.would_create_cycle(&b, &a);
        assert!(cycle.is_some());
        let cycle_path = cycle.unwrap();
        assert!(cycle_path.contains(&a));
    }

    #[test]
    fn test_detects_indirect_cycle() {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");
        let c = PathBuf::from("c.forma");

        // a -> b -> c
        graph.add_import(a.clone(), b.clone());
        graph.add_import(b.clone(), c.clone());

        // c importing a would create cycle: a -> b -> c -> a
        let cycle = graph.would_create_cycle(&c, &a);
        assert!(cycle.is_some());
    }

    #[test]
    fn test_no_false_positive_for_diamond() {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");
        let c = PathBuf::from("c.forma");
        let d = PathBuf::from("d.forma");

        // Diamond pattern: a -> b, a -> c, b -> d, c -> d
        graph.add_import(a.clone(), b.clone());
        graph.add_import(a.clone(), c.clone());
        graph.add_import(b.clone(), d.clone());
        graph.add_import(c.clone(), d.clone());

        // This is a DAG, not a cycle
        // Adding e -> a should not detect a cycle
        let e = PathBuf::from("e.forma");
        assert!(graph.would_create_cycle(&e, &a).is_none());
    }

    #[test]
    fn test_no_cycle_in_chain() {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");
        let c = PathBuf::from("c.forma");

        // Linear chain: a -> b -> c
        graph.add_import(a.clone(), b.clone());
        graph.add_import(b.clone(), c.clone());

        // Adding d -> c should not create a cycle
        let d = PathBuf::from("d.forma");
        assert!(graph.would_create_cycle(&d, &c).is_none());
    }

    #[test]
    fn test_already_visited_node() {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");
        let c = PathBuf::from("c.forma");
        let d = PathBuf::from("d.forma");

        // Graph: a -> b, a -> c, b -> d, c -> d (d has multiple incoming)
        graph.add_import(a.clone(), b.clone());
        graph.add_import(a.clone(), c.clone());
        graph.add_import(b.clone(), d.clone());
        graph.add_import(c.clone(), d.clone());

        // d importing a would create cycles through both paths
        let cycle = graph.would_create_cycle(&d, &a);
        assert!(cycle.is_some());
    }
}
