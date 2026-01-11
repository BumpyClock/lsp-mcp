// ABOUTME: Chunker module for extracting semantic code chunks from source files.
// ABOUTME: Provides tree-sitter based chunking with fallback to line-based chunking.

pub mod languages;
mod line_chunker;
mod tree_sitter;
pub mod types;

pub use line_chunker::LineChunker;
pub use tree_sitter::TreeSitterChunker;
pub use types::{ChunkBoundary, ChunkConfig, CodeChunk};

use std::path::Path;

/// Error type for chunking operations.
#[derive(Debug)]
pub enum ChunkerError {
    /// File could not be read
    IoError(std::io::Error),
    /// Tree-sitter parsing failed
    ParseError(String),
    /// Unsupported file type
    UnsupportedLanguage(String),
}

impl std::fmt::Display for ChunkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
            Self::UnsupportedLanguage(ext) => write!(f, "Unsupported language: {}", ext),
        }
    }
}

impl std::error::Error for ChunkerError {}

impl From<std::io::Error> for ChunkerError {
    fn from(err: std::io::Error) -> Self {
        ChunkerError::IoError(err)
    }
}

/// Trait for code chunking implementations.
pub trait Chunker: Send + Sync {
    /// Extract semantic chunks from file content.
    fn chunk_file(
        &self,
        file_path: &Path,
        content: &str,
        config: &ChunkConfig,
    ) -> Result<Vec<CodeChunk>, ChunkerError>;

    /// Check if this chunker supports the given file extension.
    fn supports_extension(&self, extension: &str) -> bool;
}

/// Create a chunker for the given file extension.
/// Returns TreeSitterChunker for supported languages, LineChunker otherwise.
pub fn create_chunker(extension: &str) -> Box<dyn Chunker> {
    if languages::is_supported(extension) {
        Box::new(TreeSitterChunker::new(extension))
    } else {
        Box::new(LineChunker::new())
    }
}

/// Generate a deterministic segment hash for a code chunk.
/// Uses blake3 for speed and collision resistance.
pub fn compute_segment_hash(
    file_path: &str,
    start_line: u32,
    end_line: u32,
    content: &str,
) -> String {
    use blake3::Hasher;

    let mut hasher = Hasher::new();
    hasher.update(file_path.as_bytes());
    hasher.update(&start_line.to_le_bytes());
    hasher.update(&end_line.to_le_bytes());
    hasher.update(content.as_bytes());

    // Use first 16 bytes (128 bits) encoded as hex for compact but unique ID
    let hash = hasher.finalize();
    hex::encode(&hash.as_bytes()[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_chunker_rust() {
        let chunker = create_chunker("rs");
        assert!(chunker.supports_extension("rs"));
    }

    #[test]
    fn test_create_chunker_unknown() {
        let chunker = create_chunker("xyz");
        // LineChunker supports everything
        assert!(chunker.supports_extension("xyz"));
    }

    #[test]
    fn test_segment_hash_deterministic() {
        let hash1 = compute_segment_hash("test.rs", 1, 10, "fn foo() {}");
        let hash2 = compute_segment_hash("test.rs", 1, 10, "fn foo() {}");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_segment_hash_different_content() {
        let hash1 = compute_segment_hash("test.rs", 1, 10, "fn foo() {}");
        let hash2 = compute_segment_hash("test.rs", 1, 10, "fn bar() {}");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_segment_hash_different_lines() {
        let hash1 = compute_segment_hash("test.rs", 1, 10, "fn foo() {}");
        let hash2 = compute_segment_hash("test.rs", 1, 11, "fn foo() {}");
        assert_ne!(hash1, hash2);
    }
}
