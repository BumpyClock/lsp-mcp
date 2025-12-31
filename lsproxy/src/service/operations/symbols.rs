// ABOUTME: Symbol lookup operations (find_identifier, list_files, workspace_symbol).
// ABOUTME: Handles workspace-wide symbol search and file listing.

use crate::api_types::{
    FilePosition, Identifier, Position, WorkspaceSymbolInfo, WorkspaceSymbolResponse,
};
use crate::ast_grep::types::AstGrepMatch;
use crate::lsp::manager::Manager;
use crate::utils::file_utils::{absolute_path_to_relative_path_string, uri_to_relative_path_string};
use log::debug;
use lsp_types::{Position as LspPosition, Range as LspRange};
use std::collections::HashSet;
use std::sync::Arc;

use crate::service::types::errors::{PositionError, ServiceError};
use crate::service::types::response::{McpIdentifierResponse, McpListFilesResponse};
use crate::service::utils::identifiers::find_identifier_at_position;
use crate::service::utils::pagination::paginate_items;
use crate::service::utils::signature::batch_hover_for_signatures;
use crate::service::utils::transformations::workspace_symbol_info_from_lsp;

/// Finds identifiers matching the given name in a file.
pub(crate) async fn find_identifier_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    name: &str,
    position: Option<Position>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<McpIdentifierResponse, ServiceError> {
    let file_identifiers = manager.get_file_identifiers(file_path).await?;
    let name_matched: Vec<Identifier> = file_identifiers
        .into_iter()
        .filter(|id| id.name == name)
        .collect();

    let identifiers = if name_matched.is_empty() {
        vec![]
    } else if let Some(position) = position {
        let lookup_position = FilePosition {
            path: file_path.to_string(),
            position,
        };
        match find_identifier_at_position(name_matched.clone(), &lookup_position).await {
            Ok(identifier) => vec![identifier],
            Err(PositionError::IdentifierNotFound { closest }) => closest,
        }
    } else {
        name_matched
    };
    let (identifiers, pagination) = paginate_items(identifiers, limit, offset);
    Ok(McpIdentifierResponse {
        identifiers,
        limit: pagination.limit,
        offset: pagination.offset,
        truncated: pagination.truncated,
    })
}

