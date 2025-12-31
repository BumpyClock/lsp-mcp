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
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::service::types::errors::{PositionError, ServiceError};
use crate::service::types::response::{McpDefinitionResponse, McpSymbolsResponse};
use crate::service::utils::identifiers::find_identifier_at_position;
use crate::service::utils::pagination::paginate_items;
use crate::service::utils::signature::{enrich_symbol, extract_identifier_name_from_hover, truncate_signature};
use crate::service::utils::transformations::{
    definition_item_from_location, definition_locations, definition_locations_lsp,
};
use crate::service::utils::external::ExternalInfo;
use log::{debug, warn};

use super::hover::{count_references_impl, fetch_hover_info_impl};

/// Maximum number of lines to include in source code context.
const MAX_SOURCE_CODE_LINES: usize = 100;

/// Checks if a source line looks like a TypeScript overload signature.
///
/// Returns true if the line ends with `;` and doesn't contain `{` or `=>`,
/// which indicates an overload signature rather than an implementation.
fn is_overload_signature(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.ends_with(';') && !trimmed.contains('{') && !trimmed.contains("=>")
}

fn is_overload_scan_skip_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
}

fn normalize_symbol_detail(detail: &Option<String>) -> Option<String> {
    detail
        .as_ref()
        .and_then(|value| if value.trim().is_empty() { None } else { Some(truncate_signature(value, None)) })
}

/// Reads a single source line from a file.
async fn read_source_line(manager: &Arc<Manager>, path: &str, line: u32) -> Option<String> {
    let range = LspRange {
        start: LspPosition { line, character: 0 },
        end: LspPosition { line: line + 1, character: 0 },
    };
    manager.read_source_code(path, Some(range)).await.ok()
}

/// Searches for the implementation of an overloaded function by scanning lines after the signature.
///
/// TypeScript overloads follow this pattern:
/// ```typescript
/// function foo(x: string): void;    // overload signature 1
/// function foo(x: number): void;    // overload signature 2
/// function foo(x: string | number): void { ... }  // implementation (immediately after signatures)
/// ```
///
/// This function scans up to 10 lines after the given signature to find the implementation.
async fn find_overload_implementation(
    manager: &Arc<Manager>,
    loc: &Location,
    path: &str,
) -> Option<Location> {
    // Scan up to 10 lines after the signature to find the implementation
    for offset in 1..=10 {
        let check_line = loc.range.start.line + offset;
        if let Some(line_content) = read_source_line(manager, path, check_line).await {
            // Skip empty lines and comment lines
            if is_overload_scan_skip_line(&line_content) {
                continue;
            }

            // Check if this line is the implementation (has `{` or `=>`)
            if !is_overload_signature(&line_content) {
                // This is likely the implementation
                debug!(
                    "find_overload_implementation: found implementation at line {}",
                    check_line + 1
                );
                return Some(Location {
                    uri: loc.uri.clone(),
                    range: LspRange {
                        start: LspPosition { line: check_line, character: 0 },
                        end: LspPosition { line: check_line + 1, character: 0 },
                    },
                });
            }
            // If it's another overload signature, keep scanning
        } else {
            // Couldn't read line, stop scanning
            break;
        }
    }
    None
}

