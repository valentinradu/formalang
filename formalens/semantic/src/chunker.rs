//! Document chunking for semantic search
//!
//! Splits files into meaningful chunks based on content type:
//! - Markdown: Split by headers (## sections)
//! - FormaLang: Split by top-level definitions
//! - Rust: Split by items (fn, struct, impl blocks)

use std::path::Path;

/// Type of content being chunked
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChunkType {
    Markdown,
    FormaLang,
    Rust,
    Unknown,
}

impl ChunkType {
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "md" => Self::Markdown,
            "fv" => Self::FormaLang,
            "rs" => Self::Rust,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::FormaLang => "formalang",
            Self::Rust => "rust",
            Self::Unknown => "unknown",
        }
    }
}

/// A chunk of content with metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Chunk {
    /// Unique identifier (hash of content + path)
    pub id: String,
    /// Source file path
    pub path: String,
    /// Starting line number (1-indexed)
    pub line_start: u32,
    /// Ending line number (1-indexed)
    pub line_end: u32,
    /// Type of content
    pub chunk_type: ChunkType,
    /// Raw text content
    pub content: String,
    /// Optional title/name for the chunk
    pub title: Option<String>,
}

impl Chunk {
    pub fn new(
        path: &str,
        line_start: u32,
        line_end: u32,
        chunk_type: ChunkType,
        content: String,
        title: Option<String>,
    ) -> Self {
        let id = Self::generate_id(path, &content);
        Self {
            id,
            path: path.to_string(),
            line_start,
            line_end,
            chunk_type,
            content,
            title,
        }
    }

    fn generate_id(path: &str, content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        content.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

/// Chunker for splitting files into semantic units
pub struct Chunker;

impl Chunker {
    /// Chunk a file based on its extension
    pub fn chunk_file(path: &Path, content: &str) -> Vec<Chunk> {
        let path_str = path.to_string_lossy().to_string();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let chunk_type = ChunkType::from_extension(ext);

        match chunk_type {
            ChunkType::Markdown => Self::chunk_markdown(&path_str, content),
            ChunkType::FormaLang => Self::chunk_formalang(&path_str, content),
            ChunkType::Rust => Self::chunk_rust(&path_str, content),
            ChunkType::Unknown => Self::chunk_raw(&path_str, content, chunk_type),
        }
    }

    /// Split markdown by headers
    fn chunk_markdown(path: &str, content: &str) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let mut current_start = 0;
        let mut current_title: Option<String> = None;
        let mut current_content = String::new();

        for (i, line) in lines.iter().enumerate() {
            // Check for header (## or more)
            if line.starts_with("## ") || line.starts_with("### ") || line.starts_with("# ") {
                // Save previous chunk if not empty
                if !current_content.trim().is_empty() {
                    chunks.push(Chunk::new(
                        path,
                        current_start as u32 + 1,
                        i as u32,
                        ChunkType::Markdown,
                        current_content.clone(),
                        current_title.clone(),
                    ));
                }

                // Start new chunk
                current_start = i;
                current_title = Some(line.trim_start_matches('#').trim().to_string());
                current_content = String::new();
            }

            current_content.push_str(line);
            current_content.push('\n');
        }

        // Save final chunk
        if !current_content.trim().is_empty() {
            chunks.push(Chunk::new(
                path,
                current_start as u32 + 1,
                lines.len() as u32,
                ChunkType::Markdown,
                current_content,
                current_title,
            ));
        }

        // If no chunks created, create one for the whole file
        if chunks.is_empty() && !content.trim().is_empty() {
            chunks.push(Chunk::new(
                path,
                1,
                lines.len() as u32,
                ChunkType::Markdown,
                content.to_string(),
                None,
            ));
        }

        chunks
    }

    /// Split FormaLang by top-level definitions
    fn chunk_formalang(path: &str, content: &str) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let definition_starts = [
            "pub trait ",
            "trait ",
            "pub struct ",
            "struct ",
            "pub enum ",
            "enum ",
            "pub mod ",
            "mod ",
            "pub let ",
            "let ",
            "default ",
        ];

        let mut current_start = 0;
        let mut current_title: Option<String> = None;
        let mut current_content = String::new();
        let mut brace_depth: i32 = 0;
        let mut in_definition = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Check for new definition at depth 0
            if brace_depth == 0 {
                for starter in &definition_starts {
                    if trimmed.starts_with(starter) {
                        // Save previous chunk
                        if in_definition && !current_content.trim().is_empty() {
                            chunks.push(Chunk::new(
                                path,
                                current_start as u32 + 1,
                                i as u32,
                                ChunkType::FormaLang,
                                current_content.clone(),
                                current_title.clone(),
                            ));
                        }

                        // Extract title (definition name)
                        let after_keyword = trimmed.strip_prefix(starter).unwrap_or(trimmed);
                        let name = after_keyword
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .next()
                            .unwrap_or("")
                            .to_string();

                        current_start = i;
                        current_title = if name.is_empty() { None } else { Some(name) };
                        current_content = String::new();
                        in_definition = true;
                        break;
                    }
                }
            }

            // Track brace depth
            for c in line.chars() {
                match c {
                    '{' => brace_depth += 1,
                    '}' => brace_depth = brace_depth.saturating_sub(1),
                    _ => {}
                }
            }

            if in_definition {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }

        // Save final chunk
        if in_definition && !current_content.trim().is_empty() {
            chunks.push(Chunk::new(
                path,
                current_start as u32 + 1,
                lines.len() as u32,
                ChunkType::FormaLang,
                current_content,
                current_title,
            ));
        }

        // If no chunks, chunk the whole file
        if chunks.is_empty() && !content.trim().is_empty() {
            chunks.push(Chunk::new(
                path,
                1,
                lines.len() as u32,
                ChunkType::FormaLang,
                content.to_string(),
                None,
            ));
        }

        chunks
    }

