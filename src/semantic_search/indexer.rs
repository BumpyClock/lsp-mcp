// ABOUTME: Initial scan and embedding generation for semantic code search.
// ABOUTME: Handles chunking, batching, and progress tracking during indexing.

use super::chunker::{create_chunker, ChunkConfig, CodeChunk};
use super::embedder::BatchProcessor;
use super::enrichment::EnrichmentManager;
use super::manager::SemanticSearchState;
use super::vector_store::{HnswVectorStore, IndexEntry, VectorStore};
use crate::config::SemanticSearchConfig;
use chrono::Utc;
use glob::Pattern;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Indexer for semantic search.
pub struct Indexer {
    config: SemanticSearchConfig,
    workspace_root: PathBuf,
    chunk_config: ChunkConfig,
    enricher: Option<Arc<EnrichmentManager>>,
}

impl Indexer {
    /// Create a new indexer.
    pub fn new(
        config: SemanticSearchConfig,
        workspace_root: PathBuf,
        chunk_config: ChunkConfig,
        enricher: Option<Arc<EnrichmentManager>>,
    ) -> Self {
        Self {
            config,
            workspace_root,
            chunk_config,
            enricher,
        }
    }

    /// Run initial full workspace scan.
    pub async fn run_initial_scan(
        &self,
        store: Arc<HnswVectorStore>,
        processor: Arc<BatchProcessor>,
        state: Arc<RwLock<SemanticSearchState>>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let files = self.collect_files()?;
        let total_files = files.len();

        info!(files = total_files, "Starting semantic indexing");

        *state.write().await = SemanticSearchState::Indexing {
            indexed_count: 0,
            total_count: total_files,
        };

        let mut indexed_files = 0usize;
        let mut pending_chunks: Vec<CodeChunk> = Vec::new();
        let mut pending_file_hashes: HashMap<String, String> = HashMap::new();

        let batch_size = self.config.batch_size;
        let max_size_bytes = self.max_file_size_bytes();

        for file_path in files {
            if shutdown_rx.try_recv().is_ok() {
                info!("Indexing cancelled by shutdown signal");
                break;
            }

            let relative_path = file_path
                .strip_prefix(&self.workspace_root)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .to_string();

            if let Ok(metadata) = file_path.metadata() {
                if metadata.len() > max_size_bytes {
                    debug!(
                        file = %file_path.display(),
                        size = metadata.len(),
                        max_size = max_size_bytes,
                        "Skipping large file"
                    );
                    store.delete_file(&relative_path).await?;
                    indexed_files += 1;
                    *state.write().await = SemanticSearchState::Indexing {
                        indexed_count: indexed_files,
                        total_count: total_files,
                    };
                    continue;
                }
            }

            let content = match tokio::fs::read_to_string(&file_path).await {
                Ok(content) => content,
                Err(e) => {
                    debug!(
                        file = %file_path.display(),
                        error = %e,
                        "Failed to read file for indexing"
                    );
                    indexed_files += 1;
                    *state.write().await = SemanticSearchState::Indexing {
                        indexed_count: indexed_files,
                        total_count: total_files,
                    };
                    continue;
                }
            };

            let content_hash = compute_file_hash(&content);
            let existing_hash = store.get_file_hash(&relative_path).await?;

            if existing_hash.as_deref() == Some(&content_hash) {
                indexed_files += 1;
                *state.write().await = SemanticSearchState::Indexing {
                    indexed_count: indexed_files,
                    total_count: total_files,
                };
                continue;
            }

            store.delete_file(&relative_path).await?;

            if content.is_empty() {
                store
                    .upsert_file_hash(&relative_path, &content_hash, Utc::now().timestamp())
                    .await?;
                indexed_files += 1;
                *state.write().await = SemanticSearchState::Indexing {
                    indexed_count: indexed_files,
                    total_count: total_files,
                };
                continue;
            }

            match self.chunk_content(&file_path, &content, &relative_path) {
                Ok(chunks) => {
                    if chunks.is_empty() {
                        store
                            .upsert_file_hash(&relative_path, &content_hash, Utc::now().timestamp())
                            .await?;
                    } else {
                        pending_chunks.extend(chunks);
                        pending_file_hashes.insert(relative_path.clone(), content_hash);
                    }
                }
                Err(e) => {
                    debug!(
                        file = %file_path.display(),
                        error = %e,
                        "Failed to process file for indexing"
                    );
                }
            }

            if pending_chunks.len() >= batch_size {
                let batch_count = self
                    .embed_and_index_batch(&mut pending_chunks, &store, &processor)
                    .await?;
                if batch_count > 0 {
                    self.update_file_hashes(&store, &pending_file_hashes)
                        .await?;
                    pending_file_hashes.clear();
                    if let Err(e) = store.flush().await {
                        warn!(error = %e, "Failed to flush semantic index");
                    }
                }
            }

            indexed_files += 1;
            *state.write().await = SemanticSearchState::Indexing {
                indexed_count: indexed_files,
                total_count: total_files,
            };
        }

        // Process remaining chunks
        if !pending_chunks.is_empty() {
            let batch_count = self
                .embed_and_index_batch(&mut pending_chunks, &store, &processor)
                .await?;
            if batch_count > 0 {
                self.update_file_hashes(&store, &pending_file_hashes)
                    .await?;
                pending_file_hashes.clear();
                if let Err(e) = store.flush().await {
                    warn!(error = %e, "Failed to flush semantic index");
                }
            }
        }

        let stats = store.stats().await?;
        Ok(stats.chunk_count)
    }

