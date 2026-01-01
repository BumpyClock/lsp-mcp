// ABOUTME: Embedder module for generating vector embeddings from code chunks.
// ABOUTME: Supports OpenAI-compatible APIs and local FastEmbed models.

mod batch;
mod fastembed_provider;
mod openai;
pub mod types;

pub use batch::BatchProcessor;
pub use fastembed_provider::FastEmbedProvider;
pub use openai::OpenAIEmbedder;
pub use types::{BatchConfig, EmbeddingResult};

use crate::config::EmbedderConfig;
use async_trait::async_trait;
use std::{path::PathBuf, sync::Arc};

/// Error type for embedding operations.
#[derive(Debug, Clone)]
pub enum EmbedderError {
    /// API request failed
    ApiError(String),
    /// Rate limited by provider
    RateLimited { retry_after_ms: Option<u64> },
    /// Invalid configuration
    ConfigError(String),
    /// Model not found or unavailable
    ModelNotFound(String),
}

impl std::fmt::Display for EmbedderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiError(msg) => write!(f, "API error: {}", msg),
            Self::RateLimited { retry_after_ms } => {
                write!(f, "Rate limited")?;
                if let Some(ms) = retry_after_ms {
                    write!(f, ", retry after {}ms", ms)?;
                }
                Ok(())
            }
            Self::ConfigError(msg) => write!(f, "Config error: {}", msg),
            Self::ModelNotFound(model) => write!(f, "Model not found: {}", model),
        }
    }
}

impl std::error::Error for EmbedderError {}

/// Trait for embedding providers.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for a batch of texts.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError>;

    /// Get the embedding dimension for this provider.
    fn dimension(&self) -> usize;

    /// Get the provider name for logging.
    fn name(&self) -> &str;
}

/// Create an embedding provider from configuration.
pub async fn create_provider(config: &EmbedderConfig) -> Result<Arc<dyn EmbeddingProvider>, EmbedderError> {
    match config {
        EmbedderConfig::OpenAI {
            model,
            base_url,
            api_key,
            api_key_env,
            dimension,
        } => {
            let embedder = OpenAIEmbedder::from_config(
                base_url.clone(),
                api_key.as_deref(),
                api_key_env,
                model.clone(),
                *dimension,
            )?;
            Ok(Arc::new(embedder))
        }
        EmbedderConfig::FastEmbed {
            model,
            dimension,
            cache_dir,
        } => {
            let cache_dir = cache_dir.trim();
            let cache_dir = if cache_dir.is_empty() {
                None
            } else {
                Some(PathBuf::from(cache_dir))
            };
            let embedder = FastEmbedProvider::new(model, *dimension, cache_dir).await?;
            Ok(Arc::new(embedder))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedder_error_display() {
        let error = EmbedderError::ApiError("test error".to_string());
        assert!(error.to_string().contains("test error"));

        let error = EmbedderError::RateLimited {
            retry_after_ms: Some(1000),
        };
        assert!(error.to_string().contains("1000ms"));

        let error = EmbedderError::ConfigError("missing key".to_string());
        assert!(error.to_string().contains("missing key"));

        let error = EmbedderError::ModelNotFound("gpt-5".to_string());
        assert!(error.to_string().contains("gpt-5"));
    }
}
