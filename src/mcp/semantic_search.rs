// ABOUTME: MCP tool handler for semantic code search.
// ABOUTME: Provides natural language search over indexed code chunks.

use crate::markdown_formatter::ToMarkdown;
use crate::mcp_response::{tool_result_error, tool_result_success};
use crate::semantic_search::{
    SearchResult, SemanticSearchError, SemanticSearchManager, SemanticSearchState,
};
use glob::Pattern;
use rmcp::model::CallToolResult;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Semantic search tool response.
pub struct SemanticSearchResponse {
    pub results: Vec<SemanticSearchDisplayResult>,
}

pub struct SemanticSearchDisplayResult {
    pub result: SearchResult,
    pub embed_score: f32,
    pub lexical_score: f32,
    pub snippet_truncated: bool,
    pub snippet_total_lines: usize,
}

#[allow(dead_code)]
pub struct SemanticSearchFilters {
    pub path: Option<String>,
    pub file_pattern: Vec<String>,
    pub exclude: Vec<String>,
    pub min_score: Option<f32>,
    pub per_file: bool,
    pub rerank: bool,
    pub context_lines: Option<u32>,
}

impl ToMarkdown for SemanticSearchResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        if self.results.is_empty() {
            output.push_str("No results found.\n");
            return output;
        }

        output.push_str(&format!("**Found**: {} results\n\n", self.results.len()));

        for result in &self.results {
            output.push_str(&format!(
                "## {}. {}:{}-{} (score: {:.2})\n",
                result.result.rank,
                result.result.entry.file_path,
                result.result.entry.start_line,
                result.result.entry.end_line,
                result.result.score
            ));

            // Add symbol info if available
            if let Some(ref name) = result.result.entry.symbol_name {
                if let Some(ref kind) = result.result.entry.symbol_kind {
                    output.push_str(&format!("**Symbol**: {} `{}`\n", kind, name));
                } else {
                    output.push_str(&format!("**Symbol**: `{}`\n", name));
                }
            }

            output.push_str(&format!(
                "**Embedding**: {:.2}  **Keywords**: {:.2}\n",
                result.embed_score, result.lexical_score
            ));

            // Detect language from file extension for code fence
            let lang = result
                .result
                .entry
                .file_path
                .rsplit('.')
                .next()
                .map(|ext| match ext {
                    "rs" => "rust",
                    "py" => "python",
                    "ts" | "tsx" => "typescript",
                    "js" | "jsx" => "javascript",
                    "go" => "go",
                    "java" => "java",
                    "cpp" | "cc" | "cxx" => "cpp",
                    "c" | "h" => "c",
                    "cs" => "csharp",
                    "rb" => "ruby",
                    "php" => "php",
                    "md" => "markdown",
                    _ => ext,
                })
                .unwrap_or("text");

            if !result.result.entry.code.is_empty() {
                output.push_str(&format!(
                    "**codeChunk**:\n```{}\n{}\n```\n",
                    lang, result.result.entry.code
                ));
                if result.snippet_truncated {
                    output.push_str(&format!(
                        "[truncated, {} total lines]\n",
                        result.snippet_total_lines
                    ));
                }
                output.push('\n');
            }
        }

        output
    }
}

struct SnippetInfo {
    text: Option<String>,
    truncated: bool,
    total_lines: usize,
}

fn tokenize_query(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut terms = Vec::new();
    for raw in query.split(|c: char| !c.is_alphanumeric()) {
        let term = raw.trim().to_ascii_lowercase();
        if term.len() < 2 {
            continue;
        }
        if seen.insert(term.clone()) {
            terms.push(term);
        }
    }
    terms
}

fn matched_terms(terms: &[String], code: &str, symbol_name: Option<&str>) -> Vec<String> {
    if terms.is_empty() {
        return Vec::new();
    }

    let code_lower = code.to_ascii_lowercase();
    let symbol_lower = symbol_name.map(|name| name.to_ascii_lowercase());
    terms
        .iter()
        .filter(|term| {
            code_lower.contains(term.as_str())
                || symbol_lower
                    .as_ref()
                    .map(|name| name.contains(term.as_str()))
                    .unwrap_or(false)
        })
        .cloned()
        .collect()
}

