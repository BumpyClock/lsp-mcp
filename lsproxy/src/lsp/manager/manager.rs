use crate::api_types::{get_mount_dir, Identifier, SupportedLanguages, Symbol};
use crate::config::LspMcpConfig;
use std::str::FromStr;
use crate::ast_grep::client::AstGrepClient;
use crate::ast_grep::types::AstGrepMatch;
use crate::lsp::client::LspClient;
use crate::lsp::languages::{
    CSharpClient, ClangdClient, GoplsClient, JdtlsClient, JediClient, PhpactorClient, RubyClient,
    RustAnalyzerClient, TypeScriptLanguageClient,
};
use crate::utils::file_utils::uri_to_relative_path_string;
use crate::utils::file_utils::{
    absolute_path_to_relative_path_string, detect_language, search_files,
};
use crate::utils::workspace_documents::{
    WorkspaceDocuments, CSHARP_FILE_PATTERNS, C_AND_CPP_FILE_PATTERNS, DEFAULT_EXCLUDE_PATTERNS,
    GOLANG_FILE_PATTERNS, JAVA_FILE_PATTERNS, PHP_FILE_PATTERNS, PYTHON_FILE_PATTERNS,
    RUBY_FILE_PATTERNS, RUST_FILE_PATTERNS, TYPESCRIPT_AND_JAVASCRIPT_FILE_PATTERNS,
};
use log::{debug, error, info, warn};
use lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, DebouncedEvent};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::{channel, Sender};
use tokio::sync::{Mutex, RwLock};

/// Manages language server clients for a workspace
///
/// Supports both synchronous and asynchronous initialization of language servers.
/// When using async initialization, language servers start in the background and
/// become available as they complete initialization.
pub struct Manager {
    lsp_clients: Arc<RwLock<HashMap<SupportedLanguages, Arc<Mutex<Box<dyn LspClient>>>>>>,
    pending_clients: Arc<Mutex<HashSet<SupportedLanguages>>>,
    watch_events_sender: Sender<DebouncedEvent>,
    ast_grep: AstGrepClient,
}

