// ABOUTME: LSP client trait definition for language server communication
// ABOUTME: Provides async methods for initialization, document operations, and diagnostics handling

use crate::lsp::json_rpc::JsonRpc;
use crate::lsp::process::Process;
use crate::lsp::reconnect::{DocumentTracker, SpawnConfig};
use crate::lsp::{DiagnosticsStore, ExpectedMessageKey, JsonRpcHandler, ProcessHandler};
use crate::utils::file_utils::{detect_language_string, search_directories};
use async_trait::async_trait;
use log::{debug, error, warn};
use lsp_types::{
    ClientCapabilities, DiagnosticTag, DidOpenTextDocumentParams, DocumentSymbolClientCapabilities,
    GotoDefinitionParams, GotoDefinitionResponse, InitializeParams, InitializeResult, Location,
    PartialResultParams, Position, PublishDiagnosticsClientCapabilities,
    PublishDiagnosticsParams, ReferenceContext, ReferenceParams, TagSupport,
    TextDocumentClientCapabilities, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, Url, WorkDoneProgressParams, WorkspaceFolder,
};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::utils::workspace_documents::{
    DidOpenConfiguration, WorkspaceDocuments, WorkspaceDocumentsHandler, DEFAULT_EXCLUDE_PATTERNS,
};

use super::PendingRequests;

/// Configuration for LSP client behavior
#[derive(Clone, Debug)]
pub struct LspClientConfig {
    /// Timeout for individual LSP requests (default: 30 seconds)
    pub request_timeout: Duration,
    /// Maximum retry attempts for reconnection (default: 3)
    pub max_reconnect_attempts: u32,
    /// Base delay between reconnection attempts (default: 1 second)
    pub reconnect_base_delay: Duration,
    /// Maximum delay between reconnection attempts (default: 30 seconds)
    pub reconnect_max_delay: Duration,
}

