// ABOUTME: Types for vector storage and search operations.
// ABOUTME: Defines SearchResult, IndexEntry, and search configuration.

use serde::{Deserialize, Serialize};

/// Entry stored in the vector index with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Unique identifier (segment_hash)
    pub id: String,
    /// Relative file path
    pub file_path: String,
    /// The code chunk content
    pub code: String,
    /// 1-based start line
    pub start_line: u32,
    /// 1-based end line
    pub end_line: u32,
    /// Optional symbol name
    pub symbol_name: Option<String>,
    /// Optional symbol kind
    pub symbol_kind: Option<String>,
    /// File modification time when indexed
    pub indexed_at: i64,
}

/// Result from a semantic search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The matching index entry
    pub entry: IndexEntry,
    /// Cosine similarity score (0.0 to 1.0)
    pub score: f32,
    /// Rank in results (1-based)
    pub rank: u32,
}

/// Options for search queries.
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Maximum number of results to return
    pub limit: usize,
    /// Optional path prefix filter (e.g., "src/")
    pub path_prefix: Option<String>,
    /// Minimum similarity score threshold (0.0 to 1.0)
    pub min_score: Option<f32>,
    /// Filter by symbol kinds (e.g., ["function", "class"])
    pub symbol_kinds: Option<Vec<String>>,
}

impl SearchOptions {
    /// Create search options with a limit.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    /// Set minimum score threshold.
    pub fn min_score(mut self, score: f32) -> Self {
        self.min_score = Some(score);
        self
    }

    /// Set path prefix filter.
    pub fn path_prefix(mut self, prefix: String) -> Self {
        self.path_prefix = Some(prefix);
        self
    }
}

/// Statistics about the vector index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Total number of indexed chunks
    pub chunk_count: usize,
    /// Number of indexed files
    pub file_count: usize,
    /// Embedding dimension
    pub dimension: usize,
    /// Index size in bytes
    pub index_size_bytes: u64,
    /// Metadata database size in bytes
    pub metadata_size_bytes: u64,
    /// Last index update timestamp
    pub last_updated: i64,
}

/// Index state for tracking completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexState {
    /// Index does not exist or is empty
    Empty,
    /// Index is being built (incomplete)
    Building,
    /// Index is ready for search
    Ready,
}
