// ABOUTME: Core manager struct for coordinating multiple language server clients
// ABOUTME: Provides workspace-wide LSP operations like definitions, references, and diagnostics

use super::error::LspManagerError;
use crate::api_types::{get_mount_dir, Identifier, SupportedLanguages, Symbol};
use crate::ast_grep::client::AstGrepClient;
use crate::ast_grep::types::{AstGrepMatch, SymbolLookup};
use crate::lsp::client::LspClient;
use crate::lsp::registry::LanguageMetadata;
use crate::utils::file_utils::uri_to_relative_path_string;
use crate::utils::file_utils::{absolute_path_to_relative_path_string, detect_language};
use crate::utils::file_watcher::FileWatcher;
use crate::utils::workspace_documents::{DidOpenConfiguration, WorkspaceDocuments};
use futures::{stream, StreamExt};
use log::{debug, error};
use lsp_types::{
    DocumentSymbolResponse, GotoDefinitionResponse, Location, Position, Range, SelectionRange, Url,
};
use notify_debouncer_mini::{DebounceEventResult, DebouncedEvent};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::{channel, Sender};
use tokio::sync::{Mutex, RwLock};

pub(super) const CLIENT_CONCURRENCY: usize = 4;

/// Manages language server clients for a workspace
///
/// Supports both synchronous and asynchronous initialization of language servers.
/// When using async initialization, language servers start in the background and
/// become available as they complete initialization.
pub struct Manager {
    pub(super) lsp_clients:
        Arc<RwLock<HashMap<SupportedLanguages, Arc<Mutex<Box<dyn LspClient>>>>>>,
    pub(super) pending_clients: Arc<Mutex<HashSet<SupportedLanguages>>>,
    pub(super) watch_events_sender: Sender<DebouncedEvent>,
    ast_grep: AstGrepClient,
    _watcher: FileWatcher,
}

