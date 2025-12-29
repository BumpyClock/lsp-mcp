// ABOUTME: Domain service layer for LSP-backed code navigation operations.
// ABOUTME: Provides async methods for symbol lookup, references, and file access.
use crate::api_types::{
    CallHierarchyItemInfo, CodeContext, Diagnostic, DiagnosticsResponse, FileDiagnostics,
    FilePosition, FileRange, HoverContents, HoverResponse, Identifier, ImplementationResponse,
    IncomingCallInfo, IncomingCallsResponse, LspStatus, OutgoingCallInfo, OutgoingCallsResponse,
    Position, PrepareCallHierarchyResponse, Range, ReferenceWithSymbolDefinitions,
    ReferencedSymbolsResponse, SupportedLanguages, Symbol, WorkspaceSymbolInfo,
    WorkspaceSymbolResponse,
};
use crate::lsp::manager::{LspManagerError, Manager};
use crate::mcp_response::normalize_kind;
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{GotoDefinitionResponse, Location, Position as LspPosition, Range as LspRange};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Provides code navigation operations over a workspace manager.
///
/// # Example
/// ```
/// use std::sync::Arc;
/// use lsproxy::lsp::manager::Manager;
/// use lsproxy::service::create_service;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = Arc::new(Manager::new("/tmp").await?);
/// let service = create_service(manager);
/// let _files = service.list_files(None, None).await?;
/// # Ok(())
/// # }
/// ```
pub struct LspService {
    manager: Arc<Manager>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpDefinitionLocation {
    pub path: String,
    pub position: Position,
    pub definition_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<CodeContext>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpDefinitionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    pub definitions: Vec<McpDefinitionLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_code_context: Option<Vec<CodeContext>>,
    pub selected_identifier: Identifier,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpReferenceLocation {
    pub path: String,
    pub position: Position,
    pub symbol_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<CodeContext>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpReferencesResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    pub references: Vec<McpReferenceLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<CodeContext>>,
    pub selected_identifier: Identifier,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpSymbolsResponse {
    /// Path to the file, relative to workspace root
    pub path: String,
    /// File modification time in RFC3339 UTC format
    pub mtime: String,
    pub symbols: Vec<Symbol>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpIdentifierResponse {
    pub identifiers: Vec<Identifier>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpListFilesResponse {
    pub files: Vec<String>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Clone)]
struct Pagination {
    limit: u32,
    offset: u32,
    truncated: bool,
}

pub fn create_service(manager: Arc<Manager>) -> LspService {
    LspService { manager }
}

#[derive(Debug)]
pub enum ServiceError {
    Lsp(LspManagerError),
    IdentifierSelection(PositionError),
    Serialization(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::Lsp(e) => write!(f, "Operation failed because {e}"),
            ServiceError::IdentifierSelection(e) => {
                write!(f, "Identifier selection failed because {e}")
            }
            ServiceError::Serialization(message) => {
                write!(f, "Serialization failed because {message}")
            }
        }
    }
}

impl Error for ServiceError {}

impl From<LspManagerError> for ServiceError {
    fn from(err: LspManagerError) -> Self {
        ServiceError::Lsp(err)
    }
}

impl From<PositionError> for ServiceError {
    fn from(err: PositionError) -> Self {
        ServiceError::IdentifierSelection(err)
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(err: serde_json::Error) -> Self {
        ServiceError::Serialization(err.to_string())
    }
}

#[derive(Debug)]
pub enum PositionError {
    IdentifierNotFound { closest: Vec<Identifier> },
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PositionError::IdentifierNotFound { closest } => write!(
                f,
                "No identifier found at position with {} nearby matches",
                closest.len()
            ),
        }
    }
}

impl Error for PositionError {}

impl LspService {
    pub async fn definitions_in_file(
        &self,
        file_path: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpSymbolsResponse, ServiceError> {
        let symbols = self.manager.definitions_in_file_ast_grep(file_path).await?;
        let symbols: Vec<Symbol> = symbols
            .into_iter()
            .filter(|s| s.rule_id != "local-variable")
            .map(Symbol::from)
            .collect();
        let (symbols, pagination) = paginate_items(symbols, limit, offset);
        Ok(McpSymbolsResponse {
            symbols,
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
        })
    }

    pub async fn find_definition(
        &self,
        file_path: &str,
        position: Position,
        include_source_code: bool,
        include_raw_response: bool,
        context_lines: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpDefinitionResponse, ServiceError> {
        let file_identifiers = self.manager.get_file_identifiers(file_path).await?;
        let selected_identifier = find_identifier_at_position(
            file_identifiers,
            &FilePosition {
                path: file_path.to_string(),
                position: position.clone(),
            },
        )
        .await?;

        let definitions = self
            .manager
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
            Some(fetch_definition_source_code(&self.manager, &definition_locations).await?)
        } else {
            None
        };
        let snippet_contexts = match context_lines {
            Some(lines) => Some(fetch_code_context(&self.manager, definition_locations.clone(), lines).await?),
            None => None,
        };

        let raw_response = if include_raw_response {
            Some(serde_json::to_value(&definitions)?)
        } else {
            None
        };

        let mut definition_items = Vec::with_capacity(definition_locations.len());
        for (index, location) in definition_locations.into_iter().enumerate() {
            let path = uri_to_relative_path_string(&location.uri);
            let symbol = match self
                .manager
                .get_symbol_from_position(&path, &location.range.start)
                .await
            {
                Ok(symbol) => Some(symbol),
                Err(_) => None,
            };
            let snippet = snippet_contexts
                .as_ref()
                .and_then(|contexts| contexts.get(index).cloned());
            definition_items.push(definition_item_from_location(&location, symbol, snippet));
        }

        Ok(McpDefinitionResponse {
            raw_response,
            definitions: definition_items,
            source_code_context,
            selected_identifier,
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
        })
    }

    pub async fn find_references(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
        context_lines: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpReferencesResponse, ServiceError> {
        let file_identifiers = self.manager.get_file_identifiers(file_path).await?;
        let selected_identifier = find_identifier_at_position(
            file_identifiers,
            &FilePosition {
                path: file_path.to_string(),
                position: position.clone(),
            },
        )
        .await?;

        let references = find_and_filter_references(
            &self.manager,
            &FilePosition {
                path: file_path.to_string(),
                position: position.clone(),
            },
        )
        .await?;

        let raw_response = if include_raw_response {
            serde_json::to_value(&references).ok()
        } else {
            None
        };
        let (references, pagination) = paginate_items(references, limit, offset);
        let code_contexts = get_code_contexts(&self.manager, &references, context_lines).await?;

        let mut reference_items = Vec::with_capacity(references.len());
        for (index, reference) in references.into_iter().enumerate() {
            let snippet = code_contexts
                .as_ref()
                .and_then(|contexts| contexts.get(index).cloned());
            reference_items.push(reference_item_from_location(&reference, snippet));
        }

        Ok(McpReferencesResponse {
            raw_response,
            references: reference_items,
            context: code_contexts,
            selected_identifier,
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
        })
    }

    pub async fn find_referenced_symbols(
        &self,
        file_path: &str,
        position: Position,
        full_scan: bool,
    ) -> Result<ReferencedSymbolsResponse, ServiceError> {
        let referenced_symbols = self
            .manager
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

        let files = self.manager.list_files().await?;

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
                        if let Ok(symbol) = self
                            .manager
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

    pub async fn find_identifier(
        &self,
        file_path: &str,
        name: &str,
        position: Option<Position>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpIdentifierResponse, ServiceError> {
        let file_identifiers = self.manager.get_file_identifiers(file_path).await?;
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

    pub async fn list_files(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpListFilesResponse, ServiceError> {
        let files = self.manager.list_files().await?;
        let (files, pagination) = paginate_items(files, limit, offset);
        Ok(McpListFilesResponse {
            files,
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
        })
    }

    pub async fn read_source_code(
        &self,
        file_path: &str,
        range: Option<Range>,
    ) -> Result<String, ServiceError> {
        let lsp_range = range.map(|range| LspRange::new(range.start.into(), range.end.into()));
        Ok(self.manager.read_source_code(file_path, lsp_range).await?)
    }

    pub async fn health(&self) -> HashMap<SupportedLanguages, LspStatus> {
        let mut languages = HashMap::new();
        for lang in [
            SupportedLanguages::Python,
            SupportedLanguages::TypeScriptJavaScript,
            SupportedLanguages::Rust,
            SupportedLanguages::CPP,
            SupportedLanguages::CSharp,
            SupportedLanguages::Java,
            SupportedLanguages::Golang,
            SupportedLanguages::PHP,
        ] {
            let status = if self.manager.get_client(lang).await.is_some() {
                LspStatus::Ready
            } else if self.manager.is_language_pending(lang).await {
                LspStatus::Initializing
            } else {
                LspStatus::Unavailable
            };
            languages.insert(lang, status);
        }
        languages
    }

    /// Get diagnostics (errors, warnings, hints) for a file or the entire workspace.
    ///
    /// If `file_path` is provided (relative to workspace root), returns diagnostics for that file only.
    /// If None, returns all diagnostics from all language clients.
    pub async fn get_diagnostics(
        &self,
        file_path: Option<&str>,
    ) -> Result<DiagnosticsResponse, ServiceError> {
        let raw_diagnostics = self.manager.get_diagnostics(file_path).await?;

        // Convert to API types and collect into FileDiagnostics
        let mut files: Vec<FileDiagnostics> = raw_diagnostics
            .into_iter()
            .map(|(path, lsp_diagnostics)| FileDiagnostics {
                path,
                diagnostics: lsp_diagnostics.into_iter().map(Diagnostic::from).collect(),
            })
            .collect();

        // Sort files by path for consistent output
        files.sort_by(|a, b| a.path.cmp(&b.path));

        // Calculate total count
        let total_count: usize = files.iter().map(|f| f.diagnostics.len()).sum();

        Ok(DiagnosticsResponse { total_count, files })
    }

    /// Get hover information (documentation, type info) for a symbol at a given position.
    pub async fn hover(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<HoverResponse, ServiceError> {
        let hover = self
            .manager
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

        Ok(HoverResponse {
            raw_response,
            contents,
            range,
        })
    }

    pub async fn workspace_symbol(
        &self,
        query: &str,
        include_raw_response: bool,
        exact: bool,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<WorkspaceSymbolResponse, ServiceError> {
        let symbols = self.manager.workspace_symbol(query).await?;

        let workspace_files = self.manager.list_files().await?;

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

    pub async fn find_implementation(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<ImplementationResponse, ServiceError> {
        let file_identifiers = self.manager.get_file_identifiers(file_path).await?;
        let selected_identifier = find_identifier_at_position(
            file_identifiers,
            &FilePosition {
                path: file_path.to_string(),
                position: position.clone(),
            },
        )
        .await?;

        let implementations = self
            .manager
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

    pub async fn prepare_call_hierarchy(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<PrepareCallHierarchyResponse, ServiceError> {
        let items = self
            .manager
            .prepare_call_hierarchy(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let converted_items: Vec<CallHierarchyItemInfo> = items
            .unwrap_or_default()
            .iter()
            .map(call_hierarchy_item_to_info)
            .collect();

        let raw_response = if include_raw_response {
            serde_json::to_value(&converted_items).ok()
        } else {
            None
        };

        Ok(PrepareCallHierarchyResponse {
            raw_response,
            items: converted_items,
        })
    }

    pub async fn incoming_calls(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<IncomingCallsResponse, ServiceError> {
        // First prepare the call hierarchy to get the item
        let items = self
            .manager
            .prepare_call_hierarchy(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let item = items
            .and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) })
            .ok_or_else(|| {
                ServiceError::Lsp(LspManagerError::InternalError(
                    "No call hierarchy item at position".to_string(),
                ))
            })?;

        let calls = self.manager.incoming_calls(file_path, &item).await?;

        let workspace_files = self.manager.list_files().await?;

        let converted_calls: Vec<IncomingCallInfo> = calls
            .into_iter()
            .filter(|call| {
                let path = uri_to_relative_path_string(&call.from.uri);
                workspace_files.contains(&path)
            })
            .map(|call| IncomingCallInfo {
                from: call_hierarchy_item_to_info(&call.from),
                from_ranges: call
                    .from_ranges
                    .into_iter()
                    .map(|r| Range {
                        start: Position {
                            line: r.start.line + 1,
                            character: r.start.character + 1,
                        },
                        end: Position {
                            line: r.end.line + 1,
                            character: r.end.character + 1,
                        },
                    })
                    .collect(),
            })
            .collect();

        let raw_response = if include_raw_response {
            serde_json::to_value(&converted_calls).ok()
        } else {
            None
        };

        Ok(IncomingCallsResponse {
            raw_response,
            calls: converted_calls,
        })
    }

    pub async fn outgoing_calls(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<OutgoingCallsResponse, ServiceError> {
        // First prepare the call hierarchy to get the item
        let items = self
            .manager
            .prepare_call_hierarchy(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let item = items
            .and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) })
            .ok_or_else(|| {
                ServiceError::Lsp(LspManagerError::InternalError(
                    "No call hierarchy item at position".to_string(),
                ))
            })?;

        let calls = self.manager.outgoing_calls(file_path, &item).await?;

        let workspace_files = self.manager.list_files().await?;

        let converted_calls: Vec<OutgoingCallInfo> = calls
            .into_iter()
            .filter(|call| {
                let path = uri_to_relative_path_string(&call.to.uri);
                workspace_files.contains(&path)
            })
            .map(|call| OutgoingCallInfo {
                to: call_hierarchy_item_to_info(&call.to),
                from_ranges: call
                    .from_ranges
                    .into_iter()
                    .map(|r| Range {
                        start: Position {
                            line: r.start.line + 1,
                            character: r.start.character + 1,
                        },
                        end: Position {
                            line: r.end.line + 1,
                            character: r.end.character + 1,
                        },
                    })
                    .collect(),
            })
            .collect();

        let raw_response = if include_raw_response {
            serde_json::to_value(&converted_calls).ok()
        } else {
            None
        };

        Ok(OutgoingCallsResponse {
            raw_response,
            calls: converted_calls,
        })
    }
}

fn workspace_symbol_info_from_lsp(
    sym: lsp_types::SymbolInformation,
    path: String,
) -> WorkspaceSymbolInfo {
    WorkspaceSymbolInfo {
        name: sym.name,
        kind: normalize_kind(&format!("{:?}", sym.kind)),
        location: FilePosition {
            path,
            position: Position {
                line: sym.location.range.start.line + 1,
                character: sym.location.range.start.character + 1,
            },
        },
        container_name: sym.container_name,
        match_kind: None,
        match_score: None,
    }
}

fn call_hierarchy_item_to_info(item: &lsp_types::CallHierarchyItem) -> CallHierarchyItemInfo {
    CallHierarchyItemInfo {
        name: item.name.clone(),
        kind: normalize_kind(&format!("{:?}", item.kind)),
        location: FilePosition {
            path: uri_to_relative_path_string(&item.uri),
            position: Position {
                line: item.selection_range.start.line + 1,
                character: item.selection_range.start.character + 1,
            },
        },
        range: Range {
            start: Position {
                line: item.range.start.line + 1,
                character: item.range.start.character + 1,
            },
            end: Position {
                line: item.range.end.line + 1,
                character: item.range.end.character + 1,
            },
        },
        detail: item.detail.clone(),
    }
}

fn definition_locations(definitions: &GotoDefinitionResponse) -> Vec<FilePosition> {
    match definitions {
        GotoDefinitionResponse::Scalar(location) => vec![FilePosition {
            path: uri_to_relative_path_string(&location.uri),
            position: Position {
                line: location.range.start.line + 1,
                character: location.range.start.character + 1,
            },
        }],
        GotoDefinitionResponse::Array(locations) => locations
            .iter()
            .map(|location| FilePosition {
                path: uri_to_relative_path_string(&location.uri),
                position: Position {
                    line: location.range.start.line + 1,
                    character: location.range.start.character + 1,
                },
            })
            .collect(),
        GotoDefinitionResponse::Link(links) => links
            .iter()
            .map(|link| FilePosition {
                path: uri_to_relative_path_string(&link.target_uri),
                position: Position {
                    line: link.target_range.start.line + 1,
                    character: link.target_range.start.character + 1,
                },
            })
            .collect(),
    }
}

fn definition_locations_lsp(definitions: &GotoDefinitionResponse) -> Vec<Location> {
    match definitions {
        GotoDefinitionResponse::Scalar(location) => vec![location.clone()],
        GotoDefinitionResponse::Array(locations) => locations.clone(),
        GotoDefinitionResponse::Link(links) => links
            .iter()
            .map(|link| Location::new(link.target_uri.clone(), link.target_range))
            .collect(),
    }
}

async fn fetch_definition_source_code(
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

async fn find_identifier_at_position(
    identifiers: Vec<Identifier>,
    position: &FilePosition,
) -> Result<Identifier, PositionError> {
    if let Some(exact_match) = identifiers
        .iter()
        .find(|i| i.file_range.contains(position.clone()))
    {
        return Ok(exact_match.clone());
    }

    let mut with_distances: Vec<_> = identifiers
        .iter()
        .map(|id| {
            let start_line_diff =
                (id.file_range.range.start.line as i32 - position.position.line as i32).abs();
            let start_char_diff = (id.file_range.range.start.character as i32
                - position.position.character as i32)
                .abs();
            let start_distance = start_line_diff * 100 + start_char_diff;

            let end_line_diff =
                (id.file_range.range.end.line as i32 - position.position.line as i32).abs();
            let end_char_diff = (id.file_range.range.end.character as i32
                - position.position.character as i32)
                .abs();
            let end_distance = end_line_diff * 100 + end_char_diff;

            (id.clone(), (start_distance.min(end_distance)) as f64)
        })
        .collect();

    with_distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let closest = with_distances
        .into_iter()
        .take(3)
        .map(|(id, _)| id)
        .collect();

    Err(PositionError::IdentifierNotFound { closest })
}

async fn find_and_filter_references(
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

async fn get_code_contexts(
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

async fn fetch_code_context(
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

const DEFAULT_LIST_LIMIT: u32 = 200;

fn paginate_items<T>(
    items: Vec<T>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> (Vec<T>, Pagination) {
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let offset = offset.unwrap_or(0);
    let start = offset as usize;
    let end = std::cmp::min(start.saturating_add(limit as usize), items.len());
    let truncated = end < items.len();
    let paginated = items.into_iter().skip(start).take(limit as usize).collect();
    (
        paginated,
        Pagination {
            limit,
            offset,
            truncated,
        },
    )
}

fn match_kind_and_score(query: &str, name: &str) -> (String, f32) {
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

fn is_fuzzy_match(query: &str, name: &str) -> bool {
    let mut iter = name.chars();
    for target in query.chars() {
        if !iter.any(|candidate| candidate == target) {
            return false;
        }
    }
    true
}

fn range_from_lsp(range: &LspRange) -> Range {
    Range {
        start: Position {
            line: range.start.line + 1,
            character: range.start.character + 1,
        },
        end: Position {
            line: range.end.line + 1,
            character: range.end.character + 1,
        },
    }
}

fn definition_item_from_location(
    location: &Location,
    symbol: Option<Symbol>,
    snippet: Option<CodeContext>,
) -> McpDefinitionLocation {
    let path = uri_to_relative_path_string(&location.uri);
    let position = Position {
        line: location.range.start.line + 1,
        character: location.range.start.character + 1,
    };
    let (definition_range, symbol_kind) = match symbol {
        Some(symbol) => (symbol.file_range.range, Some(symbol.kind)),
        None => (range_from_lsp(&location.range), None),
    };
    McpDefinitionLocation {
        path,
        position,
        definition_range,
        symbol_kind,
        snippet,
    }
}

fn reference_item_from_location(
    location: &Location,
    snippet: Option<CodeContext>,
) -> McpReferenceLocation {
    let path = uri_to_relative_path_string(&location.uri);
    let position = Position {
        line: location.range.start.line + 1,
        character: location.range.start.character + 1,
    };
    McpReferenceLocation {
        path,
        position,
        symbol_range: range_from_lsp(&location.range),
        snippet,
    }
}

fn extract_hover_contents(contents: &lsp_types::HoverContents) -> HoverContents {
    match contents {
        lsp_types::HoverContents::Scalar(marked) => {
            HoverContents::Markup(extract_marked_string(marked))
        }
        lsp_types::HoverContents::Array(arr) => {
            HoverContents::Array(arr.iter().map(extract_marked_string).collect())
        }
        lsp_types::HoverContents::Markup(markup) => HoverContents::Markup(markup.value.clone()),
    }
}

fn extract_marked_string(marked: &lsp_types::MarkedString) -> String {
    match marked {
        lsp_types::MarkedString::String(s) => s.clone(),
        lsp_types::MarkedString::LanguageString(ls) => {
            format!("```{}\n{}\n```", ls.language, ls.value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{
        CallHierarchyItem, Location, Position as LspPosition, Range as LspRange, SymbolInformation,
        SymbolKind, Url,
    };
    use rand::{distributions::Alphanumeric, Rng};
    use std::thread;
    use tempfile::TempDir;

    fn random_irregular_string() -> String {
        let mut rng = rand::thread_rng();
        let len: usize = rng.gen_range(6..20);
        let mut value: String = rng
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect();
        value.push('_');
        value.push('\t');
        value
    }

    fn retry_with<T, F>(mut op: F) -> T
    where
        F: FnMut() -> Option<T>,
    {
        let mut rng = rand::thread_rng();
        let attempts: usize = rng.gen_range(2..5);
        for _ in 0..attempts {
            let result = op();
            if result.is_some() {
                return result.unwrap();
            }
        }
        let message = random_irregular_string();
        panic!("{}", message);
    }

    #[allow(deprecated)]
    #[test]
    fn test_workspace_symbol_info_kind_normalized() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let range = LspRange {
            start: LspPosition {
                line: 1,
                character: 0,
            },
            end: LspPosition {
                line: 1,
                character: 4,
            },
        };
        let sym = SymbolInformation {
            name: "Example".to_string(),
            kind: SymbolKind::ENUM_MEMBER,
            tags: None,
            deprecated: None,
            location: Location { uri, range },
            container_name: None,
        };

        let info = workspace_symbol_info_from_lsp(sym, "src/main.rs".to_string());

        assert_eq!(info.kind, "enum-member");
        assert_eq!(info.location.path, "src/main.rs");
    }

    #[test]
    fn test_call_hierarchy_kind_normalized() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let range = LspRange {
            start: LspPosition {
                line: 2,
                character: 1,
            },
            end: LspPosition {
                line: 2,
                character: 6,
            },
        };
        let item = CallHierarchyItem {
            name: "Thing".to_string(),
            kind: SymbolKind::TYPE_PARAMETER,
            tags: None,
            detail: None,
            uri,
            range: range.clone(),
            selection_range: range,
            data: None,
        };

        let info = call_hierarchy_item_to_info(&item);

        assert_eq!(info.kind, "type-parameter");
    }

    #[tokio::test]
    async fn it_reports_language_servers_unavailable_without_startup(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let random_suffix: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(10)
            .map(char::from)
            .collect();
        let workspace_root = temp_dir.path().join(format!("ñ{}", random_suffix));
        tokio::fs::create_dir_all(&workspace_root).await?;
        let manager = Manager::new(
            workspace_root
                .to_str()
                .ok_or("Workspace root path must be valid utf8")?,
        )
        .await?;
        let service = create_service(Arc::new(manager));

        let mut attempts_remaining = 3;
        let mut results = tokio::join!(service.health(), service.health());
        while attempts_remaining > 0
            && (results.0.values().any(|status| *status == LspStatus::Ready) || results.0 != results.1)
        {
            attempts_remaining -= 1;
            results = tokio::join!(service.health(), service.health());
        }

        let all_unavailable = results.0.values().all(|status| *status == LspStatus::Unavailable);
        let consistent = results.0 == results.1;
        assert!(
            all_unavailable && consistent,
            "Expected language servers to be unavailable and consistent but they were not"
        );

        Ok(())
    }

    #[tokio::test]
    async fn it_cannot_crash_when_language_servers_are_missing(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_left = TempDir::new()?;
        let temp_right = TempDir::new()?;
        let random_left: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(9)
            .map(char::from)
            .collect();
        let random_right: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(9)
            .map(char::from)
            .collect();
        let irregular_left = format!("ñ{}", random_left);
        let irregular_right = format!("ñ{}", random_right);
        let workspace_left = temp_left.path().join(format!("workspace_{}", random_left));
        let workspace_right = temp_right.path().join(format!("workspace_{}", random_right));
        tokio::fs::create_dir_all(&workspace_left).await?;
        tokio::fs::create_dir_all(&workspace_right).await?;
        let file_left = workspace_left.join(format!("sample_{}.py", random_left));
        let file_right = workspace_right.join(format!("sample_{}.py", random_right));
        tokio::fs::write(&file_left, format!("print('{}')", irregular_left)).await?;
        tokio::fs::write(&file_right, format!("print('{}')", irregular_right)).await?;

        let path_override_dir = TempDir::new()?;
        let path_override = path_override_dir
            .path()
            .join(format!("path_{}", rand::thread_rng().gen::<u32>()));
        tokio::fs::create_dir_all(&path_override).await?;
        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &path_override);

        let workspace_left_str = workspace_left.to_str().ok_or(irregular_left.clone())?;
        let workspace_right_str = workspace_right.to_str().ok_or(irregular_right.clone())?;
        let mut manager_left = Manager::new(workspace_left_str).await?;
        let mut manager_right = Manager::new(workspace_right_str).await?;

        let (result_left, result_right) = tokio::join!(
            retry_start(&mut manager_left, workspace_left_str),
            retry_start(&mut manager_right, workspace_right_str)
        );

        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        } else {
            std::env::remove_var("PATH");
        }

        let left_ok = result_left.is_ok();
        let right_ok = result_right.is_ok();
        assert!(
            left_ok && right_ok,
            "Did not ignore missing language servers on startup"
        );

        Ok(())
    }

    #[test]
    fn it_paginates_items_with_offset_and_truncation() {
        let mut rng = rand::thread_rng();
        let total_len: usize = rng.gen_range(6..20);
        let offset: u32 = rng.gen_range(0..(total_len as u32 / 2 + 1));
        let limit: u32 = rng.gen_range(1..(total_len as u32 / 2 + 2));
        let mut items = Vec::with_capacity(total_len);
        for _ in 0..total_len {
            items.push(random_irregular_string());
        }
        let expected_items = items.clone();
        let response = retry_with(|| {
            let items = items.clone();
            let handle = thread::spawn(move || paginate_items(items, Some(limit), Some(offset)));
            handle.join().ok()
        });
        let (actual_items, pagination) = response;
        let start = offset as usize;
        let end = std::cmp::min(start.saturating_add(limit as usize), expected_items.len());
        let expected_slice = expected_items[start..end].to_vec();
        assert_eq!(
            actual_items,
            expected_slice,
            "negative: paginated items mismatch"
        );
        assert_eq!(pagination.limit, limit, "negative: limit mismatch");
        assert_eq!(pagination.offset, offset, "negative: offset mismatch");
        assert_eq!(
            pagination.truncated,
            end < expected_items.len(),
            "negative: truncation mismatch"
        );
    }

    #[test]
    fn it_scores_prefix_matches_for_workspace_symbols() {
        let mut rng = rand::thread_rng();
        let base = random_irregular_string();
        let prefix_len = rng.gen_range(1..(base.len().saturating_sub(1).max(2)));
        let query: String = base.chars().take(prefix_len).collect();
        let name = format!("{}{}", base, random_irregular_string());
        let response = retry_with(|| {
            let query = query.clone();
            let name = name.clone();
            let handle = thread::spawn(move || match_kind_and_score(&query, &name));
            handle.join().ok()
        });
        let expected_kind = String::from("prefix");
        let (kind, score) = response;
        assert_eq!(kind, expected_kind, "negative: match kind mismatch");
        assert!(
            score > 0.7,
            "negative: match score did not exceed expected threshold"
        );
    }

    #[test]
    fn it_builds_definition_location_with_symbol_range_and_snippet() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        let file_name = format!("file_{}.rs", random_irregular_string());
        let file_path = temp_dir.path().join(file_name);
        let uri = Url::from_file_path(&file_path).expect("negative: uri creation failed");
        let mut rng = rand::thread_rng();
        let start_line: u32 = rng.gen_range(1..100);
        let start_char: u32 = rng.gen_range(0..20);
        let end_line: u32 = start_line + rng.gen_range(0..5);
        let end_char: u32 = start_char + rng.gen_range(1..5);
        let location = Location {
            uri,
            range: LspRange {
                start: LspPosition {
                    line: start_line,
                    character: start_char,
                },
                end: LspPosition {
                    line: end_line,
                    character: end_char,
                },
            },
        };
        let expected_path = file_path.to_string_lossy().into_owned();
        let symbol_range = Range {
            start: Position {
                line: start_line + 1,
                character: 0,
            },
            end: Position {
                line: end_line + 2,
                character: 3,
            },
        };
        let symbol = Symbol {
            name: random_irregular_string(),
            kind: random_irregular_string(),
            identifier_position: FilePosition {
                path: expected_path.clone(),
                position: Position {
                    line: start_line,
                    character: start_char,
                },
            },
            file_range: FileRange {
                path: expected_path.clone(),
                range: symbol_range.clone(),
            },
        };
        let snippet = CodeContext {
            range: FileRange {
                path: expected_path.clone(),
                range: symbol_range.clone(),
            },
            source_code: random_irregular_string(),
        };
        let response = retry_with(|| {
            let location = location.clone();
            let symbol = symbol.clone();
            let snippet = snippet.clone();
            let handle = thread::spawn(move || {
                Some(definition_item_from_location(
                    &location,
                    Some(symbol),
                    Some(snippet),
                ))
            });
            handle.join().ok().flatten()
        });
        assert_eq!(response.path, expected_path, "negative: path mismatch");
        // Output is 1-indexed: LSP 0-indexed input + 1
        assert_eq!(
            response.position.line, start_line + 1,
            "negative: line mismatch"
        );
        assert_eq!(
            response.position.character, start_char + 1,
            "negative: character mismatch"
        );
        assert_eq!(
            response.definition_range, symbol_range,
            "negative: definition range mismatch"
        );
        assert_eq!(
            response.symbol_kind,
            Some(symbol.kind.clone()),
            "negative: symbol kind mismatch"
        );
        assert_eq!(
            response.snippet,
            Some(snippet),
            "negative: snippet mismatch"
        );
    }

    #[test]
    fn it_builds_reference_location_with_symbol_range_and_snippet() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        let file_name = format!("ref_{}.rs", random_irregular_string());
        let file_path = temp_dir.path().join(file_name);
        let uri = Url::from_file_path(&file_path).expect("negative: uri creation failed");
        let mut rng = rand::thread_rng();
        let start_line: u32 = rng.gen_range(1..100);
        let start_char: u32 = rng.gen_range(0..20);
        let end_line: u32 = start_line + rng.gen_range(0..5);
        let end_char: u32 = start_char + rng.gen_range(1..5);
        let location = Location {
            uri,
            range: LspRange {
                start: LspPosition {
                    line: start_line,
                    character: start_char,
                },
                end: LspPosition {
                    line: end_line,
                    character: end_char,
                },
            },
        };
        let expected_path = file_path.to_string_lossy().into_owned();
        // Expected range is 1-indexed (LSP 0-indexed + 1)
        let expected_range = Range {
            start: Position {
                line: start_line + 1,
                character: start_char + 1,
            },
            end: Position {
                line: end_line + 1,
                character: end_char + 1,
            },
        };
        let snippet = CodeContext {
            range: FileRange {
                path: expected_path.clone(),
                range: expected_range.clone(),
            },
            source_code: random_irregular_string(),
        };
        let response = retry_with(|| {
            let location = location.clone();
            let snippet = snippet.clone();
            let handle =
                thread::spawn(move || Some(reference_item_from_location(&location, Some(snippet))));
            handle.join().ok().flatten()
        });
        assert_eq!(response.path, expected_path, "negative: path mismatch");
        // Output is 1-indexed: LSP 0-indexed input + 1
        assert_eq!(
            response.position.line, start_line + 1,
            "negative: line mismatch"
        );
        assert_eq!(
            response.position.character, start_char + 1,
            "negative: character mismatch"
        );
        assert_eq!(
            response.symbol_range, expected_range,
            "negative: reference range mismatch"
        );
        assert_eq!(
            response.snippet,
            Some(snippet),
            "negative: snippet mismatch"
        );
    }

    async fn retry_start(
        manager: &mut Manager,
        workspace_root: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut attempts_remaining = 2;
        let mut result = manager.start_langservers(workspace_root, None).await;
        while attempts_remaining > 0 && result.is_err() {
            attempts_remaining -= 1;
            result = manager.start_langservers(workspace_root, None).await;
        }
        result
    }
}
