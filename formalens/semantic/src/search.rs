//! Semantic search functionality
//!
//! Provides vector similarity search over indexed documents.

use anyhow::Result;
use futures::TryStreamExt;
use lancedb::connect;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Connection;

use crate::embeddings::EmbeddingModel;

const TABLE_NAME: &str = "documents";

/// Search result from a semantic query
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub chunk_type: String,
    pub content: String,
    pub title: Option<String>,
    pub score: f32,
}

/// Semantic search engine
pub struct SearchEngine {
    db: Connection,
    model: EmbeddingModel,
}

impl SearchEngine {
    /// Create a new search engine
    pub async fn new(db_path: &str) -> Result<Self> {
        let db = connect(db_path).execute().await?;
        let model = EmbeddingModel::new()?;

        Ok(Self { db, model })
    }

    /// Check if the index exists
    pub async fn has_index(&self) -> Result<bool> {
        Ok(self.db.table_names().execute().await?.contains(&TABLE_NAME.to_string()))
    }

    /// Search for similar documents
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if !self.has_index().await? {
            anyhow::bail!("No index found. Run 'formalens index' first.");
        }

        // Generate query embedding
        let query_vector = self.model.embed_one(query)?;

        // Perform vector search
        let table = self.db.open_table(TABLE_NAME).execute().await?;

        let mut results_stream = table
            .vector_search(query_vector)?
            .limit(limit)
            .execute()
            .await?;

        // Convert results
        let mut search_results = Vec::new();

        // Use arrow to extract columns
        use arrow_array::cast::AsArray;
        use arrow_array::Array;

        while let Some(batch) = results_stream.try_next().await? {
            let paths = batch.column_by_name("path").unwrap().as_string::<i32>();
            let line_starts = batch.column_by_name("line_start").unwrap().as_primitive::<arrow_array::types::UInt32Type>();
            let line_ends = batch.column_by_name("line_end").unwrap().as_primitive::<arrow_array::types::UInt32Type>();
            let chunk_types = batch.column_by_name("chunk_type").unwrap().as_string::<i32>();
            let contents = batch.column_by_name("content").unwrap().as_string::<i32>();
            let titles = batch.column_by_name("title").unwrap().as_string::<i32>();
            let distances = batch.column_by_name("_distance").unwrap().as_primitive::<arrow_array::types::Float32Type>();

            for i in 0..batch.num_rows() {
                let title = if titles.is_null(i) {
                    None
                } else {
                    Some(titles.value(i).to_string())
                };

                search_results.push(SearchResult {
                    path: paths.value(i).to_string(),
                    line_start: line_starts.value(i),
                    line_end: line_ends.value(i),
                    chunk_type: chunk_types.value(i).to_string(),
                    content: contents.value(i).to_string(),
                    title,
                    score: 1.0 - distances.value(i), // Convert distance to similarity
                });
            }
        }

        Ok(search_results)
    }
}
