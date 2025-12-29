// ABOUTME: Domain service layer for LSP-backed code navigation operations.
// ABOUTME: Provides async methods for symbol lookup, references, and file access.
use crate::api_types::{
    CallHierarchyItemInfo, CodeContext, DefinitionResponse, Diagnostic, DiagnosticsResponse,
    FileDiagnostics, FilePosition, FileRange, HoverContents, HoverResponse, Identifier,
    IdentifierResponse, ImplementationResponse, IncomingCallInfo, IncomingCallsResponse,
    LspStatus, OutgoingCallInfo, OutgoingCallsResponse, Position, PrepareCallHierarchyResponse,
    Range, ReferenceWithSymbolDefinitions, ReferencedSymbolsResponse, ReferencesResponse,
    SupportedLanguages, Symbol, WorkspaceSymbolInfo, WorkspaceSymbolResponse,
};
use crate::lsp::manager::{LspManagerError, Manager};
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{GotoDefinitionResponse, Location, Position as LspPosition, Range as LspRange};
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
/// let _files = service.list_files().await?;
/// # Ok(())
/// # }
/// ```
pub struct LspService {
    manager: Arc<Manager>,
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
    pub async fn definitions_in_file(&self, file_path: &str) -> Result<Vec<Symbol>, ServiceError> {
        let symbols = self.manager.definitions_in_file_ast_grep(file_path).await?;
        Ok(symbols
            .into_iter()
            .filter(|s| s.rule_id != "local-variable")
            .map(Symbol::from)
            .collect())
    }

