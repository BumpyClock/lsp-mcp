// ABOUTME: Reference lookup operations (find_references, find_referenced_symbols).
// ABOUTME: Handles finding usages of symbols across the codebase.

use crate::api_types::{
    get_mount_dir, CodeContext, FilePosition, FileRange, Identifier, Position, Range,
    ReferenceWithSymbolDefinitions, ReferencedSymbolsResponse,
};
use crate::lsp::manager::Manager;
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{GotoDefinitionResponse, Location, Position as LspPosition, Range as LspRange, Url};
use std::collections::HashSet;
use std::sync::Arc;

use crate::service::types::errors::{PositionError, ServiceError};
use crate::service::types::response::{
    FileGroup, McpReferenceLocation, McpReferencesResponse, ReferenceType, TypeCounts,
};
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
    let file_identifiers = match manager.get_file_identifiers(file_path).await {
        Ok(identifiers) => Some(identifiers),
        Err(err) if err.is_ast_grep_missing() => None,
        Err(err) => return Err(err.into()),
    };

    // Try to find identifier at position first
    let identifier_result = match file_identifiers {
        Some(identifiers) => {
            find_identifier_at_position(
                identifiers,
                &FilePosition {
                    path: file_path.to_string(),
                    position: position.clone(),
                },
            )
            .await
        }
        None => Err(PositionError::IdentifierNotFound { closest: Vec::new() }),
    };

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
    let definition_keys = fetch_definition_keys(manager, file_path, position.clone()).await;
    let (reference_types, by_type) =
        classify_references_by_type(manager, &all_references, &definition_keys).await;

    let raw_response = if include_raw_response {
        serde_json::to_value(&all_references).ok()
    } else {
        None
    };
    // Only paginate when limit is explicitly specified; otherwise return all references
    let (references, limit_val, offset_val, truncated, reference_types) = match limit {
        Some(_) => {
            let (refs, pagination) = paginate_items(all_references, limit, offset);
            (
                refs,
                pagination.limit,
                pagination.offset,
                pagination.truncated,
                paginate_reference_types(reference_types, pagination.offset, pagination.limit),
            )
        }
        None => {
            // No limit specified - return all references, no truncation
            (all_references, total_count, 0, false, reference_types)
        }
    };
    let code_contexts = get_code_contexts(manager, &references, context_lines).await?;

    let mut reference_items = Vec::with_capacity(references.len());
    for (index, reference) in references.iter().enumerate() {
        let snippet = code_contexts
            .as_ref()
            .and_then(|contexts| contexts.get(index).cloned());
        let reference_type = reference_types
            .get(index)
            .copied()
            .unwrap_or(ReferenceType::Call);
        reference_items.push(reference_item_from_location(reference, snippet, reference_type));
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
///
/// When `include_externals` is false (default), external_symbols and not_found are empty.
/// Workspace symbols are deduplicated by (name + first definition location).
pub(crate) async fn find_referenced_symbols_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    full_scan: bool,
    include_externals: bool,
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
    let files_set: HashSet<String> = files.into_iter().collect();

    let mut workspace_symbols = Vec::new();
    let mut external_symbols = Vec::new();
    let mut not_found = Vec::new();
    // Track seen (name, first_definition_key) for deduplication
    let mut seen_workspace: HashSet<(String, String, u32, u32)> = HashSet::new();

    for (identifier, definitions) in unwrapped_definitions {
        if definitions.is_empty() {
            if include_externals {
                not_found.push(identifier);
            }
        } else {
            let has_internal_definition =
                definitions.iter().any(|def| files_set.contains(&def.path));
            if has_internal_definition {
                let mut symbols_with_definitions = Vec::new();
                for def in definitions.iter().filter(|def| files_set.contains(&def.path)) {
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
                    // Deduplicate: use (name, first_def_path, first_def_line, first_def_char) as key
                    let first_def = &symbols_with_definitions[0];
                    let dedup_key = (
                        identifier.name.clone(),
                        first_def.identifier_position.path.clone(),
                        first_def.identifier_position.position.line,
                        first_def.identifier_position.position.character,
                    );
                    if seen_workspace.insert(dedup_key) {
                        workspace_symbols.push(ReferenceWithSymbolDefinitions {
                            reference: identifier.clone(),
                            definitions: symbols_with_definitions,
                        });
                    }
                } else if include_externals {
                    not_found.push(identifier.clone());
                }
            } else if include_externals {
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

    if include_externals {
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
    }

    Ok(ReferencedSymbolsResponse {
        workspace_symbols,
        external_symbols,
        not_found,
    })
}

/// Checks if a URI refers to a file within the workspace.
fn is_workspace_file(uri: &Url) -> bool {
    uri.to_file_path()
        .map(|p| p.starts_with(get_mount_dir()))
        .unwrap_or(false)
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

    let mut filtered_refs: Vec<_> = references
        .into_iter()
        .filter(|reference| is_workspace_file(&reference.uri))
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

/// Reads a single source line from a file.
/// Uses range-based read to avoid loading entire file into memory.
async fn read_source_line(manager: &Manager, path: &str, line: u32) -> Option<String> {
    let range = LspRange {
        start: LspPosition { line, character: 0 },
        end: LspPosition { line: line + 1, character: 0 },
    };
    manager.read_source_code(path, Some(range)).await.ok()
}

/// Classifies references by type (definition, import, re-export, call).
/// Uses range-based reads to fetch only the lines needed for classification.
pub(crate) async fn classify_references_by_type(
    manager: &Manager,
    references: &[Location],
    definition_keys: &HashSet<(String, u32, u32)>,
) -> (Vec<ReferenceType>, TypeCounts) {
    let mut counts = TypeCounts::default();
    let mut types = Vec::with_capacity(references.len());

    for reference in references {
        let key = reference_key(reference);
        let mut reference_type = if definition_keys.contains(&key) {
            ReferenceType::Definition
        } else {
            ReferenceType::Call
        };

        if reference_type == ReferenceType::Call {
            let path = uri_to_relative_path_string(&reference.uri);
            let line_num = reference.range.start.line;

            if let Some(line) = read_source_line(manager, &path, line_num).await {
                if is_reexport_line(&line) {
                    reference_type = ReferenceType::ReExport;
                } else if is_import_line(&line) {
                    reference_type = ReferenceType::Import;
                }
            }
        }

        match reference_type {
            ReferenceType::Definition => counts.definition += 1,
            ReferenceType::Import => counts.import += 1,
            ReferenceType::Call => counts.call += 1,
            ReferenceType::ReExport => counts.reexport += 1,
        }
        types.push(reference_type);
    }

    (types, counts)
}

fn reference_key(location: &Location) -> (String, u32, u32) {
    (
        uri_to_relative_path_string(&location.uri),
        location.range.start.line,
        location.range.start.character,
    )
}

fn definition_keys_from_response(definitions: &GotoDefinitionResponse) -> HashSet<(String, u32, u32)> {
    let mut keys = HashSet::new();
    match definitions {
        GotoDefinitionResponse::Scalar(loc) => {
            keys.insert(reference_key(loc));
        }
        GotoDefinitionResponse::Array(locs) => {
            for loc in locs {
                keys.insert(reference_key(loc));
            }
        }
        GotoDefinitionResponse::Link(links) => {
            for link in links {
                let location = Location {
                    uri: link.target_uri.clone(),
                    range: link.target_selection_range,
                };
                keys.insert(reference_key(&location));
            }
        }
    }
    keys
}

async fn fetch_definition_keys(
    manager: &Manager,
    file_path: &str,
    position: Position,
) -> HashSet<(String, u32, u32)> {
    let lsp_position = LspPosition {
        line: position.line.saturating_sub(1),
        character: position.character.saturating_sub(1),
    };
    match manager.find_definition(file_path, lsp_position).await {
        Ok(definitions) => definition_keys_from_response(&definitions),
        Err(_) => HashSet::new(),
    }
}

fn paginate_reference_types(
    types: Vec<ReferenceType>,
    offset: u32,
    limit: u32,
) -> Vec<ReferenceType> {
    types
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect()
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

/// Detects if a line is a re-export statement.
/// Matches patterns like:
/// - `export { foo } from '...'`
/// - `export { foo as bar } from '...'`
/// - `export * from '...'`
/// - `export * as name from '...'`
/// - `pub use module::item;` (Rust)
pub(crate) fn is_reexport_line(line: &str) -> bool {
    let trimmed = line.trim();

    // Skip comments
    if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("/*") {
        return false;
    }

    // JavaScript/TypeScript re-exports: export { ... } from '...' or export * from '...'
    if trimmed.starts_with("export ") {
        // Check for "from" clause which indicates re-export
        if trimmed.contains(" from ") {
            return true;
        }
        // export * from ... (barrel export)
        if trimmed.starts_with("export *") {
            return true;
        }
    }

    // Rust pub use re-exports: pub use module::item
    if trimmed.starts_with("pub use ") {
        return true;
    }

    false
}
