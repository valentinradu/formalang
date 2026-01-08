//! Source map generation for FormaLang -> WGSL mappings
//!
//! Tracks the relationship between generated WGSL code and original FormaLang
//! source locations, enabling debugging and error reporting.

use std::collections::HashMap;

/// A source map that tracks WGSL line numbers to original source information.
///
/// # Example
///
/// ```
/// use formalang::codegen::SourceMap;
///
/// let mut map = SourceMap::new();
/// map.add_struct_mapping(1, "Vec2");
/// map.add_function_mapping(5, "Vec2", "length");
///
/// assert_eq!(map.get_source_name(1), Some("struct Vec2".to_string()));
/// assert_eq!(map.get_source_name(5), Some("Vec2::length".to_string()));
/// ```
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    /// Maps WGSL line number (1-indexed) to source information
    entries: HashMap<usize, SourceMapEntry>,
}

/// An entry in the source map.
#[derive(Clone, Debug)]
pub struct SourceMapEntry {
    /// The kind of source element
    pub kind: SourceKind,
    /// Name of the struct (if applicable)
    pub struct_name: Option<String>,
    /// Name of the function (if applicable)
    pub function_name: Option<String>,
    /// Original source line (if known)
    pub source_line: Option<usize>,
}

/// The kind of source element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// A struct definition
    Struct,
    /// A function definition
    Function,
    /// A field within a struct
    Field,
    /// A statement within a function
    Statement,
}

impl SourceMap {
    /// Create a new empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a struct mapping.
    ///
    /// Records that the given WGSL line corresponds to a struct definition.
    pub fn add_struct_mapping(&mut self, wgsl_line: usize, struct_name: &str) {
        self.entries.insert(
            wgsl_line,
            SourceMapEntry {
                kind: SourceKind::Struct,
                struct_name: Some(struct_name.to_string()),
                function_name: None,
                source_line: None,
            },
        );
    }

    /// Add a struct mapping with source line.
    pub fn add_struct_mapping_with_line(
        &mut self,
        wgsl_line: usize,
        struct_name: &str,
        source_line: usize,
    ) {
        self.entries.insert(
            wgsl_line,
            SourceMapEntry {
                kind: SourceKind::Struct,
                struct_name: Some(struct_name.to_string()),
                function_name: None,
                source_line: Some(source_line),
            },
        );
    }

    /// Add a function mapping.
    ///
    /// Records that the given WGSL line corresponds to a function definition.
    pub fn add_function_mapping(&mut self, wgsl_line: usize, struct_name: &str, fn_name: &str) {
        self.entries.insert(
            wgsl_line,
            SourceMapEntry {
                kind: SourceKind::Function,
                struct_name: Some(struct_name.to_string()),
                function_name: Some(fn_name.to_string()),
                source_line: None,
            },
        );
    }

    /// Add a function mapping with source line.
    pub fn add_function_mapping_with_line(
        &mut self,
        wgsl_line: usize,
        struct_name: &str,
        fn_name: &str,
        source_line: usize,
    ) {
        self.entries.insert(
            wgsl_line,
            SourceMapEntry {
                kind: SourceKind::Function,
                struct_name: Some(struct_name.to_string()),
                function_name: Some(fn_name.to_string()),
                source_line: Some(source_line),
            },
        );
    }

    /// Add a field mapping.
    pub fn add_field_mapping(&mut self, wgsl_line: usize, struct_name: &str, field_name: &str) {
        self.entries.insert(
            wgsl_line,
            SourceMapEntry {
                kind: SourceKind::Field,
                struct_name: Some(struct_name.to_string()),
                function_name: Some(field_name.to_string()), // Reusing field for name
                source_line: None,
            },
        );
    }

    /// Get the source name for a WGSL line.
    ///
    /// Returns a human-readable description of what source element corresponds
    /// to the given WGSL line.
    pub fn get_source_name(&self, wgsl_line: usize) -> Option<String> {
        self.entries.get(&wgsl_line).map(|entry| match entry.kind {
            SourceKind::Struct => format!("struct {}", entry.struct_name.as_deref().unwrap_or("?")),
            SourceKind::Function => format!(
                "{}::{}",
                entry.struct_name.as_deref().unwrap_or("?"),
                entry.function_name.as_deref().unwrap_or("?")
            ),
            SourceKind::Field => format!(
                "{}.{}",
                entry.struct_name.as_deref().unwrap_or("?"),
                entry.function_name.as_deref().unwrap_or("?")
            ),
            SourceKind::Statement => {
                if let Some(fn_name) = &entry.function_name {
                    format!(
                        "{}::{}",
                        entry.struct_name.as_deref().unwrap_or("?"),
                        fn_name
                    )
                } else {
                    "statement".to_string()
                }
            }
        })
    }

    /// Get the entry for a WGSL line.
    pub fn get_entry(&self, wgsl_line: usize) -> Option<&SourceMapEntry> {
        self.entries.get(&wgsl_line)
    }

    /// Get the original source line for a WGSL line (if known).
    pub fn get_source_line(&self, wgsl_line: usize) -> Option<usize> {
        self.entries.get(&wgsl_line).and_then(|e| e.source_line)
    }

    /// Find the closest mapping for a WGSL line.
    ///
    /// Searches backwards from the given line to find the nearest mapping.
    /// Useful for mapping lines within a function body to the function definition.
    pub fn find_closest(&self, wgsl_line: usize) -> Option<&SourceMapEntry> {
        for line in (1..=wgsl_line).rev() {
            if let Some(entry) = self.entries.get(&line) {
                return Some(entry);
            }
        }
        None
    }

    /// Get all entries in the source map.
    pub fn entries(&self) -> &HashMap<usize, SourceMapEntry> {
        &self.entries
    }

    /// Check if the source map is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_struct_mapping() {
        let mut map = SourceMap::new();
        map.add_struct_mapping(1, "Vec2");

        assert_eq!(map.get_source_name(1), Some("struct Vec2".to_string()));
        assert!(map.get_source_name(2).is_none());
    }

    #[test]
    fn test_add_function_mapping() {
        let mut map = SourceMap::new();
        map.add_function_mapping(5, "Vec2", "length");

        assert_eq!(map.get_source_name(5), Some("Vec2::length".to_string()));
    }

    #[test]
    fn test_add_field_mapping() {
        let mut map = SourceMap::new();
        map.add_field_mapping(2, "Vec2", "x");

        assert_eq!(map.get_source_name(2), Some("Vec2.x".to_string()));
    }

    #[test]
    fn test_find_closest() {
        let mut map = SourceMap::new();
        map.add_function_mapping(5, "Vec2", "length");

        // Line 7 is inside the function
        let entry = map.find_closest(7);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().function_name, Some("length".to_string()));
    }

    #[test]
    fn test_source_line_tracking() {
        let mut map = SourceMap::new();
        map.add_struct_mapping_with_line(1, "Vec2", 10);

        assert_eq!(map.get_source_line(1), Some(10));
    }
}
