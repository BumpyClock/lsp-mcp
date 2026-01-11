// ABOUTME: Hover operations (hover, fetch_hover_info, fetch_definition_location, count_references).
// ABOUTME: Handles fetching type info, documentation, and hover metadata.

use crate::api_types::{
    DefinitionLocation, FilePosition, HoverResponse, NearbySymbol, Position, Range,
};
use crate::lsp::manager::Manager;
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{
    DocumentSymbolResponse, GotoDefinitionResponse, Location, Position as LspPosition,
};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::debug;

use crate::service::types::errors::ServiceError;
use crate::service::utils::identifiers::find_identifier_at_position;
use crate::service::utils::signature::{
    extract_active_signature, extract_identifier_name_from_hover, extract_signature_and_docs,
};
use crate::service::utils::transformations::extract_hover_contents;

/// Gets hover information (documentation, type info) for a symbol at a given position.
pub(crate) async fn hover_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    include_raw_response: bool,
    include_definition: bool,
) -> Result<HoverResponse, ServiceError> {
    let lsp_line = position.line.saturating_sub(1);
    let lsp_char = position.character.saturating_sub(1);
    debug!(
        "hover_impl: position ({},{}) 1-based -> ({},{}) 0-based, file={}",
        position.line, position.character, lsp_line, lsp_char, file_path
    );

    let hover = manager
        .hover(
            file_path,
            LspPosition {
                line: lsp_line,
                character: lsp_char,
            },
        )
        .await?;

    debug!(
        "hover_impl: LSP hover response is_some={}, file={}",
        hover.is_some(),
        file_path
    );

    let (contents, range, raw_response, nearby_symbols) = match hover {
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
            (Some(contents), range, raw, Vec::new())
        }
        None => {
            // Hover returned nothing - find nearby symbols to help the user
            let nearby = find_nearby_symbols(manager, file_path, &position, 3).await;
            debug!(
                "hover_impl: no hover content, found {} nearby symbols",
                nearby.len()
            );
            (None, None, None, nearby)
        }
    };

    let lsp_position = LspPosition {
        line: lsp_line,
        character: lsp_char,
    };

    let definitions = if include_definition {
        fetch_definition_locations_impl(manager, file_path, position).await
    } else {
        Vec::new()
    };
    let (active_signature, active_parameter) =
        match manager.signature_help(file_path, lsp_position).await {
            Ok(Some(sig_help)) => match extract_active_signature(&sig_help) {
                Some(info) => (Some(info.label), info.active_parameter),
                None => (None, None),
            },
            _ => (None, None),
        };

    Ok(HoverResponse {
        raw_response,
        contents,
        range,
        definitions,
        active_signature,
        active_parameter,
        nearby_symbols,
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
        GotoDefinitionResponse::Link(links) => links
            .into_iter()
            .map(|l| Location {
                uri: l.target_uri,
                range: l.target_selection_range,
            })
            .collect(),
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
            let mut existing_keys: HashSet<(String, u32)> =
                result.iter().map(|d| (d.path.clone(), d.line)).collect();

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

/// Finds symbols near the given position when hover returns no content.
/// Searches within 3 lines of the position and returns up to `limit` symbols.
async fn find_nearby_symbols(
    manager: &Arc<Manager>,
    file_path: &str,
    position: &Position,
    limit: usize,
) -> Vec<NearbySymbol> {
    let doc_symbols = match manager.document_symbol(file_path).await {
        Ok(Some(symbols)) => symbols,
        Ok(None) | Err(_) => return Vec::new(),
    };

    let target_line = position.line;
    let line_range = 3u32; // Search within 3 lines

    // Flatten symbols and filter by proximity
    let mut nearby: Vec<(u32, NearbySymbol)> = Vec::new();
    collect_nearby_symbols(&doc_symbols, target_line, line_range, &mut nearby);

    // Sort by distance to target line
    nearby.sort_by_key(|(distance, _)| *distance);

    // Take up to limit symbols
    nearby.into_iter().take(limit).map(|(_, sym)| sym).collect()
}

/// Recursively collects symbols within the line range.
fn collect_nearby_symbols(
    response: &DocumentSymbolResponse,
    target_line: u32,
    line_range: u32,
    result: &mut Vec<(u32, NearbySymbol)>,
) {
    match response {
        DocumentSymbolResponse::Nested(symbols) => {
            for sym in symbols {
                // DocumentSymbol uses 0-based lines, our target_line is 1-based
                let sym_line = sym.selection_range.start.line + 1;
                let distance = if sym_line >= target_line {
                    sym_line - target_line
                } else {
                    target_line - sym_line
                };

                if distance <= line_range {
                    result.push((
                        distance,
                        NearbySymbol {
                            name: sym.name.clone(),
                            kind: symbol_kind_to_string(sym.kind),
                            line: sym_line,
                        },
                    ));
                }

                // Check children recursively
                if let Some(children) = &sym.children {
                    for child in children {
                        let child_line = child.selection_range.start.line + 1;
                        let child_distance = if child_line >= target_line {
                            child_line - target_line
                        } else {
                            target_line - child_line
                        };

                        if child_distance <= line_range {
                            result.push((
                                child_distance,
                                NearbySymbol {
                                    name: child.name.clone(),
                                    kind: symbol_kind_to_string(child.kind),
                                    line: child_line,
                                },
                            ));
                        }
                    }
                }
            }
        }
        DocumentSymbolResponse::Flat(symbols) => {
            for sym in symbols {
                #[allow(deprecated)]
                let sym_line = sym.location.range.start.line + 1;
                let distance = if sym_line >= target_line {
                    sym_line - target_line
                } else {
                    target_line - sym_line
                };

                if distance <= line_range {
                    result.push((
                        distance,
                        NearbySymbol {
                            name: sym.name.clone(),
                            kind: symbol_kind_to_string(sym.kind),
                            line: sym_line,
                        },
                    ));
                }
            }
        }
    }
}

/// Converts LSP SymbolKind to a human-readable string.
fn symbol_kind_to_string(kind: lsp_types::SymbolKind) -> String {
    match kind {
        lsp_types::SymbolKind::FILE => "file",
        lsp_types::SymbolKind::MODULE => "module",
        lsp_types::SymbolKind::NAMESPACE => "namespace",
        lsp_types::SymbolKind::PACKAGE => "package",
        lsp_types::SymbolKind::CLASS => "class",
        lsp_types::SymbolKind::METHOD => "method",
        lsp_types::SymbolKind::PROPERTY => "property",
        lsp_types::SymbolKind::FIELD => "field",
        lsp_types::SymbolKind::CONSTRUCTOR => "constructor",
        lsp_types::SymbolKind::ENUM => "enum",
        lsp_types::SymbolKind::INTERFACE => "interface",
        lsp_types::SymbolKind::FUNCTION => "function",
        lsp_types::SymbolKind::VARIABLE => "variable",
        lsp_types::SymbolKind::CONSTANT => "constant",
        lsp_types::SymbolKind::STRING => "string",
        lsp_types::SymbolKind::NUMBER => "number",
        lsp_types::SymbolKind::BOOLEAN => "boolean",
        lsp_types::SymbolKind::ARRAY => "array",
        lsp_types::SymbolKind::OBJECT => "object",
        lsp_types::SymbolKind::KEY => "key",
        lsp_types::SymbolKind::NULL => "null",
        lsp_types::SymbolKind::ENUM_MEMBER => "enum_member",
        lsp_types::SymbolKind::STRUCT => "struct",
        lsp_types::SymbolKind::EVENT => "event",
        lsp_types::SymbolKind::OPERATOR => "operator",
        lsp_types::SymbolKind::TYPE_PARAMETER => "type_parameter",
        _ => "unknown",
    }
    .to_string()
}
