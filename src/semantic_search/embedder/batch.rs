// ABOUTME: Batch processor for embedding generation with retry and backoff.
// ABOUTME: Handles rate limiting and transient failures gracefully.

use super::types::{BatchConfig, EmbeddingResult};
use super::{EmbedderError, EmbeddingProvider};
use crate::semantic_search::chunker::CodeChunk;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Batch processor for embedding generation with retry logic.
pub struct BatchProcessor {
    provider: Arc<dyn EmbeddingProvider>,
    config: BatchConfig,
}

impl BatchProcessor {
    /// Create a new batch processor.
    pub fn new(provider: Arc<dyn EmbeddingProvider>, config: BatchConfig) -> Self {
        Self { provider, config }
    }

    /// Process chunks in batches, returning embeddings for successful chunks.
    pub async fn process_chunks(
        &self,
        chunks: &[CodeChunk],
    ) -> Result<Vec<EmbeddingResult>, EmbedderError> {
        let mut results = Vec::with_capacity(chunks.len());

        for batch_start in (0..chunks.len()).step_by(self.config.batch_size) {
            let batch_end = (batch_start + self.config.batch_size).min(chunks.len());
            let batch = &chunks[batch_start..batch_end];

            let texts: Vec<String> = batch.iter().map(embedding_text).collect();
            let hashes: Vec<String> = batch.iter().map(|c| c.segment_hash.clone()).collect();

            let embeddings = self.embed_with_retry(&texts).await?;

            for (embedding, segment_hash) in embeddings.into_iter().zip(hashes) {
                results.push(EmbeddingResult {
                    embedding,
                    segment_hash,
                    token_count: None,
                });
            }

            debug!(
                provider = %self.provider.name(),
                batch = %batch_end,
                total = %chunks.len(),
                "Processed embedding batch"
            );
        }

        Ok(results)
    }

