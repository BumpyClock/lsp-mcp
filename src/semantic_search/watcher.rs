// ABOUTME: File watcher for incremental semantic index updates.
// ABOUTME: Batches file changes with configurable debouncing for efficiency.

use super::chunker::ChunkConfig;
use super::embedder::BatchProcessor;
use super::enrichment::EnrichmentManager;
use super::indexer::Indexer;
use super::manager::{SemanticSearchError, SemanticSearchState};
use super::vector_store::{HnswVectorStore, VectorStore};
use crate::config::SemanticSearchConfig;
use crate::utils::file_watcher::FileWatcher;
use glob::Pattern;
use notify_debouncer_mini::DebounceEventResult;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, info, warn};

/// Debounce duration for file change batching.
const DEBOUNCE_DURATION: Duration = Duration::from_secs(2);

/// Batch processing interval.
const BATCH_INTERVAL: Duration = Duration::from_secs(5);

/// Semantic search file watcher.
pub struct SemanticWatcher {
    config: SemanticSearchConfig,
    workspace_root: PathBuf,
    store: Arc<HnswVectorStore>,
    processor: Arc<BatchProcessor>,
    enricher: Option<Arc<EnrichmentManager>>,
    state: Arc<RwLock<SemanticSearchState>>,
    chunk_config: ChunkConfig,
    shutdown_rx: broadcast::Receiver<()>,
    change_rx: mpsc::Receiver<PathBuf>,
    watcher: FileWatcher,
}

impl SemanticWatcher {
    /// Create a new file watcher.
    pub fn new(
        config: SemanticSearchConfig,
        workspace_root: PathBuf,
        store: Arc<HnswVectorStore>,
        processor: Arc<BatchProcessor>,
        enricher: Option<Arc<EnrichmentManager>>,
        state: Arc<RwLock<SemanticSearchState>>,
        chunk_config: ChunkConfig,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<Self, SemanticSearchError> {
        let (change_tx, change_rx) = mpsc::channel(1000);

        let config_for_callback = config.clone();
        let root_for_callback = workspace_root.clone();
        let runtime = tokio::runtime::Handle::current();
        let watcher = FileWatcher::new(
            &workspace_root,
            DEBOUNCE_DURATION,
            move |result: DebounceEventResult| match result {
                Ok(events) => {
                    for event in events {
                        if event.path.is_dir()
                            || !Self::should_index_file_static(
                                &event.path,
                                &root_for_callback,
                                &config_for_callback,
                            )
                        {
                            continue;
                        }
                        let tx = change_tx.clone();
                        runtime.spawn(async move {
                            let _ = tx.send(event.path).await;
                        });
                    }
                }
                Err(error) => warn!(?error, "Semantic file watching failed"),
            },
        )
        .map_err(|error| {
            SemanticSearchError::IndexError(format!("Failed to watch workspace: {error}"))
        })?;

        Ok(Self {
            config,
            workspace_root,
            store,
            processor,
            enricher,
            state,
            chunk_config,
            shutdown_rx,
            change_rx,
            watcher,
        })
    }

    /// Start watching for file changes.
    pub fn start(self) {
        tokio::spawn(async move {
            let _watcher = self.watcher;
            info!("Started semantic search file watcher");
            Self::run_batch_processor(
                self.config,
                self.workspace_root,
                self.store,
                self.processor,
                self.enricher,
                self.state,
                self.chunk_config,
                self.change_rx,
                self.shutdown_rx,
            )
            .await;
        });
    }

    /// Run the batch processor loop.
    async fn run_batch_processor(
        config: SemanticSearchConfig,
        workspace_root: PathBuf,
        store: Arc<HnswVectorStore>,
        processor: Arc<BatchProcessor>,
        enricher: Option<Arc<EnrichmentManager>>,
        state: Arc<RwLock<SemanticSearchState>>,
        chunk_config: ChunkConfig,
        mut change_rx: mpsc::Receiver<PathBuf>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        let indexer = Indexer::new(
            config.clone(),
            workspace_root.clone(),
            chunk_config,
            enricher,
        );
        let mut pending_files: HashSet<PathBuf> = HashSet::new();
        let mut batch_timer = tokio::time::interval(BATCH_INTERVAL);

        loop {
            tokio::select! {
                // Receive file change
                Some(path) = change_rx.recv() => {
                    pending_files.insert(path);
                }

                // Batch processing tick
                _ = batch_timer.tick() => {
                    if pending_files.is_empty() {
                        continue;
                    }

                    let files: Vec<PathBuf> = pending_files.drain().collect();
                    let file_count = files.len();

                    // Update state to Updating
                    let current_state = state.read().await.clone();
                    if matches!(current_state, SemanticSearchState::Ready { .. }) {
                        *state.write().await = SemanticSearchState::Updating {
                            pending_files: file_count,
                        };
                    }

                    debug!(files = file_count, "Processing file changes for semantic index");

                    for path in files {
                        if path.exists() {
                            // File modified or created
                            if let Err(e) = indexer.index_file(&path, &store, &processor).await {
                                debug!(
                                    file = %path.display(),
                                    error = %e,
                                    "Failed to reindex file"
                                );
                            }
                        } else {
                            // File deleted
                            if let Err(e) = indexer.remove_file(&path, &store).await {
                                debug!(
                                    file = %path.display(),
                                    error = %e,
                                    "Failed to remove file from index"
                                );
                            }
                        }
                    }

                    // Return to Ready state
                    if let Ok(stats) = store.stats().await {
                        *state.write().await = SemanticSearchState::Ready {
                            total_chunks: stats.chunk_count,
                        };
                    }

                    // Flush changes
                    if let Err(e) = store.flush().await {
                        warn!(error = %e, "Failed to flush updated index");
                    }
                }

                // Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Semantic watcher shutting down");
                    break;
                }
            }
        }
    }

