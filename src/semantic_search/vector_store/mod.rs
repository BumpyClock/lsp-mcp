// ABOUTME: Vector store module for persisting and searching code embeddings.
// ABOUTME: Uses HNSW for vector similarity and SQLite for metadata.

mod hnsw;
mod metadata;
pub mod types;

pub use hnsw::HnswVectorStore;
pub use types::{EnrichmentData, IndexEntry, IndexState, IndexStats, SearchOptions, SearchResult};

use async_trait::async_trait;
use std::path::Path;

/// Error type for vector store operations.
#[derive(Debug)]
pub enum VectorStoreError {
    /// IO error (file access, directory creation)
    IoError(std::io::Error),
    /// SQLite error
    DatabaseError(String),
    /// HNSW index error
    IndexError(String),
    /// Dimension mismatch
    DimensionMismatch { expected: usize, got: usize },
    /// Entry not found
    NotFound(String),
}

impl std::fmt::Display for VectorStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            Self::IndexError(msg) => write!(f, "Index error: {}", msg),
            Self::DimensionMismatch { expected, got } => {
                write!(f, "Dimension mismatch: expected {}, got {}", expected, got)
            }
            Self::NotFound(id) => write!(f, "Entry not found: {}", id),
        }
    }
}

impl std::error::Error for VectorStoreError {}

/// Trait for vector store implementations.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Add or update entries in the index.
    async fn upsert(&self, entries: Vec<(IndexEntry, Vec<f32>)>)
        -> Result<usize, VectorStoreError>;

    /// Remove entries by segment hash IDs.
    async fn delete(&self, ids: &[String]) -> Result<usize, VectorStoreError>;

    /// Remove all entries for a file path.
    async fn delete_file(&self, file_path: &str) -> Result<usize, VectorStoreError>;

    /// Search for similar vectors.
    async fn search(
        &self,
        query_embedding: &[f32],
        options: SearchOptions,
    ) -> Result<Vec<SearchResult>, VectorStoreError>;

    /// Get index statistics.
    async fn stats(&self) -> Result<IndexStats, VectorStoreError>;

    /// Check if a segment hash exists in the index.
    async fn contains(&self, segment_hash: &str) -> Result<bool, VectorStoreError>;

    /// Get all segment hashes for a file.
    async fn get_file_hashes(&self, file_path: &str) -> Result<Vec<String>, VectorStoreError>;

    /// Persist any in-memory state to disk.
    async fn flush(&self) -> Result<(), VectorStoreError>;

    /// Set the index state.
    async fn set_state(&self, state: IndexState) -> Result<(), VectorStoreError>;

    /// Get the current index state.
    async fn get_state(&self) -> Result<IndexState, VectorStoreError>;
}

/// Create a new vector store at the given path.
pub async fn create_store(
    index_dir: &Path,
    dimension: usize,
) -> Result<HnswVectorStore, VectorStoreError> {
    HnswVectorStore::new(index_dir, dimension).await
}
