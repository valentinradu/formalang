//! Indexer for building and updating the semantic search index
//!
//! Walks the codebase, chunks files, generates embeddings, and stores in LanceDB.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use arrow_array::{ArrayRef, Float32Array, RecordBatch, RecordBatchIterator, StringArray, UInt32Array};
use arrow_schema::{DataType, Field, Schema};
use lancedb::connect;
use lancedb::Connection;
use walkdir::WalkDir;

use crate::chunker::{Chunk, Chunker};
use crate::embeddings::EmbeddingModel;

const TABLE_NAME: &str = "documents";
const BATCH_SIZE: usize = 100;

/// Indexer for building the semantic search database
pub struct Indexer {
    db: Connection,
    model: EmbeddingModel,
}

impl Indexer {
    /// Create a new indexer with the given database path
    pub async fn new(db_path: &str) -> Result<Self> {
        let db = connect(db_path).execute().await?;
        let model = EmbeddingModel::new()?;

        Ok(Self { db, model })
    }

    /// Get the schema for the documents table
    fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("path", DataType::Utf8, false),
            Field::new("line_start", DataType::UInt32, false),
            Field::new("line_end", DataType::UInt32, false),
            Field::new("chunk_type", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("title", DataType::Utf8, true),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    384,
                ),
                false,
            ),
        ]))
    }

    /// Index all files in the given directory
    pub async fn index_directory(&self, root: &Path, patterns: &[&str]) -> Result<IndexStats> {
        let mut stats = IndexStats::default();
        let mut all_chunks: Vec<Chunk> = Vec::new();

        // Collect all matching files
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !is_hidden(e))
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            // Check if file matches any pattern
            if !patterns.iter().any(|p| p.trim_start_matches("*.") == ext) {
                continue;
            }

            // Read and chunk the file
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    let chunks = Chunker::chunk_file(path, &content);
                    stats.files_processed += 1;
                    stats.chunks_created += chunks.len();
                    all_chunks.extend(chunks);
                }
                Err(e) => {
                    eprintln!("Warning: Could not read {}: {}", path.display(), e);
                    stats.files_skipped += 1;
                }
            }
        }

        if all_chunks.is_empty() {
            return Ok(stats);
        }

        // Process chunks in batches
        for batch in all_chunks.chunks(BATCH_SIZE) {
            self.index_chunks(batch).await?;
        }

        stats.total_indexed = all_chunks.len();
        Ok(stats)
    }

    /// Index a batch of chunks
    async fn index_chunks(&self, chunks: &[Chunk]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        // Generate embeddings for all chunks
        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let embeddings = self.model.embed(&texts)?;

        // Build Arrow arrays
        let ids: Vec<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
        let paths: Vec<&str> = chunks.iter().map(|c| c.path.as_str()).collect();
        let line_starts: Vec<u32> = chunks.iter().map(|c| c.line_start).collect();
        let line_ends: Vec<u32> = chunks.iter().map(|c| c.line_end).collect();
        let chunk_types: Vec<&str> = chunks.iter().map(|c| c.chunk_type.as_str()).collect();
        let contents: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
        let titles: Vec<Option<&str>> = chunks.iter().map(|c| c.title.as_deref()).collect();

        let id_array = Arc::new(StringArray::from(ids)) as ArrayRef;
        let path_array = Arc::new(StringArray::from(paths)) as ArrayRef;
        let line_start_array = Arc::new(UInt32Array::from(line_starts)) as ArrayRef;
        let line_end_array = Arc::new(UInt32Array::from(line_ends)) as ArrayRef;
        let chunk_type_array = Arc::new(StringArray::from(chunk_types)) as ArrayRef;
        let content_array = Arc::new(StringArray::from(contents)) as ArrayRef;
        let title_array = Arc::new(StringArray::from(titles)) as ArrayRef;

        // Build vector array using FixedSizeListArray
        let flat_embeddings: Vec<f32> = embeddings.into_iter().flatten().collect();
        let values = Float32Array::from(flat_embeddings);
        let vector_field = Arc::new(Field::new("item", DataType::Float32, true));
        let vector_array = Arc::new(
            arrow_array::FixedSizeListArray::new(vector_field, 384, Arc::new(values), None)
        ) as ArrayRef;

        let batch = RecordBatch::try_new(
            Self::schema(),
            vec![
                id_array,
                path_array,
                line_start_array,
                line_end_array,
                chunk_type_array,
                content_array,
                title_array,
                vector_array,
            ],
        )?;

        // Create or append to table
        let batches = RecordBatchIterator::new(vec![Ok(batch)], Self::schema());

        if self.db.table_names().execute().await?.contains(&TABLE_NAME.to_string()) {
            let table = self.db.open_table(TABLE_NAME).execute().await?;
            table.add(batches).execute().await?;
        } else {
            self.db.create_table(TABLE_NAME, batches).execute().await?;
        }

        Ok(())
    }

    /// Clear the index
    pub async fn clear(&self) -> Result<()> {
        if self.db.table_names().execute().await?.contains(&TABLE_NAME.to_string()) {
            self.db.drop_table(TABLE_NAME).await?;
        }
        Ok(())
    }
}

/// Check if a directory entry is hidden
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.') || s == "target" || s == "node_modules")
        .unwrap_or(false)
}

/// Statistics from an indexing operation
#[derive(Debug, Default)]
pub struct IndexStats {
    pub files_processed: usize,
    pub files_skipped: usize,
    pub chunks_created: usize,
    pub total_indexed: usize,
}

impl std::fmt::Display for IndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Indexed {} chunks from {} files ({} skipped)",
            self.total_indexed, self.files_processed, self.files_skipped
        )
    }
}
