// ABOUTME: Reference lookup operations (find_references, find_referenced_symbols).
// ABOUTME: Handles finding usages of symbols across the codebase.

use crate::api_types::{
    CodeContext, FilePosition, FileRange, Identifier, Position, Range,
    ReferenceWithSymbolDefinitions, ReferencedSymbolsResponse,
};
use crate::lsp::manager::Manager;
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{Location, Position as LspPosition, Range as LspRange};
use std::sync::Arc;

use crate::service::types::errors::{PositionError, ServiceError};
use crate::service::types::response::{FileGroup, McpReferenceLocation, McpReferencesResponse, TypeCounts};
use crate::service::utils::identifiers::find_identifier_at_position;
use crate::service::utils::pagination::paginate_items;
use crate::service::utils::signature::extract_identifier_name_from_hover;
use crate::service::utils::transformations::{definition_locations, reference_item_from_location};

/// Finds all references to the symbol at the given position.
pub(crate) async fn find_references_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    include_raw_response: bool,
    context_lines: Option<u32>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<McpReferencesResponse, ServiceError> {
    let file_identifiers = manager.get_file_identifiers(file_path).await?;

    // Try to find identifier at position first
    let identifier_result = find_identifier_at_position(
        file_identifiers,
        &FilePosition {
            path: file_path.to_string(),
            position: position.clone(),
        },
    )
    .await;

    // Always get references from LSP
    let all_references = find_and_filter_references(
        manager,
        &FilePosition {
            path: file_path.to_string(),
            position: position.clone(),
        },
    )
    .await?;

    // Determine selected_identifier
    let selected_identifier = match identifier_result {
        Ok(id) => id,
        Err(PositionError::IdentifierNotFound { closest }) => {
            // Fallback: if LSP found references, construct identifier from hover
            if all_references.is_empty() {
                // LSP also found nothing - return original error
                return Err(ServiceError::IdentifierSelection(
                    PositionError::IdentifierNotFound { closest }
                ));
            }

            // LSP found references, so position is valid
            // Construct identifier from hover info
            let hover_pos = LspPosition {
                line: position.line.saturating_sub(1),
                character: position.character.saturating_sub(1),
            };

            if let Ok(Some(hover)) = manager.hover(file_path, hover_pos).await {
                let name = extract_identifier_name_from_hover(&hover.contents);
                Identifier {
                    name,
                    kind: Some("unknown".to_string()),
                    file_range: FileRange {
                        path: file_path.to_string(),
                        range: Range {
                            start: position.clone(),
                            end: position.clone(),
                        },
                    },
                }
            } else {
                Identifier {
                    name: "unknown".to_string(),
                    kind: Some("unknown".to_string()),
                    file_range: FileRange {
                        path: file_path.to_string(),
                        range: Range {
                            start: position.clone(),
                            end: position.clone(),
                        },
                    },
                }
            }
        }
    };

    let total_count = all_references.len() as u32;

    // Build by_type counts before pagination
    let by_type = classify_references_by_type(manager, &all_references).await;

    let raw_response = if include_raw_response {
        serde_json::to_value(&all_references).ok()
    } else {
        None
    };
    // Only paginate when limit is explicitly specified; otherwise return all references
    let (references, limit_val, offset_val, truncated) = match limit {
        Some(_) => {
            let (refs, pagination) = paginate_items(all_references, limit, offset);
            (refs, pagination.limit, pagination.offset, pagination.truncated)
        }
        None => {
            // No limit specified - return all references, no truncation
            (all_references, total_count, 0, false)
        }
    };
    let code_contexts = get_code_contexts(manager, &references, context_lines).await?;

    let mut reference_items = Vec::with_capacity(references.len());
    for (index, reference) in references.iter().enumerate() {
        let snippet = code_contexts
            .as_ref()
            .and_then(|contexts| contexts.get(index).cloned());
        reference_items.push(reference_item_from_location(reference, snippet));
    }

    // Build by_file groups from paginated references
    let by_file = group_references_by_file(&reference_items);

    Ok(McpReferencesResponse {
        raw_response,
        selected_identifier,
        limit: limit_val,
        offset: offset_val,
        truncated,
        total_count,
        by_file,
        by_type,
    })
}

