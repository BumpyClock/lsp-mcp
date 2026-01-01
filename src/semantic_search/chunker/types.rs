// ABOUTME: Core types for semantic chunking of source code.
// ABOUTME: Defines CodeChunk, ChunkBoundary, and chunking configuration.

use serde::{Deserialize, Serialize};

/// A semantic code chunk extracted from source files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeChunk {
    /// Path to the source file (relative to workspace)
    pub file_path: String,
    /// The extracted code content
    pub code: String,
    /// 1-based start line (inclusive)
    pub start_line: u32,
    /// 1-based end line (inclusive)
    pub end_line: u32,
    /// Deterministic hash for deduplication and cache invalidation
    pub segment_hash: String,
    /// Optional symbol name if this chunk represents a definition
    pub symbol_name: Option<String>,
    /// Optional symbol kind (function, class, struct, etc.)
    pub symbol_kind: Option<String>,
}

/// Boundary type for semantic chunking decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkBoundary {
    /// A complete function/method definition
    Function,
    /// A class/struct/enum definition
    Type,
    /// An impl block (Rust) or class body
    Implementation,
    /// A module-level constant or static
    Constant,
    /// Import/use statements grouped together
    Imports,
    /// Top-level documentation/comments
    Documentation,
    /// Fallback line-based chunk
    Lines,
}

impl ChunkBoundary {
    /// Convert to a string representation for metadata.
    pub fn as_str(&self) -> &'static str {
        match self {
            ChunkBoundary::Function => "function",
            ChunkBoundary::Type => "type",
            ChunkBoundary::Implementation => "impl",
            ChunkBoundary::Constant => "constant",
            ChunkBoundary::Imports => "imports",
            ChunkBoundary::Documentation => "documentation",
            ChunkBoundary::Lines => "lines",
        }
    }
}

/// Configuration for chunking behavior.
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Maximum lines per chunk (default: 100)
    pub max_lines: u32,
    /// Minimum lines per chunk (default: 3)
    pub min_lines: u32,
    /// Minimum chunk size in characters (default: 50)
    pub min_chars: usize,
    /// Maximum chunk size in characters (default: 2000)
    pub max_chars: usize,
    /// Maximum chunk size for functions in characters (default: 5000)
    pub max_function_chars: usize,
    /// Overlap size between chunks in characters (default: 200)
    pub overlap_chars: usize,
    /// Include surrounding context lines (default: 2)
    pub context_lines: u32,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            max_lines: 100,
            min_lines: 3,
            min_chars: 50,
            max_chars: 2000,
            max_function_chars: 5000,
            overlap_chars: 200,
            context_lines: 2,
        }
    }
}

impl ChunkConfig {
    /// Create config from semantic search index config.
    pub fn from_index_config(
        min_chars: usize,
        max_chars: usize,
        max_function_chars: usize,
        overlap_chars: usize,
    ) -> Self {
        Self {
            min_chars,
            max_chars,
            max_function_chars,
            overlap_chars,
            ..Default::default()
        }
    }
}
