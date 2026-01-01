// ABOUTME: Semantic search manager coordinating indexing lifecycle and search API.
// ABOUTME: Manages state transitions, background indexing, and incremental updates.

use super::chunker::ChunkConfig;
use super::embedder::{BatchConfig, BatchProcessor, EmbedderError};
use super::indexer::Indexer;
use super::vector_store::{
    create_store, HnswVectorStore, IndexState, SearchOptions, SearchResult, VectorStore,
    VectorStoreError,
};
use super::watcher::SemanticWatcher;
use crate::config::SemanticSearchConfig;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

/// Current state of the semantic search system.
#[derive(Debug, Clone)]
pub enum SemanticSearchState {
    /// Feature disabled in config
    Disabled,
    /// Initializing (loading index, starting services)
    Initializing,
    /// Initial indexing in progress
    Indexing {
        /// Files indexed so far
        indexed_count: usize,
        /// Total files to index
        total_count: usize,
    },
    /// Ready for search queries
    Ready {
        /// Total indexed chunks
        total_chunks: usize,
    },
    /// Incremental update in progress (search still available)
    Updating {
        /// Files being updated
        pending_files: usize,
    },
    /// Error state with recovery possible
    Error { message: String },
}

/// Error type for semantic search operations.
#[derive(Debug, Clone)]
pub enum SemanticSearchError {
    /// Feature is disabled
    Disabled,
    /// Indexing is in progress
    IndexingInProgress { indexed: usize, total: usize },
    /// Index error
    IndexError(String),
    /// Embedding error
    EmbeddingError(String),
    /// Search failed
    SearchFailed(String),
    /// Configuration error
    ConfigError(String),
}

impl std::fmt::Display for SemanticSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "Semantic search is disabled"),
            Self::IndexingInProgress { indexed, total } => {
                write!(f, "Indexing in progress: {}/{} files", indexed, total)
            }
            Self::IndexError(msg) => write!(f, "Index error: {}", msg),
            Self::EmbeddingError(msg) => write!(f, "Embedding error: {}", msg),
            Self::SearchFailed(msg) => write!(f, "Search failed: {}", msg),
            Self::ConfigError(msg) => write!(f, "Config error: {}", msg),
        }
    }
}

impl std::error::Error for SemanticSearchError {}

impl From<VectorStoreError> for SemanticSearchError {
    fn from(err: VectorStoreError) -> Self {
        SemanticSearchError::IndexError(err.to_string())
    }
}

impl From<EmbedderError> for SemanticSearchError {
    fn from(err: EmbedderError) -> Self {
        SemanticSearchError::EmbeddingError(err.to_string())
    }
}