/// Finds all symbols referenced at the given position and resolves their definitions.
pub(crate) async fn find_referenced_symbols_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    full_scan: bool,
) -> Result<ReferencedSymbolsResponse, ServiceError> {
    let referenced_symbols = manager
        .find_referenced_symbols(
            file_path,
            LspPosition {
                line: position.line.saturating_sub(1),
                character: position.character.saturating_sub(1),
            },
            full_scan,
        )
        .await?;

    let unwrapped_definitions: Vec<(Identifier, Vec<FilePosition>)> = referenced_symbols
        .into_iter()
        .map(|(ast_grep_result, definition_response)| {
            let definitions = definition_locations(&definition_response);
            (Identifier::from(ast_grep_result), definitions)
        })
        .collect();

    let files = manager.list_files().await?;

    let mut workspace_symbols = Vec::new();
    let mut external_symbols = Vec::new();
    let mut not_found = Vec::new();

    for (identifier, definitions) in unwrapped_definitions {
        if definitions.is_empty() {
            not_found.push(identifier);
        } else {
            let has_internal_definition =
                definitions.iter().any(|def| files.contains(&def.path));
            if has_internal_definition {
                let mut symbols_with_definitions = Vec::new();
                for def in definitions.iter().filter(|def| files.contains(&def.path)) {
                    if let Ok(symbol) = manager
                        .get_symbol_from_position(
                            &def.path,
                            &lsp_types::Position {
                                line: def.position.line.saturating_sub(1),
                                character: def.position.character.saturating_sub(1),
                            },
                        )
                        .await
                    {
                        symbols_with_definitions.push(symbol);
                    }
                }
                if !symbols_with_definitions.is_empty() {
                    workspace_symbols.push(ReferenceWithSymbolDefinitions {
                        reference: identifier.clone(),
                        definitions: symbols_with_definitions,
                    });
                } else {
                    not_found.push(identifier.clone());
                }
            } else {
                external_symbols.push(identifier.clone());
            }
        }
    }

    workspace_symbols.sort_by(|a, b| {
        let path_cmp = a
            .reference
            .file_range
            .path
            .cmp(&b.reference.file_range.path);
        if path_cmp.is_eq() {
            a.reference
                .file_range
                .range
                .start
                .line
                .cmp(&b.reference.file_range.range.start.line)
        } else {
            path_cmp
        }
    });

    external_symbols.sort_by(|a, b| {
        let path_cmp = a.file_range.path.cmp(&b.file_range.path);
        if path_cmp.is_eq() {
            a.file_range
                .range
                .start
                .line
                .cmp(&b.file_range.range.start.line)
        } else {
            path_cmp
        }
    });

    not_found.sort_by(|a, b| {
        let path_cmp = a.file_range.path.cmp(&b.file_range.path);
        if path_cmp.is_eq() {
            a.file_range
                .range
                .start
                .line
                .cmp(&b.file_range.range.start.line)
        } else {
            path_cmp
        }
    });

    Ok(ReferencedSymbolsResponse {
        workspace_symbols,
        external_symbols,
        not_found,
    })
}

/// Finds and filters references to return only workspace files.
pub(crate) async fn find_and_filter_references(
    manager: &Manager,
    position: &FilePosition,
) -> Result<Vec<Location>, ServiceError> {
    let references = manager
        .find_references(
            &position.path,
            LspPosition {
                line: position.position.line.saturating_sub(1),
                character: position.position.character.saturating_sub(1),
            },
        )
        .await?;

    let files = manager.list_files().await?;
    let mut filtered_refs: Vec<_> = references
        .into_iter()
        .filter(|reference| {
            let path = uri_to_relative_path_string(&reference.uri);
            files.contains(&path)
        })
        .collect();

    filtered_refs.sort_by(|a, b| {
        let uri_cmp = a.uri.to_string().cmp(&b.uri.to_string());
        if uri_cmp.is_eq() {
            a.range.start.line.cmp(&b.range.start.line)
        } else {
            uri_cmp
        }
    });

    Ok(filtered_refs)
}

