// ABOUTME: Client for parsing files and extracting symbols/references using tree-sitter
// ABOUTME: Includes mtime-based caching to avoid redundant parsing

use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tree_sitter::{Parser, Query, QueryCursor, Tree};

use super::filters;
use super::query_registry::{QueryRegistry, QueryType};
use super::types::{
    AstGrepMatch, AstGrepPosition, AstGrepRange, ByteOffset, CharCount, MetaVariable,
    MetaVariables, MultiVariables, SingleVariable,
};
use crate::shared::languages;

#[derive(Clone)]
struct CacheEntry {
    mtime: SystemTime,
    symbols: Vec<AstGrepMatch>,
    identifiers: Vec<AstGrepMatch>,
    references: Vec<AstGrepMatch>,
}

/// Client for tree-sitter parsing with mtime-based caching
///
/// Caches parse results keyed by file path and invalidates
/// when file modification time changes.
pub struct AstGrepClient {
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl AstGrepClient {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_symbol_match_from_position(
        &self,
        file_name: &str,
        identifier_position: &lsp_types::Position,
    ) -> Result<AstGrepMatch, Box<dyn std::error::Error>> {
        let entry = self.parse_file(file_name).await?;
        match select_symbol_match(entry.symbols, identifier_position) {
            Some(matched_symbol) => Ok(matched_symbol),
            None => Err(Box::new(Error::new(
                ErrorKind::NotFound,
                "No symbol found for position",
            ))),
        }
    }

    pub async fn get_file_symbols(
        &self,
        file_name: &str,
    ) -> Result<Vec<AstGrepMatch>, Box<dyn std::error::Error>> {
        let entry = self.parse_file(file_name).await?;
        Ok(entry.symbols)
    }

    pub async fn get_file_identifiers(
        &self,
        file_name: &str,
    ) -> Result<Vec<AstGrepMatch>, Box<dyn std::error::Error>> {
        let entry = self.parse_file(file_name).await?;
        Ok(entry.identifiers)
    }

    pub async fn get_symbol_and_references(
        &self,
        file_name: &str,
        position: &lsp_types::Position,
        full_scan: bool,
    ) -> Result<(AstGrepMatch, Vec<AstGrepMatch>), Box<dyn std::error::Error>> {
        let symbol_match = self
            .get_symbol_match_from_position(file_name, position)
            .await?;
        let references = self
            .get_references_contained_in_symbol_match(file_name, &symbol_match, full_scan)
            .await?;
        Ok((symbol_match, references))
    }

    pub async fn get_references_contained_in_symbol_match(
        &self,
        file_name: &str,
        symbol_match: &AstGrepMatch,
        full_scan: bool,
    ) -> Result<Vec<AstGrepMatch>, Box<dyn std::error::Error>> {
        let entry = self.parse_file(file_name).await?;

        let contained_references = entry
            .references
            .into_iter()
            .filter(|m| {
                let contained = symbol_match.contains(m);
                let all_ref = m.rule_id == "all-references";

                contained && ((full_scan && all_ref) || (!full_scan && !all_ref))
            })
            .collect();

        Ok(contained_references)
    }

    async fn parse_file(&self, file_name: &str) -> Result<CacheEntry, Box<dyn std::error::Error>> {
        let mtime = tokio::fs::metadata(file_name)
            .await
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(file_name) {
                if entry.mtime == mtime {
                    return Ok(entry.clone());
                }
            }
        }

        let source = tokio::fs::read(file_name).await?;
        let file_path = Path::new(file_name);

        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "No file extension"))?;

        let lang = languages::from_extension(extension).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "Unsupported language for extension: {}. Supported: rs, ts, tsx, js, jsx, py, go, java, c, cpp, h, hpp, cs, rb, php, md",
                    extension
                ),
            )
        })?;

        let query_lang = lang.query_language();
        let ts_lang = lang.tree_sitter_language();

        let source_clone = source.clone();
        let tree = tokio::task::spawn_blocking(move || {
            let mut parser = Parser::new();
            parser
                .set_language(&ts_lang)
                .expect("Failed to set language");
            parser
                .parse(&source_clone, None)
                .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Failed to parse file"))
        })
        .await??;

        let registry = QueryRegistry::global();

        let symbols = self.execute_query(
            &tree,
            &source,
            file_name,
            query_lang,
            &lang,
            registry.get_query(query_lang, QueryType::Symbol),
            QueryType::Symbol,
        )?;

        let identifiers = self.execute_query(
            &tree,
            &source,
            file_name,
            query_lang,
            &lang,
            registry.get_query(query_lang, QueryType::Identifier),
            QueryType::Identifier,
        )?;

        let references = self.execute_query(
            &tree,
            &source,
            file_name,
            query_lang,
            &lang,
            registry.get_query(query_lang, QueryType::Reference),
            QueryType::Reference,
        )?;

        let entry = CacheEntry {
            mtime,
            symbols,
            identifiers,
            references,
        };

        {
            let mut cache = self.cache.write().await;
            cache.insert(file_name.to_string(), entry.clone());
        }

        Ok(entry)
    }

    fn execute_query(
        &self,
        tree: &Tree,
        source: &[u8],
        file_name: &str,
        query_lang: &str,
        lang: &languages::ProgrammingLanguage,
        query: Option<&Query>,
        query_type: QueryType,
    ) -> Result<Vec<AstGrepMatch>, Box<dyn std::error::Error>> {
        let query = match query {
            Some(q) => q,
            None => return Ok(vec![]),
        };

        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(query, tree.root_node(), source);

        let mut results = Vec::new();
        let source_str = std::str::from_utf8(source)?;

        for m in matches {
            let mut name_node = None;
            let mut definition_node = None;
            let mut rule_id = String::new();

            for capture in m.captures {
                let capture_name = &query.capture_names()[capture.index as usize];

                if *capture_name == "name" {
                    name_node = Some(capture.node);
                } else if let Some(suffix) = capture_name.strip_prefix("definition.") {
                    definition_node = Some(capture.node);
                    rule_id = suffix.to_string();
                } else if *capture_name == "identifier" {
                    name_node = Some(capture.node);
                    definition_node = Some(capture.node);
                    rule_id = "all-identifiers".to_string();
                } else if let Some(suffix) = capture_name.strip_prefix("reference.") {
                    if name_node.is_none() {
                        name_node = Some(capture.node);
                    }
                    definition_node = Some(capture.node);
                    rule_id = suffix.to_string();
                }
            }

            let name_node = match name_node {
                Some(n) => n,
                None => continue,
            };

            let def_node = definition_node.unwrap_or(name_node);

            if query_type == QueryType::Reference {
                if filters::is_inside_definition(name_node, query_lang) {
                    continue;
                }
                if filters::is_inside_import(name_node, query_lang) {
                    continue;
                }
                if filters::is_assignment_target(name_node, query_lang) {
                    continue;
                }
                if filters::is_property_key(name_node, query_lang) {
                    continue;
                }
                if query_lang == "tsx" && filters::is_jsx_html_element(name_node, source) {
                    continue;
                }
            }

            let name_text = name_node.utf8_text(source).unwrap_or("").to_string();
            let def_text = def_node.utf8_text(source).unwrap_or("").to_string();

            let name_range = node_to_range(name_node);
            let def_range = node_to_range(def_node);

            let start_line = def_node.start_position().row;
            let end_line = def_node.end_position().row;
            let lines: Vec<&str> = source_str
                .lines()
                .skip(start_line)
                .take(end_line - start_line + 1)
                .collect();
            let lines_text = lines.join("\n");

            let ast_match = AstGrepMatch {
                text: name_text.clone(),
                range: def_range.clone(),
                file: file_name.to_string(),
                lines: lines_text,
                // TODO: Implement leading/trailing char counts for context display
                char_count: CharCount {
                    leading: 0,
                    trailing: 0,
                },
                language: lang.name().to_string(),
                meta_variables: MetaVariables {
                    single: SingleVariable {
                        name: MetaVariable {
                            text: name_text,
                            range: name_range.clone(),
                        },
                        context: if def_range != name_range {
                            Some(MetaVariable {
                                text: def_text,
                                range: def_range.clone(),
                            })
                        } else {
                            None
                        },
                    },
                    multi: MultiVariables { secondary: None },
                },
                rule_id,
                labels: None,
            };

            results.push(ast_match);
        }

        results.sort_by_key(|m| m.get_identifier_range().start.line);

        Ok(results)
    }

    /// Removes cache entries for a specific file (call when file changes)
    #[allow(dead_code)]
    pub async fn invalidate_file(&self, file_name: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(file_name);
    }

    /// Clears entire cache
    #[allow(dead_code)]
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