impl Default for LspClientConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            max_reconnect_attempts: 3,
            reconnect_base_delay: Duration::from_secs(1),
            reconnect_max_delay: Duration::from_secs(30),
        }
    }
}

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
            .send_request("initialize", Some(serde_json::to_value(params)?))
            .await?;
        let init_result: InitializeResult = serde_json::from_value(result)?;
        debug!("Initialization successful: {:?}", init_result);
        self.send_initialized().await?;
        Ok(init_result)
    }

    fn get_capabilities(&mut self) -> ClientCapabilities {
        let mut capabilities = ClientCapabilities::default();
        capabilities.text_document = Some(TextDocumentClientCapabilities {
            document_symbol: Some(DocumentSymbolClientCapabilities {
                dynamic_registration: Some(false),
                hierarchical_document_symbol_support: Some(true),
                ..Default::default()
            }),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                related_information: Some(true),
                tag_support: Some(TagSupport {
                    value_set: vec![DiagnosticTag::UNNECESSARY, DiagnosticTag::DEPRECATED],
                }),
                code_description_support: Some(true),
                data_support: Some(false),
                version_support: Some(true),
            }),
            ..Default::default()
        });

        capabilities.experimental = Some(serde_json::json!({
            "serverStatusNotification": true
        }));
        capabilities
    }

    async fn get_initialize_params(
        &mut self,
        root_path: String,
    ) -> Result<InitializeParams, Box<dyn Error + Send + Sync>> {
        let workspace_folders = self.find_workspace_folders(root_path.clone()).await?;
        Ok(InitializeParams {
            capabilities: self.get_capabilities(),
            workspace_folders: Some(workspace_folders),
            root_uri: Some(Url::from_file_path(&root_path).unwrap()), // primarily for python
            ..Default::default()
        })
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
        let (id, request) = self.get_json_rpc().create_request(method, params);

        let mut response_receiver = self.get_pending_requests().add_request(id).await?;

        let message = format!("Content-Length: {}\r\n\r\n{}", request.len(), request);
        debug!("Message: {:?}", message);
        self.get_process().send(&message).await?;

        let timeout_duration = self.get_config().request_timeout;
        let response = tokio::time::timeout(timeout_duration, response_receiver.recv())
            .await
            .map_err(|_| {
                format!(
                    "Request '{}' timed out after {:?}",
                    method, timeout_duration
                )
            })?
            .map_err(|e| format!("Failed to receive response: {}", e))?;

        if let Some(result) = response.result {
            Ok(result)
        } else if let Some(error) = response.error.clone() {
            error!("Recieved error: {:?}", response);
            if error.message.starts_with("KeyError") {
                return Ok(serde_json::Value::Array(vec![]));
            }
            Err(error.into())
        } else {
            Ok(serde_json::Value::Null)
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
                            if let Some(id) = message.id {
                                debug!("Received response for request {}", id);
                                if let Ok(Some(sender)) = pending_requests.remove_request(id).await {
                                    if sender.send(message.clone()).is_err() {
                                        error!("Failed to send response for request {}", id);
                                    }
                                } else {
                                    debug!(
                                        "Responding to server message {} - Message: {:?}",
                                        id, message
                                    );
                                    let response = json_rpc.create_success_response(id);

                                    let message = format!(
                                        "Content-Length: {}\r\n\r\n{}",
                                        response.len(),
                                        response
                                    );
                                    let _ = process.send(&message).await;
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
                        error!("LSP process communication failed: {}. Response listener exiting.", reason);
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
        // Track the document for potential reconnection
        if let Some(path) = item.uri.to_file_path().ok() {
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

    async fn text_document_definition(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<GotoDefinitionResponse, Box<dyn Error + Send + Sync>> {
        debug!(
            "Requesting goto definition for {}, line {}, character {}",
            file_path, position.line, position.character
        );

        let needs_open = {
            let workspace_documents = self.get_workspace_documents();
            workspace_documents.get_did_open_configuration() == DidOpenConfiguration::Lazy
                && !workspace_documents.is_did_open_document(file_path)
        };

        // If needed, read the document text and send didOpen
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

        let result = self
            .send_request(
                "textDocument/definition",
                Some(serde_json::to_value(params)?),
            )
            .await?;

        // If result is null, default to an empty array response instead of failing deserialization
        let goto_resp: GotoDefinitionResponse = if result.is_null() {
            GotoDefinitionResponse::Array(Vec::new())
        } else {
            serde_json::from_value(result)?
        };

        debug!("Received goto definition response");
        Ok(goto_resp)
    }

    async fn text_document_reference(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<Vec<Location>, Box<dyn Error + Send + Sync>> {
        // Get the configuration and check if document is opened first
        let needs_open = {
            let workspace_documents = self.get_workspace_documents();
            workspace_documents.get_did_open_configuration() == DidOpenConfiguration::Lazy
                && !workspace_documents.is_did_open_document(file_path)
        };

        // If needed, read the document text and send didOpen
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

        let result = self
            .send_request(
                "textDocument/references",
                Some(serde_json::to_value(params)?),
            )
            .await?;

        let ref_resp: Vec<Location> = if result.is_null() {
            Vec::new()
        } else {
            serde_json::from_value(result)?
        };
        debug!("Received references response");
        Ok(ref_resp)
    }

    async fn text_document_hover(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<Option<lsp_types::Hover>, Box<dyn Error + Send + Sync>> {
        debug!(
            "Requesting hover for {}, line {}, character {}",
            file_path, position.line, position.character
        );

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

        let params = lsp_types::HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let result = self
            .send_request("textDocument/hover", Some(serde_json::to_value(params)?))
            .await?;

        let hover_resp: Option<lsp_types::Hover> = if result.is_null() {
            None
        } else {
            serde_json::from_value(result)?
        };

        debug!("Received hover response");
        Ok(hover_resp)
    }

    async fn workspace_symbol(
        &mut self,
        query: &str,
    ) -> Result<Vec<lsp_types::SymbolInformation>, Box<dyn Error + Send + Sync>> {
        debug!("Requesting workspace symbols for query: {}", query);

        let params = lsp_types::WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let result = self
            .send_request("workspace/symbol", Some(serde_json::to_value(params)?))
            .await?;

        // workspace/symbol can return null or SymbolInformation[]
        let symbols: Vec<lsp_types::SymbolInformation> = if result.is_null() {
            Vec::new()
        } else {
            serde_json::from_value(result)?
        };

        debug!("Received {} workspace symbols", symbols.len());
        Ok(symbols)
    }

    async fn text_document_implementation(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<GotoDefinitionResponse, Box<dyn Error + Send + Sync>> {
        debug!(
            "Requesting implementation for {}, line {}, character {}",
            file_path, position.line, position.character
        );

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

        // GotoImplementationParams is an alias for GotoDefinitionParams
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

        let result = self
            .send_request(
                "textDocument/implementation",
                Some(serde_json::to_value(params)?),
            )
            .await?;

        // GotoImplementationResponse is the same as GotoDefinitionResponse
        let impl_resp: GotoDefinitionResponse = if result.is_null() {
            GotoDefinitionResponse::Array(Vec::new())
        } else {
            serde_json::from_value(result)?
        };

        debug!("Received implementation response");
        Ok(impl_resp)
    }

    async fn prepare_call_hierarchy(
        &mut self,
        file_path: &str,
        position: Position,
    ) -> Result<Option<Vec<lsp_types::CallHierarchyItem>>, Box<dyn Error + Send + Sync>> {
        debug!(
            "Preparing call hierarchy for {}, line {}, character {}",
            file_path, position.line, position.character
        );

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

        let params = lsp_types::CallHierarchyPrepareParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(file_path).map_err(|_| "Invalid file path")?,
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let result = self
            .send_request(
                "textDocument/prepareCallHierarchy",
                Some(serde_json::to_value(params)?),
            )
            .await?;

        let items: Option<Vec<lsp_types::CallHierarchyItem>> = if result.is_null() {
            None
        } else {
            serde_json::from_value(result)?
        };

        debug!("Received call hierarchy prepare response");
        Ok(items)
    }

    async fn call_hierarchy_incoming_calls(
        &mut self,
        item: &lsp_types::CallHierarchyItem,
    ) -> Result<Vec<lsp_types::CallHierarchyIncomingCall>, Box<dyn Error + Send + Sync>> {
        debug!("Requesting incoming calls for {}", item.name);

        let params = lsp_types::CallHierarchyIncomingCallsParams {
            item: item.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let result = self
            .send_request(
                "callHierarchy/incomingCalls",
                Some(serde_json::to_value(params)?),
            )
            .await?;

        let calls: Vec<lsp_types::CallHierarchyIncomingCall> = if result.is_null() {
            Vec::new()
        } else {
            serde_json::from_value(result)?
        };

        debug!("Received {} incoming calls", calls.len());
        Ok(calls)
    }

    async fn call_hierarchy_outgoing_calls(
        &mut self,
        item: &lsp_types::CallHierarchyItem,
    ) -> Result<Vec<lsp_types::CallHierarchyOutgoingCall>, Box<dyn Error + Send + Sync>> {
        debug!("Requesting outgoing calls for {}", item.name);

        let params = lsp_types::CallHierarchyOutgoingCallsParams {
            item: item.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let result = self
            .send_request(
                "callHierarchy/outgoingCalls",
                Some(serde_json::to_value(params)?),
            )
            .await?;

        let calls: Vec<lsp_types::CallHierarchyOutgoingCall> = if result.is_null() {
            Vec::new()
        } else {
            serde_json::from_value(result)?
        };

        debug!("Received {} outgoing calls", calls.len());
        Ok(calls)
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
            // Fallback: use the root_path itself as a workspace folder
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
}
