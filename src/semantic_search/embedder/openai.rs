// ABOUTME: OpenAI-compatible embedding provider using reqwest.
// ABOUTME: Works with OpenAI, Azure OpenAI, and compatible APIs.

use super::{EmbedderError, EmbeddingProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// OpenAI-compatible embedding provider.
pub struct OpenAIEmbedder {
    client: Client,
    api_url: String,
    api_key: String,
    model: String,
    dimension: usize,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    input: Vec<String>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    #[allow(dead_code)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct Usage {
    #[allow(dead_code)]
    total_tokens: u32,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    error_type: Option<String>,
}

impl OpenAIEmbedder {
    /// Create a new OpenAI embedding provider.
    pub fn new(base_url: String, api_key: String, model: String, dimension: usize) -> Self {
        let api_url = if base_url.ends_with('/') {
            format!("{}embeddings", base_url)
        } else {
            format!("{}/embeddings", base_url)
        };

        Self {
            client: Client::new(),
            api_url,
            api_key,
            model,
            dimension,
        }
    }

    /// Create from config using environment variable for API key.
    pub fn from_config(
        base_url: String,
        api_key_env: &str,
        model: String,
        dimension: usize,
    ) -> Result<Self, EmbedderError> {
        let api_key = std::env::var(api_key_env).map_err(|_| {
            EmbedderError::ConfigError(format!(
                "API key environment variable '{}' not set",
                api_key_env
            ))
        })?;

        Ok(Self::new(base_url, api_key, model, dimension))
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbedder {
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let request = EmbeddingRequest {
            input: texts.to_vec(),
            model: self.model.clone(),
            dimensions: Some(self.dimension),
        };

        let response = self
            .client
            .post(&self.api_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| EmbedderError::ApiError(e.to_string()))?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(|s| s * 1000); // Convert seconds to ms

            return Err(EmbedderError::RateLimited {
                retry_after_ms: retry_after,
            });
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Try to parse as API error
            if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(&error_text) {
                return Err(EmbedderError::ApiError(error_response.error.message));
            }

            return Err(EmbedderError::ApiError(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let result: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| EmbedderError::ApiError(e.to_string()))?;

        // Sort by index to maintain input order
        let mut embeddings: Vec<_> = result.data.into_iter().collect();
        embeddings.sort_by_key(|e| e.index);

        Ok(embeddings.into_iter().map(|e| e.embedding).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        "openai"
    }
}
