// ABOUTME: LSP client trait definition for language server communication
// ABOUTME: Provides async methods for initialization, document operations, and diagnostics handling

use super::capabilities::create_default_capabilities;
use super::config::LspClientConfig;
use super::{Connection, LspResponse};
use crate::lsp::json_rpc::{JsonRpc, JsonRpcHandler};
use crate::lsp::process::{Process, ProcessHandler};
use crate::lsp::reconnect::{DocumentTracker, SpawnConfig};
use crate::lsp::{DiagnosticsStore, ExpectedMessageKey, PendingRequests};
use crate::utils::file_utils::{detect_language_string, search_directories};
use async_trait::async_trait;
use log::{debug, error, warn};
use lsp_types::{
    ClientCapabilities, CodeActionContext, CodeActionOrCommand, CodeActionParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, InitializeParams, InitializeResult, Location, PartialResultParams,
    Position, ProgressParams, PublishDiagnosticsParams, Range, ReferenceContext, ReferenceParams,
    SelectionRange, SelectionRangeParams, SignatureHelp, SignatureHelpParams,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Url,
    WorkDoneProgressCreateParams, WorkDoneProgressParams, WorkspaceFolder,
};
use std::error::Error;
use std::path::{Path, PathBuf};

use crate::utils::workspace_documents::{
    DidOpenConfiguration, WorkspaceDocuments, WorkspaceDocumentsHandler, DEFAULT_EXCLUDE_PATTERNS,
};

#[async_trait]
pub trait LspClient: Send {
    async fn initialize(
        &mut self,
        root_path: String,
    ) -> Result<InitializeResult, Box<dyn Error + Send + Sync>> {
        debug!("Initializing LSP client with root path: {:?}", root_path);
        self.start_response_listener().await?;
        self.start_diagnostics_handler().await?;

        let params = self.get_initialize_params(root_path).await?;

        let result = self
            .connection()
            .send_request(
                "initialize",
                Some(serde_json::to_value(params)?),
                self.get_config().initialize_timeout,
            )
            .await?;
        let init_result: InitializeResult = serde_json::from_value(result)?;
        debug!("Initialization successful: {:?}", init_result);
        self.send_initialized().await?;
        Ok(init_result)
    }

    fn get_capabilities(&mut self) -> ClientCapabilities {
        create_default_capabilities()
    }

    async fn get_initialize_params(
        &mut self,
        root_path: String,
    ) -> Result<InitializeParams, Box<dyn Error + Send + Sync>> {
        let workspace_folders = self.find_workspace_folders(root_path.clone()).await?;
        Ok(InitializeParams {
            capabilities: self.get_capabilities(),
            workspace_folders: Some(workspace_folders),
            root_uri: Some(Url::from_file_path(&root_path).unwrap()),
            ..Default::default()
        })
    }

    fn connection(&mut self) -> Connection {
        Connection {
            rpc: self.get_json_rpc().clone(),
            pending: self.get_pending_requests().clone(),
            process: self.get_process().clone(),
            config: self.get_config().clone(),
        }
    }

