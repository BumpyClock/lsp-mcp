// ABOUTME: Definition lookup operations (find_definition, find_implementation, definitions_in_file).
// ABOUTME: Handles symbol definition resolution and source code context fetching.

use crate::api_types::{
    CodeContext, FilePosition, FileRange, Identifier, ImplementationResponse, Position, Range,
    RelatedSymbols, Symbol,
};
use crate::lsp::manager::Manager;
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{Location, Position as LspPosition, Range as LspRange};
use std::sync::Arc;

use crate::service::types::errors::ServiceError;
use crate::service::types::response::{McpDefinitionResponse, McpSymbolsResponse};
use crate::service::utils::identifiers::find_identifier_at_position;
use crate::service::utils::pagination::paginate_items;
use crate::service::utils::signature::{enrich_symbol, is_internal_builder_symbol};
use crate::service::utils::transformations::{
    definition_item_from_location, definition_locations, definition_locations_lsp,
};

use super::hover::{count_references_impl, fetch_hover_info_impl};
use super::references::fetch_code_context;

/// Retrieves all symbol definitions in a file with enriched metadata.
pub(crate) async fn definitions_in_file_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<McpSymbolsResponse, ServiceError> {
    use crate::api_types::get_mount_dir;

    // Get file mtime
    let full_path = get_mount_dir().join(file_path);
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(|e| ServiceError::Lsp(crate::lsp::manager::LspManagerError::FileNotFound(
            format!("{}: {}", file_path, e)
        )))?;
    let mtime = metadata.modified()
        .map_err(|e| ServiceError::Lsp(crate::lsp::manager::LspManagerError::InternalError(
            format!("Failed to get mtime: {}", e)
        )))?;
    let mtime_rfc3339 = chrono::DateTime::<chrono::Utc>::from(mtime).to_rfc3339();

    // Get symbols from ast-grep
    let ast_symbols = manager.definitions_in_file_ast_grep(file_path).await?;
    let mut symbols: Vec<Symbol> = ast_symbols
        .into_iter()
        .filter(|s| s.rule_id != "local-variable")
        .map(Symbol::from)
        .filter(|s| !is_internal_builder_symbol(&s.name))
        .collect();

    // Enrich each symbol
    for symbol in &mut symbols {
        enrich_symbol(manager, file_path, symbol).await;
    }

    let (mut symbols, pagination) = paginate_items(symbols, limit, offset);

    // Clear paths from nested structures since McpSymbolsResponse.path provides it
    for symbol in &mut symbols {
        symbol.file_range.path.clear();
        symbol.identifier_position.path.clear();
    }

    Ok(McpSymbolsResponse {
        path: file_path.to_string(),
        mtime: mtime_rfc3339,
        symbols,
        limit: pagination.limit,
        offset: pagination.offset,
        truncated: pagination.truncated,
    })
}

/// Finds the definition of a symbol at the given position.
pub(crate) async fn find_definition_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    include_source_code: bool,
    include_raw_response: bool,
    context_lines: Option<u32>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<McpDefinitionResponse, ServiceError> {
    let file_identifiers = manager.get_file_identifiers(file_path).await?;
    let selected_identifier = find_identifier_at_position(
        file_identifiers,
        &FilePosition {
            path: file_path.to_string(),
            position: position.clone(),
        },
    )
    .await?;

    let definitions = manager
        .find_definition(
            file_path,
            LspPosition {
                line: position.line.saturating_sub(1),
                character: position.character.saturating_sub(1),
            },
        )
        .await?;

    let definition_locations = definition_locations_lsp(&definitions);
    let (definition_locations, pagination) =
        paginate_items(definition_locations, limit, offset);
    let source_code_context = if include_source_code {
        Some(fetch_definition_source_code(manager, &definition_locations).await?)
    } else {
        None
    };
    let snippet_contexts = match context_lines {
        Some(lines) => Some(fetch_code_context(manager, definition_locations.clone(), lines).await?),
        None => None,
    };

    let raw_response = if include_raw_response {
        Some(serde_json::to_value(&definitions)?)
    } else {
        None
    };

    let mut definition_items = Vec::with_capacity(definition_locations.len());
    let mut first_definition_path: Option<String> = None;
    for (index, location) in definition_locations.into_iter().enumerate() {
        let path = uri_to_relative_path_string(&location.uri);
        let is_external = path.contains("node_modules");

        if index == 0 {
            first_definition_path = Some(path.clone());
        }

        // Skip workspace-dependent operations for external files
        // (ast-grep and find_references require workspace files)
        let (symbol, snippet, reference_count) = if is_external {
            (None, None, None)
        } else {
            let symbol = manager
                .get_symbol_from_position(&path, &location.range.start)
                .await
                .ok();
            let snippet = snippet_contexts
                .as_ref()
                .and_then(|contexts| contexts.get(index).cloned());
            let ref_count = count_references_impl(manager, &path, &location.range.start).await;
            (symbol, snippet, ref_count)
        };

        // Hover may still work for external files (LSP handles it)
        let (signature, doc) = fetch_hover_info_impl(manager, &path, &location.range.start).await;

        definition_items.push(definition_item_from_location(
            &location,
            symbol,
            snippet,
            signature,
            doc,
            reference_count,
        ));
    }

    let related = compute_related_symbols(
        manager,
        first_definition_path.as_deref(),
        &selected_identifier,
    )
    .await;

    Ok(McpDefinitionResponse {
        raw_response,
        definitions: definition_items,
        source_code_context,
        selected_identifier,
        related: Some(related),
        limit: pagination.limit,
        offset: pagination.offset,
        truncated: pagination.truncated,
    })
}