fn lexical_score(terms: &[String], matched: &[String]) -> f32 {
    if terms.is_empty() {
        return 0.0;
    }
    matched.len() as f32 / terms.len() as f32
}

fn truncate_code_lines(code: &str, max_lines: Option<u32>) -> SnippetInfo {
    let lines: Vec<&str> = code.lines().collect();
    let total_lines = lines.len();

    let max_lines = match max_lines {
        None => {
            return SnippetInfo {
                text: Some(code.to_string()),
                truncated: false,
                total_lines,
            }
        }
        Some(0) => {
            return SnippetInfo {
                text: None,
                truncated: false,
                total_lines,
            }
        }
        Some(value) => value as usize,
    };

    if total_lines <= max_lines {
        return SnippetInfo {
            text: Some(code.to_string()),
            truncated: false,
            total_lines,
        };
    }

    SnippetInfo {
        text: Some(lines[..max_lines].join("\n")),
        truncated: true,
        total_lines,
    }
}

fn compile_exclude_patterns(patterns: &[String]) -> Result<Vec<Pattern>, String> {
    let mut compiled = Vec::new();
    for pattern in patterns {
        let glob = Pattern::new(pattern).map_err(|_| {
            format!("Invalid exclude pattern: {}", pattern)
        })?;
        compiled.push(glob);
    }
    Ok(compiled)
}

fn dedupe_by_file(
    results: Vec<SemanticSearchDisplayResult>,
) -> Vec<SemanticSearchDisplayResult> {
    let mut best: HashMap<String, SemanticSearchDisplayResult> = HashMap::new();

    for result in results {
        let file = result.result.entry.file_path.clone();
        let replace = match best.get(&file) {
            Some(existing) => {
                result
                    .result
                    .score
                    .partial_cmp(&existing.result.score)
                    .unwrap_or(Ordering::Equal)
                    == Ordering::Greater
            }
            None => true,
        };

        if replace {
            best.insert(file, result);
        }
    }

    let mut values: Vec<SemanticSearchDisplayResult> = best.into_values().collect();
    values.sort_by(|a, b| {
        b.result
            .score
            .partial_cmp(&a.result.score)
            .unwrap_or(Ordering::Equal)
    });
    values
}

fn dedupe_by_segment(
    results: Vec<SemanticSearchDisplayResult>,
) -> Vec<SemanticSearchDisplayResult> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut deduped = Vec::new();

    for result in results {
        let segment_hash = result.result.entry.id.clone();
        if seen.insert(segment_hash) {
            deduped.push(result);
        }
    }

    deduped
}