    /// Check if a file should be indexed (static version for use in callback).
    fn should_index_file_static(
        path: &PathBuf,
        workspace_root: &PathBuf,
        config: &SemanticSearchConfig,
    ) -> bool {
        let relative_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(path)
            .to_string_lossy();

        // Check exclude patterns
        for pattern in config.expanded_exclude_patterns() {
            if let Ok(p) = Pattern::new(&pattern) {
                if p.matches(&relative_path) {
                    return false;
                }
            }
        }

        // Check include patterns
        if config.include.is_empty() {
            return true;
        }

        for pattern in &config.include {
            if let Ok(p) = Pattern::new(pattern) {
                if p.matches(&relative_path) {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_search::embedder::{BatchConfig, EmbedderError, EmbeddingProvider};

    struct TestEmbeddings;

    #[async_trait::async_trait]
    impl EmbeddingProvider for TestEmbeddings {
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
            Ok(texts.iter().map(|_| vec![1.0; 4]).collect())
        }
        fn dimension(&self) -> usize {
            4
        }
        fn name(&self) -> &str {
            "test"
        }
    }

    async fn wait_for_hash(store: &HnswVectorStore, previous: Option<&str>) -> String {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(hash) = store.get_file_hash("watched.rs").await.unwrap() {
                    if Some(hash.as_str()) != previous {
                        return hash;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("file events must update the semantic index")
    }

    #[tokio::test]
    async fn semantic_watcher_indexes_file_creation_and_modification() {
        let root = tempfile::tempdir().unwrap();
        let index = tempfile::tempdir().unwrap();
        let store = Arc::new(HnswVectorStore::new(index.path(), 4).await.unwrap());
        let processor = Arc::new(BatchProcessor::new(
            Arc::new(TestEmbeddings),
            BatchConfig::default(),
        ));
        let (shutdown, rx) = broadcast::channel(1);
        let state = Arc::new(RwLock::new(SemanticSearchState::Ready { total_chunks: 0 }));
        let watcher = SemanticWatcher::new(
            SemanticSearchConfig::default_enabled(),
            root.path().to_path_buf(),
            store.clone(),
            processor,
            None,
            state,
            ChunkConfig::default(),
            rx,
        )
        .unwrap();
        watcher.start();
        let path = root.path().join("watched.rs");
        std::fs::write(&path, "pub fn original() -> &'static str { \"a sufficiently long function for semantic indexing\" }\n").unwrap();
        let first = wait_for_hash(&store, None).await;
        std::fs::write(&path, "pub fn modified() -> &'static str { \"different content must replace the previous indexed function\" }\n").unwrap();
        wait_for_hash(&store, Some(&first)).await;
        std::fs::remove_file(&path).unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            while !store
                .get_file_hashes("watched.rs")
                .await
                .unwrap()
                .is_empty()
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("deleted files must leave the semantic index");
        shutdown.send(()).unwrap();
    }

    #[tokio::test]
    async fn semantic_watcher_rejects_a_missing_workspace() {
        let root = tempfile::tempdir().unwrap();
        let index = tempfile::tempdir().unwrap();
        let store = Arc::new(HnswVectorStore::new(index.path(), 4).await.unwrap());
        let processor = Arc::new(BatchProcessor::new(
            Arc::new(TestEmbeddings),
            BatchConfig::default(),
        ));
        let (_shutdown, rx) = broadcast::channel(1);
        let result = SemanticWatcher::new(
            SemanticSearchConfig::default_enabled(),
            root.path().join("missing"),
            store,
            processor,
            None,
            Arc::new(RwLock::new(SemanticSearchState::Initializing)),
            ChunkConfig::default(),
            rx,
        );
        assert!(
            result.is_err(),
            "watcher failure must propagate to its caller"
        );
    }
}