impl Manager {
    pub async fn new(root_path: &str) -> Result<Self, Box<dyn Error>> {
        let (tx, _) = channel(100);
        let event_sender = tx.clone();
        let root = Path::new(root_path);
        let watcher = FileWatcher::new(
            root,
            Duration::from_secs(2),
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    for event in events {
                        let _ = tx.send(event);
                    }
                }
                Err(e) => error!("Debounce error: {:?}", e),
            },
        )?;

        let ast_grep = AstGrepClient::new();
        Ok(Self {
            lsp_clients: Arc::new(RwLock::new(HashMap::new())),
            pending_clients: Arc::new(Mutex::new(HashSet::new())),
            watch_events_sender: event_sender,
            ast_grep,
            _watcher: watcher,
        })
    }

    pub async fn definitions_in_file_ast_grep(
        &self,
        file_path: &str,
    ) -> Result<Vec<AstGrepMatch>, LspManagerError> {
        let workspace_files = self.list_files().await?;
        if !workspace_files.contains(&file_path.to_string()) {
            return Err(LspManagerError::FileNotFound(file_path.to_string()));
        }
        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();

        self.ast_grep
            .get_file_symbols(full_path_str)
            .await
            .map_err(|e| LspManagerError::InternalError(format!("Symbol retrieval failed: {}", e)))
    }

    pub async fn document_symbol(
        &self,
        file_path: &str,
    ) -> Result<Option<DocumentSymbolResponse>, LspManagerError> {
        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        locked_client
            .text_document_document_symbol(full_path_str)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Document symbol retrieval failed: {}", e))
            })
    }

    pub async fn selection_range(
        &self,
        file_path: &str,
        positions: Vec<Position>,
    ) -> Result<Option<Vec<SelectionRange>>, LspManagerError> {
        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        locked_client
            .text_document_selection_range(full_path_str, positions)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Selection range retrieval failed: {}", e))
            })
    }

    pub(crate) async fn get_symbols_from_positions(
        &self,
        lookups: &[SymbolLookup],
    ) -> Vec<Result<Symbol, LspManagerError>> {
        let root = get_mount_dir();
        let absolute_lookups: Vec<_> = lookups
            .iter()
            .map(|lookup| SymbolLookup {
                path: root.join(&lookup.path).to_string_lossy().into_owned(),
                position: lookup.position,
            })
            .collect();
        self.ast_grep
            .get_symbol_matches_from_positions(&absolute_lookups)
            .await
            .into_iter()
            .map(|result| {
                result
                    .map(Symbol::from)
                    .map_err(LspManagerError::InternalError)
            })
            .collect()
    }

    pub async fn find_definition(
        &self,
        file_path: &str,
        position: Position,
    ) -> Result<GotoDefinitionResponse, LspManagerError> {
        let workspace_files = self.list_files().await.map_err(|e| {
            LspManagerError::InternalError(format!("Workspace file retrieval failed: {}", e))
        })?;
        if !workspace_files.contains(&file_path.to_string()) {
            return Err(LspManagerError::FileNotFound(file_path.to_string()));
        }
        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;
        let mut definition = locked_client
            .text_document_definition(full_path_str, position)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Definition retrieval failed: {}", e))
            })?;

        match &mut definition {
            GotoDefinitionResponse::Array(locations) => {
                locations.sort_by(|a, b| {
                    let path_a = uri_to_relative_path_string(&a.uri);
                    let path_b = uri_to_relative_path_string(&b.uri);
                    path_a
                        .cmp(&path_b)
                        .then(a.range.start.line.cmp(&b.range.start.line))
                        .then(a.range.start.character.cmp(&b.range.start.character))
                });
            }
            GotoDefinitionResponse::Link(links) => {
                links.sort_by(|a, b| {
                    let path_a = uri_to_relative_path_string(&a.target_uri);
                    let path_b = uri_to_relative_path_string(&b.target_uri);
                    path_a
                        .cmp(&path_b)
                        .then(a.target_range.start.line.cmp(&b.target_range.start.line))
                        .then(
                            a.target_range
                                .start
                                .character
                                .cmp(&b.target_range.start.character),
                        )
                });
            }
            _ => {}
        }
        Ok(definition)
    }

    pub async fn get_client(
        &self,
        lsp_type: SupportedLanguages,
    ) -> Option<Arc<Mutex<Box<dyn LspClient>>>> {
        self.lsp_clients.read().await.get(&lsp_type).cloned()
    }

    async fn clients_snapshot(&self) -> Vec<Arc<Mutex<Box<dyn LspClient>>>> {
        let clients = self.lsp_clients.read().await;
        LanguageMetadata::all()
            .filter_map(|metadata| clients.get(&metadata.id).cloned())
            .collect()
    }

    /// Returns an error appropriate for when a client is not available
    ///
    /// Returns `LspClientInitializing` if the language is still being initialized,
    /// or `LspClientNotFound` if initialization failed or wasn't attempted.
    pub async fn client_unavailable_error(&self, lang: SupportedLanguages) -> LspManagerError {
        if self.is_language_pending(lang).await {
            LspManagerError::LspClientInitializing(lang)
        } else {
            LspManagerError::LspClientNotFound(lang)
        }
    }

    pub async fn find_references(
        &self,
        file_path: &str,
        position: Position,
    ) -> Result<Vec<Location>, LspManagerError> {
        let workspace_files = self.list_files().await.map_err(|e| {
            LspManagerError::InternalError(format!("Workspace file retrieval failed: {}", e))
        })?;

        if !workspace_files.contains(&file_path.to_string()) {
            return Err(LspManagerError::FileNotFound(file_path.to_string()));
        }

        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;
        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        locked_client
            .text_document_reference(full_path_str, position)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Reference retrieval failed: {}", e))
            })
    }

    pub async fn hover(
        &self,
        file_path: &str,
        position: Position,
    ) -> Result<Option<lsp_types::Hover>, LspManagerError> {
        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        locked_client
            .text_document_hover(full_path_str, position)
            .await
            .map_err(|e| LspManagerError::InternalError(format!("Hover retrieval failed: {}", e)))
    }

    pub async fn signature_help(
        &self,
        file_path: &str,
        position: Position,
    ) -> Result<Option<lsp_types::SignatureHelp>, LspManagerError> {
        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        locked_client
            .text_document_signature_help(full_path_str, position)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Signature help retrieval failed: {}", e))
            })
    }

    pub async fn code_action(
        &self,
        file_path: &str,
        range: Range,
        diagnostics: Vec<lsp_types::Diagnostic>,
    ) -> Result<Option<Vec<lsp_types::CodeActionOrCommand>>, LspManagerError> {
        let workspace_files = self.list_files().await.map_err(|e| {
            LspManagerError::InternalError(format!("Workspace file retrieval failed: {}", e))
        })?;

        if !workspace_files.contains(&file_path.to_string()) {
            return Err(LspManagerError::FileNotFound(file_path.to_string()));
        }

        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        locked_client
            .text_document_code_action(full_path_str, range, diagnostics)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Code action retrieval failed: {}", e))
            })
    }

    pub async fn workspace_symbol(
        &self,
        query: &str,
    ) -> Result<Vec<lsp_types::SymbolInformation>, LspManagerError> {
        // Wait for any pending language servers to initialize (with short timeout)
        // This prevents race conditions where workspace_symbol is called before
        // all LSP clients have finished async initialization
        if !self.pending_clients.lock().await.is_empty() {
            log::debug!("workspace_symbol: waiting for pending LSP clients to initialize");
            self.wait_for_pending_clients(Duration::from_secs(5)).await;
        }

        let clients = self.clients_snapshot().await;
        let mut results = stream::iter(clients)
            .map(|client| async move { client.lock().await.workspace_symbol(query).await })
            .buffered(CLIENT_CONCURRENCY)
            .boxed();
        let mut all_symbols = Vec::new();
        while let Some(result) = results.next().await {
            match result {
                Ok(symbols) => all_symbols.extend(symbols),
                Err(e) => {
                    log::warn!("Workspace symbol query failed for a client: {}", e);
                }
            }
        }

        all_symbols.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(all_symbols)
    }

    pub async fn find_implementation(
        &self,
        file_path: &str,
        position: Position,
    ) -> Result<GotoDefinitionResponse, LspManagerError> {
        let workspace_files = self.list_files().await.map_err(|e| {
            LspManagerError::InternalError(format!("Workspace file retrieval failed: {}", e))
        })?;

        if !workspace_files.contains(&file_path.to_string()) {
            return Err(LspManagerError::FileNotFound(file_path.to_string()));
        }

        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        let mut implementations = locked_client
            .text_document_implementation(full_path_str, position)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Implementation retrieval failed: {}", e))
            })?;

        match &mut implementations {
            GotoDefinitionResponse::Array(locations) => {
                locations.sort_by(|a, b| {
                    let path_a = uri_to_relative_path_string(&a.uri);
                    let path_b = uri_to_relative_path_string(&b.uri);
                    path_a
                        .cmp(&path_b)
                        .then(a.range.start.line.cmp(&b.range.start.line))
                        .then(a.range.start.character.cmp(&b.range.start.character))
                });
            }
            GotoDefinitionResponse::Link(links) => {
                links.sort_by(|a, b| {
                    let path_a = uri_to_relative_path_string(&a.target_uri);
                    let path_b = uri_to_relative_path_string(&b.target_uri);
                    path_a
                        .cmp(&path_b)
                        .then(a.target_range.start.line.cmp(&b.target_range.start.line))
                        .then(
                            a.target_range
                                .start
                                .character
                                .cmp(&b.target_range.start.character),
                        )
                });
            }
            _ => {}
        }

        Ok(implementations)
    }

    pub async fn find_referenced_symbols(
        &self,
        file_path: &str,
        position: Position,
        full_scan: bool,
    ) -> Result<Vec<(AstGrepMatch, GotoDefinitionResponse)>, LspManagerError> {
        let workspace_files = self.list_files().await.map_err(|e| {
            LspManagerError::InternalError(format!("Workspace file retrieval failed: {}", e))
        })?;

        if !workspace_files.iter().any(|f| f == file_path) {
            return Err(LspManagerError::FileNotFound(file_path.to_string()));
        }

        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();

        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        match lsp_type {
            SupportedLanguages::Python
            | SupportedLanguages::TypeScriptJavaScript
            | SupportedLanguages::CSharp
            | SupportedLanguages::Rust => (),
            _ => {
                return Err(LspManagerError::NotImplemented(
                    "Find referenced symbols is only implemented for Python, TypeScript/JavaScript, C#, and Rust"
                        .to_string(),
                ))
            }
        }

        let (_, references_to_symbols) = match self
            .ast_grep
            .get_symbol_and_references(full_path_str, &position, full_scan)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                return Err(LspManagerError::InternalError(format!(
                    "Failed to find referenced symbols, {}",
                    e
                )));
            }
        };

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;
        let mut definitions = Vec::new();

        for ast_match in references_to_symbols.iter() {
            match locked_client
                .text_document_definition(full_path_str, lsp_types::Position::from(ast_match))
                .await
            {
                Ok(definition) => {
                    definitions.push((ast_match.clone(), definition));
                }
                Err(e) => {
                    log::warn!(
                        "Definition retrieval failed for reference: {}, error: {}",
                        ast_match.meta_variables.single.name.text,
                        e
                    );
                }
            }
        }

        if definitions.is_empty() && !references_to_symbols.is_empty() {
            return Err(LspManagerError::InternalError(
                "Failed to retrieve any definitions for the referenced symbols".to_string(),
            ));
        }

        Ok(definitions)
    }

    pub async fn list_files(&self) -> Result<Vec<String>, LspManagerError> {
        let clients = self.clients_snapshot().await;
        let mut results = stream::iter(clients)
            .map(|client| async move {
                client
                    .lock()
                    .await
                    .get_workspace_documents()
                    .list_files()
                    .await
            })
            .buffered(CLIENT_CONCURRENCY)
            .boxed();
        let mut files = Vec::new();
        while let Some(client_files) = results.next().await {
            files.extend(
                client_files
                    .iter()
                    .map(|f| absolute_path_to_relative_path_string(f))
                    .collect::<Vec<String>>(),
            );
        }
        files.sort();
        Ok(files)
    }

    pub async fn read_source_code(
        &self,
        file_path: &str,
        range: Option<Range>,
    ) -> Result<String, LspManagerError> {
        let lang = detect_language(file_path)?;
        let client = self
            .get_client(lang)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lang))?;
        let full_path = get_mount_dir().join(file_path);
        let mut locked_client = client.lock().await;
        locked_client
            .get_workspace_documents()
            .read_text_document(&full_path, range)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Source code retrieval failed: {}", e))
            })
    }

    pub async fn get_file_identifiers(
        &self,
        file_path: &str,
    ) -> Result<Vec<Identifier>, LspManagerError> {
        let full_path = get_mount_dir().join(file_path);
        let workspace_files = self.list_files().await.map_err(|e| {
            LspManagerError::InternalError(format!("Workspace file retrieval failed: {}", e))
        })?;
        if !workspace_files.contains(&file_path.to_string()) {
            return Err(LspManagerError::FileNotFound(file_path.to_string()));
        }
        let full_path_str = full_path.to_str().unwrap_or_default();
        let ast_grep_result = self
            .ast_grep
            .get_file_identifiers(full_path_str)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Symbol retrieval failed: {}", e))
            })?;
        Ok(ast_grep_result.into_iter().map(|s| s.into()).collect())
    }

    /// Get diagnostics from language servers.
    ///
    /// If `file_path` is provided (relative to workspace root), returns diagnostics for that file only.
    /// If None, returns all diagnostics from all language clients.
    ///
    /// Returns a HashMap where keys are relative file paths and values are vectors of LSP diagnostics.
    pub async fn get_diagnostics(
        &self,
        file_path: Option<&str>,
    ) -> Result<HashMap<String, Vec<lsp_types::Diagnostic>>, LspManagerError> {
        let mut all_diagnostics: HashMap<String, Vec<lsp_types::Diagnostic>> = HashMap::new();

        match file_path {
            Some(path) => {
                let full_path = get_mount_dir().join(path);
                let full_path_str = full_path.to_str().ok_or_else(|| {
                    LspManagerError::InternalError("Invalid file path".to_string())
                })?;

                let lsp_type = detect_language(full_path_str).map_err(|e| {
                    LspManagerError::InternalError(format!("Language detection failed: {}", e))
                })?;

                let client = self
                    .get_client(lsp_type)
                    .await
                    .ok_or_else(|| LspManagerError::LspClientNotFound(lsp_type))?;

                let mut locked_client = client.lock().await;
                let uri = Url::from_file_path(&full_path).map_err(|_| {
                    LspManagerError::InternalError("Invalid file path for URI".to_string())
                })?;

                let needs_open = {
                    let workspace_documents = locked_client.get_workspace_documents();
                    workspace_documents.get_did_open_configuration() == DidOpenConfiguration::Lazy
                        && !workspace_documents.is_did_open_document(full_path_str)
                };

                if needs_open {
                    locked_client
                        .ensure_document_open(full_path_str)
                        .await
                        .map_err(|e| {
                            LspManagerError::InternalError(format!(
                                "Failed to open document for diagnostics: {}",
                                e
                            ))
                        })?;
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }

                if let Some(diagnostics) = locked_client.get_diagnostics_store().get(&uri).await {
                    all_diagnostics.insert(path.to_string(), diagnostics);
                }
            }
            None => {
                let clients = self.clients_snapshot().await;
                let mut results = stream::iter(clients)
                    .map(|client| async move {
                        let store = client.lock().await.get_diagnostics_store().clone();
                        store.get_all().await
                    })
                    .buffered(CLIENT_CONCURRENCY)
                    .boxed();
                while let Some(client_diagnostics) = results.next().await {
                    for (uri, diagnostics) in client_diagnostics {
                        let relative_path = uri_to_relative_path_string(&uri);
                        all_diagnostics.insert(relative_path, diagnostics);
                    }
                }
            }
        }

        Ok(all_diagnostics)
    }

    pub async fn prepare_call_hierarchy(
        &self,
        file_path: &str,
        position: Position,
    ) -> Result<Option<Vec<lsp_types::CallHierarchyItem>>, LspManagerError> {
        debug!(
            "prepare_call_hierarchy called: file={}, position=({}, {})",
            file_path, position.line, position.character
        );

        let workspace_files = self.list_files().await.map_err(|e| {
            LspManagerError::InternalError(format!("Workspace file retrieval failed: {}", e))
        })?;

        if !workspace_files.contains(&file_path.to_string()) {
            return Err(LspManagerError::FileNotFound(file_path.to_string()));
        }

        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        debug!("prepare_call_hierarchy: detected language {:?}", lsp_type);

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        let result = locked_client
            .prepare_call_hierarchy(full_path_str, position)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Call hierarchy preparation failed: {}", e))
            })?;

        debug!(
            "prepare_call_hierarchy raw response: {:?}",
            result.as_ref().map(|items| items.len())
        );
        if let Some(ref items) = result {
            for (i, item) in items.iter().enumerate() {
                debug!(
                    "  item[{}]: name={}, kind={:?}, uri={}, range=({},{})-({},{})",
                    i,
                    item.name,
                    item.kind,
                    uri_to_relative_path_string(&item.uri),
                    item.range.start.line,
                    item.range.start.character,
                    item.range.end.line,
                    item.range.end.character
                );
            }
        }

        Ok(result)
    }

    pub async fn incoming_calls(
        &self,
        file_path: &str,
        item: &lsp_types::CallHierarchyItem,
    ) -> Result<Vec<lsp_types::CallHierarchyIncomingCall>, LspManagerError> {
        debug!(
            "incoming_calls called: file={}, item.name={}, item.kind={:?}",
            file_path, item.name, item.kind
        );

        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        let result = locked_client
            .call_hierarchy_incoming_calls(item)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Incoming calls retrieval failed: {}", e))
            })?;

        debug!("incoming_calls raw response: {} calls", result.len());
        for (i, call) in result.iter().enumerate() {
            debug!(
                "  call[{}]: from={}, from_ranges={:?}",
                i,
                call.from.name,
                call.from_ranges.len()
            );
        }

        Ok(result)
    }

    pub async fn outgoing_calls(
        &self,
        file_path: &str,
        item: &lsp_types::CallHierarchyItem,
    ) -> Result<Vec<lsp_types::CallHierarchyOutgoingCall>, LspManagerError> {
        debug!(
            "outgoing_calls called: file={}, item.name={}, item.kind={:?}",
            file_path, item.name, item.kind
        );

        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        let lsp_type = detect_language(full_path_str).map_err(|e| {
            LspManagerError::InternalError(format!("Language detection failed: {}", e))
        })?;

        let client = self
            .get_client(lsp_type)
            .await
            .ok_or(LspManagerError::LspClientNotFound(lsp_type))?;
        let mut locked_client = client.lock().await;

        let result = locked_client
            .call_hierarchy_outgoing_calls(item)
            .await
            .map_err(|e| {
                LspManagerError::InternalError(format!("Outgoing calls retrieval failed: {}", e))
            })?;

        debug!("outgoing_calls raw response: {} calls", result.len());
        for (i, call) in result.iter().enumerate() {
            debug!(
                "  call[{}]: to={}, from_ranges={:?}",
                i,
                call.to.name,
                call.from_ranges.len()
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
#[path = "core_tests.rs"]
mod tests;
