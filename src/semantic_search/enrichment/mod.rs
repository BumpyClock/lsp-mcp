// ABOUTME: Optional LLM enrichment for semantic search embeddings.
// ABOUTME: Adds summaries and tags to chunks before embedding.

use crate::config::{EmbedderConfig, EnrichmentConfig};
use crate::semantic_search::chunker::CodeChunk;
use crate::semantic_search::vector_store::{EnrichmentData, HnswVectorStore};
use chrono::Utc;
use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, warn};

#[derive(Debug)]
pub enum EnrichmentError {
    ConfigError(String),
    ApiError(String),
}

impl std::fmt::Display for EnrichmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnrichmentError::ConfigError(msg) => write!(f, "{}", msg),
            EnrichmentError::ApiError(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for EnrichmentError {}

pub struct EnrichmentManager {
    client: Client,
    api_url: String,
    api_key: String,
    model: String,
    config: EnrichmentConfig,
}

#[derive(Clone)]
struct EnrichmentInput {
    id: String,
    path: String,
    symbol_kind: Option<String>,
    symbol_name: Option<String>,
    doc: Option<String>,
    code: String,
}

#[derive(Serialize)]
struct EnrichmentRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Serialize)]
struct PromptInput<'a> {
    items: Vec<PromptItem<'a>>,
    summary_max_chars: usize,
    max_tags: usize,
}

#[derive(Serialize)]
struct PromptItem<'a> {
    id: &'a str,
    path: &'a str,
    symbol_kind: Option<&'a str>,
    symbol_name: Option<&'a str>,
    doc: Option<&'a str>,
    code: &'a str,
}

#[derive(Deserialize)]
struct EnrichmentResponse {
    items: Vec<EnrichmentItem>,
}

#[derive(Deserialize)]
struct EnrichmentItem {
    id: String,
    summary: String,
    tags: Vec<String>,
}

impl EnrichmentManager {
    pub fn from_embedder_config(
        embedder: &EmbedderConfig,
        config: &EnrichmentConfig,
    ) -> Result<Option<Self>, EnrichmentError> {
        if !config.enabled {
            return Ok(None);
        }

        let (base_url, api_key, api_key_env) = match embedder {
            EmbedderConfig::OpenAI {
                base_url,
                api_key,
                api_key_env,
                ..
            } => (base_url.clone(), api_key.clone(), api_key_env.clone()),
            _ => {
                return Ok(None);
            }
        };

        let api_key = resolve_api_key(api_key.as_deref(), &api_key_env)?;
        Ok(Some(Self::new(base_url, api_key, config.clone())))
    }

    pub fn new(base_url: String, api_key: String, config: EnrichmentConfig) -> Self {
        let api_url = if base_url.ends_with('/') {
            format!("{}chat/completions", base_url)
        } else {
            format!("{}/chat/completions", base_url)
        };

        let model = config.model.clone();
        Self {
            client: Client::new(),
            api_url,
            api_key,
            model,
            config,
        }
    }

