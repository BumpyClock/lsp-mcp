use super::core::{Manager, CLIENT_CONCURRENCY};
use super::detection::detect_languages_in_workspace;
use super::error::LspManagerError;
use crate::api_types::{LanguageServerHealth, LspStatus, SupportedLanguages};
use crate::config::{LanguageSelection, LspMcpConfig};
use crate::lsp::client::LspClient;
use crate::lsp::process::ProcessHealth;
use crate::lsp::registry::LanguageMetadata;
use crate::lsp::{PendingRequests, ProcessHandler};
use futures::{stream, StreamExt};
use log::{info, warn};
use notify_debouncer_mini::DebouncedEvent;
use std::collections::HashMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Clone)]
pub(super) struct ServerState {
    pub health: LanguageServerHealth,
    runtime: Option<(PendingRequests, ProcessHandler)>,
}

pub async fn create_lsp_client(
    lsp: SupportedLanguages,
    workspace_path: &str,
    events_rx: tokio::sync::broadcast::Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Result<Box<dyn LspClient>, Box<dyn Error + Send + Sync>> {
    let metadata = LanguageMetadata::get(lsp)
        .ok_or_else(|| format!("No registry entry found for language {lsp:?}"))?;
    (metadata.factory)(workspace_path, events_rx, binary).await
}

fn allows(config: &LspMcpConfig, language: SupportedLanguages) -> bool {
    match &config.languages {
        LanguageSelection::Auto => true,
        LanguageSelection::Explicit(languages) => languages.contains(&language),
    }
}

impl Manager {
    pub async fn pending_languages(&self) -> Vec<SupportedLanguages> {
        self.server_health
            .read()
            .await
            .iter()
            .filter_map(|(language, state)| {
                (state.health.status == LspStatus::Initializing).then_some(*language)
            })
            .collect()
    }

    pub async fn is_language_pending(&self, language: SupportedLanguages) -> bool {
        self.server_health
            .read()
            .await
            .get(&language)
            .is_some_and(|state| state.health.status == LspStatus::Initializing)
    }

    /// Wait for discovery and the startup tasks it selected, without polling.
    pub async fn wait_for_pending_clients(&self, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, async {
            let _discovery = self.discovery_lock.lock().await;
            for language in self.pending_languages().await {
                let _guard = self.startup_locks[&language].lock().await;
            }
        })
        .await
        .is_ok()
    }

    /// Read live progress and process state without acquiring the request mutex or starting servers.
    pub async fn language_health(&self) -> HashMap<SupportedLanguages, LanguageServerHealth> {
        let config = self.config.read().await.clone();
        let states = self.server_health.read().await.clone();
        let clients = self.lsp_clients.read().await.clone();
        let discovering = self.discovery_lock.try_lock().is_err();
        let mut result = HashMap::new();
        for metadata in LanguageMetadata::all() {
            let command = config
                .get_binary(&metadata.id.to_string())
                .cloned()
                .unwrap_or_else(|| metadata.default_binary.to_owned());
            let mut health = if !allows(&config, metadata.id) {
                LanguageServerHealth {
                    status: LspStatus::Disabled,
                    command: Some(command),
                    reason: Some("Excluded by the configured languages list".into()),
                }
            } else if let Some(state) = states.get(&metadata.id) {
                state.health.clone()
            } else if clients.contains_key(&metadata.id) {
                LanguageServerHealth {
                    status: LspStatus::Ready,
                    command: Some(command),
                    reason: None,
                }
            } else if discovering {
                LanguageServerHealth {
                    status: LspStatus::Initializing,
                    command: Some(command),
                    reason: Some("Workspace language discovery in progress".into()),
                }
            } else {
                LanguageServerHealth {
                    status: LspStatus::NotDetected,
                    command: Some(command),
                    reason: Some("No workspace file or request has selected this language".into()),
                }
            };
            if health.status == LspStatus::Ready {
                if let Some((progress, process)) = states
                    .get(&metadata.id)
                    .and_then(|state| state.runtime.as_ref())
                {
                    if !process.is_alive().await {
                        health.status = LspStatus::Failed;
                        health.reason = Some(match &*process.health_receiver().borrow() {
                            ProcessHealth::Unhealthy(reason) => reason.clone(),
                            _ => "Language server process exited".into(),
                        });
                    } else if progress.is_loading().await {
                        health.status = LspStatus::Initializing;
                        health.reason = Some("Server reports work in progress".into());
                    }
                }
            }
            result.insert(metadata.id, health);
        }
        result
    }

    async fn schedule_language(&self, language: SupportedLanguages) {
        let config = self.config.read().await.clone();
        if !allows(&config, language) || self.get_client(language).await.is_some() {
            return;
        }
        // The task owns the language's startup slot. Waiting requests share it,
        // and cancellation of a requester cannot cancel initialization for others.
        let Ok(guard) = self.startup_locks[&language].clone().try_lock_owned() else {
            return;
        };
        if self.get_client(language).await.is_some() {
            return;
        }
        let metadata = LanguageMetadata::get(language).expect("registered language");
        let binary = config.get_binary(&language.to_string()).map(String::as_str);
        let command = binary.unwrap_or(metadata.default_binary).to_owned();
        let resolved = match metadata.resolve_binary(binary, Path::new(&self.root_path)) {
            Ok(path) => path,
            Err(reason) => {
                warn!("{reason}");
                self.server_health.write().await.insert(
                    language,
                    ServerState {
                        health: LanguageServerHealth {
                            status: LspStatus::MissingExecutable,
                            command: Some(command),
                            reason: Some(reason),
                        },
                        runtime: None,
                    },
                );
                return;
            }
        };
        let command = resolved.to_string_lossy().into_owned();
        self.server_health.write().await.insert(
            language,
            ServerState {
                health: LanguageServerHealth {
                    status: LspStatus::Initializing,
                    command: Some(command.clone()),
                    reason: None,
                },
                runtime: None,
            },
        );
        let clients = self.lsp_clients.clone();
        let health = self.server_health.clone();
        let events = self.watch_events_sender.subscribe();
        let root = self.root_path.clone();
        let permits = self.startup_permits.clone();
        tokio::spawn(async move {
            let _guard = guard;
            let _permit = permits
                .acquire_owned()
                .await
                .expect("startup semaphore remains open");
            let result = async {
                let mut client = create_lsp_client(language, &root, events, Some(&command))
                    .await
                    .map_err(|error| format!("Failed to spawn '{command}': {error}"))?;
                let initialized = async {
                    client.initialize(root.clone()).await.map_err(|error| {
                        format!("Initialization failed for '{command}': {error}")
                    })?;
                    client.setup_workspace(&root).await.map_err(|error| {
                        format!("Workspace setup failed for '{command}': {error}")
                    })?;
                    Ok::<_, String>(())
                }
                .await;
                if let Err(error) = initialized {
                    let _ = client.get_process().kill().await;
                    return Err(error);
                }
                let runtime = (
                    client.get_pending_requests().clone(),
                    client.get_process().clone(),
                );
                clients
                    .write()
                    .await
                    .insert(language, Arc::new(Mutex::new(client)));
                Ok::<_, String>(runtime)
            }
            .await;
            let (status, reason, runtime) = match result {
                Ok(runtime) => {
                    info!("{language} language server is ready ({command})");
                    (LspStatus::Ready, None, Some(runtime))
                }
                Err(reason) => {
                    warn!("{reason}");
                    (LspStatus::Failed, Some(reason), None)
                }
            };
            health.write().await.insert(
                language,
                ServerState {
                    health: LanguageServerHealth {
                        status,
                        command: Some(command),
                        reason,
                    },
                    runtime,
                },
            );
        });
    }

    /// Acquire a selected client, starting it if a file request introduces its language.
    pub async fn ensure_client(
        &self,
        language: SupportedLanguages,
    ) -> Result<Arc<Mutex<Box<dyn LspClient>>>, LspManagerError> {
        if !allows(&*self.config.read().await, language) {
            return Err(LspManagerError::LspStartupFailed {
                language,
                reason: "Disabled by the configured languages list".into(),
            });
        }
        if let Some(client) = self.get_client(language).await {
            let runtime = self
                .server_health
                .read()
                .await
                .get(&language)
                .and_then(|state| state.runtime.clone());
            if let Some((_, process)) = runtime {
                if process.is_alive().await {
                    return Ok(client);
                }
                return Err(self.client_unavailable_error(language).await);
            } else {
                return Ok(client);
            }
        }
        self.schedule_language(language).await;
        let _guard = self.startup_locks[&language].lock().await;
        if let Some(client) = self.get_client(language).await {
            return Ok(client);
        }
        Err(self.client_unavailable_error(language).await)
    }

    async fn discover_and_start(&self, workspace_path: String, selection: LanguageSelection) {
        let languages = match selection {
            LanguageSelection::Auto => {
                tokio::task::spawn_blocking(move || detect_languages_in_workspace(&workspace_path))
                    .await
                    .expect("workspace discovery task completes")
            }
            LanguageSelection::Explicit(languages) => languages,
        };
        for language in languages {
            self.schedule_language(language).await;
        }
    }

    pub async fn start_langservers(
        &mut self,
        workspace_path: &str,
        config: Option<&LspMcpConfig>,
    ) -> Result<(), Box<dyn Error>> {
        let config = config.cloned().unwrap_or_default();
        let selection = config.languages.clone();
        *self.config.write().await = config;
        self.discover_and_start(workspace_path.to_owned(), selection)
            .await;
        let pending = self.pending_languages().await;
        let startup_locks = &self.startup_locks;
        let mut waits = stream::iter(pending)
            .map(|language| async move {
                let _guard = startup_locks[&language].lock().await;
            })
            .buffer_unordered(CLIENT_CONCURRENCY);
        while waits.next().await.is_some() {}
        if matches!(&self.config.read().await.languages, LanguageSelection::Explicit(languages) if !languages.is_empty())
            && self.lsp_clients.read().await.is_empty()
        {
            let reasons: Vec<_> = self
                .server_health
                .read()
                .await
                .values()
                .filter_map(|state| state.health.reason.clone())
                .collect();
            return Err(format!(
                "No configured language servers could start: {}",
                reasons.join("; ")
            )
            .into());
        }
        Ok(())
    }

    /// Return before filesystem discovery or language-server initialization finishes.
    pub async fn start_langservers_async(
        self: &Arc<Self>,
        workspace_path: &str,
        config: Option<LspMcpConfig>,
    ) {
        let config = config.unwrap_or_default();
        let selection = config.languages.clone();
        *self.config.write().await = config;
        let guard = self.discovery_lock.clone().lock_owned().await;
        let manager = self.clone();
        let workspace_path = workspace_path.to_owned();
        tokio::spawn(async move {
            let _guard = guard;
            manager.discover_and_start(workspace_path, selection).await;
        });
    }
}