fn node_to_range(node: tree_sitter::Node) -> AstGrepRange {
    AstGrepRange {
        byte_offset: ByteOffset {
            start: node.start_byte(),
            end: node.end_byte(),
        },
        start: AstGrepPosition {
            line: node.start_position().row as u32,
            column: node.start_position().column as u32,
        },
        end: AstGrepPosition {
            line: node.end_position().row as u32,
            column: node.end_position().column as u32,
        },
    }
}

fn select_symbol_match(
    file_symbols: Vec<AstGrepMatch>,
    position: &lsp_types::Position,
) -> Option<AstGrepMatch> {
    let mut containing = Vec::new();
    let mut nearest: Option<(AstGrepMatch, u64, u64)> = None;

    for symbol in file_symbols {
        let context_range = symbol.get_context_range();
        if range_contains_position(&context_range, position) {
            containing.push(symbol);
            continue;
        }

        let distance = range_distance(&context_range, position);
        let span_score = range_span_score(&context_range);
        match &nearest {
            None => nearest = Some((symbol, distance, span_score)),
            Some((_, best_distance, best_span)) => {
                if distance < *best_distance
                    || (distance == *best_distance && span_score < *best_span)
                {
                    nearest = Some((symbol, distance, span_score));
                }
            }
        }
    }

    if !containing.is_empty() {
        containing.sort_by_key(|symbol| range_span_score(&symbol.get_context_range()));
        return containing.into_iter().next();
    }

    nearest.map(|(symbol, _, _)| symbol)
}