    pub async fn enrich_chunks(
        &self,
        store: &HnswVectorStore,
        chunks: &mut [CodeChunk],
    ) -> Result<(), EnrichmentError> {
        let mut candidates = Vec::new();
        for chunk in chunks.iter() {
            if !is_enrichable(chunk) {
                continue;
            }
            candidates.push(EnrichmentInput {
                id: chunk.segment_hash.clone(),
                path: chunk.file_path.clone(),
                symbol_kind: chunk.symbol_kind.clone(),
                symbol_name: chunk.symbol_name.clone(),
                doc: chunk.doc_comment.clone(),
                code: chunk.code.clone(),
            });
        }

        if candidates.is_empty() {
            return Ok(());
        }

        let mut cached = HashMap::new();
        let mut pending = Vec::new();
        for input in candidates {
            match store.get_enrichment(&input.id).await {
                Ok(Some(enrichment)) => {
                    cached.insert(input.id, enrichment);
                }
                Ok(None) => pending.push(input),
                Err(e) => {
                    warn!(error = %e, "Failed to read enrichment cache");
                    pending.push(input);
                }
            }
        }

        if !pending.is_empty() {
            let batch_size = self.config.batch_size.max(1);
            let max_concurrency = self.config.max_concurrency.max(1);
            let batches: Vec<Vec<EnrichmentInput>> =
                pending.chunks(batch_size).map(|chunk| chunk.to_vec()).collect();

            let mut results = HashMap::new();
            let responses = stream::iter(batches.into_iter())
                .map(|batch| async move { self.fetch_batch(&batch).await })
                .buffer_unordered(max_concurrency)
                .collect::<Vec<_>>()
                .await;

            for response in responses {
                match response {
                    Ok(items) => {
                        for (id, enrichment) in items {
                            results.insert(id, enrichment);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Enrichment batch failed");
                    }
                }
            }

            if !results.is_empty() {
                let now = Utc::now().timestamp();
                for (id, enrichment) in results {
                    if let Err(e) = store
                        .upsert_enrichment(&id, &enrichment.summary, &enrichment.tags, now)
                        .await
                    {
                        warn!(error = %e, "Failed to store enrichment");
                    }
                    cached.insert(id, enrichment);
                }
            }
        }

        if cached.is_empty() {
            return Ok(());
        }

        for chunk in chunks.iter_mut() {
            if let Some(enrichment) = cached.get(&chunk.segment_hash) {
                if !enrichment.summary.trim().is_empty() {
                    chunk.summary = Some(enrichment.summary.clone());
                }
                if !enrichment.tags.is_empty() {
                    chunk.tags = Some(enrichment.tags.clone());
                }
            }
        }

        Ok(())
    }

    async fn fetch_batch(
        &self,
        batch: &[EnrichmentInput],
    ) -> Result<HashMap<String, EnrichmentData>, EnrichmentError> {
        if batch.is_empty() {
            return Ok(HashMap::new());
        }

        let prompt = build_prompt(batch, self.config.summary_max_chars, self.config.max_tags);
        let request = EnrichmentRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt(self.config.summary_max_chars, self.config.max_tags),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: prompt,
                },
            ],
            temperature: 0.0,
        };

        let timeout_duration = Duration::from_millis(self.config.timeout_ms.max(1));
        let response = timeout(
            timeout_duration,
            self.client
                .post(&self.api_url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send(),
        )
        .await
        .map_err(|_| EnrichmentError::ApiError("Enrichment request timed out".to_string()))?
        .map_err(|e| EnrichmentError::ApiError(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(&error_text) {
                return Err(EnrichmentError::ApiError(error_response.error.message));
            }
            return Err(EnrichmentError::ApiError(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let result: ChatResponse = response
            .json()
            .await
            .map_err(|e| EnrichmentError::ApiError(e.to_string()))?;

        let content = result
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .unwrap_or_default();

        parse_enrichment_response(&content, self.config.summary_max_chars, self.config.max_tags)
    }
}

fn resolve_api_key(api_key: Option<&str>, api_key_env: &str) -> Result<String, EnrichmentError> {
    let api_key = api_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var(api_key_env).ok())
        .ok_or_else(|| {
            EnrichmentError::ConfigError(format!(
                "OpenAI API key missing; set api_key or environment variable: {}",
                api_key_env
            ))
        })?;
    Ok(api_key)
}

fn is_enrichable(chunk: &CodeChunk) -> bool {
    match chunk.symbol_kind.as_deref() {
        Some("function") | Some("type") | Some("impl") | Some("constant") => !chunk.code.is_empty(),
        _ => false,
    }
}

fn system_prompt(summary_max_chars: usize, max_tags: usize) -> String {
    format!(
        "Return JSON with an items array of {{id, summary, tags}} objects only\nEach summary must be <= {} characters\nEach tags array must have at most {} tags\nUse concise, concrete summaries and short lowercase tags\nReturn only JSON",
        summary_max_chars, max_tags
    )
}

fn build_prompt(batch: &[EnrichmentInput], summary_max_chars: usize, max_tags: usize) -> String {
    let items: Vec<PromptItem<'_>> = batch
        .iter()
        .map(|item| PromptItem {
            id: item.id.as_str(),
            path: item.path.as_str(),
            symbol_kind: item.symbol_kind.as_deref(),
            symbol_name: item.symbol_name.as_deref(),
            doc: item.doc.as_deref(),
            code: item.code.as_str(),
        })
        .collect();

    let input = PromptInput {
        items,
        summary_max_chars,
        max_tags,
    };

    let input_json =
        serde_json::to_string(&input).unwrap_or_else(|_| "{\"items\":[]}".to_string());
    format!(
        "Generate summaries and tags for the input items\n{}\n",
        input_json
    )
}

fn parse_enrichment_response(
    content: &str,
    summary_max_chars: usize,
    max_tags: usize,
) -> Result<HashMap<String, EnrichmentData>, EnrichmentError> {
    let parsed: EnrichmentResponse = serde_json::from_str(content)
        .map_err(|e| EnrichmentError::ApiError(format!("Failed to parse enrichment response: {}", e)))?;
    let mut results = HashMap::new();

    for item in parsed.items {
        let mut summary = item.summary.trim().to_string();
        if summary.len() > summary_max_chars {
            summary = summary.chars().take(summary_max_chars).collect::<String>();
        }

        let mut tags = Vec::new();
        if max_tags > 0 {
            for tag in item.tags {
                let trimmed = tag.trim();
                if trimmed.is_empty() {
                    continue;
                }
                tags.push(trimmed.to_string());
                if tags.len() >= max_tags {
                    break;
                }
            }
        }

        if summary.is_empty() && tags.is_empty() {
            continue;
        }

        results.insert(item.id, EnrichmentData { summary, tags });
    }

    debug!(items = results.len(), "Parsed enrichment results");
    Ok(results)
}
