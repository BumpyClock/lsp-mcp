// ABOUTME: Definition lookup operations (find_definition, find_implementation, definitions_in_file).
// ABOUTME: Handles symbol definition resolution and source code context fetching.

use crate::api_types::{
    CodeContext, FilePosition, FileRange, Identifier, ImplementationResponse, Position, Range,
    RelatedSymbols, Symbol,
};
use crate::lsp::manager::Manager;
use crate::utils::external_file::{is_external_path, read_file_range};
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{DocumentSymbol, DocumentSymbolResponse, Location, Position as LspPosition, Range as LspRange, SymbolKind};
use std::sync::Arc;

use crate::service::types::errors::{PositionError, ServiceError};
use crate::service::types::response::{McpDefinitionResponse, McpSymbolsResponse};
use crate::service::utils::identifiers::find_identifier_at_position;
use crate::service::utils::pagination::paginate_items;
use crate::service::utils::signature::{enrich_symbol, extract_identifier_name_from_hover};
use crate::service::utils::transformations::{
    definition_item_from_location, definition_locations, definition_locations_lsp,
};
use log::{debug, warn};

use super::hover::{count_references_impl, fetch_hover_info_impl};

/// Maximum number of lines to include in source code context.
const MAX_SOURCE_CODE_LINES: usize = 100;

/// Truncates source code to a maximum number of lines with indicator.
fn truncate_source_code(code: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = code.lines().collect();
    if lines.len() <= max_lines {
        return code.to_string();
    }
    let truncated: String = lines[..max_lines].join("\n");
    format!("{}\n[truncated, {} total lines]", truncated, lines.len())
}

fn symbol_kind_to_string(kind: SymbolKind) -> String {
    match kind {
        SymbolKind::FILE => "File",
        SymbolKind::MODULE => "Module",
        SymbolKind::NAMESPACE => "Namespace",
        SymbolKind::PACKAGE => "Package",
        SymbolKind::CLASS => "Class",
        SymbolKind::METHOD => "Method",
        SymbolKind::PROPERTY => "Property",
        SymbolKind::FIELD => "Field",
        SymbolKind::CONSTRUCTOR => "Constructor",
        SymbolKind::ENUM => "Enum",
        SymbolKind::INTERFACE => "Interface",
        SymbolKind::FUNCTION => "Function",
        SymbolKind::VARIABLE => "Variable",
        SymbolKind::CONSTANT => "Constant",
        SymbolKind::STRING => "String",
        SymbolKind::NUMBER => "Number",
        SymbolKind::BOOLEAN => "Boolean",
        SymbolKind::ARRAY => "Array",
        SymbolKind::OBJECT => "Object",
        SymbolKind::KEY => "Key",
        SymbolKind::NULL => "Null",
        SymbolKind::ENUM_MEMBER => "EnumMember",
        SymbolKind::STRUCT => "Struct",
        SymbolKind::EVENT => "Event",
        SymbolKind::OPERATOR => "Operator",
        SymbolKind::TYPE_PARAMETER => "TypeParameter",
        _ => "Unknown",
    }.to_string()
}

fn convert_document_symbol(doc_sym: &DocumentSymbol, file_path: &str, is_top_level: bool) -> Symbol {
    let children = doc_sym.children.as_ref().map(|kids| {
        kids.iter()
            .map(|child| convert_document_symbol(child, file_path, false))
            .collect()
    });

    Symbol {
        name: doc_sym.name.clone(),
        kind: symbol_kind_to_string(doc_sym.kind),
        identifier_position: FilePosition {
            path: if is_top_level { String::new() } else { file_path.to_string() },
            position: Position {
                line: doc_sym.selection_range.start.line + 1,
                character: doc_sym.selection_range.start.character + 1,
            },
        },
        file_range: FileRange {
            path: if is_top_level { String::new() } else { file_path.to_string() },
            range: Range {
                start: Position {
                    line: doc_sym.range.start.line + 1,
                    character: doc_sym.range.start.character + 1,
                },
                end: Position {
                    line: doc_sym.range.end.line + 1,
                    character: doc_sym.range.end.character + 1,
                },
            },
        },
        signature: doc_sym.detail.clone(),
        exported: None,
        jsdoc_summary: None,
        dependencies: None,
        line_count: Some(doc_sym.range.end.line.saturating_sub(doc_sym.range.start.line) + 1),
        children,
    }
}

