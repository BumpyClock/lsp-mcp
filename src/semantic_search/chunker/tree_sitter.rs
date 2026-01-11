// ABOUTME: Tree-sitter based semantic chunker for supported programming languages.
// ABOUTME: Extracts meaningful code boundaries like functions, classes, and modules.

use super::languages::{get_language, get_node_kinds, NodeKinds};
use super::types::{ChunkBoundary, ChunkConfig, CodeChunk};
use super::{compute_segment_hash, Chunker, ChunkerError};
use std::path::Path;
use tree_sitter::{Node, Parser, Tree};

/// Tree-sitter based chunker for semantic code extraction.
pub struct TreeSitterChunker {
    extension: String,
}

impl TreeSitterChunker {
    /// Create a new TreeSitterChunker for the given file extension.
    pub fn new(extension: &str) -> Self {
        Self {
            extension: extension.to_string(),
        }
    }

    fn parse_content(&self, content: &str) -> Result<Tree, ChunkerError> {
        let language = get_language(&self.extension)
            .ok_or_else(|| ChunkerError::UnsupportedLanguage(self.extension.clone()))?;

        let mut parser = Parser::new();
        parser
            .set_language(&language)
            .map_err(|e| ChunkerError::ParseError(e.to_string()))?;

        parser
            .parse(content, None)
            .ok_or_else(|| ChunkerError::ParseError("Failed to parse content".to_string()))
    }

    fn extract_chunks(
        &self,
        tree: &Tree,
        content: &str,
        file_path: &str,
        config: &ChunkConfig,
    ) -> Vec<CodeChunk> {
        let mut chunks = Vec::new();
        let node_kinds = get_node_kinds(&self.extension);
        let lines: Vec<&str> = content.lines().collect();

        self.walk_tree(
            tree.root_node(),
            content,
            file_path,
            config,
            &node_kinds,
            &lines,
            &mut chunks,
        );

        // Filter out chunks that are too small
        chunks.retain(|chunk| chunk.code.len() >= config.min_chars);

        chunks
    }

    fn walk_tree(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        config: &ChunkConfig,
        node_kinds: &NodeKinds,
        lines: &[&str],
        chunks: &mut Vec<CodeChunk>,
    ) {
        let kind = node.kind();

        // Check if this node represents a semantic boundary
        if let Some(boundary) = node_kinds.classify(kind) {
            let start_line = node.start_position().row as u32 + 1;
            let end_line = node.end_position().row as u32 + 1;
            let line_count = end_line - start_line + 1;

            // Extract symbol name if available
            let symbol_name = self.extract_symbol_name(&node, content);
            let symbol_kind = boundary.as_str().to_string();
            let doc_comment = extract_leading_doc(lines, start_line);

            let ctx_start = start_line.saturating_sub(config.context_lines).max(1);
            let ctx_end = (end_line + config.context_lines).min(lines.len() as u32);
            let mut code_start = ctx_start;
            let mut code_end = ctx_end;
            let base_code: String = lines[(ctx_start as usize - 1)..(ctx_end as usize)].join("\n");
            let base_len = base_code.len();
            let is_function = matches!(boundary, ChunkBoundary::Function);
            let allow_oversize = is_function && base_len <= config.max_function_chars;
            let max_code_chars = if allow_oversize {
                config.max_function_chars
            } else {
                config.max_chars
            };

            if config.overlap_chars > 0 {
                let (expanded_start, expanded_end) = expand_with_overlap(
                    lines,
                    code_start,
                    code_end,
                    max_code_chars,
                    config.overlap_chars,
                );
                code_start = expanded_start;
                code_end = expanded_end;
            }

            let code: String = lines[(code_start as usize - 1)..(code_end as usize)].join("\n");
            let code_len = code.len();

            if line_count > config.max_lines && !allow_oversize {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i as u32) {
                        self.walk_tree(
                            child, content, file_path, config, node_kinds, lines, chunks,
                        );
                    }
                }
            } else if code_len <= max_code_chars {
                let segment_hash = compute_segment_hash(file_path, start_line, end_line, &code);

                chunks.push(CodeChunk {
                    file_path: file_path.to_string(),
                    code,
                    doc_comment,
                    summary: None,
                    tags: None,
                    start_line,
                    end_line,
                    segment_hash,
                    symbol_name,
                    symbol_kind: Some(symbol_kind),
                });
                return;
            }
        }

        // Recurse into children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                self.walk_tree(child, content, file_path, config, node_kinds, lines, chunks);
            }
        }
    }

    fn extract_symbol_name(&self, node: &Node, content: &str) -> Option<String> {
        // Look for identifier child node
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                let kind = child.kind();
                if kind.contains("identifier") || kind.contains("name") || kind == "identifier" {
                    let start = child.start_byte();
                    let end = child.end_byte();
                    if end <= content.len() {
                        return Some(content[start..end].to_string());
                    }
                }
            }
        }
        None
    }
}