    /// Embed texts for a query (single item, no batching needed).
    pub async fn embed_query(&self, query: &str) -> Result<Vec<f32>, EmbedderError> {
        let texts = vec![query.to_string()];
        let embeddings = self.embed_with_retry(&texts).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| EmbedderError::ApiError("No embedding returned".to_string()))
    }

    async fn embed_with_retry(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        let mut attempt = 0;
        let mut backoff_ms = self.config.retry.initial_backoff_ms;

        loop {
            match self.provider.embed_batch(texts).await {
                Ok(embeddings) => return Ok(embeddings),
                Err(EmbedderError::RateLimited { retry_after_ms }) => {
                    if attempt >= self.config.retry.max_retries {
                        return Err(EmbedderError::RateLimited { retry_after_ms });
                    }

                    let wait_ms = retry_after_ms.unwrap_or(backoff_ms);
                    warn!(
                        provider = %self.provider.name(),
                        wait_ms = %wait_ms,
                        attempt = %(attempt + 1),
                        max_retries = %self.config.retry.max_retries,
                        "Rate limited, waiting before retry"
                    );

                    sleep(Duration::from_millis(wait_ms)).await;

                    backoff_ms =
                        ((backoff_ms as f64) * self.config.retry.backoff_multiplier) as u64;
                    backoff_ms = backoff_ms.min(self.config.retry.max_backoff_ms);
                    attempt += 1;
                }
                Err(EmbedderError::ApiError(msg)) if attempt < self.config.retry.max_retries => {
                    warn!(
                        provider = %self.provider.name(),
                        error = %msg,
                        backoff_ms = %backoff_ms,
                        attempt = %(attempt + 1),
                        max_retries = %self.config.retry.max_retries,
                        "API error, retrying"
                    );

                    sleep(Duration::from_millis(backoff_ms)).await;

                    backoff_ms =
                        ((backoff_ms as f64) * self.config.retry.backoff_multiplier) as u64;
                    backoff_ms = backoff_ms.min(self.config.retry.max_backoff_ms);
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

fn embedding_text(chunk: &CodeChunk) -> String {
    let mut sections = Vec::new();

    if let Some(doc) = chunk.doc_comment.as_ref() {
        if !doc.trim().is_empty() {
            sections.push(doc.clone());
        }
    }

    if let Some(summary) = chunk.summary.as_ref() {
        let summary = summary.trim();
        if !summary.is_empty() {
            sections.push(format!("Summary: {}", summary));
        }
    }

    if let Some(tags) = chunk.tags.as_ref() {
        let tags: Vec<String> = tags
            .iter()
            .map(|tag| tag.trim())
            .filter(|tag| !tag.is_empty())
            .map(|tag| tag.to_string())
            .collect();
        if !tags.is_empty() {
            sections.push(format!("Tags: {}", tags.join(", ")));
        }
    }

    sections.push(chunk.code.clone());
    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockProvider {
        dimension: usize,
        call_count: AtomicUsize,
        fail_first_n: usize,
    }

    impl MockProvider {
        fn new(dimension: usize) -> Self {
            Self {
                dimension,
                call_count: AtomicUsize::new(0),
                fail_first_n: 0,
            }
        }

        fn with_failures(dimension: usize, fail_first_n: usize) -> Self {
            Self {
                dimension,
                call_count: AtomicUsize::new(0),
                fail_first_n,
            }
        }
    }

    #[async_trait]
    impl EmbeddingProvider for MockProvider {
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            if count < self.fail_first_n {
                return Err(EmbedderError::ApiError("Transient error".to_string()));
            }

            Ok(texts.iter().map(|_| vec![0.0; self.dimension]).collect())
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_batch_processing() {
        let provider = Arc::new(MockProvider::new(384));
        let processor = BatchProcessor::new(provider, BatchConfig::default());

        let chunks = vec![
            CodeChunk {
                file_path: "test.rs".to_string(),
                code: "fn foo() {}".to_string(),
                doc_comment: None,
                summary: None,
                tags: None,
                start_line: 1,
                end_line: 1,
                segment_hash: "hash1".to_string(),
                symbol_name: Some("foo".to_string()),
                symbol_kind: Some("function".to_string()),
            },
            CodeChunk {
                file_path: "test.rs".to_string(),
                code: "fn bar() {}".to_string(),
                doc_comment: None,
                summary: None,
                tags: None,
                start_line: 3,
                end_line: 3,
                segment_hash: "hash2".to_string(),
                symbol_name: Some("bar".to_string()),
                symbol_kind: Some("function".to_string()),
            },
        ];

        let results = processor.process_chunks(&chunks).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].segment_hash, "hash1");
        assert_eq!(results[1].segment_hash, "hash2");
    }

    #[tokio::test]
    async fn test_retry_on_failure() {
        let provider = Arc::new(MockProvider::with_failures(384, 2));
        let mut config = BatchConfig::default();
        config.retry.initial_backoff_ms = 1; // Speed up test
        let processor = BatchProcessor::new(provider.clone(), config);

        let texts = vec!["test".to_string()];
        let result = processor.embed_with_retry(&texts).await;

        assert!(result.is_ok());
        assert_eq!(provider.call_count.load(Ordering::SeqCst), 3);
    }

    struct CapturingProvider {
        dimension: usize,
        last_texts: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl EmbeddingProvider for CapturingProvider {
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
            let mut guard = self
                .last_texts
                .lock()
                .map_err(|_| EmbedderError::ApiError("Failed to lock".to_string()))?;
            *guard = texts.to_vec();
            Ok(texts.iter().map(|_| vec![0.0; self.dimension]).collect())
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn name(&self) -> &str {
            "capture"
        }
    }

    #[tokio::test]
    async fn test_doc_comment_is_prefixed_in_embeddings() {
        let last_texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingProvider {
            dimension: 8,
            last_texts: last_texts.clone(),
        });
        let processor = BatchProcessor::new(provider, BatchConfig::default());

        let chunks = vec![CodeChunk {
            file_path: "test.rs".to_string(),
            code: "fn foo() {}".to_string(),
            doc_comment: Some("/** doc */".to_string()),
            summary: None,
            tags: None,
            start_line: 1,
            end_line: 1,
            segment_hash: "hash-doc".to_string(),
            symbol_name: Some("foo".to_string()),
            symbol_kind: Some("function".to_string()),
        }];

        let result = processor.process_chunks(&chunks).await;
        assert!(result.is_ok());

        let captured = last_texts.lock().unwrap().clone();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0], "/** doc */\n\nfn foo() {}");
    }
}