    async fn start_response_listener(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut process = self.get_process().clone();
        let pending_requests = self.get_pending_requests().clone();
        let json_rpc = self.get_json_rpc().clone();

        tokio::spawn(async move {
            loop {
                match process.receive().await {
                    Ok(raw_response) => {
                        if let Ok(message) = json_rpc.parse_message(&raw_response) {
                            if message.method.as_deref() == Some("window/workDoneProgress/create") {
                                match serde_json::from_value::<WorkDoneProgressCreateParams>(
                                    message.params.clone().unwrap_or_default(),
                                ) {
                                    Ok(params) => {
                                        pending_requests.create_progress(params.token).await
                                    }
                                    Err(e) => error!("Invalid work progress creation: {e}"),
                                }
                            } else if message.method.as_deref() == Some("$/progress") {
                                match serde_json::from_value::<ProgressParams>(
                                    message.params.clone().unwrap_or_default(),
                                ) {
                                    Ok(params) => pending_requests.update_progress(params).await,
                                    Err(e) => error!("Invalid work progress notification: {e}"),
                                }
                            }
                            // Servers and clients have independent request ID sequences.
                            if let Some(id) = message.id.filter(|_| message.method.is_some()) {
                                let response = json_rpc.create_success_response(id);
                                let message = format!(
                                    "Content-Length: {}\r\n\r\n{}",
                                    response.len(),
                                    response
                                );
                                let _ = process.send(&message).await;
                            } else if let Some(id) = message.id {
                                debug!("Received response for request {}", id);
                                if let Ok(Some(sender)) = pending_requests.remove_request(id).await
                                {
                                    if sender.send(message.clone()).is_err() {
                                        error!("Failed to send response for request {}", id);
                                    }
                                } else {
                                    debug!("Ignoring response for completed request {}", id);
                                }
                            } else if let Some(method) = message.method.as_ref() {
                                if pending_requests
                                    .route_to_method_handler(method, message.clone())
                                    .await
                                {
                                    continue;
                                }
                                if let Some(params) = message.params.clone() {
                                    let message_key = ExpectedMessageKey {
                                        method: method.clone(),
                                        params,
                                    };
                                    if let Some(sender) =
                                        pending_requests.remove_notification(message_key).await
                                    {
                                        if sender.send(message).is_err() {
                                            warn!("Failed to send notification: receiver dropped");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let reason = e.to_string();
                        error!(
                            "LSP process communication failed: {}. Response listener exiting.",
                            reason
                        );
                        process.report_unhealthy(reason.clone());
                        pending_requests
                            .fail_all_requests(format!("LSP process died: {}", reason))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    async fn send_initialized(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        debug!("Sending 'initialized' notification");
        let notification = self
            .get_json_rpc()
            .create_notification("initialized", serde_json::json!({}));
        let message = format!(
            "Content-Length: {}\r\n\r\n{}",
            notification.len(),
            notification
        );
        self.get_process().send(&message).await
    }

    async fn text_document_did_open(
        &mut self,
        item: lsp_types::TextDocumentItem,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Ok(path) = item.uri.to_file_path() {
            if let Some(path_str) = path.to_str() {
                self.track_opened_document(path_str, item.version).await;
            }
        }

        let params = DidOpenTextDocumentParams {
            text_document: item,
        };
        let notification = self
            .get_json_rpc()
            .create_notification("textDocument/didOpen", serde_json::to_value(params)?);
        let message = format!(
            "Content-Length: {}\r\n\r\n{}",
            notification.len(),
            notification
        );
        self.get_process().send(&message).await
    }

    async fn prepare_definition_request(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<LspResponse<GotoDefinitionResponse>, Box<dyn Error + Send + Sync>> {
        debug!(
            "Requesting goto definition for {}, line {}, character {}",
            file_path, position.line, position.character
        );

        self.ensure_document_open(file_path).await?;

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        self.connection().request(
            "textDocument/definition",
            params,
            GotoDefinitionResponse::Array(Vec::new()),
        )
    }

    async fn prepare_reference_request(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<LspResponse<Vec<Location>>, Box<dyn Error + Send + Sync>> {
        self.ensure_document_open(file_path).await?;

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration: true,
            },
        };

        self.connection()
            .request("textDocument/references", params, Vec::new())
    }

    async fn prepare_hover_request(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<LspResponse<Option<lsp_types::Hover>>, Box<dyn Error + Send + Sync>> {
        debug!(
            "Requesting hover for {}, line {}, character {}",
            file_path, position.line, position.character
        );

        self.ensure_document_open(file_path).await?;

        let params = lsp_types::HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        self.connection()
            .request("textDocument/hover", params, None)
    }

    async fn prepare_signature_help_request(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<LspResponse<Option<SignatureHelp>>, Box<dyn Error + Send + Sync>> {
        debug!(
            "Requesting signature help for {}, line {}, character {}",
            file_path, position.line, position.character
        );

        self.ensure_document_open(file_path).await?;

        let params = SignatureHelpParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            context: None,
        };

        self.connection()
            .request("textDocument/signatureHelp", params, None)
    }

    async fn prepare_document_symbol_request(
        &mut self,
        file_path: &str,
    ) -> Result<LspResponse<Option<DocumentSymbolResponse>>, Box<dyn Error + Send + Sync>> {
        debug!("Requesting document symbols for {}", file_path);

        self.ensure_document_open(file_path).await?;

        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        self.connection()
            .request("textDocument/documentSymbol", params, None)
    }

    async fn prepare_selection_range_request(
        &mut self,
        file_path: &str,
        positions: Vec<Position>,
    ) -> Result<LspResponse<Option<Vec<SelectionRange>>>, Box<dyn Error + Send + Sync>> {
        debug!("Requesting selection ranges for {}", file_path);

        self.ensure_document_open(file_path).await?;

        let params = SelectionRangeParams {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
            },
            positions,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        self.connection()
            .request("textDocument/selectionRange", params, None)
    }

    async fn prepare_code_action_request(
        &mut self,
        file_path: &str,
        range: Range,
        diagnostics: Vec<lsp_types::Diagnostic>,
    ) -> Result<LspResponse<Option<Vec<CodeActionOrCommand>>>, Box<dyn Error + Send + Sync>> {
        debug!(
            "Requesting code actions for {}, range {:?}",
            file_path, range
        );

        self.ensure_document_open(file_path).await?;

        let params = CodeActionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
            },
            range,
            context: CodeActionContext {
                diagnostics,
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        self.connection()
            .request("textDocument/codeAction", params, None)
    }

    async fn prepare_workspace_symbol_request(
        &mut self,
        query: &str,
    ) -> Result<LspResponse<Vec<lsp_types::SymbolInformation>>, Box<dyn Error + Send + Sync>> {
        debug!("Requesting workspace symbols for query: {}", query);

        let params = lsp_types::WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        self.connection()
            .request("workspace/symbol", params, Vec::new())
    }

    async fn prepare_implementation_request(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<LspResponse<GotoDefinitionResponse>, Box<dyn Error + Send + Sync>> {
        debug!(
            "Requesting implementation for {}, line {}, character {}",
            file_path, position.line, position.character
        );

        self.ensure_document_open(file_path).await?;

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        self.connection().request(
            "textDocument/implementation",
            params,
            GotoDefinitionResponse::Array(Vec::new()),
        )
    }

    async fn prepare_call_hierarchy_request(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<LspResponse<Option<Vec<lsp_types::CallHierarchyItem>>>, Box<dyn Error + Send + Sync>>
    {
        debug!(
            "Preparing call hierarchy for {}, line {}, character {}",
            file_path, position.line, position.character
        );

        self.ensure_document_open(file_path).await?;

        let params = lsp_types::CallHierarchyPrepareParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        self.connection()
            .request("textDocument/prepareCallHierarchy", params, None)
    }

    async fn prepare_call_hierarchy_incoming_calls_request(
        &mut self,
        item: &lsp_types::CallHierarchyItem,
    ) -> Result<LspResponse<Vec<lsp_types::CallHierarchyIncomingCall>>, Box<dyn Error + Send + Sync>>
    {
        debug!("Requesting incoming calls for {}", item.name);

        let params = lsp_types::CallHierarchyIncomingCallsParams {
            item: item.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        self.connection()
            .request("callHierarchy/incomingCalls", params, Vec::new())
    }

    async fn prepare_call_hierarchy_outgoing_calls_request(
        &mut self,
        item: &lsp_types::CallHierarchyItem,
    ) -> Result<LspResponse<Vec<lsp_types::CallHierarchyOutgoingCall>>, Box<dyn Error + Send + Sync>>
    {
        debug!("Requesting outgoing calls for {}", item.name);
        debug!(
            "outgoing_calls CallHierarchyItem: name={}, kind={:?}, uri={}, data={:?}",
            item.name, item.kind, item.uri, item.data
        );

        let params = lsp_types::CallHierarchyOutgoingCallsParams {
            item: item.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        debug!(
            "outgoing_calls request params JSON: {}",
            serde_json::to_string(&params).unwrap_or_else(|_| "failed to serialize".to_string())
        );

        self.connection()
            .request("callHierarchy/outgoingCalls", params, Vec::new())
    }

    fn get_process(&mut self) -> &mut ProcessHandler;

    fn get_json_rpc(&mut self) -> &mut JsonRpcHandler;

    fn get_root_files(&mut self) -> Vec<String> {
        vec![".git".to_string()]
    }

    fn get_pending_requests(&mut self) -> &mut PendingRequests;

    fn get_workspace_documents(&mut self) -> &mut WorkspaceDocumentsHandler;

    fn get_diagnostics_store(&self) -> &DiagnosticsStore;

    /// Get the configuration for this LSP client
    fn get_config(&self) -> &LspClientConfig;

    /// Get the spawn config for respawning the LSP process (optional)
    fn get_spawn_config(&self) -> Option<SpawnConfig> {
        None
    }

    /// Get the document tracker for tracking opened documents (optional)
    fn get_document_tracker(&self) -> Option<&DocumentTracker> {
        None
    }

    /// Check if the LSP process is healthy
    async fn is_healthy(&mut self) -> bool {
        self.get_process().is_alive().await
    }

    /// Track a document that was opened (for reconnection purposes)
    async fn track_opened_document(&mut self, file_path: &str, version: i32) {
        if let Some(tracker) = self.get_document_tracker() {
            tracker.track_document(file_path.to_string(), version).await;
        }
    }

    /// Start the diagnostics handler that listens for publishDiagnostics notifications
    async fn start_diagnostics_handler(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let pending_requests = self.get_pending_requests().clone();
        let diagnostics_store = self.get_diagnostics_store().clone();

        let mut receiver = pending_requests
            .register_method_handler("textDocument/publishDiagnostics".to_string())
            .await;

        tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                if let Some(params) = message.params {
                    match serde_json::from_value::<PublishDiagnosticsParams>(params) {
                        Ok(diag_params) => {
                            debug!(
                                "Received {} diagnostics for {}",
                                diag_params.diagnostics.len(),
                                diag_params.uri
                            );
                            diagnostics_store
                                .update(diag_params.uri, diag_params.diagnostics)
                                .await;
                        }
                        Err(e) => {
                            warn!("Failed to parse publishDiagnostics params: {}", e);
                        }
                    }
                }
            }
            debug!("Diagnostics handler exiting");
        });

        Ok(())
    }

    /// Sets up the workspace for the language server.
    ///
    /// Some language servers require specific commands to be run before
    /// workspace-wide features are available. For example:
    /// - TypeScript Language Server needs an explicit didOpen notification for each file
    /// - Rust Analyzer needs a reloadWorkspace command
    ///
    /// # Arguments
    ///
    /// * `root_path` - The root path of the workspace
    ///
    /// # Returns
    ///
    /// A Result containing () if successful, or a boxed Error if an error occurred
    #[allow(unused)]
    async fn setup_workspace(
        &mut self,
        root_path: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    async fn find_workspace_folders(
        &mut self,
        root_path: String,
    ) -> Result<Vec<WorkspaceFolder>, Box<dyn Error + Send + Sync>> {
        let mut workspace_folders: Vec<WorkspaceFolder> = Vec::new();
        let include_patterns = self
            .get_root_files()
            .into_iter()
            .map(|f| format!("**/{f}"))
            .collect();
        let exclude_patterns = DEFAULT_EXCLUDE_PATTERNS
            .iter()
            .map(|&s| s.to_string())
            .collect();

        match search_directories(Path::new(&root_path), include_patterns, exclude_patterns) {
            Ok(dirs) => {
                for dir in dirs {
                    let folder_path = Path::new(&root_path).join(&dir);
                    if let Ok(uri) = Url::from_file_path(&folder_path) {
                        workspace_folders.push(WorkspaceFolder {
                            uri,
                            name: folder_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("")
                                .to_string(),
                        });
                    }
                }
            }
            Err(e) => return Err(Box::new(e)),
        }

        if workspace_folders.is_empty() {
            warn!("No workspace folders found. Using root path as workspace.");
            if let Ok(uri) = Url::from_file_path(&root_path) {
                workspace_folders.push(WorkspaceFolder {
                    uri,
                    name: root_path.to_string(),
                });
            }
        }

        Ok(workspace_folders.into_iter().collect())
    }

    /// Ensures a document is open for LSP operations that require it
    async fn ensure_document_open(
        &mut self,
        file_path: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let needs_open = {
            let workspace_documents = self.get_workspace_documents();
            workspace_documents.get_did_open_configuration() == DidOpenConfiguration::Lazy
                && !workspace_documents.is_did_open_document(file_path)
        };

        if needs_open {
            let document_text = self
                .get_workspace_documents()
                .read_text_document(&PathBuf::from(file_path), None)
                .await?;

            self.text_document_did_open(TextDocumentItem {
                uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
                language_id: detect_language_string(file_path)?,
                version: 1,
                text: document_text,
            })
            .await?;

            self.get_workspace_documents()
                .add_did_open_document(file_path);
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "trait_tests.rs"]
mod tests;
