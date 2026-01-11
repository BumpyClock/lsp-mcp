// ABOUTME: Codemap module for graph-based code intelligence.
// ABOUTME: Provides codebase structure, dependency, and impact analysis.

pub mod cache;
pub mod indexer;
pub mod query;
pub mod store;
pub mod types;
pub mod watcher;

pub use cache::{GraphCache, TraversalDirection};
pub use indexer::{CodemapIndexer, IndexStats, IndexerError, IndexerState};
pub use query::{execute_query, QueryError};
pub use store::{CodemapStore, CodemapStoreError};
pub use types::*;
pub use watcher::CodemapWatcher;

use crate::lsp::manager::Manager;
use parking_lot::RwLock as ParkingLotRwLock;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum CodemapError {
    #[error("Store error: {0}")]
    StoreError(#[from] CodemapStoreError),
    #[error("Indexer error: {0}")]
    IndexerError(#[from] IndexerError),
    #[error("Query error: {0}")]
    QueryError(#[from] QueryError),
    #[error("Not ready: {0}")]
    NotReady(String),
}

/// Codemap state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CodemapState {
    Disabled,
    Initializing,
    Indexing { progress: f32 },
    Ready,
    Updating,
    Error,
}

/// Codemap manager - orchestrates the codemap feature
pub struct CodemapManager {
    store: Arc<CodemapStore>,
    cache: Arc<RwLock<GraphCache>>,
    indexer: Arc<CodemapIndexer>,
    state: Arc<ParkingLotRwLock<CodemapState>>,
    state_tx: broadcast::Sender<CodemapState>,
}

impl CodemapManager {
    /// Create a new codemap manager
    pub async fn new(db_path: &Path, manager: Arc<Manager>) -> Result<Self, CodemapError> {
        info!("Initializing codemap manager");

        let store = Arc::new(CodemapStore::new(db_path).await?);
        let cache = Arc::new(RwLock::new(GraphCache::new()));
        let indexer = Arc::new(CodemapIndexer::new(Arc::clone(&store), manager));
        let (state_tx, _) = broadcast::channel(16);

        Ok(Self {
            store,
            cache,
            indexer,
            state: Arc::new(ParkingLotRwLock::new(CodemapState::Initializing)),
            state_tx,
        })
    }

    /// Get current state
    pub fn state(&self) -> CodemapState {
        *self.state.read()
    }

    /// Subscribe to state changes
    pub fn subscribe_state(&self) -> broadcast::Receiver<CodemapState> {
        self.state_tx.subscribe()
    }

    /// Initialize and build the index
    pub async fn initialize(&self) -> Result<(), CodemapError> {
        self.set_state(CodemapState::Initializing);

        // Run full index
        self.set_state(CodemapState::Indexing { progress: 0.0 });
        let stats = self.indexer.full_index().await?;
        info!(
            "Codemap indexed {} files, {} symbols, {} edges",
            stats.files, stats.symbols, stats.edges
        );

        // Load into cache
        self.load_cache().await?;

        self.set_state(CodemapState::Ready);
        Ok(())
    }

    /// Load data from store into cache
    async fn load_cache(&self) -> Result<(), CodemapError> {
        let files = self.store.get_all_files().await?;
        let symbols = self.store.get_all_symbols().await?;

        let cache = self.cache.write().await;
        cache.clear();

        // Insert file nodes
        for file in files {
            cache.insert_node(Node::File(file));
        }

        // Insert symbol nodes
        for symbol in symbols {
            cache.insert_node(Node::Symbol(symbol));
        }

        // Load edges would go here
        // For now, edges are loaded on-demand from store

        Ok(())
    }

    /// Execute a query
    pub async fn query(&self, query: CodemapQuery) -> Result<CodemapResponse, CodemapError> {
        if self.state() != CodemapState::Ready {
            return Err(CodemapError::NotReady("Codemap not ready".to_string()));
        }

        let cache = self.cache.read().await;
        let response = execute_query(&cache, &self.store, query).await?;
        Ok(response)
    }

    /// Re-index a single file (for incremental updates)
    pub async fn update_file(&self, path: &str) -> Result<(), CodemapError> {
        let previous_state = self.state();
        self.set_state(CodemapState::Updating);

        match self.indexer.index_file(path).await {
            Ok(stats) => {
                info!(
                    "Updated codemap for {}: {} symbols, {} edges",
                    path, stats.symbols, stats.edges
                );

                // Reload affected data into cache
                let symbols = self.store.get_symbols_in_file(path).await?;
                let cache = self.cache.write().await;
                cache.remove_file_data(path);
                for symbol in symbols {
                    cache.insert_node(Node::Symbol(symbol));
                }
            }
            Err(e) => {
                warn!("Failed to update codemap for {}: {}", path, e);
            }
        }

        self.set_state(previous_state);
        Ok(())
    }

    /// Remove a file from the index
    pub async fn remove_file(&self, path: &str) -> Result<(), CodemapError> {
        self.indexer.remove_file(path).await?;
        self.cache.write().await.remove_file_data(path);
        Ok(())
    }

    /// Get stats
    pub async fn get_stats(&self) -> Result<(u32, u32, u32), CodemapError> {
        Ok(self.store.get_stats().await?)
    }

    fn set_state(&self, state: CodemapState) {
        *self.state.write() = state;
        let _ = self.state_tx.send(state);
    }
}