/// Semantic search manager.
pub struct SemanticSearchManager {
    config: SemanticSearchConfig,
    state: Arc<RwLock<SemanticSearchState>>,
    store: Option<Arc<HnswVectorStore>>,
    processor: Option<Arc<BatchProcessor>>,
    #[allow(dead_code)]
    indexer: Option<Arc<Indexer>>,
    workspace_root: PathBuf,
    /// Shutdown signal sender
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl SemanticSearchManager {
    /// Create a new manager (does not start indexing).
    pub fn new(config: SemanticSearchConfig, workspace_root: PathBuf) -> Self {
        let initial_state = if config.enabled {
            SemanticSearchState::Initializing
        } else {
            SemanticSearchState::Disabled
        };

        Self {
            config,
            state: Arc::new(RwLock::new(initial_state)),
            store: None,
            processor: None,
            indexer: None,
            workspace_root,
            shutdown_tx: None,
        }
    }

    /// Initialize and start background indexing.
    pub async fn start(&mut self) -> Result<(), SemanticSearchError> {
        if !self.config.enabled {
            *self.state.write().await = SemanticSearchState::Disabled;
            return Ok(());
        }

        // Validate config
        if let Err(msg) = self.config.is_valid() {
            *self.state.write().await = SemanticSearchState::Error {
                message: msg.clone(),
            };
            return Err(SemanticSearchError::ConfigError(msg));
        }

        let (shutdown_tx, _) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        // Initialize vector store
        let index_dir = self.workspace_root.join(self.config.storage_path());
        let dimension = self.config.embedder.dimension();

        let store = match create_store(&index_dir, dimension).await {
            Ok(s) => Arc::new(s),
            Err(e) => {
                let msg = format!("Failed to create vector store: {}", e);
                *self.state.write().await = SemanticSearchState::Error { message: msg.clone() };
                return Err(SemanticSearchError::IndexError(msg));
            }
        };
        self.store = Some(Arc::clone(&store));

        let provider = match super::embedder::create_provider(&self.config.embedder).await {
            Ok(p) => p,
            Err(e) => {
                let msg = format!("Failed to create embedding provider: {}", e);
                *self.state.write().await = SemanticSearchState::Error { message: msg.clone() };
                return Err(SemanticSearchError::EmbeddingError(msg));
            }
        };

        let batch_config = BatchConfig::with_batch_size(self.config.index.batch_size);
        let processor = Arc::new(BatchProcessor::new(provider, batch_config));
        self.processor = Some(Arc::clone(&processor));

        // Check if index is already ready
        let index_state = store.get_state().await?;
        if index_state == IndexState::Ready {
            let stats = store.stats().await?;
            *self.state.write().await = SemanticSearchState::Ready {
                total_chunks: stats.chunk_count,
            };
            info!(chunks = stats.chunk_count, "Loaded existing semantic index");

            // Start watcher for incremental updates
            self.start_watcher(shutdown_tx.subscribe()).await?;
            return Ok(());
        }

        // Create indexer
        let chunk_config = ChunkConfig::from_index_config(
            self.config.index.min_chunk_chars,
            self.config.index.max_chunk_chars,
            self.config.index.max_function_chunk_chars,
            self.config.index.chunk_overlap_chars,
        );

        let indexer = Arc::new(Indexer::new(
            self.config.clone(),
            self.workspace_root.clone(),
            chunk_config,
        ));
        self.indexer = Some(Arc::clone(&indexer));

        let now = Utc::now().timestamp();
        if let Err(e) = store.set_index_started_at(now).await {
            warn!(error = %e, "Failed to record index start timestamp");
        }
        if let Err(e) = store.clear_index_completed_at().await {
            warn!(error = %e, "Failed to clear index completion timestamp");
        }

        // Mark as building
        store.set_state(IndexState::Building).await?;

        // Spawn background indexing task
        let state = Arc::clone(&self.state);
        let store_clone = Arc::clone(&store);
        let processor_clone = Arc::clone(&processor);
        let indexer_clone = Arc::clone(&indexer);
        let shutdown_rx = shutdown_tx.subscribe();

        tokio::spawn(async move {
            match indexer_clone
                .run_initial_scan(store_clone.clone(), processor_clone, state.clone(), shutdown_rx)
                .await
            {
                Ok(chunk_count) => {
                    if let Err(e) = store_clone.set_state(IndexState::Ready).await {
                        error!(error = %e, "Failed to set index state to ready");
                    }
                    let now = Utc::now().timestamp();
                    if let Err(e) = store_clone.set_index_completed_at(now).await {
                        warn!(error = %e, "Failed to record index completion timestamp");
                    }

                    *state.write().await = SemanticSearchState::Ready {
                        total_chunks: chunk_count,
                    };

                    if let Err(e) = store_clone.flush().await {
                        warn!(error = %e, "Failed to flush index");
                    }

                    info!(chunks = chunk_count, "Semantic indexing complete");
                }
                Err(e) => {
                    *state.write().await = SemanticSearchState::Error {
                        message: e.to_string(),
                    };
                    error!(error = %e, "Semantic indexing failed");
                }
            }
        });

        // Start file watcher
        self.start_watcher(shutdown_tx.subscribe()).await?;

        Ok(())
    }

    /// Start the file watcher for incremental updates.
    async fn start_watcher(
        &mut self,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), SemanticSearchError> {
        if self.store.is_none() {
            return Ok(());
        }

        let store = self.store.as_ref().unwrap().clone();

        // Create processor if not already created
        let processor = if let Some(ref p) = self.processor {
            p.clone()
        } else {
            let provider = super::embedder::create_provider(&self.config.embedder).await?;
            let batch_config = BatchConfig::with_batch_size(self.config.index.batch_size);
            Arc::new(BatchProcessor::new(provider, batch_config))
        };

        let chunk_config = ChunkConfig::from_index_config(
            self.config.index.min_chunk_chars,
            self.config.index.max_chunk_chars,
            self.config.index.max_function_chunk_chars,
            self.config.index.chunk_overlap_chars,
        );

        let watcher = SemanticWatcher::new(
            self.config.clone(),
            self.workspace_root.clone(),
            store,
            processor,
            Arc::clone(&self.state),
            chunk_config,
            shutdown_rx,
        )?;

        // start() takes ownership of watcher - it runs in a background task
        watcher.start();

        Ok(())
    }

    /// Get current state.
    pub async fn state(&self) -> SemanticSearchState {
        self.state.read().await.clone()
    }

    /// Perform semantic search.
    pub async fn search(
        &self,
        query: &str,
        limit: Option<usize>,
        path_prefix: Option<String>,
    ) -> Result<Vec<SearchResult>, SemanticSearchError> {
        let state = self.state.read().await.clone();

        match &state {
            SemanticSearchState::Disabled => {
                return Err(SemanticSearchError::Disabled);
            }
            SemanticSearchState::Indexing {
                indexed_count,
                total_count,
            } => {
                return Err(SemanticSearchError::IndexingInProgress {
                    indexed: *indexed_count,
                    total: *total_count,
                });
            }
            SemanticSearchState::Error { message } => {
                return Err(SemanticSearchError::IndexError(message.clone()));
            }
            SemanticSearchState::Initializing => {
                return Err(SemanticSearchError::IndexingInProgress {
                    indexed: 0,
                    total: 0,
                });
            }
            SemanticSearchState::Ready { .. } | SemanticSearchState::Updating { .. } => {
                // Proceed with search
            }
        }

        let store = self
            .store
            .as_ref()
            .ok_or_else(|| SemanticSearchError::IndexError("Store not initialized".to_string()))?;

        let processor = self.processor.as_ref().ok_or_else(|| {
            SemanticSearchError::EmbeddingError("Processor not initialized".to_string())
        })?;

        // Embed the query
        let query_embedding = processor.embed_query(query).await?;

        let max_results = limit.unwrap_or(self.config.search.max_results);
        let options = SearchOptions {
            limit: max_results,
            min_score: Some(self.config.search.min_score),
            path_prefix,
            symbol_kinds: None,
        };

        let results = store.search(&query_embedding, options).await?;
        Ok(results)
    }

    /// Get index statistics.
    pub async fn stats(&self) -> Result<super::vector_store::IndexStats, SemanticSearchError> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| SemanticSearchError::IndexError("Store not initialized".to_string()))?;

        Ok(store.stats().await?)
    }

    /// Graceful shutdown.
    pub async fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Flush index before shutdown
        if let Some(ref store) = self.store {
            if let Err(e) = store.flush().await {
                warn!(error = %e, "Failed to flush index on shutdown");
            }
        }
    }
}