fn range_contains_position(range: &AstGrepRange, position: &lsp_types::Position) -> bool {
    let start = &range.start;
    let end = &range.end;

    (start.line < position.line
        || (start.line == position.line && start.column <= position.character))
        && (end.line > position.line || (end.line == position.line && end.column >= position.character))
}

fn range_span_score(range: &AstGrepRange) -> u64 {
    let line_span = range.end.line.saturating_sub(range.start.line) as u64;
    let col_span = range.end.column.saturating_sub(range.start.column) as u64;
    line_span * 1_000_000 + col_span
}

fn range_distance(range: &AstGrepRange, position: &lsp_types::Position) -> u64 {
    let start = &range.start;
    let end = &range.end;

    if position.line < start.line {
        let line = (start.line - position.line) as u64;
        let col = if position.line == start.line {
            start.column.saturating_sub(position.character) as u64
        } else {
            start.column as u64
        };
        return line * 1_000_000 + col;
    }

    if position.line > end.line {
        let line = (position.line - end.line) as u64;
        let col = if position.line == end.line {
            position.character.saturating_sub(end.column) as u64
        } else {
            position.character as u64
        };
        return line * 1_000_000 + col;
    }

    if position.character < start.column {
        return (start.column - position.character) as u64;
    }

    if position.character > end.column {
        return (position.character - end.column) as u64;
    }

    0
}