    /// Split Rust by top-level items
    fn chunk_rust(path: &str, content: &str) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let item_starts = [
            "pub fn ",
            "fn ",
            "pub struct ",
            "struct ",
            "pub enum ",
            "enum ",
            "pub trait ",
            "trait ",
            "impl ",
            "pub mod ",
            "mod ",
            "pub type ",
            "type ",
            "pub const ",
            "const ",
            "pub static ",
            "static ",
        ];

        let mut current_start = 0;
        let mut current_title: Option<String> = None;
        let mut current_content = String::new();
        let mut brace_depth: i32 = 0;
        let mut in_item = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip doc comments and attributes when detecting item start
            if trimmed.starts_with("///")
                || trimmed.starts_with("//!")
                || trimmed.starts_with("#[")
            {
                if !in_item {
                    if current_content.is_empty() {
                        current_start = i;
                    }
                    current_content.push_str(line);
                    current_content.push('\n');
                }
                continue;
            }

            // Check for new item at depth 0
            if brace_depth == 0 && !in_item {
                for starter in &item_starts {
                    if trimmed.starts_with(starter) {
                        let after_keyword = trimmed.strip_prefix(starter).unwrap_or(trimmed);
                        let name = after_keyword
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .next()
                            .unwrap_or("")
                            .to_string();

                        current_title = if name.is_empty() { None } else { Some(name) };
                        in_item = true;
                        break;
                    }
                }
            }

            // Track brace depth
            for c in line.chars() {
                match c {
                    '{' => brace_depth += 1,
                    '}' => brace_depth = brace_depth.saturating_sub(1),
                    _ => {}
                }
            }

            current_content.push_str(line);
            current_content.push('\n');

            // End of item
            if in_item && brace_depth == 0 && (trimmed.ends_with('}') || trimmed.ends_with(';')) {
                if !current_content.trim().is_empty() {
                    chunks.push(Chunk::new(
                        path,
                        current_start as u32 + 1,
                        i as u32 + 1,
                        ChunkType::Rust,
                        current_content.clone(),
                        current_title.clone(),
                    ));
                }
                current_start = i + 1;
                current_title = None;
                current_content = String::new();
                in_item = false;
            }
        }

        // Save any remaining content
        if !current_content.trim().is_empty() {
            chunks.push(Chunk::new(
                path,
                current_start as u32 + 1,
                lines.len() as u32,
                ChunkType::Rust,
                current_content,
                current_title,
            ));
        }

        chunks
    }

    /// Fallback: chunk entire file as one unit
    fn chunk_raw(path: &str, content: &str, chunk_type: ChunkType) -> Vec<Chunk> {
        if content.trim().is_empty() {
            return Vec::new();
        }

        let line_count = content.lines().count() as u32;
        vec![Chunk::new(path, 1, line_count, chunk_type, content.to_string(), None)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_markdown() {
        let content = r#"# Title

Intro text.

## Section One

Content of section one.

## Section Two

Content of section two.
"#;

        let chunks = Chunker::chunk_markdown("test.md", content);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].title, Some("Title".to_string()));
        assert_eq!(chunks[1].title, Some("Section One".to_string()));
        assert_eq!(chunks[2].title, Some("Section Two".to_string()));
    }

    #[test]
    fn test_chunk_formalang() {
        let content = r#"pub struct Point {
  x: Number,
  y: Number
}

pub enum Color {
  red,
  green,
  blue
}
"#;

        let chunks = Chunker::chunk_formalang("test.fv", content);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].title, Some("Point".to_string()));
        assert_eq!(chunks[1].title, Some("Color".to_string()));
    }
}
