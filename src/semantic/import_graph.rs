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

