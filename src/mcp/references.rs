// ABOUTME: MCP tool handlers for reference-related operations.
// ABOUTME: Provides find_references and find_referenced_symbols tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_result_from_error, tool_result_success};
use crate::service::{LspService, ServiceError};
use crate::service::types::response::{ReferenceCandidate, ReferencesSelection};
use crate::utils::file_utils::normalize_path;
use rmcp::model::CallToolResult;

pub async fn find_references(
    service: &LspService,
    output_mode: OutputMode,
    symbol: String,
    path: Option<String>,
    context_lines: Option<u32>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> CallToolResult {
    let context_lines = resolve_context_lines(context_lines);

    let preferred_path = match path.as_deref() {
        Some(p) => match normalize_path(p) {
            Ok(normalized) => Some(normalized),
            Err(err) => {
                return tool_result_from_error(ServiceError::InvalidPath(err.to_string()));
            }
        },
        None => None,
    };

    // Resolve the best symbol candidate (definition-like) from workspace symbols.
    // We prefer exact matches, but fall back to fuzzy when no exact matches exist.
    let mut candidates = match service
        .workspace_symbol(
            &symbol,
            output_mode == OutputMode::Verbose,
            true,
            Some(100),
            None,
            0,
        )
        .await
    {
        Ok(resp) => resp.symbols,
        Err(e) => return tool_result_from_error(e),
    };

    if candidates.is_empty() {
        candidates = match service
            .workspace_symbol(
                &symbol,
                output_mode == OutputMode::Verbose,
                false,
                Some(100),
                None,
                0,
            )
            .await
        {
            Ok(resp) => resp.symbols,
            Err(e) => return tool_result_from_error(e),
        };
    }

    // Prefer exact name matches over fuzzy prefix/substring matches.
    let exact_name: Vec<_> = candidates
        .iter()
        .filter(|c| c.name == symbol)
        .cloned()
        .collect();
    if !exact_name.is_empty() {
        candidates = exact_name;
    }

    if candidates.is_empty() {
        return tool_result_from_error(ServiceError::SymbolResolution(format!(
            "No workspace symbols found matching '{}'",
            symbol
        )));
    }

    // If a preferred path is provided, narrow to that file when possible.
    let narrowed: Vec<_> = preferred_path
        .as_deref()
        .map(|p| {
            candidates
                .iter()
                .filter(|c| c.location.path == p)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let selection_pool = if !narrowed.is_empty() { &narrowed } else { &candidates };

    let mut ranked: Vec<_> = selection_pool.clone();
    ranked.sort_by(|a, b| {
        let a_key = candidate_rank_key(a, preferred_path.as_deref());
        let b_key = candidate_rank_key(b, preferred_path.as_deref());
        b_key.cmp(&a_key)
    });
    let chosen = ranked
        .first()
        .cloned()
        .expect("candidate list is non-empty");

    // Build alternates list from the full candidate set (not just narrowed), ranked with the same heuristic.
    let mut all_ranked = candidates.clone();
    all_ranked.sort_by(|a, b| {
        let a_key = candidate_rank_key(a, preferred_path.as_deref());
        let b_key = candidate_rank_key(b, preferred_path.as_deref());
        b_key.cmp(&a_key)
    });

    let chosen_summary = candidate_summary(&chosen);
    let mut others: Vec<ReferenceCandidate> = all_ranked
        .into_iter()
        .filter(|c| !(c.location.path == chosen.location.path && c.location.position == chosen.location.position))
        .take(5)
        .map(|c| candidate_summary(&c))
        .collect();
    // If we narrowed by path and ended up choosing from that set, ensure we still show alternates.
    if others.is_empty() {
        others = Vec::new();
    }
    let selection = ReferencesSelection {
        chosen: chosen_summary,
        others,
    };

    match service
        .find_references(
            &chosen.location.path,
            chosen.location.position.clone(),
            output_mode == OutputMode::Verbose,
            context_lines,
            limit,
            offset,
        )
        .await
    {
        Ok(mut response) => {
            response.selection = Some(selection);
            let resp = format_response(&response, output_mode);
            tool_result_success(resp)
        }
        Err(e) => tool_result_from_error(e),
    }
}

fn candidate_summary(symbol: &crate::api_types::WorkspaceSymbolInfo) -> ReferenceCandidate {
    ReferenceCandidate {
        name: symbol.name.clone(),
        kind: Some(symbol.kind.clone()),
        module: symbol.container_name.clone(),
        path: symbol.location.path.clone(),
        position: symbol.location.position.clone(),
    }
}

fn candidate_rank_key(
    candidate: &crate::api_types::WorkspaceSymbolInfo,
    preferred_path: Option<&str>,
) -> (i32, i32, i32, i32, i32, String) {
    let mut score: i32 = 0;

    // Strongly prefer the explicitly requested path.
    if let Some(p) = preferred_path {
        if candidate.location.path == p {
            score += 10_000;
        }
    }

    // Prefer internal/workspace files over external dependency paths.
    if is_external_path(&candidate.location.path) {
        score -= 2_000;
    } else {
        score += 2_000;
    }

    // Prefer exact query matches over fuzzy.
    score += match candidate.match_kind.as_deref() {
        Some("exact") => 1_000,
        Some("prefix") => 800,
        Some("substring") => 600,
        Some("fuzzy") => 400,
        _ => 0,
    };

    // Prefer definition-like symbol kinds.
    score += kind_weight(&candidate.kind);

    // Prefer candidates with container/module context.
    if candidate.container_name.is_some() {
        score += 100;
    }

    let match_score_scaled = (candidate.match_score.unwrap_or(0.0) * 1000.0) as i32;

    // Stable, deterministic tie-break by path.
    (score, match_score_scaled, -((candidate.location.position.line as i32)), -((candidate.location.position.character as i32)), 0, candidate.location.path.clone())
}

fn is_external_path(path: &str) -> bool {
    path.starts_with("node_modules/") || path.contains("/node_modules/")
}

fn kind_weight(kind: &str) -> i32 {
    let mut weight = 0;
    let lowered = kind.to_ascii_lowercase();
    let base = lowered.replace(" (re-export)", "");

    if lowered.contains("re-export") {
        weight -= 50;
    }

    weight += match base.as_str() {
        "class" | "struct" | "trait" | "interface" | "enum" | "module" => 900,
        "function" | "method" => 850,
        "type" | "type-alias" | "alias" => 800,
        "const" | "variable" | "field" => 400,
        _ => 100,
    };

    weight
}

pub async fn find_referenced_symbols(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    full_scan: Option<bool>,
    externals: Option<bool>,
) -> CallToolResult {
    let pos = Position { line, character };
    let include_externals = resolve_include_externals(externals);
    match service
        .find_referenced_symbols(
            &path,
            pos,
            full_scan.unwrap_or(false),
            include_externals,
        )
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            tool_result_success(resp)
        }
        Err(e) => tool_result_from_error(e),
    }
}

fn resolve_context_lines(context_lines: Option<u32>) -> Option<u32> {
    Some(context_lines.unwrap_or(1))
}

fn resolve_include_externals(externals: Option<bool>) -> bool {
    externals.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn it_defaults_context_lines_to_one_when_omitted() {
        let context_lines = None;
        let resolved = resolve_context_lines(context_lines);
        assert_eq!(
            resolved,
            Some(1),
            "negative: omitted context lines must default to one line"
        );
    }

    #[test]
    fn it_preserves_explicit_context_line_values() {
        let mut rng = rand::rng();
        let value: u32 = rng.random_range(2..50);
        let resolved = resolve_context_lines(Some(value));
        assert_eq!(
            resolved,
            Some(value),
            "negative: explicit context lines must be preserved"
        );
    }

    #[test]
    fn it_defaults_externals_to_false_when_omitted() {
        let resolved = resolve_include_externals(None);
        assert!(
            !resolved,
            "negative: externals must default to false when omitted"
        );
    }

    #[test]
    fn it_preserves_explicit_externals_value() {
        let mut rng = rand::rng();
        let explicit = rng.random_range(0..2) == 1;
        let resolved = resolve_include_externals(Some(explicit));
        assert_eq!(
            resolved,
            explicit,
            "negative: explicit externals value must be preserved"
        );
    }
}
