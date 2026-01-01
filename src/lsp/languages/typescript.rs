use std::error::Error;
use std::path::Path;
use std::process::Stdio;

use async_trait::async_trait;
use lsp_types::InitializeParams;
use notify_debouncer_mini::DebouncedEvent;
use tokio::process::Command;
use tokio::sync::broadcast::Receiver;
use url::Url;

use crate::lsp::client::LspClientConfig;
use crate::lsp::{DiagnosticsStore, JsonRpcHandler, LspClient, PendingRequests, ProcessHandler};

use crate::utils::workspace_documents::{
    DidOpenConfiguration, WorkspaceDocuments, WorkspaceDocumentsHandler, DEFAULT_EXCLUDE_PATTERNS,
    TYPESCRIPT_AND_JAVASCRIPT_FILE_PATTERNS, TYPESCRIPT_AND_JAVASCRIPT_ROOT_FILES,
};

pub struct TypeScriptLanguageClient {
    process: ProcessHandler,
    json_rpc: JsonRpcHandler,
    workspace_documents: WorkspaceDocumentsHandler,
    pending_requests: PendingRequests,
    diagnostics_store: DiagnosticsStore,
    config: LspClientConfig,
}

#[async_trait]
impl LspClient for TypeScriptLanguageClient {
    fn get_process(&mut self) -> &mut ProcessHandler {
        &mut self.process
    }

    fn get_json_rpc(&mut self) -> &mut JsonRpcHandler {
        &mut self.json_rpc
    }

    fn get_root_files(&mut self) -> Vec<String> {
        TYPESCRIPT_AND_JAVASCRIPT_ROOT_FILES
            .iter()
            .map(|&s| s.to_owned())
            .collect()
    }

    fn get_pending_requests(&mut self) -> &mut PendingRequests {
        &mut self.pending_requests
    }

    fn get_workspace_documents(&mut self) -> &mut WorkspaceDocumentsHandler {
        &mut self.workspace_documents
    }

    fn get_diagnostics_store(&self) -> &DiagnosticsStore {
        &self.diagnostics_store
    }

    fn get_config(&self) -> &LspClientConfig {
        &self.config
    }

    async fn get_initialize_params(
        &mut self,
        root_path: String,
    ) -> Result<InitializeParams, Box<dyn Error + Send + Sync>> {
        let capabilities = self.get_capabilities();
        Ok(InitializeParams {
            capabilities,
            root_uri: Some(Url::from_file_path(root_path).map_err(|_| "Invalid root path")?),
            initialization_options: Some(serde_json::json!({
                "tsserver": {
                    "useSyntaxServer": "never"
                }
            })),
            ..Default::default()
        })
    }

    async fn setup_workspace(
        &mut self,
        root_path: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Ensure tsserver has an inferred/configured project by opening a seed file.
        let preferred = Path::new(root_path).join("src/main.tsx");
        if preferred.exists() {
            if let Some(path_str) = preferred.to_str() {
                self.ensure_document_open(path_str).await?;
                return Ok(());
            }
        }

        let candidates = self.get_workspace_documents().list_files().await;
        if let Some(seed) = candidates.iter().find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext, "ts" | "tsx"))
        }) {
            if let Some(path_str) = seed.to_str() {
                self.ensure_document_open(path_str).await?;
            }
        }

        Ok(())
    }
}

impl TypeScriptLanguageClient {
    pub const DEFAULT_BINARY: &'static str = "typescript-language-server";

    pub async fn new(
        root_path: &str,
        watch_events_rx: Receiver<DebouncedEvent>,
        binary: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let binary = binary.unwrap_or(Self::DEFAULT_BINARY);
        let process = Command::new(binary)
            .arg("--stdio")
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
            TYPESCRIPT_AND_JAVASCRIPT_FILE_PATTERNS
                .iter()
                .map(|&s| s.to_string())
                .collect(),
            DEFAULT_EXCLUDE_PATTERNS
                .iter()
                .map(|&s| s.to_string())
                .collect(),
            watch_events_rx,
            DidOpenConfiguration::Lazy,
        );
        Ok(Self {
            process: process_handler,
            json_rpc: json_rpc_handler,
            workspace_documents,
            pending_requests: PendingRequests::new(),
            diagnostics_store: DiagnosticsStore::new(),
            config: LspClientConfig::default(),
        })
    }
}
