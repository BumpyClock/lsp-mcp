// ABOUTME: File watcher for incremental codemap updates.
// ABOUTME: Batches file changes with configurable debouncing for efficiency.

use super::CodemapManager;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebouncedEvent, Debouncer};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

/// Debounce duration for file change batching.
const DEBOUNCE_DURATION: Duration = Duration::from_secs(2);

/// Batch processing interval.
const BATCH_INTERVAL: Duration = Duration::from_secs(5);

/// Codemap file watcher.
pub struct CodemapWatcher {
    workspace_root: PathBuf,
    manager: Arc<CodemapManager>,
    shutdown_rx: broadcast::Receiver<()>,
    /// Channel for file change events
    change_tx: mpsc::Sender<PathBuf>,
    change_rx: Option<mpsc::Receiver<PathBuf>>,
    /// File watcher handle
    _debouncer: Option<Debouncer<RecommendedWatcher>>,
}

impl CodemapWatcher {
    /// Create a new file watcher.
    pub fn new(
        workspace_root: PathBuf,
        manager: Arc<CodemapManager>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        let (change_tx, change_rx) = mpsc::channel(1000);

        Self {
            workspace_root,
            manager,
            shutdown_rx,
            change_tx,
            change_rx: Some(change_rx),
            _debouncer: None,
        }
    }

    /// Start watching for file changes.
    pub fn start(mut self) {
        let workspace_root = self.workspace_root.clone();
        let manager = self.manager.clone();
        let shutdown_rx = self.shutdown_rx;
        let runtime = tokio::runtime::Handle::current();

        // Take ownership of the receiver
        let change_rx = self.change_rx.take().expect("change_rx already taken");
        let change_tx = self.change_tx.clone();

        // Start the notify watcher in a separate thread
        let workspace_for_watcher = workspace_root.clone();
        let workspace_for_processor = workspace_root.clone();

        std::thread::spawn(move || {
            let tx_clone = change_tx.clone();
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

                            // Skip external paths
                            if !Self::should_watch_file_static(path, &workspace_for_callback) {
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
                    if let Err(e) = debouncer.watcher().watch(&workspace_for_watcher, RecursiveMode::Recursive) {
                        warn!(error = %e, "Failed to watch workspace");
                        return;
                    }

                    info!("Started codemap file watcher");

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
                workspace_for_processor,
                manager,
                change_rx,
                shutdown_rx,
            )
            .await;
        });
    }

    /// Run the batch processor loop.
    async fn run_batch_processor(
        _workspace_root: PathBuf,
        manager: Arc<CodemapManager>,
        mut change_rx: mpsc::Receiver<PathBuf>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
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

                    debug!(files = file_count, "Processing file changes for codemap");

                    for path in files {
                        let path_str = path.to_string_lossy().to_string();

                        if path.exists() {
                            // File modified or created
                            if let Err(e) = manager.update_file(&path_str).await {
                                debug!(
                                    file = %path.display(),
                                    error = %e,
                                    "Failed to update file in codemap"
                                );
                            }
                        } else {
                            // File deleted
                            if let Err(e) = manager.remove_file(&path_str).await {
                                debug!(
                                    file = %path.display(),
                                    error = %e,
                                    "Failed to remove file from codemap"
                                );
                            }
                        }
                    }
                }

                // Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Codemap watcher shutting down");
                    break;
                }
            }
        }
    }

    /// Check if a file should be watched (static version for use in callback).
    fn should_watch_file_static(
        path: &PathBuf,
        workspace_root: &PathBuf,
    ) -> bool {
        let relative_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(path)
            .to_string_lossy();

        // Skip external dependency directories
        let external_patterns = [
            "node_modules/",
            ".cargo/registry/",
            "target/debug/",
            "target/release/",
            ".git/",
            "dist/",
            "build/",
            ".next/",
            "vendor/",
        ];

        for pattern in &external_patterns {
            if relative_path.contains(pattern) {
                return false;
            }
        }

        true
    }
}