fn convert_symbol_information(sym_info: &lsp_types::SymbolInformation, _file_path: &str) -> Symbol {
    Symbol {
        name: sym_info.name.clone(),
        kind: symbol_kind_to_string(sym_info.kind),
        identifier_position: FilePosition {
            path: String::new(),
            position: Position {
                line: sym_info.location.range.start.line + 1,
                character: sym_info.location.range.start.character + 1,
            },
        },
        file_range: FileRange {
            path: String::new(),
            range: Range {
                start: Position {
                    line: sym_info.location.range.start.line + 1,
                    character: sym_info.location.range.start.character + 1,
                },
                end: Position {
                    line: sym_info.location.range.end.line + 1,
                    character: sym_info.location.range.end.character + 1,
                },
            },
        },
        signature: None,
        exported: None,
        jsdoc_summary: None,
        dependencies: None,
        line_count: Some(sym_info.location.range.end.line.saturating_sub(sym_info.location.range.start.line) + 1),
        children: None,
    }
}

/// Recursively enriches a symbol and all its children with LSP hover data.
/// Uses Box::pin to handle recursive async calls.
async fn enrich_symbol_tree(manager: &Manager, file_path: &str, symbol: &mut Symbol) {
    enrich_symbol(manager, file_path, symbol).await;

    if let Some(ref mut children) = symbol.children {
        for child in children {
            Box::pin(enrich_symbol_tree(manager, file_path, child)).await;
        }
    }
}