/// Execute semantic search tool.
pub async fn semantic_search(
    manager: &Arc<RwLock<SemanticSearchManager>>,
    query: String,
    limit: Option<u32>,
    path: Option<String>,
    file_pattern: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    min_score: Option<f32>,
    per_file: Option<bool>,
    rerank: Option<bool>,
    context_lines: Option<u32>,
) -> CallToolResult {
    let manager = manager.read().await;
    let state = manager.state().await;
    let resolved_context_lines = context_lines.or_else(|| manager.default_context_lines());

    // Handle non-ready states gracefully
    match &state {
        SemanticSearchState::Disabled => {
            return tool_result_success(
                "Semantic search is disabled. Enable it in `.lsp-mcp.json`:\n\n```json\n{\n  \"tools\": {\n    \"enable\": [\"semanticSearch\"]\n  },\n  \"semantic_search\": {\n    \"enabled\": true,\n    \"embedder\": {\n      \"provider\": \"fastembed\"\n    }\n  }\n}\n```".to_string()
            );
        }
        SemanticSearchState::Initializing => {
            return tool_result_success(
                "# Semantic Search\n\n**Status**: Initializing...\n\nSemantic search is starting up. Please try again in a moment.".to_string()
            );
        }
        SemanticSearchState::Indexing {
            indexed_count,
            total_count,
        } => {
            let progress = if *total_count > 0 {
                (*indexed_count as f64 / *total_count as f64) * 100.0
            } else {
                0.0
            };

            return tool_result_success(format!(
                "# Indexing in Progress\n\n\
                **Progress**: {}/{} files ({:.1}%)\n\n\
                Semantic search will be available once indexing completes.\n\
                Use the `health` tool to check status.",
                indexed_count, total_count, progress
            ));
        }
        SemanticSearchState::Error { message } => {
            return tool_result_success(format!(
                "# Semantic Search Error\n\n**Error**: {}\n\n\
                This error may be recoverable. Try restarting the server.",
                message
            ));
        }
        SemanticSearchState::Ready { .. } | SemanticSearchState::Updating { .. } => {
            // Proceed with search
        }
    }

    let exclude_patterns = match exclude.as_ref() {
        Some(patterns) => match compile_exclude_patterns(patterns) {
            Ok(compiled) => compiled,
            Err(message) => return tool_result_error(message),
        },
        None => Vec::new(),
    };
    let include_patterns = match file_pattern.as_ref() {
        Some(patterns) => match compile_exclude_patterns(patterns) {
            Ok(compiled) => compiled,
            Err(message) => return tool_result_error(message),
        },
        None => Vec::new(),
    };
    let rerank = rerank.unwrap_or(false);
    let per_file = per_file.unwrap_or(false);

    match manager
        .search(&query, limit.map(|l| l as usize), path.clone())
        .await
    {
        Ok(results) => {
            let query_terms = tokenize_query(&query);
            let mut display_results = Vec::new();

            for result in results {
                // Apply exclude patterns
                if !exclude_patterns.is_empty()
                    && exclude_patterns
                        .iter()
                        .any(|pattern| pattern.matches(&result.entry.file_path))
                {
                    continue;
                }

                // Apply include patterns (file_pattern)
                if !include_patterns.is_empty()
                    && !include_patterns
                        .iter()
                        .any(|pattern| pattern.matches(&result.entry.file_path))
                {
                    continue;
                }

                // Apply min_score filter
                if let Some(min) = min_score {
                    if result.score < min {
                        continue;
                    }
                }

                let embed_score = result.score;
                let keyword_score = if rerank {
                    let matched = matched_terms(
                        &query_terms,
                        &result.entry.code,
                        result.entry.symbol_name.as_deref(),
                    );
                    lexical_score(&query_terms, &matched)
                } else {
                    0.0
                };
                let combined_score = if rerank {
                    (embed_score * 0.85) + (keyword_score * 0.15)
                } else {
                    embed_score
                };

                let snippet_info = truncate_code_lines(&result.entry.code, resolved_context_lines);
                let mut result = result;
                if let Some(snippet) = snippet_info.text {
                    result.entry.code = snippet;
                } else {
                    result.entry.code.clear();
                }
                result.score = combined_score;

                display_results.push(SemanticSearchDisplayResult {
                    result,
                    embed_score,
                    lexical_score: keyword_score,
                    snippet_truncated: snippet_info.truncated,
                    snippet_total_lines: snippet_info.total_lines,
                });
            }

            let display_results = dedupe_by_segment(display_results);

            let mut display_results = if per_file {
                dedupe_by_file(display_results)
            } else {
                display_results
            };

            display_results.sort_by(|a, b| {
                b.result
                    .score
                    .partial_cmp(&a.result.score)
                    .unwrap_or(Ordering::Equal)
            });

            for (idx, item) in display_results.iter_mut().enumerate() {
                item.result.rank = (idx + 1) as u32;
            }

            let response = SemanticSearchResponse {
                results: display_results,
            };

            tool_result_success(response.to_markdown())
        }
        Err(e) => match e {
            SemanticSearchError::Disabled => tool_result_success(
                "Semantic search is disabled. Enable it in `.lsp-mcp.json`.".to_string(),
            ),
            SemanticSearchError::IndexingInProgress { indexed, total } => {
                tool_result_success(format!(
                    "Indexing in progress: {}/{} files. Please wait.",
                    indexed, total
                ))
            }
            _ => tool_result_success(format!("Search failed: {}", e)),
        },
    }
}