    pub async fn find_definition(
        &self,
        file_path: &str,
        position: Position,
        include_source_code: bool,
        include_raw_response: bool,
    ) -> Result<DefinitionResponse, ServiceError> {
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
                    line: position.line,
                    character: position.character,
                },
            )
            .await?;

        let source_code_context = if include_source_code {
            Some(fetch_definition_source_code(&self.manager, &definitions).await?)
        } else {
            None
        };

        let raw_response = if include_raw_response {
            Some(serde_json::to_value(&definitions)?)
        } else {
            None
        };

        Ok(DefinitionResponse {
            raw_response,
            definitions: definition_locations(&definitions),
            source_code_context,
            selected_identifier,
        })
    }

    pub async fn find_references(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
        include_code_context_lines: Option<u32>,
    ) -> Result<ReferencesResponse, ServiceError> {
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

        let code_contexts =
            get_code_contexts(&self.manager, &references, include_code_context_lines).await?;

        let raw_response = if include_raw_response {
            serde_json::to_value(&references).ok()
        } else {
            None
        };

        Ok(ReferencesResponse {
            raw_response,
            references: references
                .into_iter()
                .map(|loc| FilePosition {
                    path: uri_to_relative_path_string(&loc.uri),
                    position: Position {
                        line: loc.range.start.line,
                        character: loc.range.start.character,
                    },
                })
                .collect(),
            context: code_contexts,
            selected_identifier,
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
                    line: position.line,
                    character: position.character,
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
                                    line: def.position.line,
                                    character: def.position.character,
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
    ) -> Result<IdentifierResponse, ServiceError> {
        let file_identifiers = self.manager.get_file_identifiers(file_path).await?;
        let name_matched: Vec<Identifier> = file_identifiers
            .into_iter()
            .filter(|id| id.name == name)
            .collect();

        if name_matched.is_empty() {
            return Ok(IdentifierResponse { identifiers: vec![] });
        }

        if let Some(position) = position {
            let lookup_position = FilePosition {
                path: file_path.to_string(),
                position,
            };
            match find_identifier_at_position(name_matched.clone(), &lookup_position).await {
                Ok(identifier) => Ok(IdentifierResponse {
                    identifiers: vec![identifier],
                }),
                Err(PositionError::IdentifierNotFound { closest }) => Ok(IdentifierResponse {
                    identifiers: closest,
                }),
            }
        } else {
            Ok(IdentifierResponse {
                identifiers: name_matched,
            })
        }
    }

    pub async fn list_files(&self) -> Result<Vec<String>, ServiceError> {
        Ok(self.manager.list_files().await?)
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
                    line: position.line,
                    character: position.character,
                },
            )
            .await?;

        let (contents, range, raw_response) = match hover {
            Some(h) => {
                let contents = extract_hover_contents(&h.contents);
                let range = h.range.map(|r| Range {
                    start: Position {
                        line: r.start.line,
                        character: r.start.character,
                    },
                    end: Position {
                        line: r.end.line,
                        character: r.end.character,
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
    ) -> Result<WorkspaceSymbolResponse, ServiceError> {
        let symbols = self.manager.workspace_symbol(query).await?;

        let workspace_files = self.manager.list_files().await?;

        let filtered_symbols: Vec<WorkspaceSymbolInfo> = symbols
            .into_iter()
            .filter_map(|sym| {
                let path = uri_to_relative_path_string(&sym.location.uri);
                // Only include symbols from workspace files
                if workspace_files.contains(&path) {
                    Some(WorkspaceSymbolInfo {
                        name: sym.name,
                        kind: format!("{:?}", sym.kind),
                        location: FilePosition {
                            path,
                            position: Position {
                                line: sym.location.range.start.line,
                                character: sym.location.range.start.character,
                            },
                        },
                        container_name: sym.container_name,
                    })
                } else {
                    None
                }
            })
            .collect();

        let raw_response = if include_raw_response {
            serde_json::to_value(&filtered_symbols).ok()
        } else {
            None
        };

        Ok(WorkspaceSymbolResponse {
            raw_response,
            symbols: filtered_symbols,
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
                    line: position.line,
                    character: position.character,
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
                    line: position.line,
                    character: position.character,
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
                    line: position.line,
                    character: position.character,
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
                            line: r.start.line,
                            character: r.start.character,
                        },
                        end: Position {
                            line: r.end.line,
                            character: r.end.character,
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
                    line: position.line,
                    character: position.character,
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
                            line: r.start.line,
                            character: r.start.character,
                        },
                        end: Position {
                            line: r.end.line,
                            character: r.end.character,
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

fn call_hierarchy_item_to_info(item: &lsp_types::CallHierarchyItem) -> CallHierarchyItemInfo {
    CallHierarchyItemInfo {
        name: item.name.clone(),
        kind: format!("{:?}", item.kind),
        location: FilePosition {
            path: uri_to_relative_path_string(&item.uri),
            position: Position {
                line: item.selection_range.start.line,
                character: item.selection_range.start.character,
            },
        },
        range: Range {
            start: Position {
                line: item.range.start.line,
                character: item.range.start.character,
            },
            end: Position {
                line: item.range.end.line,
                character: item.range.end.character,
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
                line: location.range.start.line,
                character: location.range.start.character,
            },
        }],
        GotoDefinitionResponse::Array(locations) => locations
            .iter()
            .map(|location| FilePosition {
                path: uri_to_relative_path_string(&location.uri),
                position: Position {
                    line: location.range.start.line,
                    character: location.range.start.character,
                },
            })
            .collect(),
        GotoDefinitionResponse::Link(links) => links
            .iter()
            .map(|link| FilePosition {
                path: uri_to_relative_path_string(&link.target_uri),
                position: Position {
                    line: link.target_range.start.line,
                    character: link.target_range.start.character,
                },
            })
            .collect(),
    }
}

async fn fetch_definition_source_code(
    manager: &Manager,
    definitions_response: &GotoDefinitionResponse,
) -> Result<Vec<CodeContext>, ServiceError> {
    let definitions: Vec<Location> = match definitions_response {
        GotoDefinitionResponse::Scalar(definition) => vec![definition.clone()],
        GotoDefinitionResponse::Array(definitions) => definitions.clone(),
        GotoDefinitionResponse::Link(links) => links
            .iter()
            .map(|link| Location::new(link.target_uri.clone(), link.target_range))
            .collect(),
    };

    let mut code_contexts = Vec::new();
    for definition in definitions {
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
                            line: ast_grep_match.get_context_range().start.line,
                            character: ast_grep_match.get_context_range().start.column,
                        },
                        end: Position {
                            line: ast_grep_match.get_context_range().end.line,
                            character: ast_grep_match.get_context_range().end.column,
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
                                line: definition.range.start.line.saturating_sub(3),
                                character: 0,
                            },
                            end: Position {
                                line: definition.range.end.line + 3,
                                character: 0,
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
                line: position.position.line,
                character: position.position.character,
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
                        line: reference.range.start.line.saturating_sub(context_lines),
                        character: 0,
                    },
                    end: Position {
                        line: reference.range.end.line.saturating_add(context_lines),
                        character: 0,
                    },
                },
            },
            source_code,
        });
    }
    Ok(code_contexts)
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
    use rand::{distributions::Alphanumeric, Rng};
    use tempfile::TempDir;

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
