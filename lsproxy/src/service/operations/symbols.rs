// ABOUTME: Symbol lookup operations (find_identifier, list_files, workspace_symbol).
// ABOUTME: Handles workspace-wide symbol search and file listing.

use crate::api_types::{FilePosition, Identifier, Position, WorkspaceSymbolResponse};
use crate::lsp::manager::Manager;
use crate::utils::file_utils::uri_to_relative_path_string;
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
) -> Result<WorkspaceSymbolResponse, ServiceError> {
    let symbols = manager.workspace_symbol(query).await?;

    let workspace_files = manager.list_files().await?;

    let mut filtered_symbols = Vec::new();
    for sym in symbols {
        let path = uri_to_relative_path_string(&sym.location.uri);
        if !workspace_files.contains(&path) {
            continue;
        }
        let mut info = workspace_symbol_info_from_lsp(sym, path);
        let (match_kind, match_score) = match_kind_and_score(query, &info.name);
        if exact && match_kind != "exact" {
            continue;
        }
        info.match_kind = Some(match_kind);
        info.match_score = Some(match_score);
        filtered_symbols.push(info);
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

    let raw_response = if include_raw_response {
        serde_json::to_value(&filtered_symbols).ok()
    } else {
        None
    };

    let (symbols, pagination) = paginate_items(filtered_symbols, limit, offset);
    Ok(WorkspaceSymbolResponse {
        raw_response,
        symbols,
        limit: pagination.limit,
        offset: pagination.offset,
        truncated: pagination.truncated,
    })
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
