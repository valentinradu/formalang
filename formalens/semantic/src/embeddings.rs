//! Embedding generation using fastembed
//!
//! Uses the BAAI/bge-small-en-v1.5 model for generating text embeddings locally.

use anyhow::Result;
use fastembed::{EmbeddingModel as FastEmbedModel, InitOptions, TextEmbedding};

/// Wrapper around fastembed for generating embeddings
pub struct EmbeddingModel {
    model: TextEmbedding,
}

impl EmbeddingModel {
    /// Initialize the embedding model
    ///
    /// Downloads the model on first use (~30MB for bge-small-en-v1.5)
    pub fn new() -> Result<Self> {
        let mut options = InitOptions::default();
        options.model_name = FastEmbedModel::BGESmallENV15;
        options.show_download_progress = true;

        let model = TextEmbedding::try_new(options)?;

        Ok(Self { model })
    }

    /// Generate embeddings for a batch of texts
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let embeddings = self.model.embed(texts.to_vec(), None)?;
        Ok(embeddings)
    }

    /// Generate embedding for a single text
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed(&[text.to_string()])?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding generated"))
    }

    /// Get the embedding dimension for this model
    pub fn dimension(&self) -> usize {
        384 // bge-small-en-v1.5 produces 384-dimensional embeddings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires model download
    fn test_embedding_generation() {
        let model = EmbeddingModel::new().unwrap();
        let embedding = model.embed_one("Hello, world!").unwrap();
        assert_eq!(embedding.len(), 384);
    }
}