impl PartialEq for AstGrepRange {
    fn eq(&self, other: &Self) -> bool {
        self.byte_offset == other.byte_offset
            && self.start.line == other.start.line
            && self.start.column == other.start.column
            && self.end.line == other.end.line
            && self.end.column == other.end.column
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_grep::types::{
        AstGrepPosition, ByteOffset, CharCount, MetaVariable, MetaVariables, MultiVariables,
        SingleVariable,
    };

    fn make_range(start_line: u32, start_col: u32, end_line: u32, end_col: u32) -> AstGrepRange {
        AstGrepRange {
            byte_offset: ByteOffset { start: 0, end: 0 },
            start: AstGrepPosition {
                line: start_line,
                column: start_col,
            },
            end: AstGrepPosition {
                line: end_line,
                column: end_col,
            },
        }
    }

    fn make_match(
        name: &str,
        name_range: (u32, u32, u32, u32),
        context_range: Option<(u32, u32, u32, u32)>,
    ) -> AstGrepMatch {
        let name_range = make_range(name_range.0, name_range.1, name_range.2, name_range.3);
        let context_range =
            context_range.map(|range| make_range(range.0, range.1, range.2, range.3));
        let context_var = context_range.clone().map(|range| MetaVariable {
            text: "context".to_string(),
            range,
        });

        AstGrepMatch {
            text: name.to_string(),
            range: context_range.clone().unwrap_or_else(|| name_range.clone()),
            file: "test.rs".to_string(),
            lines: String::new(),
            char_count: CharCount {
                leading: 0,
                trailing: 0,
            },
            language: "rust".to_string(),
            meta_variables: MetaVariables {
                single: SingleVariable {
                    name: MetaVariable {
                        text: name.to_string(),
                        range: name_range,
                    },
                    context: context_var,
                },
                multi: MultiVariables { secondary: None },
            },
            rule_id: "function".to_string(),
            labels: None,
        }
    }

    #[test]
    fn select_symbol_match_prefers_smallest_container() {
        let outer = make_match("outer", (1, 0, 1, 5), Some((1, 0, 10, 0)));
        let inner = make_match("inner", (3, 0, 3, 5), Some((3, 0, 4, 0)));
        let position = lsp_types::Position {
            line: 3,
            character: 2,
        };

        let selected =
            select_symbol_match(vec![outer, inner], &position).expect("expected match");

        assert_eq!(selected.meta_variables.single.name.text, "inner");
    }

    #[test]
    fn select_symbol_match_uses_context_range() {
        let symbol = make_match("symbol", (1, 0, 1, 5), Some((1, 0, 5, 0)));
        let position = lsp_types::Position {
            line: 3,
            character: 4,
        };

        let selected = select_symbol_match(vec![symbol], &position).expect("expected match");

        assert_eq!(selected.meta_variables.single.name.text, "symbol");
    }

    #[tokio::test]
    async fn test_get_file_symbols_parses_rust_file() {
        let client = AstGrepClient::new();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let test_file = manifest_dir.join("src/ast_grep/client.rs");
        let test_file = test_file.to_str().expect("Invalid path");

        let result = client.get_file_symbols(test_file).await;

        assert!(
            result.is_ok(),
            "Should parse Rust file without error: {:?}",
            result.err()
        );
        let symbols = result.unwrap();
        assert!(
            !symbols.is_empty(),
            "Should find symbols in the client.rs file"
        );
    }

    #[tokio::test]
    async fn test_get_file_identifiers_parses_rust_file() {
        let client = AstGrepClient::new();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let test_file = manifest_dir.join("src/ast_grep/client.rs");
        let test_file = test_file.to_str().expect("Invalid path");

        let result = client.get_file_identifiers(test_file).await;

        assert!(
            result.is_ok(),
            "Should parse Rust file without error: {:?}",
            result.err()
        );
        let identifiers = result.unwrap();
        assert!(
            !identifiers.is_empty(),
            "Should find identifiers in the client.rs file"
        );
    }

    #[tokio::test]
    async fn test_cache_returns_same_results_on_second_call() {
        let client = AstGrepClient::new();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let test_file = manifest_dir.join("src/ast_grep/client.rs");
        let test_file = test_file.to_str().expect("Invalid path");

        let first = client.get_file_symbols(test_file).await.unwrap();
        let second = client.get_file_symbols(test_file).await.unwrap();

        assert_eq!(
            first.len(),
            second.len(),
            "Cache should return consistent results"
        );
    }

    #[tokio::test]
    async fn test_unsupported_extension_returns_error() {
        let client = AstGrepClient::new();
        let test_file = "/tmp/test.xyz";

        std::fs::write(test_file, "some content").unwrap();
        let result = client.get_file_symbols(test_file).await;
        std::fs::remove_file(test_file).ok();

        assert!(result.is_err(), "Should return error for unsupported extension");
    }

    #[tokio::test]
    async fn test_invalidate_file_clears_cache() {
        let client = AstGrepClient::new();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let test_file = manifest_dir.join("src/ast_grep/client.rs");
        let test_file = test_file.to_str().expect("Invalid path");

        client.get_file_symbols(test_file).await.unwrap();
        client.invalidate_file(test_file).await;

        let cache = client.cache.read().await;
        assert!(
            !cache.contains_key(test_file),
            "Cache should be cleared after invalidation"
        );
    }

    #[tokio::test]
    async fn test_clear_cache_removes_all_entries() {
        let client = AstGrepClient::new();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let test_file = manifest_dir.join("src/ast_grep/client.rs");
        let test_file = test_file.to_str().expect("Invalid path");

        client.get_file_symbols(test_file).await.unwrap();
        client.clear_cache().await;

        let cache = client.cache.read().await;
        assert!(cache.is_empty(), "Cache should be empty after clear");
    }

    #[tokio::test]
    async fn test_get_symbol_and_references_returns_symbol_with_references() {
        let client = AstGrepClient::new();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let test_file = manifest_dir.join("src/ast_grep/client.rs");
        let test_file = test_file.to_str().expect("Invalid path");

        let position = lsp_types::Position {
            line: 40,
            character: 10,
        };
        let result = client.get_symbol_and_references(test_file, &position, false).await;

        assert!(
            result.is_ok(),
            "Should return symbol and references: {:?}",
            result.err()
        );
        let (symbol, _references) = result.unwrap();
        assert!(
            !symbol.text.is_empty(),
            "Symbol should have non-empty text"
        );
    }

    #[tokio::test]
    async fn test_get_references_contained_in_symbol_match_filters_by_position() {
        let client = AstGrepClient::new();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let test_file = manifest_dir.join("src/ast_grep/client.rs");
        let test_file = test_file.to_str().expect("Invalid path");

        let symbols = client.get_file_symbols(test_file).await.unwrap();
        assert!(!symbols.is_empty(), "Should have symbols to test");

        let symbol = &symbols[0];
        let result = client
            .get_references_contained_in_symbol_match(test_file, symbol, false)
            .await;

        assert!(
            result.is_ok(),
            "Should return references: {:?}",
            result.err()
        );
    }
}
