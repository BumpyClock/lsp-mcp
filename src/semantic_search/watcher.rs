// ABOUTME: File watcher for incremental semantic index updates.
// ABOUTME: Batches file changes with configurable debouncing for efficiency.

use super::chunker::ChunkConfig;
use super::embedder::BatchProcessor;
use super::enrichment::EnrichmentManager;
use super::indexer::Indexer;
use super::manager::{SemanticSearchError, SemanticSearchState};
use super::vector_store::{HnswVectorStore, VectorStore};
use crate::config::SemanticSearchConfig;
use glob::Pattern;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebouncedEvent, Debouncer};
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
    /// Channel for file change events
    change_tx: mpsc::Sender<PathBuf>,
    change_rx: Option<mpsc::Receiver<PathBuf>>,
    /// File watcher handle
    _debouncer: Option<Debouncer<RecommendedWatcher>>,
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

        Ok(Self {
            config,
            workspace_root,
            store,
            processor,
            enricher,
            state,
            chunk_config,
            shutdown_rx,
            change_tx,
            change_rx: Some(change_rx),
            _debouncer: None,
        })
    }

    /// Start watching for file changes.
    pub fn start(mut self) {
        let config = self.config.clone();
        let workspace_root = self.workspace_root.clone();
        let store = self.store.clone();
        let processor = self.processor.clone();
        let enricher = self.enricher.clone();
        let state = self.state.clone();
        let chunk_config = self.chunk_config.clone();
        let shutdown_rx = self.shutdown_rx;
        let runtime = tokio::runtime::Handle::current();

        // Take ownership of the receiver
        let change_rx = self.change_rx.take().expect("change_rx already taken");
        let change_tx = self.change_tx.clone();

        // Start the notify watcher in a separate thread
        let workspace_for_watcher = workspace_root.clone();
        let workspace_for_processor = workspace_root.clone();
        let config_clone = config.clone();

        std::thread::spawn(move || {
            let tx_clone = change_tx.clone();
            let config_ref = config_clone.clone();
            let workspace_for_callback = workspace_for_watcher.clone();

            let debouncer_result = new_debouncer(
                DEBOUNCE_DURATION,
                move |res: Result<Vec<DebouncedEvent>, _>| {
                    if let Ok(events) = res {
                        for event in events {
                            let path = &event.path;

                            // Skip directories
                            if path.is_dir() {
                                continue;
                            }

                            // Skip if not a matching file
                            if !Self::should_index_file_static(
                                path,
                                &workspace_for_callback,
                                &config_ref,
                            ) {
                                continue;
                            }

                            let tx = tx_clone.clone();
                            let path = path.clone();
                            runtime.spawn(async move {
                                let _ = tx.send(path).await;
                            });
                        }
                    }
                },
            );

            match debouncer_result {
                Ok(mut debouncer) => {
                    if let Err(e) = debouncer
                        .watcher()
                        .watch(&workspace_for_watcher, RecursiveMode::Recursive)
                    {
                        warn!(error = %e, "Failed to watch workspace");
                        return;
                    }

                    info!("Started semantic search file watcher");

                    // Keep the watcher alive
                    loop {
                        std::thread::park();
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to create debouncer");
                }
            }
        });

        // Spawn batch processor
        tokio::spawn(async move {
            Self::run_batch_processor(
                config,
                workspace_for_processor,
                store,
                processor,
                enricher,
                state,
                chunk_config,
                change_rx,
                shutdown_rx,
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