fn extract_leading_doc(lines: &[&str], start_line: u32) -> Option<String> {
    if start_line <= 1 || lines.is_empty() {
        return None;
    }

    let mut idx = start_line.saturating_sub(2) as i32;
    if idx < 0 {
        return None;
    }

    let mut collected: Vec<&str> = Vec::new();
    let mut saw_marker = false;
    let mut in_block = false;

    while idx >= 0 {
        let line = lines[idx as usize];
        let trimmed = line.trim();
        if in_block {
            collected.push(line);
            if is_block_comment_start(trimmed) {
                saw_marker = true;
                in_block = false;
            }
            idx -= 1;
            continue;
        }

        if trimmed.is_empty() {
            break;
        }

        if is_block_comment_end(trimmed) && !is_block_comment_start(trimmed) {
            in_block = true;
            collected.push(line);
            idx -= 1;
            continue;
        }

        if is_block_comment_start(trimmed) {
            saw_marker = true;
            collected.push(line);
            idx -= 1;
            continue;
        }

        if is_line_comment_start(trimmed) {
            saw_marker = true;
            collected.push(line);
            idx -= 1;
            continue;
        }

        break;
    }

    if !saw_marker {
        return None;
    }

    collected.reverse();
    let doc = collected.join("\n");
    if doc.trim().is_empty() {
        None
    } else {
        Some(doc)
    }
}

fn is_line_comment_start(line: &str) -> bool {
    line.starts_with("//") || line.starts_with('#') || line.starts_with("--")
}

fn is_block_comment_start(line: &str) -> bool {
    line.starts_with("/*")
}

fn is_block_comment_end(line: &str) -> bool {
    line.ends_with("*/") || line.starts_with("*/")
}

fn range_len(lines: &[&str], start: usize, end: usize) -> usize {
    if end <= start {
        return 0;
    }
    let mut len = 0usize;
    for (idx, line) in lines[start..end].iter().enumerate() {
        len += line.len();
        if idx + 1 < end - start {
            len += 1;
        }
    }
    len
}

fn expand_with_overlap(
    lines: &[&str],
    start_line: u32,
    end_line: u32,
    max_chars: usize,
    overlap_chars: usize,
) -> (u32, u32) {
    if overlap_chars == 0 || lines.is_empty() {
        return (start_line, end_line);
    }

    let mut start_idx = (start_line.saturating_sub(1) as usize).min(lines.len());
    let mut end_idx = (end_line as usize).min(lines.len());
    let mut current_len = range_len(lines, start_idx, end_idx);
    if current_len >= max_chars {
        return (start_line, end_line);
    }

    let mut remaining = overlap_chars;
    let mut take_from_start = true;

    while remaining > 0 {
        let mut extended = false;

        if take_from_start && start_idx > 0 {
            let add_len = lines[start_idx - 1].len() + 1;
            if current_len + add_len <= max_chars {
                start_idx -= 1;
                current_len += add_len;
                remaining = remaining.saturating_sub(add_len);
                extended = true;
            }
        }

        if !extended && end_idx < lines.len() {
            let add_len = lines[end_idx].len() + 1;
            if current_len + add_len <= max_chars {
                end_idx += 1;
                current_len += add_len;
                remaining = remaining.saturating_sub(add_len);
                extended = true;
            }
        }

        if !extended {
            break;
        }

        take_from_start = !take_from_start;
    }

    let new_start = (start_idx + 1) as u32;
    let new_end = end_idx as u32;
    (new_start.max(1), new_end.max(new_start))
}

impl Chunker for TreeSitterChunker {
    fn chunk_file(
        &self,
        file_path: &Path,
        content: &str,
        config: &ChunkConfig,
    ) -> Result<Vec<CodeChunk>, ChunkerError> {
        let tree = self.parse_content(content)?;
        let path_str = file_path.to_string_lossy();
        Ok(self.extract_chunks(&tree, content, &path_str, config))
    }

    fn supports_extension(&self, extension: &str) -> bool {
        super::languages::is_supported(extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_chunking() {
        let chunker = TreeSitterChunker::new("rs");
        let content = r#"
fn hello_world() {
    println!("Hello, world!");
}

struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}
"#;

        let config = ChunkConfig::default();
        let chunks = chunker
            .chunk_file(Path::new("test.rs"), content, &config)
            .unwrap();

        assert!(!chunks.is_empty());
        // Should find functions and structs
        let has_function = chunks
            .iter()
            .any(|c| c.symbol_kind.as_deref() == Some("function"));
        let has_type = chunks
            .iter()
            .any(|c| c.symbol_kind.as_deref() == Some("type"));
        assert!(has_function || has_type);
    }

    #[test]
    fn test_segment_hash_stability() {
        let hash1 = compute_segment_hash("test.rs", 1, 10, "fn foo() {}");
        let hash2 = compute_segment_hash("test.rs", 1, 10, "fn foo() {}");
        assert_eq!(hash1, hash2);

        let hash3 = compute_segment_hash("test.rs", 1, 10, "fn bar() {}");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_large_function_allowed() {
        let chunker = TreeSitterChunker::new("rs");
        let mut body = String::new();
        for _ in 0..40 {
            body.push_str("    println!(\"hello\");\n");
        }
        let content = format!("fn big_function() {{\n{}}}\n", body);

        let mut config = ChunkConfig::default();
        config.max_chars = 120;
        config.max_function_chars = 5000;
        config.max_lines = 5;

        let chunks = chunker
            .chunk_file(Path::new("test.rs"), &content, &config)
            .unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].symbol_kind.as_deref(), Some("function"));
        assert!(chunks[0].code.len() > config.max_chars);
    }

    #[test]
    fn test_extract_leading_doc_from_block_comment() {
        let lines = vec!["/**", " * doc line", " */", "fn foo() {}"];
        let doc = extract_leading_doc(&lines, 4);
        assert_eq!(doc, Some("/**\n * doc line\n */".to_string()));
    }
}
