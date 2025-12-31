use std::{error::Error, path::Path, process::Stdio};

use async_trait::async_trait;
use lsp_types::{
    ClientCapabilities, DocumentSymbolClientCapabilities, InitializeParams,
    TextDocumentClientCapabilities,
};
use notify_debouncer_mini::DebouncedEvent;
use tokio::process::Command;
use tokio::sync::broadcast::Receiver;
use url::Url;

use crate::lsp::client::LspClientConfig;
use crate::lsp::reconnect::{DocumentTracker, SpawnConfig};
use crate::lsp::{DiagnosticsStore, JsonRpcHandler, LspClient, PendingRequests, ProcessHandler};

use crate::utils::workspace_documents::{
    DidOpenConfiguration, WorkspaceDocumentsHandler, DEFAULT_EXCLUDE_PATTERNS, RUST_FILE_PATTERNS,
    RUST_ROOT_FILES,
};

pub struct RustAnalyzerClient {
    process: ProcessHandler,
    json_rpc: JsonRpcHandler,
    workspace_documents: WorkspaceDocumentsHandler,
    pending_requests: PendingRequests,
    diagnostics_store: DiagnosticsStore,
    config: LspClientConfig,
    spawn_config: SpawnConfig,
    document_tracker: DocumentTracker,
}

#[async_trait]
impl LspClient for RustAnalyzerClient {
    fn get_capabilities(&mut self) -> ClientCapabilities {
        let mut capabilities = ClientCapabilities::default();
        capabilities.text_document = Some(TextDocumentClientCapabilities {
            document_symbol: Some(DocumentSymbolClientCapabilities {
                hierarchical_document_symbol_support: Some(true),
                ..Default::default()
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
        Ok(InitializeParams {
            capabilities: self.get_capabilities(),
            workspace_folders: Some(
                self.find_workspace_folders(root_path.clone())
                    .await
                    .unwrap(),
            ),
            root_uri: Some(Url::from_file_path(&root_path).map_err(|_| "Invalid root path")?),
            initialization_options: Some(serde_json::json!({
                "cargo": {
                    "sysroot": serde_json::Value::Null
                }
            })),
            ..Default::default()
        })
    }

    fn get_process(&mut self) -> &mut ProcessHandler {
        &mut self.process
    }

    fn get_json_rpc(&mut self) -> &mut JsonRpcHandler {
        &mut self.json_rpc
    }

    fn get_root_files(&mut self) -> Vec<String> {
        RUST_ROOT_FILES.iter().map(|&s| s.to_owned()).collect()
    }

    fn get_workspace_documents(&mut self) -> &mut WorkspaceDocumentsHandler {
        &mut self.workspace_documents
    }

    fn get_pending_requests(&mut self) -> &mut PendingRequests {
        &mut self.pending_requests
    }

    fn get_diagnostics_store(&self) -> &DiagnosticsStore {
        &self.diagnostics_store
    }

    fn get_config(&self) -> &LspClientConfig {
        &self.config
    }

    fn get_spawn_config(&self) -> Option<SpawnConfig> {
        Some(self.spawn_config.clone())
    }

    fn get_document_tracker(&self) -> Option<&DocumentTracker> {
        Some(&self.document_tracker)
    }

    async fn setup_workspace(
        &mut self,
        _root_path: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // This is required for workspace features like go to definition to work
        self.send_request("rust-analyzer/reloadWorkspace", None)
            .await?;
        Ok(())
    }
}

impl RustAnalyzerClient {
    pub const DEFAULT_BINARY: &'static str = "rust-analyzer";

    pub async fn new(
        root_path: &str,
        watch_events_rx: Receiver<DebouncedEvent>,
        binary: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let binary = binary.unwrap_or(Self::DEFAULT_BINARY);
        let process = Command::new(binary)
            .current_dir(root_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let process_handler = ProcessHandler::new(process)
            .await
            .map_err(|e| format!("Failed to create ProcessHandler: {}", e))?;
        let json_rpc_handler = JsonRpcHandler::new();

        let workspace_documents = WorkspaceDocumentsHandler::new(
            Path::new(root_path),
            RUST_FILE_PATTERNS.iter().map(|&s| s.to_string()).collect(),
            DEFAULT_EXCLUDE_PATTERNS
                .iter()
                .map(|&s| s.to_string())
                .collect(),
            watch_events_rx,
            DidOpenConfiguration::None,
        );

        let spawn_config = SpawnConfig::new(binary, root_path);
        let document_tracker = DocumentTracker::new();

        Ok(Self {
            process: process_handler,
            json_rpc: json_rpc_handler,
            workspace_documents,
            pending_requests: PendingRequests::new(),
            diagnostics_store: DiagnosticsStore::new(),
            config: LspClientConfig::default(),
            spawn_config,
            document_tracker,
        })
    }
}