/// Finds implementations of an interface or abstract method.
pub(crate) async fn find_implementation_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    include_raw_response: bool,
) -> Result<ImplementationResponse, ServiceError> {
    let file_identifiers = manager.get_file_identifiers(file_path).await?;
    let selected_identifier = find_identifier_at_position(
        file_identifiers,
        &FilePosition {
            path: file_path.to_string(),
            position: position.clone(),
        },
    )
    .await?;

    let implementations = manager
        .find_implementation(
            file_path,
            LspPosition {
                line: position.line.saturating_sub(1),
                character: position.character.saturating_sub(1),
            },
        )
        .await?;

    let raw_response = if include_raw_response {
        Some(serde_json::to_value(&implementations)?)
    } else {
        None
    };

    Ok(ImplementationResponse {
        raw_response,
        implementations: definition_locations(&implementations),
        selected_identifier,
    })
}

/// Fetches full source code context for definition locations.
pub(crate) async fn fetch_definition_source_code(
    manager: &Manager,
    definitions: &[Location],
) -> Result<Vec<CodeContext>, ServiceError> {
    let mut code_contexts = Vec::new();
    for definition in definitions.iter() {
        let relative_path = uri_to_relative_path_string(&definition.uri);
        let file_symbols = manager.definitions_in_file_ast_grep(&relative_path).await?;
        let symbol = file_symbols.iter().find(|s| {
            s.get_identifier_range().start.line == definition.range.start.line
                && s.get_identifier_range().start.column == definition.range.start.character
        });

        let source_code_context = match symbol {
            Some(ast_grep_match) => CodeContext {
                range: FileRange {
                    path: relative_path,
                    range: Range {
                        start: Position {
                            line: ast_grep_match.get_context_range().start.line + 1,
                            character: ast_grep_match.get_context_range().start.column + 1,
                        },
                        end: Position {
                            line: ast_grep_match.get_context_range().end.line + 1,
                            character: ast_grep_match.get_context_range().end.column + 1,
                        },
                    },
                },
                source_code: ast_grep_match.get_source_code(),
            },
            None => {
                let range = LspRange {
                    start: LspPosition {
                        line: definition.range.start.line.saturating_sub(3),
                        character: 0,
                    },
                    end: LspPosition {
                        line: definition.range.end.line + 3,
                        character: 0,
                    },
                };
                let source_code = manager.read_source_code(&relative_path, Some(range)).await?;
                CodeContext {
                    range: FileRange {
                        path: relative_path,
                        range: Range {
                            start: Position {
                                line: definition.range.start.line.saturating_sub(3) + 1,
                                character: 1,
                            },
                            end: Position {
                                line: definition.range.end.line + 3 + 1,
                                character: 1,
                            },
                        },
                    },
                    source_code,
                }
            }
        };

        code_contexts.push(source_code_context);
    }
    Ok(code_contexts)
}

/// Computes related symbols for a definition (sibling exports, implements, extends).
pub(crate) async fn compute_related_symbols(
    manager: &Manager,
    definition_file_path: Option<&str>,
    selected_identifier: &Identifier,
) -> RelatedSymbols {
    let mut related = RelatedSymbols::default();

    let Some(def_path) = definition_file_path else {
        return related;
    };

    if let Ok(file_symbols) = manager.definitions_in_file_ast_grep(def_path).await {
        let sibling_exports: Vec<Symbol> = file_symbols
            .into_iter()
            .filter(|s| s.rule_id != "local-variable" && s.rule_id != "all-identifiers")
            .filter(|s| s.meta_variables.single.name.text != selected_identifier.name)
            .map(Symbol::from)
            .collect();

        related.sibling_exports = sibling_exports;
    }

    related
}
