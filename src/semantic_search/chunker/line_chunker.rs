// ABOUTME: Fallback line-based chunker for unsupported file types.
// ABOUTME: Splits files by line count when tree-sitter parsing unavailable.

use super::types::{ChunkConfig, CodeChunk};
use super::{compute_segment_hash, Chunker, ChunkerError};
use std::path::Path;

/// Line-based chunker for unsupported file types.
pub struct LineChunker;

impl LineChunker {
    /// Create a new LineChunker.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LineChunker {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunker for LineChunker {
    fn chunk_file(
        &self,
        file_path: &Path,
        content: &str,
        config: &ChunkConfig,
    ) -> Result<Vec<CodeChunk>, ChunkerError> {
        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();
        let path_str = file_path.to_string_lossy();

        if lines.is_empty() {
            return Ok(chunks);
        }

        let mut start = 0usize;
        while start < lines.len() {
            let end = (start + config.max_lines as usize).min(lines.len());
            let chunk_lines = &lines[start..end];
            let code = chunk_lines.join("\n");

            // Skip chunks that are too small
            if code.len() >= config.min_chars {
                let start_line = (start + 1) as u32;
                let end_line = end as u32;

                let segment_hash = compute_segment_hash(&path_str, start_line, end_line, &code);

                chunks.push(CodeChunk {
                    file_path: path_str.to_string(),
                    code,
                    doc_comment: None,
                    summary: None,
                    tags: None,
                    start_line,
                    end_line,
                    segment_hash,
                    symbol_name: None,
                    symbol_kind: Some("lines".to_string()),
                });
            }

            let mut next_start = end;
            if config.overlap_chars > 0 && end < lines.len() {
                let mut chars = 0usize;
                let mut idx = end;
                while idx > start && chars < config.overlap_chars {
                    idx -= 1;
                    chars += lines[idx].len() + 1;
                }
                if idx > start {
                    next_start = idx;
                }
            }
            if next_start <= start {
                next_start = end;
            }
            start = next_start;
        }

        Ok(chunks)
    }

    fn supports_extension(&self, _extension: &str) -> bool {
        true // Fallback supports everything
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_chunking() {
        let chunker = LineChunker::new();
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";

        let mut config = ChunkConfig::default();
        config.max_lines = 2;
        config.min_chars = 5;

        let chunks = chunker
            .chunk_file(Path::new("test.txt"), content, &config)
            .unwrap();

        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.symbol_kind.as_deref() == Some("lines")));
    }

    #[test]
    fn test_empty_content() {
        let chunker = LineChunker::new();
        let config = ChunkConfig::default();
        let chunks = chunker
            .chunk_file(Path::new("empty.txt"), "", &config)
            .unwrap();

        assert!(chunks.is_empty());
    }
}
