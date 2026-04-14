use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Import graph for detecting circular dependencies
///
/// Tracks which modules import which other modules.
/// Uses depth-first search to detect cycles.
pub(crate) struct ImportGraph {
    /// Map from module path to its direct imports
    edges: HashMap<PathBuf, HashSet<PathBuf>>,
}

impl ImportGraph {
    pub(crate) fn new() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    /// Add an import edge from `from` module to `to` module
    pub(crate) fn add_import(&mut self, from: PathBuf, to: PathBuf) {
        self.edges.entry(from).or_default().insert(to);
    }

    /// Check if adding an import would create a cycle
    ///
    /// Returns the cycle path if one would be created, None otherwise
    pub(crate) fn would_create_cycle(&self, from: &PathBuf, to: &PathBuf) -> Option<Vec<PathBuf>> {
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
    fn test_new_graph_is_empty() -> Result<(), Box<dyn std::error::Error>> {
        let graph = ImportGraph::new();
        if !graph.edges.is_empty() { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_default_creates_empty_graph() -> Result<(), Box<dyn std::error::Error>> {
        let graph = ImportGraph::default();
        if !graph.edges.is_empty() { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_add_import_creates_edge() -> Result<(), Box<dyn std::error::Error>> {
        let mut graph = ImportGraph::new();
        let from = PathBuf::from("a.forma");
        let to = PathBuf::from("b.forma");

        graph.add_import(from.clone(), to.clone());

        if !graph.edges.contains_key(&from) {
            return Err("Expected edge from 'from'".into());
        }
        let edges = graph.edges.get(&from).ok_or("edges not found")?;
        if !edges.contains(&to) {
            return Err(format!("Expected edges to contain {to:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_add_multiple_imports_from_same_module() -> Result<(), Box<dyn std::error::Error>> {
        let mut graph = ImportGraph::new();
        let from = PathBuf::from("main.forma");
        let to1 = PathBuf::from("utils.forma");
        let to2 = PathBuf::from("helpers.forma");

        graph.add_import(from.clone(), to1.clone());
        graph.add_import(from.clone(), to2.clone());

        let imports = graph.edges.get(&from).ok_or("imports not found")?;
        if imports.len() != 2 {
            return Err(format!("Expected 2 imports, got {}", imports.len()).into());
        }
        if !imports.contains(&to1) {
            return Err(format!("Expected imports to contain {to1:?}").into());
        }
        if !imports.contains(&to2) {
            return Err(format!("Expected imports to contain {to2:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_no_cycle_simple() -> Result<(), Box<dyn std::error::Error>> {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");

        graph.add_import(a.clone(), b);

        // Adding c -> a should not create a cycle
        let c = PathBuf::from("c.forma");
        if graph.would_create_cycle(&c, &a).is_some() { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_detects_direct_cycle() -> Result<(), Box<dyn std::error::Error>> {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");

        // a imports b
        graph.add_import(a.clone(), b.clone());

        // b importing a would create cycle: a -> b -> a
        let cycle = graph.would_create_cycle(&b, &a);
        if cycle.is_none() {
            return Err("Expected cycle to be detected".into());
        }
        let cycle_path = cycle.ok_or("cycle not found")?;
        if !cycle_path.contains(&a) {
            return Err(format!("Expected cycle to contain {a:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_detects_indirect_cycle() -> Result<(), Box<dyn std::error::Error>> {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");
        let c = PathBuf::from("c.forma");

        // a -> b -> c
        graph.add_import(a.clone(), b.clone());
        graph.add_import(b, c.clone());

        // c importing a would create cycle: a -> b -> c -> a
        let cycle = graph.would_create_cycle(&c, &a);
        if cycle.is_none() {
            return Err("Expected indirect cycle to be detected".into());
        }
        Ok(())
    }

    #[test]
    #[expect(clippy::many_single_char_names, reason = "graph node names a-e are conventional")]
    fn test_no_false_positive_for_diamond() -> Result<(), Box<dyn std::error::Error>> {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");
        let c = PathBuf::from("c.forma");
        let d = PathBuf::from("d.forma");

        // Diamond pattern: a -> b, a -> c, b -> d, c -> d
        graph.add_import(a.clone(), b.clone());
        graph.add_import(a.clone(), c.clone());
        graph.add_import(b, d.clone());
        graph.add_import(c, d);

        // This is a DAG, not a cycle
        // Adding e -> a should not detect a cycle
        let e = PathBuf::from("e.forma");
        if graph.would_create_cycle(&e, &a).is_some() { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_no_cycle_in_chain() -> Result<(), Box<dyn std::error::Error>> {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");
        let c = PathBuf::from("c.forma");

        // Linear chain: a -> b -> c
        graph.add_import(a, b.clone());
        graph.add_import(b, c.clone());

        // Adding d -> c should not create a cycle
        let d = PathBuf::from("d.forma");
        if graph.would_create_cycle(&d, &c).is_some() { return Err("assertion failed".into()); }
        Ok(())
    }

    #[test]
    fn test_already_visited_node() -> Result<(), Box<dyn std::error::Error>> {
        let mut graph = ImportGraph::new();
        let a = PathBuf::from("a.forma");
        let b = PathBuf::from("b.forma");
        let c = PathBuf::from("c.forma");
        let d = PathBuf::from("d.forma");

        // Graph: a -> b, a -> c, b -> d, c -> d (d has multiple incoming)
        graph.add_import(a.clone(), b.clone());
        graph.add_import(a.clone(), c.clone());
        graph.add_import(b, d.clone());
        graph.add_import(c, d.clone());

        // d importing a would create cycles through both paths
        let cycle = graph.would_create_cycle(&d, &a);
        if cycle.is_none() { return Err("assertion failed".into()); }
        Ok(())
    }
}