/// Filters definition locations to prefer implementations over overload signatures.
///
/// When multiple locations are in the same file (TypeScript overloads), reads
/// the source line to classify each as signature or implementation:
/// - Signature: Line ends with `;` (e.g., `function foo(): void;`)
/// - Implementation: Line contains `{` or `=>` or doesn't end with `;`
///
/// For single locations that are overload signatures, searches for the implementation
/// by scanning subsequent lines in the same file.
///
/// Returns only implementations if any are found, otherwise all locations.
async fn filter_overload_implementations(
    manager: &Arc<Manager>,
    locations: Vec<Location>,
) -> Vec<Location> {
    if locations.is_empty() {
        return locations;
    }

    // Handle single location case - check if it's an overload signature
    if locations.len() == 1 {
        let loc = &locations[0];
        let path = uri_to_relative_path_string(&loc.uri);

        if let Some(line_content) = read_source_line(manager, &path, loc.range.start.line).await {
            if is_overload_signature(&line_content) {
                debug!(
                    "filter_overload_implementations: single location is overload signature at line {}",
                    loc.range.start.line + 1
                );
                // Try to find the implementation
                if let Some(impl_loc) = find_overload_implementation(manager, loc, &path).await {
                    return vec![impl_loc];
                }
            }
        }
        return locations;
    }

    let first_path = uri_to_relative_path_string(&locations[0].uri);
    let all_same_file = locations
        .iter()
        .all(|loc| uri_to_relative_path_string(&loc.uri) == first_path);

    if !all_same_file {
        return locations;
    }

    let mut implementations = Vec::new();
    let mut signatures = Vec::new();

    for loc in locations {
        let path = uri_to_relative_path_string(&loc.uri);
        match read_source_line(manager, &path, loc.range.start.line).await {
            Some(line) => {
                let preview: String = line.trim().chars().take(60).collect();
                if is_overload_signature(&line) {
                    debug!(
                        "filter_overload_implementations: signature at line {}: {}",
                        loc.range.start.line + 1,
                        preview
                    );
                    signatures.push(loc);
                } else {
                    debug!(
                        "filter_overload_implementations: implementation at line {}: {}",
                        loc.range.start.line + 1,
                        preview
                    );
                    implementations.push(loc);
                }
            }
            None => {
                debug!(
                    "filter_overload_implementations: failed to read line {}",
                    loc.range.start.line + 1
                );
                implementations.push(loc);
            }
        }
    }

    if implementations.is_empty() {
        debug!(
            "filter_overload_implementations: no implementations found, returning all {} signatures",
            signatures.len()
        );
        signatures
    } else {
        debug!(
            "filter_overload_implementations: returning {} implementations (filtered {} signatures)",
            implementations.len(),
            signatures.len()
        );
        implementations
    }
}

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
        signature: normalize_symbol_detail(&doc_sym.detail),
        exported: None,
        jsdoc_summary: None,
        dependencies: None,
        line_count: Some(doc_sym.range.end.line.saturating_sub(doc_sym.range.start.line) + 1),
        children,
        snippet: None,
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
        snippet: None,
    }
}

#[derive(Clone)]
struct DefinitionCandidate {
    location: Location,
    path: String,
    is_external: bool,
    line_span: u32,
    original_index: usize,
}

fn sort_definition_candidates(mut candidates: Vec<DefinitionCandidate>) -> Vec<Location> {
    candidates.sort_by(|a, b| {
        a.is_external
            .cmp(&b.is_external)
            .then_with(|| b.line_span.cmp(&a.line_span))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.location.range.start.line.cmp(&b.location.range.start.line))
            .then_with(|| a.location.range.start.character.cmp(&b.location.range.start.character))
            .then_with(|| a.original_index.cmp(&b.original_index))
    });
    candidates.into_iter().map(|c| c.location).collect()
}

fn strip_symbol_children(mut symbols: Vec<Symbol>) -> Vec<Symbol> {
    for symbol in &mut symbols {
        symbol.children = None;
    }
    symbols
}

fn filter_flat_symbol_information(
    sym_infos: Vec<lsp_types::SymbolInformation>,
    include_locals: bool,
) -> Vec<lsp_types::SymbolInformation> {
    if include_locals {
        return sym_infos;
    }
    sym_infos
        .into_iter()
        .filter(|sym| sym.container_name.is_none())
        .collect()
}