/// Retrieves all symbol definitions in a file with enriched metadata.
pub(crate) async fn definitions_in_file_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<McpSymbolsResponse, ServiceError> {
    use crate::api_types::get_mount_dir;

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

    let lsp_response = manager.document_symbol(file_path).await?;

    let mut symbols: Vec<Symbol> = match lsp_response {
        Some(DocumentSymbolResponse::Nested(doc_symbols)) => {
            doc_symbols
                .iter()
                .map(|ds| convert_document_symbol(ds, file_path, true))
                .collect()
        }
        Some(DocumentSymbolResponse::Flat(sym_infos)) => {
            sym_infos
                .iter()
                .map(|si| convert_symbol_information(si, file_path))
                .collect()
        }
        None => Vec::new(),
    };

    for symbol in &mut symbols {
        enrich_symbol_tree(manager, file_path, symbol).await;
    }

    let (symbols, pagination) = paginate_items(symbols, limit, offset);

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
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<McpDefinitionResponse, ServiceError> {
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

    // Try to get definitions from LSP regardless of identifier result
    let lsp_position = LspPosition {
        line: position.line.saturating_sub(1),
        character: position.character.saturating_sub(1),
    };
    let definitions = manager.find_definition(file_path, lsp_position).await?;
    let all_definition_locations = definition_locations_lsp(&definitions);

    // Determine selected_identifier with LSP hover fallback
    let selected_identifier = match identifier_result {
        Ok(id) => id,
        Err(PositionError::IdentifierNotFound { closest }) => {
            debug!(
                "find_definition_impl: identifier not found at position, trying LSP hover fallback"
            );

            // Check if LSP found definitions - if so, position is valid
            if all_definition_locations.is_empty() {
                // LSP also found nothing - return original error
                debug!("find_definition_impl: LSP also found no definitions, returning error");
                return Err(ServiceError::IdentifierSelection(
                    PositionError::IdentifierNotFound { closest },
                ));
            }

            // LSP found definitions, construct identifier from hover info
            if let Ok(Some(hover)) = manager.hover(file_path, lsp_position).await {
                let name = extract_identifier_name_from_hover(&hover.contents);
                debug!(
                    "find_definition_impl: constructed identifier '{}' from hover",
                    name
                );
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
                debug!("find_definition_impl: hover also failed, using 'unknown' identifier");
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

    let (definition_locations, pagination) =
        paginate_items(all_definition_locations, limit, offset);

    let workspace_files = manager.list_files().await.unwrap_or_default();

    let source_code_context = if include_source_code {
        Some(fetch_definition_source_code(manager, &definition_locations).await?)
    } else {
        None
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
        let is_external = !workspace_files.contains(&path);

        if index == 0 {
            first_definition_path = Some(path.clone());
        }

        let (symbol, reference_count) = if is_external {
            (None, None)
        } else {
            let symbol = manager
                .get_symbol_from_position(&path, &location.range.start)
                .await
                .ok();
            let ref_count = count_references_impl(manager, &path, &location.range.start).await;
            (symbol, ref_count)
        };

        let (signature, doc) = fetch_hover_info_impl(manager, &path, &location.range.start).await;

        definition_items.push(definition_item_from_location(
            &location,
            symbol,
            None, // snippets removed - source_code_context provides full context
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

    // Try to find identifier at position first
    let identifier_result = find_identifier_at_position(
        file_identifiers,
        &FilePosition {
            path: file_path.to_string(),
            position: position.clone(),
        },
    )
    .await;

    // Try to get implementations from LSP regardless of identifier result
    let lsp_position = LspPosition {
        line: position.line.saturating_sub(1),
        character: position.character.saturating_sub(1),
    };
    let implementations = manager.find_implementation(file_path, lsp_position).await?;
    let all_impl_locations = definition_locations(&implementations);

    // Determine selected_identifier with LSP hover fallback
    let selected_identifier = match identifier_result {
        Ok(id) => id,
        Err(PositionError::IdentifierNotFound { closest }) => {
            debug!(
                "find_implementation_impl: identifier not found at position, trying LSP hover fallback"
            );

            // Check if LSP found implementations - if so, position is valid
            if all_impl_locations.is_empty() {
                // LSP also found nothing - return original error
                debug!("find_implementation_impl: LSP also found no implementations, returning error");
                return Err(ServiceError::IdentifierSelection(
                    PositionError::IdentifierNotFound { closest },
                ));
            }

            // LSP found implementations, construct identifier from hover info
            if let Ok(Some(hover)) = manager.hover(file_path, lsp_position).await {
                let name = extract_identifier_name_from_hover(&hover.contents);
                debug!(
                    "find_implementation_impl: constructed identifier '{}' from hover",
                    name
                );
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
                debug!("find_implementation_impl: hover also failed, using 'unknown' identifier");
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

    let raw_response = if include_raw_response {
        Some(serde_json::to_value(&implementations)?)
    } else {
        None
    };

    Ok(ImplementationResponse {
        raw_response,
        implementations: all_impl_locations,
        selected_identifier,
    })
}

/// Fetches full source code context for definition locations.
/// Source code is truncated to MAX_SOURCE_CODE_LINES (100 lines).
///
/// For workspace files: uses ast_grep for precise symbol bounds.
/// For external files (e.g., node_modules): reads file directly with context around definition.
pub(crate) async fn fetch_definition_source_code(
    manager: &Manager,
    definitions: &[Location],
) -> Result<Vec<CodeContext>, ServiceError> {
    let mut code_contexts = Vec::new();

    for definition in definitions.iter() {
        let relative_path = uri_to_relative_path_string(&definition.uri);

        if is_external_path(&relative_path) {
            let start_line = definition.range.start.line.saturating_sub(3);
            let end_line = definition.range.end.line.saturating_add(10);

            match read_file_range(&relative_path, start_line, end_line).await {
                Ok(source_code) => {
                    code_contexts.push(CodeContext {
                        range: FileRange {
                            path: relative_path,
                            range: Range {
                                start: Position {
                                    line: start_line + 1,
                                    character: 1,
                                },
                                end: Position {
                                    line: end_line + 1,
                                    character: 1,
                                },
                            },
                        },
                        source_code: truncate_source_code(&source_code, MAX_SOURCE_CODE_LINES),
                    });
                }
                Err(e) => {
                    warn!("Failed to read external file {}: {}", relative_path, e);
                }
            }
            continue;
        }

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
                source_code: truncate_source_code(&ast_grep_match.get_source_code(), MAX_SOURCE_CODE_LINES),
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
                    source_code: truncate_source_code(&source_code, MAX_SOURCE_CODE_LINES),
                }
            }
        };

        code_contexts.push(source_code_context);
    }
    Ok(code_contexts)
}

/// Computes related symbols for a definition (sibling exports, implements, extends).
///
/// For external files (e.g., node_modules): returns empty RelatedSymbols since ast_grep
/// cannot parse files outside the workspace.
pub(crate) async fn compute_related_symbols(
    manager: &Manager,
    definition_file_path: Option<&str>,
    selected_identifier: &Identifier,
) -> RelatedSymbols {
    let mut related = RelatedSymbols::default();

    let Some(def_path) = definition_file_path else {
        return related;
    };

    if is_external_path(def_path) {
        return related;
    }

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
