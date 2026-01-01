// ABOUTME: Semantic search module for vector-based code search.
// ABOUTME: Provides indexing, embedding, and search capabilities for code.

pub mod chunker;
pub mod embedder;
mod enrichment;
mod indexer;
pub mod manager;
pub mod vector_store;
mod watcher;

pub use manager::{
    SemanticSearchError,
    SemanticSearchHealthSnapshot,
    SemanticSearchManager,
    SemanticSearchState,
};
pub use vector_store::{IndexStats, SearchResult};