    /// Collect all files to index based on config patterns.
    fn collect_files(&self) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
        let mut files = Vec::new();

        // Compile include patterns
        let include_patterns: Vec<Pattern> = self
            .config
            .include
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        // Compile exclude patterns
        let exclude_patterns: Vec<Pattern> = self
            .config
            .expanded_exclude_patterns()
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        let walker = WalkBuilder::new(&self.workspace_root)
            .standard_filters(true)
            .hidden(true)
            .git_ignore(self.config.respect_gitignore)
            .build();

        for entry in walker {
            let entry = entry?;
            if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                continue;
            }

            let path = entry.path();

            // Get relative path for pattern matching
            let relative_path = path
                .strip_prefix(&self.workspace_root)
                .unwrap_or(path)
                .to_string_lossy();

            // Check exclude patterns first
            let excluded = exclude_patterns.iter().any(|p| p.matches(&relative_path));
            if excluded {
                continue;
            }

            // Check include patterns
            let included = include_patterns.is_empty()
                || include_patterns.iter().any(|p| p.matches(&relative_path));
            if !included {
                continue;
            }

            files.push(path.to_path_buf());
        }

        Ok(files)
    }

    fn chunk_content(
        &self,
        path: &Path,
        content: &str,
        relative_path: &str,
    ) -> Result<Vec<CodeChunk>, Box<dyn std::error::Error + Send + Sync>> {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let chunker = create_chunker(extension);

        let mut chunks = chunker.chunk_file(path, content, &self.chunk_config)?;
        for chunk in &mut chunks {
            chunk.file_path = relative_path.to_string();
        }

        Ok(chunks)
    }

    /// Embed a batch of chunks and add to index.
    async fn embed_and_index_batch(
        &self,
        chunks: &mut Vec<CodeChunk>,
        store: &Arc<HnswVectorStore>,
        processor: &Arc<BatchProcessor>,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        if chunks.is_empty() {
            return Ok(0);
        }

        if let Some(enricher) = &self.enricher {
            if let Err(e) = enricher.enrich_chunks(store, chunks).await {
                warn!(error = %e, "Failed to enrich semantic chunks");
            }
        }

        let embeddings = processor.process_chunks(chunks).await?;

        let now = Utc::now().timestamp();
        let count = chunks.len();

        let entries: Vec<_> = chunks
            .drain(..)
            .zip(embeddings)
            .map(|(chunk, emb)| {
                let entry = IndexEntry {
                    id: chunk.segment_hash,
                    file_path: chunk.file_path,
                    code: chunk.code,
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    symbol_name: chunk.symbol_name,
                    symbol_kind: chunk.symbol_kind,
                    indexed_at: now,
                };
                (entry, emb.embedding)
            })
            .collect();

        let upserted = self.upsert_with_retry(store, &entries).await?;

        debug!(chunks = count, "Indexed batch");

        Ok(upserted)
    }

    /// Index a single file (for incremental updates).
    pub async fn index_file(
        &self,
        path: &Path,
        store: &Arc<HnswVectorStore>,
        processor: &Arc<BatchProcessor>,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        self.index_file_internal(path, store, processor, false)
            .await
    }

    /// Index a single file and ignore stored hashes.
    pub async fn index_file_force(
        &self,
        path: &Path,
        store: &Arc<HnswVectorStore>,
        processor: &Arc<BatchProcessor>,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        self.index_file_internal(path, store, processor, true).await
    }

    async fn index_file_internal(
        &self,
        path: &Path,
        store: &Arc<HnswVectorStore>,
        processor: &Arc<BatchProcessor>,
        force: bool,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let relative_path = path
            .strip_prefix(&self.workspace_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let max_size_bytes = self.max_file_size_bytes();
        if let Ok(metadata) = path.metadata() {
            if metadata.len() > max_size_bytes {
                store.delete_file(&relative_path).await?;
                return Ok(0);
            }
        }

        let content = tokio::fs::read_to_string(path).await?;
        let content_hash = compute_file_hash(&content);

        if !force {
            let existing_hash = store.get_file_hash(&relative_path).await?;
            if existing_hash.as_deref() == Some(&content_hash) {
                return Ok(0);
            }
        }

        store.delete_file(&relative_path).await?;

        if content.is_empty() {
            store
                .upsert_file_hash(&relative_path, &content_hash, Utc::now().timestamp())
                .await?;
            return Ok(0);
        }

        let mut chunks = self.chunk_content(path, &content, &relative_path)?;
        if chunks.is_empty() {
            store
                .upsert_file_hash(&relative_path, &content_hash, Utc::now().timestamp())
                .await?;
            return Ok(0);
        }

        let count = self
            .embed_and_index_batch(&mut chunks, store, processor)
            .await?;
        store
            .upsert_file_hash(&relative_path, &content_hash, Utc::now().timestamp())
            .await?;

        Ok(count)
    }

    /// Remove a file from the index.
    pub async fn remove_file(
        &self,
        path: &Path,
        store: &Arc<HnswVectorStore>,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let relative_path = path
            .strip_prefix(&self.workspace_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let deleted = store.delete_file(&relative_path).await?;
        Ok(deleted)
    }

    fn max_file_size_bytes(&self) -> u64 {
        (self.config.max_file_size_mb * 1024.0 * 1024.0) as u64
    }

    async fn update_file_hashes(
        &self,
        store: &Arc<HnswVectorStore>,
        file_hashes: &HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if file_hashes.is_empty() {
            return Ok(());
        }
        let updated_at = Utc::now().timestamp();
        for (path, hash) in file_hashes {
            store.upsert_file_hash(path, hash, updated_at).await?;
        }
        Ok(())
    }

    /// TODO: Make retry policy configurable once we expose vector store retry settings.
    async fn upsert_with_retry(
        &self,
        store: &Arc<HnswVectorStore>,
        entries: &[(IndexEntry, Vec<f32>)],
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let mut attempt = 0u32;
        let mut backoff_ms = 100u64;
        let max_retries = 3u32;

        loop {
            match store.upsert(entries.to_vec()).await {
                Ok(count) => return Ok(count),
                Err(e) => {
                    if attempt >= max_retries {
                        return Err(Box::new(e));
                    }
                    warn!(
                        error = %e,
                        attempt = %(attempt + 1),
                        max_retries = %max_retries,
                        backoff_ms = %backoff_ms,
                        "Failed to upsert batch, retrying"
                    );
                    sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms as f64 * 2.0) as u64;
                    backoff_ms = backoff_ms.min(10_000);
                    attempt += 1;
                }
            }
        }
    }
}

fn compute_file_hash(content: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    hex::encode(&hash.as_bytes()[..16])
}