/// Lists all files tracked by the workspace.
pub(crate) async fn list_files_impl(
    manager: &Arc<Manager>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<McpListFilesResponse, ServiceError> {
    let files = manager.list_files().await?;
    let (files, pagination) = paginate_items(files, limit, offset);
    Ok(McpListFilesResponse {
        files,
        limit: pagination.limit,
        offset: pagination.offset,
        truncated: pagination.truncated,
    })
}

/// Searches for symbols across the workspace.
pub(crate) async fn workspace_symbol_impl(
    manager: &Arc<Manager>,
    query: &str,
    include_raw_response: bool,
    exact: bool,
    limit: Option<u32>,
    offset: Option<u32>,
    context_lines: u32,
) -> Result<WorkspaceSymbolResponse, ServiceError> {
    let symbols = manager.workspace_symbol(query).await?;
    debug!(
        "workspace_symbol_impl: LSP returned {} symbols for query '{}'",
        symbols.len(),
        query
    );

    let workspace_files = manager.list_files().await?;
    let workspace_len = workspace_files.len();
    let workspace_set: HashSet<String> = workspace_files.into_iter().collect();
    debug!(
        "workspace_symbol_impl: workspace has {} files",
        workspace_len
    );

    let mut filtered_symbols = Vec::new();
    let mut filtered_count = 0;
    for sym in symbols {
        let path = uri_to_relative_path_string(&sym.location.uri);
        if !workspace_set.is_empty() && !workspace_set.contains(&path) {
            if filtered_count < 5 {
                debug!(
                    "workspace_symbol_impl: filtered out '{}' at path '{}' (not in workspace files)",
                    sym.name, path
                );
            }
            filtered_count += 1;
            continue;
        }
        let info = workspace_symbol_info_from_lsp(sym, path);
        if let Some(info) = apply_query_match(query, exact, info) {
            filtered_symbols.push(info);
        }
    }

    if filtered_count > 0 {
        debug!(
            "workspace_symbol_impl: filtered out {} symbols (not in workspace files)",
            filtered_count
        );
    }
    debug!(
        "workspace_symbol_impl: returning {} symbols after filtering",
        filtered_symbols.len()
    );

    if filtered_symbols.is_empty() {
        debug!("workspace_symbol_impl: falling back to ast-grep symbol scan");
        let workspace_files_vec: Vec<String> = workspace_set.into_iter().collect();
        filtered_symbols = workspace_symbol_fallback(manager, &workspace_files_vec, query, exact).await;
        debug!(
            "workspace_symbol_impl: fallback returned {} symbols",
            filtered_symbols.len()
        );
    }

    // Fetch signatures for all filtered symbols in batch
    let positions: Vec<_> = filtered_symbols
        .iter()
        .map(|s| (s.location.path.as_str(), s.location.position.clone()))
        .collect();
    let signatures = batch_hover_for_signatures(manager, positions).await;
    for (symbol, sig) in filtered_symbols.iter_mut().zip(signatures.into_iter()) {
        symbol.signature = sig;
    }

    for symbol in &mut filtered_symbols {
        if let Some(line) =
            read_symbol_line(manager, &symbol.location.path, symbol.location.position.line).await
        {
            if is_reexport_line(&line) && !symbol.kind.contains("re-export") {
                symbol.kind = format!("{} (re-export)", symbol.kind);
            }
        }
    }

    filtered_symbols = dedupe_workspace_symbols(filtered_symbols);

    let raw_response = if include_raw_response {
        serde_json::to_value(&filtered_symbols).ok()
    } else {
        None
    };

    let (mut symbols, pagination) = paginate_items(filtered_symbols, limit, offset);

    if context_lines > 0 {
        attach_snippets_to_workspace_symbols(manager, &mut symbols, context_lines).await;
    }

    Ok(WorkspaceSymbolResponse {
        raw_response,
        symbols,
        limit: pagination.limit,
        offset: pagination.offset,
        truncated: pagination.truncated,
    })
}

async fn attach_snippets_to_workspace_symbols(
    manager: &Arc<Manager>,
    symbols: &mut [WorkspaceSymbolInfo],
    context_lines: u32,
) {
    use crate::api_types::{CodeContext, FileRange, Range};
    use lsp_types::Position as LspPosition;
    use lsp_types::Range as LspRange;

    for symbol in symbols.iter_mut() {
        let line = symbol.location.position.line;
        let start_line = line.saturating_sub(context_lines).max(1);
        let end_line = line.saturating_add(context_lines).max(1);

        let lsp_range = LspRange {
            start: LspPosition {
                line: start_line.saturating_sub(1),
                character: 0,
            },
            end: LspPosition {
                line: end_line,
                character: 0,
            },
        };

        match manager.read_source_code(&symbol.location.path, Some(lsp_range)).await {
            Ok(source_code) => {
                symbol.snippet = Some(CodeContext {
                    range: FileRange {
                        path: symbol.location.path.clone(),
                        range: Range {
                            start: crate::api_types::Position {
                                line: start_line,
                                character: 1,
                            },
                            end: crate::api_types::Position {
                                line: end_line,
                                character: 1,
                            },
                        },
                    },
                    source_code,
                });
            }
            Err(e) => {
                debug!("Failed to read snippet for workspace symbol {}: {}", symbol.name, e);
            }
        }
    }
}

fn workspace_symbol_info_from_ast_match(ast_match: &AstGrepMatch) -> WorkspaceSymbolInfo {
    let identifier_range = ast_match.get_identifier_range();
    let path = absolute_path_to_relative_path_string(&std::path::PathBuf::from(
        ast_match.file.clone(),
    ));
    WorkspaceSymbolInfo {
        name: ast_match.meta_variables.single.name.text.clone(),
        kind: ast_match.rule_id.clone(),
        location: FilePosition {
            path,
            position: Position {
                line: identifier_range.start.line + 1,
                character: identifier_range.start.column + 1,
            },
        },
        container_name: None,
        match_kind: None,
        match_score: None,
        signature: None,
        snippet: None,
    }
}

fn apply_query_match(
    query: &str,
    exact: bool,
    mut info: WorkspaceSymbolInfo,
) -> Option<WorkspaceSymbolInfo> {
    if query.is_empty() {
        info.match_kind = Some("none".to_string());
        info.match_score = Some(0.0);
        return Some(info);
    }
    let (match_kind, match_score) = match_kind_and_score(query, &info.name);
    if match_kind == "none" {
        return None;
    }
    if exact && match_kind != "exact" {
        return None;
    }
    info.match_kind = Some(match_kind);
    info.match_score = Some(match_score);
    Some(info)
}

async fn read_symbol_line(manager: &Manager, path: &str, line: u32) -> Option<String> {
    let start = LspPosition {
        line: line.saturating_sub(1),
        character: 0,
    };
    let end = LspPosition {
        line,
        character: 0,
    };
    manager
        .read_source_code(path, Some(LspRange::new(start, end)))
        .await
        .ok()
}

fn is_reexport_line(line: &str) -> bool {
    let trimmed = line.trim();
    (trimmed.starts_with("export {") && trimmed.contains(" from "))
        || trimmed.starts_with("export * from ")
}

async fn workspace_symbol_fallback(
    manager: &Arc<Manager>,
    workspace_files: &[String],
    query: &str,
    exact: bool,
) -> Vec<WorkspaceSymbolInfo> {
    let mut symbols = Vec::new();
    for file in workspace_files {
        let matches = match manager.definitions_in_file_ast_grep(file).await {
            Ok(matches) => matches,
            Err(_) => continue,
        };
        for ast_match in matches {
            let info = workspace_symbol_info_from_ast_match(&ast_match);
            if let Some(info) = apply_query_match(query, exact, info) {
                symbols.push(info);
            }
        }
    }
    symbols
}

/// Determines the match kind and score for a symbol name against a query.
pub(crate) fn match_kind_and_score(query: &str, name: &str) -> (String, f32) {
    if query.is_empty() {
        return ("none".to_string(), 0.0);
    }
    let query_lower = query.to_ascii_lowercase();
    let name_lower = name.to_ascii_lowercase();
    if name_lower == query_lower {
        return ("exact".to_string(), 1.0);
    }
    if name_lower.starts_with(&query_lower) {
        return ("prefix".to_string(), 0.8);
    }
    if name_lower.contains(&query_lower) {
        return ("substring".to_string(), 0.6);
    }
    if is_fuzzy_match(&query_lower, &name_lower) {
        return ("fuzzy".to_string(), 0.4);
    }
    ("none".to_string(), 0.0)
}

/// Checks if the query is a fuzzy match for the name (characters appear in order).
pub(crate) fn is_fuzzy_match(query: &str, name: &str) -> bool {
    let mut iter = name.chars();
    for target in query.chars() {
        if !iter.any(|candidate| candidate == target) {
            return false;
        }
    }
    true
}

/// Deduplicates workspace symbols by (name, kind) with stable primary selection.
/// When duplicates exist, prefers non-reexport over reexport kind, then first occurrence.
fn dedupe_workspace_symbols(symbols: Vec<WorkspaceSymbolInfo>) -> Vec<WorkspaceSymbolInfo> {
    use std::collections::HashMap;

    let mut seen: HashMap<(String, String), WorkspaceSymbolInfo> = HashMap::new();

    for symbol in symbols {
        let base_kind = symbol.kind.replace(" (re-export)", "");
        let key = (symbol.name.clone(), base_kind.clone());

        let is_reexport = symbol.kind.contains("re-export");

        match seen.get(&key) {
            Some(existing) => {
                let existing_is_reexport = existing.kind.contains("re-export");
                if !is_reexport && existing_is_reexport {
                    seen.insert(key, symbol);
                }
            }
            None => {
                seen.insert(key, symbol);
            }
        }
    }

    let mut result: Vec<WorkspaceSymbolInfo> = seen.into_values().collect();
    result.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.kind.cmp(&b.kind)));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{
        set_thread_local_mount_dir, unset_thread_local_mount_dir, FilePosition, Position,
        WorkspaceSymbolInfo,
    };
    use crate::ast_grep::types::{
        AstGrepMatch, AstGrepPosition, AstGrepRange, ByteOffset, CharCount, MetaVariable,
        MetaVariables, MultiVariables, SingleVariable,
    };
    use rand::{distr::Alphanumeric, Rng};
    use std::fs;
    use tempfile::TempDir;

    fn random_irregular_string() -> String {
        let mut rng = rand::rng();
        let len: usize = rng.random_range(6..20);
        let mut value: String = rng
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect();
        value.push('_');
        value.push('\t');
        value
    }

    fn make_ast_match(file_path: &str, name: &str, start_line: u32, start_col: u32) -> AstGrepMatch {
        let range = AstGrepRange {
            byte_offset: ByteOffset { start: 0, end: 0 },
            start: AstGrepPosition {
                line: start_line,
                column: start_col,
            },
            end: AstGrepPosition {
                line: start_line,
                column: start_col + 4,
            },
        };
        AstGrepMatch {
            text: name.to_string(),
            range: range.clone(),
            file: file_path.to_string(),
            lines: random_irregular_string(),
            char_count: CharCount { leading: 0, trailing: 0 },
            language: "typescript".to_string(),
            meta_variables: MetaVariables {
                single: SingleVariable {
                    name: MetaVariable {
                        text: name.to_string(),
                        range: range.clone(),
                    },
                    context: None,
                },
                multi: MultiVariables { secondary: None },
            },
            rule_id: "function".to_string(),
            labels: None,
        }
    }

    #[test]
    fn match_kind_and_score_returns_exact_for_identical_names() {
        let (kind, score) = match_kind_and_score("scoreMember", "scoreMember");
        assert_eq!(kind, "exact", "identical names must be exact match");
        assert_eq!(score, 1.0, "exact match must have score 1.0");
    }

    #[test]
    fn match_kind_and_score_returns_prefix_for_starts_with() {
        let (kind, score) = match_kind_and_score("score", "scoreMember");
        assert_eq!(kind, "prefix", "prefix match expected");
        assert!(score > 0.7, "prefix match score must be above 0.7");
    }

    #[test]
    fn match_kind_and_score_returns_substring_for_contains() {
        let (kind, score) = match_kind_and_score("Member", "scoreMember");
        assert_eq!(kind, "substring", "substring match expected");
        assert!(score > 0.5, "substring match score must be above 0.5");
    }

    #[test]
    fn match_kind_and_score_returns_none_for_no_match() {
        let (kind, score) = match_kind_and_score("xyz", "scoreMember");
        assert_eq!(kind, "none", "no match expected");
        assert_eq!(score, 0.0, "no match must have score 0.0");
    }

    #[test]
    fn match_kind_and_score_is_case_insensitive() {
        let (kind, _) = match_kind_and_score("SCOREMEMBER", "scoreMember");
        assert_eq!(kind, "exact", "case-insensitive exact match expected");
    }

    #[test]
    fn workspace_symbol_info_from_ast_match_uses_identifier_range_and_relative_path() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        set_thread_local_mount_dir(temp_dir.path());
        let file_path = temp_dir.path().join("src").join("main.ts");
        fs::create_dir_all(file_path.parent().unwrap()).expect("negative: mkdir failed");
        fs::write(&file_path, "export function scoreMember() {}").expect("negative: write failed");
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let name = format!("score{}{}", unicode, random_irregular_string());

        let ast_match = make_ast_match(file_path.to_str().unwrap(), &name, 3, 5);

        let info = workspace_symbol_info_from_ast_match(&ast_match);

        assert_eq!(info.name, name, "negative: name mismatch");
        assert_eq!(info.kind, "function", "negative: kind mismatch");
        assert_eq!(
            info.location.path,
            "src/main.ts",
            "negative: path mismatch"
        );
        assert_eq!(
            info.location.position.line, 4,
            "negative: line mismatch"
        );
        assert_eq!(
            info.location.position.character, 6,
            "negative: character mismatch"
        );
        assert!(info.signature.is_none(), "negative: signature must be None");

        unset_thread_local_mount_dir();
    }

    #[test]
    fn is_reexport_line_detects_reexport_from() {
        assert!(
            is_reexport_line("export { scoreMember } from '@/utilities/memberScoring';"),
            "re-export lines with from must be detected"
        );
    }

    #[test]
    fn is_reexport_line_rejects_regular_exports() {
        assert!(
            !is_reexport_line("export function scoreMember() {}"),
            "regular exports must not be treated as re-exports"
        );
    }

    #[test]
    fn apply_query_match_assigns_match_kind_and_score() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let name = format!("score{}{}", unicode, random_irregular_string());
        let info = WorkspaceSymbolInfo {
            name,
            kind: "function".to_string(),
            location: FilePosition {
                path: "src/lib.rs".to_string(),
                position: Position { line: 1, character: 1 },
            },
            container_name: None,
            match_kind: None,
            match_score: None,
            signature: None,
            snippet: None,
        };

        let matched = apply_query_match(&format!("score{}", unicode), false, info)
            .expect("negative: expected symbol to match");

        assert_eq!(
            matched.match_kind.as_deref(),
            Some("prefix"),
            "negative: match kind mismatch"
        );
        assert!(
            matched.match_score.unwrap_or(0.0) > 0.7,
            "negative: match score too low"
        );
    }

    #[test]
    fn apply_query_match_respects_exact_flag() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let name = format!("score{}{}", unicode, random_irregular_string());
        let info = WorkspaceSymbolInfo {
            name,
            kind: "function".to_string(),
            location: FilePosition {
                path: "src/lib.rs".to_string(),
                position: Position { line: 1, character: 1 },
            },
            container_name: None,
            match_kind: None,
            match_score: None,
            signature: None,
            snippet: None,
        };

        let matched = apply_query_match(&format!("score{}", unicode), true, info);

        assert!(matched.is_none(), "negative: exact match should fail");
    }

    #[test]
    fn apply_query_match_accepts_empty_query() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let name = format!("score{}{}", unicode, random_irregular_string());
        let info = WorkspaceSymbolInfo {
            name,
            kind: "function".to_string(),
            location: FilePosition {
                path: "src/lib.rs".to_string(),
                position: Position { line: 1, character: 1 },
            },
            container_name: None,
            match_kind: None,
            match_score: None,
            signature: None,
            snippet: None,
        };

        let matched = apply_query_match("", false, info).expect("negative: expected match");

        assert_eq!(
            matched.match_kind.as_deref(),
            Some("none"),
            "negative: empty query must keep match kind"
        );
        assert_eq!(
            matched.match_score.unwrap_or(1.0),
            0.0,
            "negative: empty query must set score to zero"
        );
    }

    #[test]
    fn dedupe_workspace_symbols_prefers_non_reexport() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let name = format!("api{}{}", unicode, random_irregular_string());
        let primary = WorkspaceSymbolInfo {
            name: name.clone(),
            kind: "function".to_string(),
            location: FilePosition {
                path: "src/api.ts".to_string(),
                position: Position { line: 5, character: 10 },
            },
            container_name: None,
            match_kind: Some("exact".to_string()),
            match_score: Some(1.0),
            signature: None,
            snippet: None,
        };
        let reexport = WorkspaceSymbolInfo {
            name: name.clone(),
            kind: "function (re-export)".to_string(),
            location: FilePosition {
                path: "src/index.ts".to_string(),
                position: Position { line: 2, character: 15 },
            },
            container_name: None,
            match_kind: Some("exact".to_string()),
            match_score: Some(1.0),
            signature: None,
            snippet: None,
        };

        let deduped = dedupe_workspace_symbols(vec![reexport, primary]);

        assert_eq!(deduped.len(), 1, "negative: must dedupe to single symbol");
        assert_eq!(
            deduped[0].kind, "function",
            "negative: must prefer non-reexport kind"
        );
        assert_eq!(
            deduped[0].location.path, "src/api.ts",
            "negative: must select primary definition"
        );
    }

    #[test]
    fn dedupe_workspace_symbols_stable_order_when_same_kind() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let name = format!("util{}{}", unicode, random_irregular_string());
        let first = WorkspaceSymbolInfo {
            name: name.clone(),
            kind: "function".to_string(),
            location: FilePosition {
                path: "src/util.ts".to_string(),
                position: Position { line: 3, character: 8 },
            },
            container_name: None,
            match_kind: Some("exact".to_string()),
            match_score: Some(1.0),
            signature: None,
            snippet: None,
        };
        let second = WorkspaceSymbolInfo {
            name: name.clone(),
            kind: "function".to_string(),
            location: FilePosition {
                path: "src/helpers.ts".to_string(),
                position: Position { line: 10, character: 5 },
            },
            container_name: None,
            match_kind: Some("exact".to_string()),
            match_score: Some(1.0),
            signature: None,
            snippet: None,
        };

        let deduped = dedupe_workspace_symbols(vec![first.clone(), second]);

        assert_eq!(deduped.len(), 1, "negative: must dedupe to single symbol");
        assert_eq!(
            deduped[0].location.path, "src/util.ts",
            "negative: must preserve first occurrence for stable order"
        );
    }
}