impl Manager {
    pub async fn new(root_path: &str) -> Result<Self, Box<dyn Error>> {
        let (tx, _) = channel(100);
        let event_sender = tx.clone();
        let mut debouncer = new_debouncer(
            Duration::from_secs(2),
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    for event in events {
                        let _ = tx.send(event.clone());
                    }
                }
                Err(e) => error!("Debounce error: {:?}", e),
            },
        )
        .expect("Failed to create debouncer");

        // Watch the root path recursively
        debouncer
            .watcher()
            .watch(Path::new(root_path), RecursiveMode::Recursive)
            .expect("Failed to watch path");

        let ast_grep = AstGrepClient {};
        Ok(Self {
            lsp_clients: Arc::new(RwLock::new(HashMap::new())),
            pending_clients: Arc::new(Mutex::new(HashSet::new())),
            watch_events_sender: event_sender,
            ast_grep,
        })
    }

    /// Returns languages currently being initialized
    pub async fn pending_languages(&self) -> Vec<SupportedLanguages> {
        self.pending_clients.lock().await.iter().copied().collect()
    }

    /// Checks if a specific language is still initializing
    pub async fn is_language_pending(&self, lang: SupportedLanguages) -> bool {
        self.pending_clients.lock().await.contains(&lang)
    }

    /// Detects the languages in the workspace by searching for files that match the language server's file patterns, before LSPs are started.
    fn detect_languages_in_workspace(&self, root_path: &str) -> Vec<SupportedLanguages> {
        let mut lsps = Vec::new();
        for lsp in [
            SupportedLanguages::Python,
            SupportedLanguages::TypeScriptJavaScript,
            SupportedLanguages::Rust,
            SupportedLanguages::CPP,
            SupportedLanguages::CSharp,
            SupportedLanguages::Java,
            SupportedLanguages::Golang,
            SupportedLanguages::PHP,
            SupportedLanguages::Ruby,
        ] {
            let patterns = match lsp {
                SupportedLanguages::Python => PYTHON_FILE_PATTERNS
                    .iter()
                    .map(|&s| s.to_string())
                    .collect(),
                SupportedLanguages::TypeScriptJavaScript => TYPESCRIPT_AND_JAVASCRIPT_FILE_PATTERNS
                    .iter()
                    .map(|&s| s.to_string())
                    .collect(),
                SupportedLanguages::Rust => {
                    RUST_FILE_PATTERNS.iter().map(|&s| s.to_string()).collect()
                }
                SupportedLanguages::CPP => C_AND_CPP_FILE_PATTERNS
                    .iter()
                    .map(|&s| s.to_string())
                    .collect(),
                SupportedLanguages::CSharp => CSHARP_FILE_PATTERNS
                    .iter()
                    .map(|&s| s.to_string())
                    .collect(),
                SupportedLanguages::Java => {
                    JAVA_FILE_PATTERNS.iter().map(|&s| s.to_string()).collect()
                }
                SupportedLanguages::Golang => GOLANG_FILE_PATTERNS
                    .iter()
                    .map(|&s| s.to_string())
                    .collect(),
                SupportedLanguages::PHP => {
                    PHP_FILE_PATTERNS.iter().map(|&s| s.to_string()).collect()
                }
                SupportedLanguages::Ruby => {
                    RUBY_FILE_PATTERNS.iter().map(|&s| s.to_string()).collect()
                }
            };
            if !search_files(
                Path::new(root_path),
                patterns,
                DEFAULT_EXCLUDE_PATTERNS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                true,
            )
            .map_err(|e| warn!("Error searching files: {}", e))
            .unwrap_or_default()
            .is_empty()
            {
                lsps.push(lsp);
            }
        }
        debug!("Starting LSPs: {:?}", lsps);
        lsps
    }

    pub async fn start_langservers(
        &mut self,
        workspace_path: &str,
        config: Option<&LspMcpConfig>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let lsps = match config {
            Some(cfg) => {
                cfg.languages
                    .iter()
                    .filter_map(|s| SupportedLanguages::from_str(s).ok())
                    .collect()
            }
            None => self.detect_languages_in_workspace(workspace_path),
        };

        let mut started_count = 0;
        for lsp in lsps {
            if self.get_client(lsp).await.is_some() {
                continue;
            }

            let binary = config
                .and_then(|c| c.get_binary(&lsp.to_string().to_lowercase()))
                .map(|s| s.as_str());

            debug!("Starting {:?} LSP", lsp);
            let client_result: Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>> = match lsp {
                SupportedLanguages::Python => {
                    JediClient::new(workspace_path, self.watch_events_sender.subscribe(), binary)
                        .await
                        .map(|c| Box::new(c) as Box<dyn LspClient>)
                }
                SupportedLanguages::TypeScriptJavaScript => {
                    TypeScriptLanguageClient::new(
                        workspace_path,
                        self.watch_events_sender.subscribe(),
                        binary,
                    )
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
                }
                SupportedLanguages::Rust => {
                    RustAnalyzerClient::new(workspace_path, self.watch_events_sender.subscribe(), binary)
                        .await
                        .map(|c| Box::new(c) as Box<dyn LspClient>)
                }
                SupportedLanguages::CPP => {
                    ClangdClient::new(workspace_path, self.watch_events_sender.subscribe(), binary)
                        .await
                        .map(|c| Box::new(c) as Box<dyn LspClient>)
                }
                SupportedLanguages::CSharp => {
                    CSharpClient::new(workspace_path, self.watch_events_sender.subscribe(), binary)
                        .await
                        .map(|c| Box::new(c) as Box<dyn LspClient>)
                }
                SupportedLanguages::Java => {
                    JdtlsClient::new(workspace_path, self.watch_events_sender.subscribe(), binary)
                        .await
                        .map(|c| Box::new(c) as Box<dyn LspClient>)
                }
                SupportedLanguages::Golang => {
                    GoplsClient::new(workspace_path, self.watch_events_sender.subscribe(), binary)
                        .await
                        .map(|c| Box::new(c) as Box<dyn LspClient>)
                }
                SupportedLanguages::PHP => {
                    PhpactorClient::new(workspace_path, self.watch_events_sender.subscribe(), binary)
                        .await
                        .map(|c| Box::new(c) as Box<dyn LspClient>)
                }
                SupportedLanguages::Ruby => {
                    RubyClient::new(workspace_path, self.watch_events_sender.subscribe(), binary)
                        .await
                        .map(|c| Box::new(c) as Box<dyn LspClient>)
                }
            };

            match client_result {
                Ok(mut client) => {
                    match client.initialize(workspace_path.to_string()).await {
                        Ok(_) => {
                            debug!("Setting up workspace for {:?}", lsp);
                            if let Err(e) = client.setup_workspace(workspace_path).await {
                                warn!("Failed to setup workspace for {:?}: {}. Skipping", lsp, e);
                                continue;
                            }
                            self.lsp_clients.write().await.insert(lsp, Arc::new(Mutex::new(client)));
                            started_count += 1;
                        }
                        Err(e) => {
                            warn!("Failed to initialize {:?} language server: {}. Skipping", lsp, e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to start {:?} language server: {}. Skipping", lsp, e);
                }
            }
        }

        if started_count == 0 && config.map_or(false, |c| !c.languages.is_empty()) {
            return Err("No language servers could be started from config".into());
        }
        Ok(())
    }

    /// Starts language servers asynchronously in the background
    ///
    /// Unlike `start_langservers`, this method returns immediately after spawning
    /// background tasks for each language server. Language servers become available
    /// as they complete initialization. Check `pending_languages()` or `is_language_pending()`
    /// to determine initialization status.
    pub async fn start_langservers_async(
        &self,
        workspace_path: &str,
        config: Option<LspMcpConfig>,
    ) {
        let lsps: Vec<SupportedLanguages> = match &config {
            Some(cfg) => {
                cfg.languages
                    .iter()
                    .filter_map(|s| SupportedLanguages::from_str(s).ok())
                    .collect()
            }
            None => self.detect_languages_in_workspace(workspace_path),
        };

        for lsp in lsps {
            if self.get_client(lsp).await.is_some() {
                continue;
            }

            self.pending_clients.lock().await.insert(lsp);

            let binary = config
                .as_ref()
                .and_then(|c| c.get_binary(&lsp.to_string().to_lowercase()))
                .cloned();

            let lsp_clients = self.lsp_clients.clone();
            let pending_clients = self.pending_clients.clone();
            let events_rx = self.watch_events_sender.subscribe();
            let ws_path = workspace_path.to_string();

            tokio::spawn(async move {
                debug!("Starting {:?} LSP in background", lsp);
                let binary_ref = binary.as_deref();
                let client_result = Self::create_lsp_client(lsp, &ws_path, events_rx, binary_ref).await;

                match client_result {
                    Ok(mut client) => {
                        match client.initialize(ws_path.clone()).await {
                            Ok(_) => {
                                debug!("Setting up workspace for {:?}", lsp);
                                if let Err(e) = client.setup_workspace(&ws_path).await {
                                    warn!("Failed to setup workspace for {:?}: {}. Skipping", lsp, e);
                                    pending_clients.lock().await.remove(&lsp);
                                    return;
                                }
                                lsp_clients.write().await.insert(lsp, Arc::new(Mutex::new(client)));
                                pending_clients.lock().await.remove(&lsp);
                                info!("{:?} language server is now ready", lsp);
                            }
                            Err(e) => {
                                warn!("Failed to initialize {:?} language server: {}. Skipping", lsp, e);
                                pending_clients.lock().await.remove(&lsp);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to start {:?} language server: {}. Skipping", lsp, e);
                        pending_clients.lock().await.remove(&lsp);
                    }
                }
            });
        }
    }

    async fn create_lsp_client(
        lsp: SupportedLanguages,
        workspace_path: &str,
        events_rx: tokio::sync::broadcast::Receiver<DebouncedEvent>,
        binary: Option<&str>,
    ) -> Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>> {
        match lsp {
            SupportedLanguages::Python => {
                JediClient::new(workspace_path, events_rx, binary)
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
            }
            SupportedLanguages::TypeScriptJavaScript => {
                TypeScriptLanguageClient::new(workspace_path, events_rx, binary)
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
            }
            SupportedLanguages::Rust => {
                RustAnalyzerClient::new(workspace_path, events_rx, binary)
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
            }
            SupportedLanguages::CPP => {
                ClangdClient::new(workspace_path, events_rx, binary)
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
            }
            SupportedLanguages::CSharp => {
                CSharpClient::new(workspace_path, events_rx, binary)
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
            }
            SupportedLanguages::Java => {
                JdtlsClient::new(workspace_path, events_rx, binary)
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
            }
            SupportedLanguages::Golang => {
                GoplsClient::new(workspace_path, events_rx, binary)
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
            }
            SupportedLanguages::PHP => {
                PhpactorClient::new(workspace_path, events_rx, binary)
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
            }
            SupportedLanguages::Ruby => {
                RubyClient::new(workspace_path, events_rx, binary)
                    .await
                    .map(|c| Box::new(c) as Box<dyn LspClient>)
            }
        }
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

    pub async fn get_symbol_from_position(
        &self,
        file_path: &str,
        identifier_position: &lsp_types::Position,
    ) -> Result<Symbol, LspManagerError> {
        let full_path = get_mount_dir().join(file_path);
        let full_path_str = full_path.to_str().unwrap_or_default();
        match self
            .ast_grep
            .get_symbol_match_from_position(full_path_str, identifier_position)
            .await
        {
            Ok(ast_grep_symbol) => Ok(Symbol::from(ast_grep_symbol)),
            Err(e) => Err(LspManagerError::InternalError(e.to_string())),
        }
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

        // Sort the locations if there are multiple
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
            .text_document_hover(full_path_str, position)
            .await
            .map_err(|e| LspManagerError::InternalError(format!("Hover retrieval failed: {}", e)))
    }

    pub async fn workspace_symbol(
        &self,
        query: &str,
    ) -> Result<Vec<lsp_types::SymbolInformation>, LspManagerError> {
        let mut all_symbols = Vec::new();
        let clients = self.lsp_clients.read().await;

        // Query all available language servers
        for client in clients.values() {
            let mut locked_client = client.lock().await;
            match locked_client.workspace_symbol(query).await {
                Ok(symbols) => all_symbols.extend(symbols),
                Err(e) => {
                    log::warn!("Workspace symbol query failed for a client: {}", e);
                    // Continue with other clients
                }
            }
        }

        // Sort by name for consistent results
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

        // Sort implementations for consistent output (same pattern as find_definition)
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
                        .then(a.target_range.start.character.cmp(&b.target_range.start.character))
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

        // Only Python and TypeScript/JavaScript are currently supported
        match lsp_type {
            SupportedLanguages::Python | SupportedLanguages::TypeScriptJavaScript | SupportedLanguages::CSharp => (),
            _ => return Err(LspManagerError::NotImplemented(
                "Find referenced symbols is only implemented for Python, TypeScript/JavaScript, and C#"
                    .to_string(),
            )),
        }

        // Get the symbol and its references
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

        // Get direct definitions for each reference
        for ast_match in references_to_symbols.iter() {
            match locked_client
                .text_document_definition(full_path_str, lsp_types::Position::from(ast_match))
                .await
            {
                Ok(definition) => {
                    definitions.push((ast_match.clone(), definition));
                }
                Err(e) => {
                    // Log the error but continue processing other references
                    log::warn!(
                        "Definition retrieval failed for reference: {}, error: {}",
                        ast_match.meta_variables.single.name.text,
                        e
                    );
                }
            }
        }

        // Only return an error if we couldn't get any definitions at all
        if definitions.is_empty() && !references_to_symbols.is_empty() {
            return Err(LspManagerError::InternalError(
                "Failed to retrieve any definitions for the referenced symbols".to_string(),
            ));
        }

        Ok(definitions)
    }

    pub async fn list_files(&self) -> Result<Vec<String>, LspManagerError> {
        let mut files = Vec::new();
        let clients = self.lsp_clients.read().await;
        for client in clients.values() {
            let mut locked_client = client.lock().await;
            files.extend(
                locked_client
                    .get_workspace_documents()
                    .list_files()
                    .await
                    .iter()
                    .filter_map(|f| Some(absolute_path_to_relative_path_string(f)))
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
        let client = self.get_client(lang).await.ok_or(
            LspManagerError::LspClientNotFound(lang),
        )?;
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
                // Get diagnostics for a specific file
                let full_path = get_mount_dir().join(path);
                let full_path_str = full_path
                    .to_str()
                    .ok_or_else(|| LspManagerError::InternalError("Invalid file path".to_string()))?;

                let lsp_type = detect_language(full_path_str).map_err(|e| {
                    LspManagerError::InternalError(format!("Language detection failed: {}", e))
                })?;

                let client = self.get_client(lsp_type).await.ok_or_else(|| {
                    LspManagerError::LspClientNotFound(lsp_type)
                })?;

                let locked_client = client.lock().await;
                let uri = Url::from_file_path(&full_path)
                    .map_err(|_| LspManagerError::InternalError("Invalid file path for URI".to_string()))?;

                if let Some(diagnostics) = locked_client.get_diagnostics_store().get(&uri).await {
                    all_diagnostics.insert(path.to_string(), diagnostics);
                }
            }
            None => {
                // Get all diagnostics from all language clients
                let clients = self.lsp_clients.read().await;
                for client in clients.values() {
                    let locked_client = client.lock().await;
                    let client_diagnostics = locked_client.get_diagnostics_store().get_all().await;

                    for (uri, diagnostics) in client_diagnostics {
                        let relative_path = uri_to_relative_path_string(&uri);
                        all_diagnostics.insert(relative_path, diagnostics);
                    }
                }
            }
        }

        Ok(all_diagnostics)
    }
}

#[derive(Debug)]
pub enum LspManagerError {
    FileNotFound(String),
    LspClientNotFound(SupportedLanguages),
    LspClientInitializing(SupportedLanguages),
    InternalError(String),
    UnsupportedFileType(String),
    NotImplemented(String),
}

impl fmt::Display for LspManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LspManagerError::FileNotFound(path) => {
                write!(f, "File '{}' not found in workspace", path)
            }
            LspManagerError::LspClientNotFound(lang) => {
                write!(f, "LSP client not found for {:?}", lang)
            }
            LspManagerError::LspClientInitializing(lang) => {
                write!(f, "The {:?} language server is still initializing, please try again shortly", lang)
            }
            LspManagerError::InternalError(msg) => write!(f, "Internal error: {}", msg),
            LspManagerError::UnsupportedFileType(path) => {
                write!(f, "Unsupported file type: {}", path)
            }
            LspManagerError::NotImplemented(msg) => {
                write!(f, "Not implemented: {}", msg)
            }
        }
    }
}

impl std::error::Error for LspManagerError {}
