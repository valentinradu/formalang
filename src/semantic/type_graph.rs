use std::collections::{HashMap, HashSet};

/// Type dependency graph for detecting circular type dependencies
///
/// Tracks which types reference which other types (e.g., model fields, trait fields).
/// Uses depth-first search to detect cycles.
pub struct TypeGraph {
    /// Map from type name to the types it depends on
    edges: HashMap<String, HashSet<String>>,
}

impl TypeGraph {
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    /// Add a type dependency edge from `from` type to `to` type
    pub fn add_dependency(&mut self, from: impl Into<String>, to: impl Into<String>) {
        self.edges
            .entry(from.into())
            .or_default()
            .insert(to.into());
    }

    /// Detect all cycles in the type dependency graph
    ///
    /// Returns a list of cycles found, where each cycle is a sequence of type names
    pub fn find_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        // Check each type as a potential starting point
        for type_name in self.edges.keys() {
            if !visited.contains(type_name) {
                self.dfs_detect_cycle(
                    type_name,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                    &mut cycles,
                );
            }
        }

        cycles
    }

    /// Depth-first search to detect cycles
    fn dfs_detect_cycle(
        &self,
        current: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(current.to_string());
        rec_stack.insert(current.to_string());
        path.push(current.to_string());

        if let Some(neighbors) = self.edges.get(current) {
            for neighbor in neighbors {
                // If neighbor is in recursion stack, we found a cycle
                if rec_stack.contains(neighbor) {
                    // Extract the cycle from the path
                    if let Some(cycle_start) = path.iter().position(|t| t == neighbor) {
                        let mut cycle = path[cycle_start..].to_vec();
                        cycle.push(neighbor.clone()); // Close the cycle
                        cycles.push(cycle);
                    }
                } else if !visited.contains(neighbor) {
                    // Continue DFS
                    self.dfs_detect_cycle(neighbor, visited, rec_stack, path, cycles);
                }
            }
        }

        path.pop();
        rec_stack.remove(current);
    }
}

impl Default for TypeGraph {
    fn default() -> Self {
        Self::new()
    }
}

