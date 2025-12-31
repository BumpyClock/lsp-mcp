// ABOUTME: Hover operations (hover, fetch_hover_info, fetch_definition_location, count_references).
// ABOUTME: Handles fetching type info, documentation, and hover metadata.

use crate::api_types::{DefinitionLocation, FilePosition, HoverResponse, Position, Range};
use crate::lsp::manager::Manager;
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{GotoDefinitionResponse, Location, Position as LspPosition};
use std::collections::HashSet;
use std::sync::Arc;

use crate::service::types::errors::ServiceError;
use crate::service::utils::identifiers::find_identifier_at_position;
use crate::service::utils::signature::{extract_identifier_name_from_hover, extract_signature_and_docs};
use crate::service::utils::transformations::extract_hover_contents;

/// Gets hover information (documentation, type info) for a symbol at a given position.
pub(crate) async fn hover_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    include_raw_response: bool,
    include_definition: bool,
) -> Result<HoverResponse, ServiceError> {
    let hover = manager
        .hover(
            file_path,
            LspPosition {
                line: position.line.saturating_sub(1),
                character: position.character.saturating_sub(1),
            },
        )
        .await?;

    let (contents, range, raw_response) = match hover {
        Some(h) => {
            let contents = extract_hover_contents(&h.contents);
            let range = h.range.map(|r| Range {
                start: Position {
                    line: r.start.line + 1,
                    character: r.start.character + 1,
                },
                end: Position {
                    line: r.end.line + 1,
                    character: r.end.character + 1,
                },
            });
            let raw = if include_raw_response {
                serde_json::to_value(&h).ok()
            } else {
                None
            };
            (Some(contents), range, raw)
        }
        None => (None, None, None),
    };

    // Optionally fetch definition location
    let definitions = if include_definition {
        fetch_definition_locations_impl(manager, file_path, position).await
    } else {
        Vec::new()
    };

    Ok(HoverResponse {
        raw_response,
        contents,
        range,
        definitions,
    })
}

/// Fetches minimal definition locations for hover response.
/// Merges LSP definitions with workspace symbol results to surface multiple definitions
/// for ambiguous types (e.g., when the same name is defined in multiple workspace files).
pub(crate) async fn fetch_definition_locations_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
) -> Vec<DefinitionLocation> {
    let lsp_position = LspPosition {
        line: position.line.saturating_sub(1),
        character: position.character.saturating_sub(1),
    };

    // Get LSP definitions first
    let definitions = match manager.find_definition(file_path, lsp_position).await {
        Ok(definitions) => definitions,
        Err(_) => return Vec::new(),
    };
    let locations = match definitions {
        GotoDefinitionResponse::Scalar(loc) => vec![loc],
        GotoDefinitionResponse::Array(locs) => locs,
        GotoDefinitionResponse::Link(links) => {
            links
                .into_iter()
                .map(|l| Location {
                    uri: l.target_uri,
                    range: l.target_selection_range,
                })
                .collect()
        }
    };

    // Convert LSP locations to DefinitionLocation
    let mut result: Vec<DefinitionLocation> = locations
        .into_iter()
        .map(|location| {
            let path = uri_to_relative_path_string(&location.uri);
            let external = if path.contains("node_modules") {
                Some(true)
            } else {
                None
            };
            DefinitionLocation {
                path,
                line: location.range.start.line + 1,
                external,
            }
        })
        .collect();

    // Attempt to find additional definitions via workspace_symbol
    // First, determine the identifier name at the hover position
    let identifier_name = get_identifier_name_at_position(manager, file_path, position).await;

    if let Some(name) = identifier_name {
        // Get workspace files for filtering
        let workspace_files: HashSet<String> = manager
            .list_files()
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();

        // Query workspace symbols for the name
        if let Ok(symbols) = manager.workspace_symbol(&name).await {
            // Filter to exact name matches and workspace files only
            let additional_locations: Vec<DefinitionLocation> = symbols
                .into_iter()
                .filter(|sym| sym.name == name)
                .filter_map(|sym| {
                    #[allow(deprecated)]
                    let location = &sym.location;
                    let path = uri_to_relative_path_string(&location.uri);

                    // Only include workspace files (internal definitions)
                    if workspace_files.contains(&path) {
                        Some(DefinitionLocation {
                            path,
                            line: location.range.start.line + 1,
                            external: None,
                        })
                    } else {
                        None
                    }
                })
                .collect();

            // Merge with existing definitions, deduplicating by (path, line)
            let mut existing_keys: HashSet<(String, u32)> = result
                .iter()
                .map(|d| (d.path.clone(), d.line))
                .collect();

            for loc in additional_locations {
                let key = (loc.path.clone(), loc.line);
                if existing_keys.insert(key) {
                    result.push(loc);
                }
            }
        }
    }

    // Sort deterministically by (path, line) for consistent output
    result.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));

    result
}

/// Attempts to get the identifier name at a given position.
/// First tries get_file_identifiers + find_identifier_at_position,
/// then falls back to extracting from hover contents.
async fn get_identifier_name_at_position(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
) -> Option<String> {
    // Try to find identifier via AST-based identifiers first
    if let Ok(identifiers) = manager.get_file_identifiers(file_path).await {
        let file_position = FilePosition {
            path: file_path.to_string(),
            position: position.clone(),
        };

        if let Ok(identifier) = find_identifier_at_position(identifiers, &file_position).await {
            return Some(identifier.name);
        }
    }

    // Fallback: Extract from hover contents
    let lsp_position = LspPosition {
        line: position.line.saturating_sub(1),
        character: position.character.saturating_sub(1),
    };

    if let Ok(Some(hover)) = manager.hover(file_path, lsp_position).await {
        let name = extract_identifier_name_from_hover(&hover.contents);
        if name != "unknown" {
            return Some(name);
        }
    }

    None
}

/// Fetches signature and documentation from hover info for a definition position.
/// Used internally by find_definition to enrich response with type info.
pub(crate) async fn fetch_hover_info_impl(
    manager: &Manager,
    file_path: &str,
    position: &LspPosition,
) -> (Option<String>, Option<String>) {
    let hover_result = manager.hover(file_path, *position).await;

    match hover_result {
        Ok(Some(hover)) => extract_signature_and_docs(&hover.contents),
        _ => (None, None),
    }
}

/// Counts references to a symbol at the given position.
/// Used to populate reference_count in find_definition responses.
pub(crate) async fn count_references_impl(
    manager: &Manager,
    file_path: &str,
    position: &LspPosition,
) -> Option<u32> {
    let references = manager.find_references(file_path, *position).await.ok()?;
    // Don't include the definition itself in the count
    let count = references.len().saturating_sub(1) as u32;
    Some(count)
}