async fn rank_definition_locations(
    manager: &Arc<Manager>,
    locations: Vec<Location>,
    workspace_files: &[String],
) -> Vec<Location> {
    let workspace_set: HashSet<&str> = workspace_files.iter().map(|s| s.as_str()).collect();
    let mut candidates = Vec::with_capacity(locations.len());

    for (index, location) in locations.into_iter().enumerate() {
        let path = uri_to_relative_path_string(&location.uri);
        let is_external = ExternalInfo::from_path(&path).is_some()
            || (!workspace_set.is_empty() && !workspace_set.contains(path.as_str()));
        let line_span = if is_external {
            0
        } else {
            manager
                .get_symbol_from_position(&path, &location.range.start)
                .await
                .ok()
                .map(|symbol| {
                    symbol
                        .file_range
                        .range
                        .end
                        .line
                        .saturating_sub(symbol.file_range.range.start.line)
                })
                .unwrap_or(0)
        };
        candidates.push(DefinitionCandidate {
            location,
            path,
            is_external,
            line_span,
            original_index: index,
        });
    }

    sort_definition_candidates(candidates)
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
    include_locals: bool,
    limit: Option<u32>,
    offset: Option<u32>,
    context_lines: u32,
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
            let filtered = filter_flat_symbol_information(sym_infos, include_locals);
            filtered
                .iter()
                .map(|si| convert_symbol_information(si, file_path))
                .collect()
        }
        None => Vec::new(),
    };

    if include_locals {
        for symbol in &mut symbols {
            enrich_symbol_tree(manager, file_path, symbol).await;
        }
    } else {
        for symbol in &mut symbols {
            enrich_symbol(manager, file_path, symbol).await;
        }
        symbols = strip_symbol_children(symbols);
    }

    let (mut symbols, pagination) = paginate_items(symbols, limit, offset);

    if context_lines > 0 {
        attach_snippets_to_symbols(manager, file_path, &mut symbols, context_lines).await;
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

async fn attach_snippets_to_symbols(
    manager: &Arc<Manager>,
    file_path: &str,
    symbols: &mut [Symbol],
    context_lines: u32,
) {
    for symbol in symbols.iter_mut() {
        attach_snippet_to_symbol(manager, file_path, symbol, context_lines).await;
        if let Some(ref mut children) = symbol.children {
            Box::pin(attach_snippets_to_symbols(manager, file_path, children, context_lines)).await;
        }
    }
}

async fn attach_snippet_to_symbol(
    manager: &Arc<Manager>,
    file_path: &str,
    symbol: &mut Symbol,
    context_lines: u32,
) {
    use crate::api_types::{CodeContext, FileRange, Range};

    let line = symbol.identifier_position.position.line;
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

    match manager.read_source_code(file_path, Some(lsp_range)).await {
        Ok(source_code) => {
            symbol.snippet = Some(CodeContext {
                range: FileRange {
                    path: file_path.to_string(),
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
            debug!("Failed to read snippet for symbol {}: {}", symbol.name, e);
        }
    }
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
    let raw_definition_locations = definition_locations_lsp(&definitions);

    // Filter overload signatures, preferring implementations
    let all_definition_locations =
        filter_overload_implementations(manager, raw_definition_locations).await;

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

    let workspace_files = manager.list_files().await.unwrap_or_default();
    let ordered_locations =
        rank_definition_locations(manager, all_definition_locations, &workspace_files).await;
    let (definition_locations, pagination) = paginate_items(ordered_locations, limit, offset);

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

    // Separate external paths (handle immediately) from workspace paths (batch by file)
    let mut external_definitions: Vec<&Location> = Vec::new();
    let mut workspace_by_file: HashMap<String, Vec<&Location>> = HashMap::new();

    for definition in definitions.iter() {
        let relative_path = uri_to_relative_path_string(&definition.uri);
        if is_external_path(&relative_path) {
            external_definitions.push(definition);
        } else {
            workspace_by_file
                .entry(relative_path)
                .or_default()
                .push(definition);
        }
    }

    // Process external definitions (no ast-grep, just read file ranges)
    for definition in external_definitions {
        let relative_path = uri_to_relative_path_string(&definition.uri);
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
    }

    // Process workspace definitions batched by file (one ast-grep call per unique file)
    for (relative_path, file_definitions) in workspace_by_file {
        // Call ast-grep once for this file
        let file_symbols = manager.definitions_in_file_ast_grep(&relative_path).await?;

        for definition in file_definitions {
            let symbol = file_symbols.iter().find(|s| {
                s.get_identifier_range().start.line == definition.range.start.line
                    && s.get_identifier_range().start.column == definition.range.start.character
            });

            let source_code_context = match symbol {
                Some(ast_grep_match) => CodeContext {
                    range: FileRange {
                        path: relative_path.clone(),
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
                    source_code: truncate_source_code(
                        &ast_grep_match.get_source_code(),
                        MAX_SOURCE_CODE_LINES,
                    ),
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
                    let source_code = manager
                        .read_source_code(&relative_path, Some(range))
                        .await?;
                    CodeContext {
                        range: FileRange {
                            path: relative_path.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{FilePosition, FileRange, Position, Range, Symbol};
    use lsp_types::{
        DocumentSymbol, Location, Position as LspPosition, Range as LspRange, SymbolInformation,
        SymbolKind, Url,
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

    fn make_location(path: &str, line: u32, character: u32) -> Location {
        let uri = Url::from_file_path(path).expect("negative: uri creation failed");
        Location {
            uri,
            range: LspRange {
                start: LspPosition { line, character },
                end: LspPosition {
                    line: line + 1,
                    character: character + 2,
                },
            },
        }
    }

    #[test]
    fn is_overload_signature_detects_typescript_signature_ending_with_semicolon() {
        assert!(
            is_overload_signature("function scoreMember(member: LibraryMember): number;"),
            "function ending with semicolon must be signature"
        );
        assert!(
            is_overload_signature("  export function foo(x: string): void;  "),
            "signature with whitespace must be detected"
        );
    }

    #[test]
    fn is_overload_signature_rejects_implementation_with_brace() {
        assert!(
            !is_overload_signature("function scoreMember(member: LibraryMember): number {"),
            "function with opening brace must not be signature"
        );
        assert!(
            !is_overload_signature("export function foo() { return 42; }"),
            "inline function must not be signature"
        );
    }

    #[test]
    fn is_overload_signature_rejects_arrow_function() {
        assert!(
            !is_overload_signature("const foo = (x: string) => x.length;"),
            "arrow function must not be signature"
        );
        assert!(
            !is_overload_signature("export const bar = () => {};"),
            "arrow function with empty body must not be signature"
        );
    }

    #[test]
    fn is_overload_signature_rejects_line_without_semicolon() {
        assert!(
            !is_overload_signature("function foo()"),
            "line without semicolon must not be signature"
        );
        assert!(
            !is_overload_signature("export function bar(x: number)"),
            "incomplete declaration must not be signature"
        );
    }

    #[test]
    fn is_overload_signature_handles_empty_and_whitespace() {
        assert!(
            !is_overload_signature(""),
            "empty line must not be signature"
        );
        assert!(
            !is_overload_signature("   "),
            "whitespace-only line must not be signature"
        );
    }

    #[test]
    fn is_overload_scan_skip_line_skips_jsdoc_and_comments() {
        assert!(
            is_overload_scan_skip_line(" * Score a member"),
            "jsdoc interior lines must be skipped"
        );
        assert!(
            is_overload_scan_skip_line("*/"),
            "jsdoc end lines must be skipped"
        );
        assert!(
            is_overload_scan_skip_line("/**"),
            "jsdoc start lines must be skipped"
        );
        assert!(
            is_overload_scan_skip_line("// comment"),
            "single-line comments must be skipped"
        );
        assert!(
            is_overload_scan_skip_line("/* block */"),
            "block comment lines must be skipped"
        );
    }

    #[test]
    fn is_overload_scan_skip_line_allows_code_lines() {
        assert!(
            !is_overload_scan_skip_line("export function scoreMember(member: string): number;"),
            "code lines must not be skipped"
        );
    }

    #[test]
    fn convert_document_symbol_ignores_empty_detail() {
        let doc_symbol = DocumentSymbol {
            name: "example".to_string(),
            detail: Some("   ".to_string()),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: LspRange {
                start: LspPosition { line: 2, character: 0 },
                end: LspPosition { line: 4, character: 0 },
            },
            selection_range: LspRange {
                start: LspPosition { line: 2, character: 3 },
                end: LspPosition { line: 2, character: 10 },
            },
            children: None,
        };

        let symbol = convert_document_symbol(&doc_symbol, "src/test.ts", false);

        assert!(
            symbol.signature.is_none(),
            "empty detail must not become a signature"
        );
    }

    #[test]
    fn convert_document_symbol_truncates_giant_detail() {
        let giant_type = format!("{{ {} }}", "field: string; ".repeat(50));
        let doc_symbol = DocumentSymbol {
            name: "hugeConfig".to_string(),
            detail: Some(giant_type.clone()),
            kind: SymbolKind::CONSTANT,
            tags: None,
            deprecated: None,
            range: LspRange {
                start: LspPosition { line: 7, character: 0 },
                end: LspPosition { line: 9, character: 0 },
            },
            selection_range: LspRange {
                start: LspPosition { line: 7, character: 6 },
                end: LspPosition { line: 7, character: 16 },
            },
            children: None,
        };

        let symbol = convert_document_symbol(&doc_symbol, "src/config.ts", false);

        assert!(
            symbol.signature.is_some(),
            "negative: giant detail must produce a signature"
        );
        let sig = symbol.signature.unwrap();
        assert!(
            sig.len() <= 103,
            "negative: giant detail must be truncated to max length"
        );
        assert!(
            sig.ends_with("..."),
            "negative: truncated signature must end with ellipsis"
        );
    }

    #[test]
    fn sort_definition_candidates_prefers_internal_over_external() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let internal_path = temp_dir
            .path()
            .join(format!("internal_{}{}.rs", unicode, random_irregular_string()));
        let external_path = temp_dir
            .path()
            .join(format!("external_{}{}.rs", unicode, random_irregular_string()));
        fs::write(&internal_path, "fn internal() {}").expect("negative: write failed");
        fs::write(&external_path, "fn external() {}").expect("negative: write failed");

        let internal = DefinitionCandidate {
            location: make_location(internal_path.to_str().unwrap(), 5, 3),
            path: internal_path.to_string_lossy().into_owned(),
            is_external: false,
            line_span: 1,
            original_index: 1,
        };
        let external = DefinitionCandidate {
            location: make_location(external_path.to_str().unwrap(), 1, 1),
            path: external_path.to_string_lossy().into_owned(),
            is_external: true,
            line_span: 20,
            original_index: 0,
        };

        let result = sort_definition_candidates(vec![external, internal]);

        assert_eq!(
            result[0].uri,
            Url::from_file_path(internal_path).expect("negative: uri creation failed"),
            "negative: internal definition should be ranked first"
        );
    }

    #[test]
    fn sort_definition_candidates_prefers_larger_spans_when_internal() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        let path = temp_dir.path().join(format!("file_{}.rs", random_irregular_string()));
        fs::write(&path, "fn a() {}\nfn b() {}").expect("negative: write failed");

        let small_span = DefinitionCandidate {
            location: make_location(path.to_str().unwrap(), 1, 2),
            path: path.to_string_lossy().into_owned(),
            is_external: false,
            line_span: 1,
            original_index: 0,
        };
        let large_span = DefinitionCandidate {
            location: make_location(path.to_str().unwrap(), 5, 2),
            path: path.to_string_lossy().into_owned(),
            is_external: false,
            line_span: 10,
            original_index: 1,
        };

        let result = sort_definition_candidates(vec![small_span, large_span]);

        assert_eq!(
            result[0].range.start.line, 5,
            "negative: larger span should be ranked first"
        );
    }

    #[test]
    fn sort_definition_candidates_preserves_order_when_equal() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        let path = temp_dir.path().join(format!("file_{}.rs", random_irregular_string()));
        fs::write(&path, "fn a() {}\nfn b() {}").expect("negative: write failed");

        let first = DefinitionCandidate {
            location: make_location(path.to_str().unwrap(), 1, 2),
            path: path.to_string_lossy().into_owned(),
            is_external: false,
            line_span: 1,
            original_index: 0,
        };
        let second = DefinitionCandidate {
            location: make_location(path.to_str().unwrap(), 2, 2),
            path: path.to_string_lossy().into_owned(),
            is_external: false,
            line_span: 1,
            original_index: 1,
        };

        let result = sort_definition_candidates(vec![first, second]);

        assert_eq!(
            result[0].range.start.line, 1,
            "negative: original order should be preserved"
        );
    }

    #[test]
    fn strip_symbol_children_removes_nested_symbols() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let child = Symbol {
            name: format!("child_{}{}", unicode, random_irregular_string()),
            kind: "function".to_string(),
            identifier_position: FilePosition {
                path: "src/lib.rs".to_string(),
                position: Position { line: 2, character: 1 },
            },
            file_range: FileRange {
                path: "src/lib.rs".to_string(),
                range: Range {
                    start: Position { line: 2, character: 1 },
                    end: Position { line: 3, character: 1 },
                },
            },
            signature: None,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
            children: None,
            snippet: None,
        };

        let mut parent = child.clone();
        parent.name = format!("parent_{}{}", unicode, random_irregular_string());
        parent.children = Some(vec![child]);

        let result = strip_symbol_children(vec![parent]);

        assert!(
            result[0].children.is_none(),
            "negative: nested symbols must be stripped"
        );
    }

    #[test]
    #[allow(deprecated)]
    fn filter_flat_symbol_information_excludes_nested_when_disabled() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let top_level = SymbolInformation {
            name: format!("top_{}{}", unicode, random_irregular_string()),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: make_location("/tmp/top.rs", 1, 1),
            container_name: None,
        };
        let nested = SymbolInformation {
            name: format!("nested_{}{}", unicode, random_irregular_string()),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: make_location("/tmp/nested.rs", 2, 1),
            container_name: Some("Container".to_string()),
        };

        let result = filter_flat_symbol_information(vec![top_level, nested], false);

        assert_eq!(result.len(), 1, "negative: nested symbols must be filtered");
        assert!(result[0].container_name.is_none(), "negative: container must be none");
    }
}
