//! FormaLens Semantic Search
//!
//! Provides semantic search capabilities for FormaLang projects using
//! LanceDB for vector storage and fastembed for embeddings.

pub mod chunker;
pub mod embeddings;
pub mod indexer;
pub mod search;

pub use chunker::{Chunk, ChunkType, Chunker};
pub use embeddings::EmbeddingModel;
pub use indexer::Indexer;
pub use search::SearchEngine;
