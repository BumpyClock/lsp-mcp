// ABOUTME: LSP client creation and startup logic for language server initialization
// ABOUTME: Provides factory method for creating language-specific LSP clients

use super::core::{Manager, CLIENT_CONCURRENCY};
use super::detection::detect_languages_in_workspace;
use crate::api_types::SupportedLanguages;
use crate::config::LspMcpConfig;
use crate::lsp::client::LspClient;
use crate::lsp::registry::LanguageMetadata;
use futures::{stream, StreamExt};
use log::{debug, info, warn};
use notify_debouncer_mini::DebouncedEvent;
use std::collections::HashSet;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Creates an LSP client for the given language using the registry factory
pub async fn create_lsp_client(
    lsp: SupportedLanguages,
    workspace_path: &str,
    events_rx: tokio::sync::broadcast::Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Result<Box<dyn LspClient>, Box<dyn Error + Send + Sync>> {
    let metadata = LanguageMetadata::get(lsp)
        .ok_or_else(|| format!("No registry entry found for language {:?}", lsp))?;

    (metadata.factory)(workspace_path, events_rx, binary).await
}

impl Manager {
    /// Returns languages currently being initialized
    pub async fn pending_languages(&self) -> Vec<SupportedLanguages> {
        self.pending_clients.lock().await.iter().copied().collect()
    }

    /// Checks if a specific language is still initializing
    pub async fn is_language_pending(&self, lang: SupportedLanguages) -> bool {
        self.pending_clients.lock().await.contains(&lang)
    }

    /// Waits for all pending language servers to initialize with a timeout.
    ///
    /// Returns true if all pending clients finished initializing within the timeout,
    /// or false if the timeout was reached with some clients still pending.
    pub async fn wait_for_pending_clients(&self, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(100);

        loop {
            let pending = self.pending_clients.lock().await;
            if pending.is_empty() {
                return true;
            }
            drop(pending); // Release lock before sleeping

            if start.elapsed() >= timeout {
                log::debug!(
                    "wait_for_pending_clients: timeout reached with {} clients still pending",
                    self.pending_clients.lock().await.len()
                );
                return false;
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    pub async fn start_langservers(
        &mut self,
        workspace_path: &str,
        config: Option<&LspMcpConfig>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let lsps = match config {
            Some(cfg) => cfg
                .languages
                .iter()
                .filter_map(|s| SupportedLanguages::from_str(s).ok())
                .collect(),
            None => detect_languages_in_workspace(workspace_path),
        };

        let mut seen = HashSet::new();
        let manager = &*self;
        let mut starts = stream::iter(lsps.into_iter().filter(|lsp| seen.insert(*lsp)))
            .map(|lsp| async move {
                if manager.get_client(lsp).await.is_some() {
                    return false;
                }

                let binary = config
                    .and_then(|c| c.get_binary(&lsp.to_string().to_lowercase()))
                    .map(|s| s.as_str());

                debug!("Starting {:?} LSP", lsp);
                let client_result = create_lsp_client(
                    lsp,
                    workspace_path,
                    manager.watch_events_sender.subscribe(),
                    binary,
                )
                .await;

                match client_result {
                    Ok(mut client) => match client.initialize(workspace_path.to_string()).await {
                        Ok(_) => {
                            debug!("Setting up workspace for {:?}", lsp);
                            if let Err(e) = client.setup_workspace(workspace_path).await {
                                warn!("Failed to setup workspace for {:?}: {}. Skipping", lsp, e);
                                return false;
                            }
                            manager
                                .lsp_clients
                                .write()
                                .await
                                .insert(lsp, Arc::new(Mutex::new(client)));
                            true
                        }
                        Err(e) => {
                            warn!(
                                "Failed to initialize {:?} language server: {}. Skipping",
                                lsp, e
                            );
                            false
                        }
                    },
                    Err(e) => {
                        warn!("Failed to start {:?} language server: {}. Skipping", lsp, e);
                        false
                    }
                }
            })
            .buffer_unordered(CLIENT_CONCURRENCY);
        let mut started_count = 0;
        while let Some(started) = starts.next().await {
            started_count += usize::from(started);
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
            Some(cfg) => cfg
                .languages
                .iter()
                .filter_map(|s| SupportedLanguages::from_str(s).ok())
                .collect(),
            None => detect_languages_in_workspace(workspace_path),
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
                let client_result = create_lsp_client(lsp, &ws_path, events_rx, binary_ref).await;

                match client_result {
                    Ok(mut client) => match client.initialize(ws_path.clone()).await {
                        Ok(_) => {
                            debug!("Setting up workspace for {:?}", lsp);
                            if let Err(e) = client.setup_workspace(&ws_path).await {
                                warn!("Failed to setup workspace for {:?}: {}. Skipping", lsp, e);
                                pending_clients.lock().await.remove(&lsp);
                                return;
                            }
                            lsp_clients
                                .write()
                                .await
                                .insert(lsp, Arc::new(Mutex::new(client)));
                            pending_clients.lock().await.remove(&lsp);
                            info!("{:?} language server is now ready", lsp);
                        }
                        Err(e) => {
                            warn!(
                                "Failed to initialize {:?} language server: {}. Skipping",
                                lsp, e
                            );
                            pending_clients.lock().await.remove(&lsp);
                        }
                    },
                    Err(e) => {
                        warn!("Failed to start {:?} language server: {}. Skipping", lsp, e);
                        pending_clients.lock().await.remove(&lsp);
                    }
                }
            });
        }
    }
}