/// Gets code contexts for references if context_lines is specified.
pub(crate) async fn get_code_contexts(
    manager: &Manager,
    references: &Vec<Location>,
    context_lines: Option<u32>,
) -> Result<Option<Vec<CodeContext>>, ServiceError> {
    match context_lines {
        Some(lines) => fetch_code_context(manager, references.clone(), lines)
            .await
            .map(Some),
        None => Ok(None),
    }
}

/// Fetches source code context around each reference location.
pub(crate) async fn fetch_code_context(
    manager: &Manager,
    references: Vec<Location>,
    context_lines: u32,
) -> Result<Vec<CodeContext>, ServiceError> {
    let mut code_contexts = Vec::new();
    for reference in references {
        let range = LspRange {
            start: LspPosition {
                line: reference.range.start.line.saturating_sub(context_lines),
                character: 0,
            },
            end: LspPosition {
                line: reference.range.end.line.saturating_add(context_lines),
                character: 0,
            },
        };
        let relative_path = uri_to_relative_path_string(&reference.uri);
        let source_code = manager.read_source_code(&relative_path, Some(range)).await?;
        code_contexts.push(CodeContext {
            range: FileRange {
                path: relative_path,
                range: Range {
                    start: Position {
                        line: reference.range.start.line.saturating_sub(context_lines) + 1,
                        character: 1,
                    },
                    end: Position {
                        line: reference.range.end.line.saturating_add(context_lines) + 1,
                        character: 1,
                    },
                },
            },
            source_code,
        });
    }
    Ok(code_contexts)
}

/// Groups references by file path.
/// Clears individual ref paths since FileGroup.path provides the file.
pub(crate) fn group_references_by_file(references: &[McpReferenceLocation]) -> Vec<FileGroup> {
    use std::collections::HashMap;

    let mut groups: HashMap<String, Vec<McpReferenceLocation>> = HashMap::new();

    for reference in references {
        // Use path from reference, defaulting to empty string if None (shouldn't happen)
        let path = reference.path.clone().unwrap_or_default();
        groups.entry(path)
            .or_insert_with(Vec::new)
            .push(reference.clone());
    }

    let mut file_groups: Vec<FileGroup> = groups.into_iter()
        .map(|(path, refs)| {
            // Clear paths from individual refs since FileGroup.path provides it
            let refs_without_path: Vec<McpReferenceLocation> = refs.into_iter()
                .map(|mut r| { r.path = None; r })
                .collect();
            FileGroup {
                count: refs_without_path.len() as u32,
                path,
                refs: refs_without_path,
            }
        })
        .collect();

    // Sort by path for consistent output
    file_groups.sort_by(|a, b| a.path.cmp(&b.path));

    file_groups
}

/// Classifies references by type (import vs call).
pub(crate) async fn classify_references_by_type(manager: &Manager, references: &[Location]) -> TypeCounts {
    let mut counts = TypeCounts::default();

    for reference in references {
        let path = uri_to_relative_path_string(&reference.uri);
        let line_num = reference.range.start.line;

        // Try to read the line containing this reference
        if let Ok(source) = manager.read_source_code(
            &path,
            Some(LspRange::new(
                LspPosition { line: line_num, character: 0 },
                LspPosition { line: line_num + 1, character: 0 },
            )),
        ).await {
            if is_import_line(&source) {
                counts.import += 1;
            } else {
                counts.call += 1;
            }
        } else {
            // If we can't read the line, assume it's a call
            counts.call += 1;
        }
    }

    counts
}

/// Detects if a line is an import statement.
pub(crate) fn is_import_line(line: &str) -> bool {
    let trimmed = line.trim();

    // Skip comments
    if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("/*") {
        return false;
    }

    trimmed.starts_with("import ")
        || trimmed.starts_with("use ")
        || trimmed.contains("require(")
        || trimmed.starts_with("from \"")
        || trimmed.starts_with("from '")
        || trimmed.starts_with("from ")
}
