// ABOUTME: Types for embedding generation and provider configuration.
// ABOUTME: Defines EmbeddingResult, retry settings, and batch configuration.

/// Result of embedding a text chunk.
#[derive(Debug, Clone)]
pub struct EmbeddingResult {
    /// The computed embedding vector
    pub embedding: Vec<f32>,
    /// The segment hash this embedding corresponds to
    pub segment_hash: String,
    /// Token count (if available from provider)
    pub token_count: Option<u32>,
}

/// Retry configuration for batch processing.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Initial backoff in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff in milliseconds
    pub max_backoff_ms: u64,
    /// Backoff multiplier (exponential)
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 10000,
            backoff_multiplier: 2.0,
        }
    }
}

/// Batch processing configuration.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Number of texts per batch (default: 60)
    pub batch_size: usize,
    /// Retry configuration
    pub retry: RetryConfig,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            batch_size: 60,
            retry: RetryConfig::default(),
        }
    }
}

impl BatchConfig {
    /// Create batch config from index config batch size.
    pub fn with_batch_size(batch_size: usize) -> Self {
        Self {
            batch_size,
            ..Default::default()
        }
    }
}
