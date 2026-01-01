// ABOUTME: Local embedding provider using fastembed-rs.
// ABOUTME: Runs models locally without API calls for privacy and cost savings.

use super::{EmbedderError, EmbeddingProvider};
use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

/// FastEmbed local embedding provider.
pub struct FastEmbedProvider {
    model: Arc<RwLock<TextEmbedding>>,
    dimension: usize,
    model_name: String,
}

impl FastEmbedProvider {
    /// Create a new FastEmbed provider with the specified model.
    pub async fn new(
        model_name: &str,
        dimension: usize,
        cache_dir: Option<PathBuf>,
    ) -> Result<Self, EmbedderError> {
        let model_enum = match model_name {
            "BAAI/bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
            "BAAI/bge-base-en-v1.5" => EmbeddingModel::BGEBaseENV15,
            "BAAI/bge-large-en-v1.5" => EmbeddingModel::BGELargeENV15,
            _ => {
                return Err(EmbedderError::ModelNotFound(format!(
                    "Unknown fastembed model: {}. Supported: BAAI/bge-small-en-v1.5, BAAI/bge-base-en-v1.5, BAAI/bge-large-en-v1.5",
                    model_name
                )))
            }
        };
        let expected_dim = TextEmbedding::get_model_info(&model_enum)
            .map(|info| info.dim)
            .map_err(|e| {
                EmbedderError::ConfigError(format!(
                    "Failed to read fastembed model info: {}",
                    e
                ))
            })?;
        if dimension != expected_dim {
            return Err(EmbedderError::ConfigError(format!(
                "Fastembed dimension must be {} for model {}",
                expected_dim, model_name
            )));
        }

        // Initialize model in blocking context
        let model_name_owned = model_name.to_string();
        let model = tokio::task::spawn_blocking(move || {
            let mut options = InitOptions::new(model_enum).with_show_download_progress(true);
            if let Some(cache_dir) = cache_dir {
                options = options.with_cache_dir(cache_dir);
            }

            TextEmbedding::try_new(options)
                .map_err(|e| EmbedderError::ConfigError(format!("Failed to load model: {}", e)))
        })
        .await
        .map_err(|e| EmbedderError::ConfigError(format!("Task join error: {}", e)))??;

        Ok(Self {
            model: Arc::new(RwLock::new(model)),
            dimension: expected_dim,
            model_name: model_name_owned,
        })
    }

    /// Create with default model (BAAI/bge-base-en-v1.5).
    pub async fn default_model() -> Result<Self, EmbedderError> {
        Self::new("BAAI/bge-base-en-v1.5", 768, None).await
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // FastEmbed is sync, so we use blocking task
        let model = Arc::clone(&self.model);
        let texts_owned: Vec<String> = texts.to_vec();

        tokio::task::spawn_blocking(move || {
            let model = model.read();
            model
                .embed(texts_owned, None)
                .map_err(|e| EmbedderError::ApiError(format!("Embedding failed: {}", e)))
        })
        .await
        .map_err(|e| EmbedderError::ApiError(format!("Task join error: {}", e)))?
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        &self.model_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require model download and are slow
    // Run with: cargo test --features=slow-tests

    #[tokio::test]
    #[ignore = "requires model download"]
    async fn test_fastembed_basic() {
        let provider = FastEmbedProvider::default_model().await.unwrap();
        let texts = vec!["Hello, world!".to_string()];
        let embeddings = provider.embed_batch(&texts).await.unwrap();

        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), 768);
    }
}
