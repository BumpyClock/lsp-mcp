use super::*;
use crate::lsp::client::LspClientConfig;
use crate::lsp::{DiagnosticsStore, JsonRpcHandler, PendingRequests, ProcessHandler};
use crate::utils::workspace_documents::WorkspaceDocumentsHandler;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{mpsc, Semaphore};

#[derive(Default)]
struct QueryStats {
    active: AtomicUsize,
    peak: AtomicUsize,
}

struct ActiveQuery(Arc<QueryStats>);

impl Drop for ActiveQuery {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

struct MockClient {
    name: String,
    events: mpsc::UnboundedSender<String>,
    permits: Arc<Semaphore>,
    stats: Arc<QueryStats>,
    fail_query: bool,
    documents: WorkspaceDocumentsHandler,
    diagnostics: DiagnosticsStore,
}

#[async_trait::async_trait]
impl LspClient for MockClient {
    async fn prepare_workspace_symbol_request(
        &mut self,
        _query: &str,
    ) -> Result<
        crate::lsp::client::LspResponse<Vec<lsp_types::SymbolInformation>>,
        Box<dyn Error + Send + Sync>,
    > {
        let stats = self.stats.clone();
        let events = self.events.clone();
        let name = self.name.clone();
        let permits = self.permits.clone();
        let fail_query = self.fail_query;
        Ok(Box::pin(async move {
            let active = stats.active.fetch_add(1, Ordering::SeqCst) + 1;
            stats.peak.fetch_max(active, Ordering::SeqCst);
            let _active = ActiveQuery(stats.clone());
            events.send(name.clone()).unwrap();
            permits.acquire().await.unwrap().forget();
            if fail_query {
                return Err("query failure".into());
            }
            #[allow(deprecated)]
            Ok(vec![lsp_types::SymbolInformation {
                name: "shared".into(),
                kind: lsp_types::SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: Url::parse(&format!("file:///{}.rs", name)).unwrap(),
                    range: Range::default(),
                },
                container_name: None,
            }])
        }))
    }

    async fn prepare_document_symbol_request(
        &mut self,
        _path: &str,
    ) -> Result<
        crate::lsp::client::LspResponse<Option<DocumentSymbolResponse>>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(Box::pin(async {
            #[allow(deprecated)]
            Ok(Some(DocumentSymbolResponse::Nested(vec![
                lsp_types::DocumentSymbol {
                    name: "example".into(),
                    detail: Some("fn example()".into()),
                    kind: lsp_types::SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range: Range::default(),
                    selection_range: Range::default(),
                    children: None,
                },
            ])))
        }))
    }

    fn get_process(&mut self) -> &mut ProcessHandler {
        unreachable!("workspace queries do not use a process")
    }

    fn get_json_rpc(&mut self) -> &mut JsonRpcHandler {
        unreachable!("workspace queries do not use JSON-RPC")
    }

    fn get_pending_requests(&mut self) -> &mut PendingRequests {
        unreachable!("workspace queries do not use pending requests")
    }

    fn get_workspace_documents(&mut self) -> &mut WorkspaceDocumentsHandler {
        self.events.send(self.name.clone()).unwrap();
        &mut self.documents
    }

    fn get_diagnostics_store(&self) -> &DiagnosticsStore {
        self.events.send(self.name.clone()).unwrap();
        &self.diagnostics
    }

    fn get_config(&self) -> &LspClientConfig {
        unreachable!("workspace queries do not use client configuration")
    }
}

struct Fixture {
    manager: Manager,
    events: mpsc::UnboundedReceiver<String>,
    permits: Arc<Semaphore>,
    stats: Arc<QueryStats>,
    _root: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut manager = Manager::new(root.path().to_str().unwrap()).await.unwrap();
        let watch_events_sender = manager.watch_events_sender.clone();
        let (events_tx, events) = mpsc::unbounded_channel();
        let permits = Arc::new(Semaphore::new(0));
        let stats = Arc::new(QueryStats::default());
        let mut clients = HashMap::new();
        for (index, metadata) in LanguageMetadata::all().take(6).enumerate() {
            let name = format!("client{index}");
            let directory = root.path().join(&name);
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(directory.join("source.rs"), "fn example() {}\n").unwrap();
            let diagnostics = DiagnosticsStore::new();
            diagnostics
                .update(
                    Url::parse("file:///shared.rs").unwrap(),
                    vec![lsp_types::Diagnostic {
                        message: name.clone(),
                        ..Default::default()
                    }],
                )
                .await;
            let client: Box<dyn LspClient> = Box::new(MockClient {
                name,
                events: events_tx.clone(),
                permits: permits.clone(),
                stats: stats.clone(),
                fail_query: index == 1,
                documents: WorkspaceDocumentsHandler::new(
                    &directory,
                    vec!["**/*.rs".into()],
                    vec![],
                    watch_events_sender.subscribe(),
                    DidOpenConfiguration::None,
                ),
                diagnostics,
            });
            clients.insert(metadata.id, Arc::new(Mutex::new(client)));
        }
        manager.lsp_clients = Arc::new(RwLock::new(clients));
        Self {
            manager,
            events,
            permits,
            stats,
            _root: root,
        }
    }
}

#[tokio::test]
async fn declaration_page_reports_file_identity_mtime_and_pagination() {
    let fixture = Fixture::new().await;
    let root = fixture._root.path();
    let file = std::fs::File::open(root.join("client0/source.rs")).unwrap();
    file.set_times(
        std::fs::FileTimes::new()
            .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
    )
    .unwrap();
    let client = fixture
        .manager
        .get_client(SupportedLanguages::Python)
        .await
        .unwrap();
    fixture
        .manager
        .lsp_clients
        .write()
        .await
        .insert(SupportedLanguages::Rust, client);
    crate::api_types::set_thread_local_mount_dir(root);
    let service = crate::service::create_service(Arc::new(fixture.manager));
    let result = service
        .definitions_in_file("client0/source.rs", false, false, Some(1), Some(1), 0)
        .await;
    crate::api_types::unset_thread_local_mount_dir();
    let page = result.unwrap();
    assert_eq!(page.path, "client0/source.rs");
    assert_eq!(page.mtime, "2023-11-14T22:13:20+00:00");
    assert_eq!((page.limit, page.offset), (1, 1));
    assert!(page.symbols.is_empty());
    assert!(!page.truncated);
}

#[tokio::test]
async fn workspace_watching_survives_manager_initialization() {
    let root = tempfile::tempdir().unwrap();
    let manager = Manager::new(root.path().to_str().unwrap()).await.unwrap();
    let mut events = manager.watch_events_sender.subscribe();
    let file = root.path().join("created.rs");
    std::fs::write(&file, "fn created() {}\n").unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if events.recv().await.unwrap().path == file {
                break;
            }
        }
    })
    .await
    .expect("file changes must reach subscribers after Manager::new returns");
}

async fn next_event(events: &mut mpsc::UnboundedReceiver<String>) -> String {
    tokio::time::timeout(Duration::from_secs(1), events.recv())
        .await
        .expect("another client must make progress")
        .unwrap()
}

#[tokio::test]
async fn workspace_symbols_bound_concurrency_preserve_order_and_partial_success() {
    let mut fixture = Fixture::new().await;
    let (symbols, ()) = tokio::join!(fixture.manager.workspace_symbol("query"), async {
        for _ in 0..CLIENT_CONCURRENCY {
            next_event(&mut fixture.events).await;
        }
        assert_eq!(fixture.stats.active.load(Ordering::SeqCst), 4);
        assert!(fixture.events.try_recv().is_err());
        let mut clients =
            tokio::time::timeout(Duration::from_secs(1), fixture.manager.lsp_clients.write())
                .await
                .expect("in-flight queries must allow ready clients to be published");
        let client = clients.get(&SupportedLanguages::Python).unwrap().clone();
        clients.insert(SupportedLanguages::Ruby, client);
        drop(clients);
        fixture.permits.add_permits(6);
    });
    let names: Vec<_> = symbols
        .unwrap()
        .into_iter()
        .map(|symbol| symbol.location.uri.path().to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "/client0.rs",
            "/client2.rs",
            "/client3.rs",
            "/client4.rs",
            "/client5.rs"
        ]
    );
    assert_eq!(fixture.stats.peak.load(Ordering::SeqCst), 4);
    assert_eq!(fixture.stats.active.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn dropping_workspace_query_releases_client_locks_and_inflight_work() {
    let mut fixture = Fixture::new().await;
    {
        let query = fixture.manager.workspace_symbol("query");
        tokio::pin!(query);
        tokio::select! {
            _ = &mut query => panic!("requests must wait for permits"),
            _ = async {
                for _ in 0..CLIENT_CONCURRENCY {
                    next_event(&mut fixture.events).await;
                }
            } => {}
        }
        assert_eq!(fixture.stats.active.load(Ordering::SeqCst), 4);
    }
    assert_eq!(fixture.stats.active.load(Ordering::SeqCst), 0);
    for client in fixture.manager.clients_snapshot().await {
        assert!(client.try_lock().is_ok());
    }
}

#[tokio::test]
async fn listing_files_progresses_past_busy_client_and_returns_sorted_files() {
    let mut fixture = Fixture::new().await;
    let first_client = fixture
        .manager
        .get_client(SupportedLanguages::Python)
        .await
        .unwrap();
    let busy_client = first_client.lock().await;
    let (files, ()) = tokio::join!(fixture.manager.list_files(), async {
        assert_ne!(next_event(&mut fixture.events).await, "client0");
        drop(busy_client);
    });
    let files = files.unwrap();
    assert_eq!(files.len(), 6);
    assert!(files.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[tokio::test]
async fn diagnostics_progress_past_busy_client_and_merge_in_registry_order() {
    let mut fixture = Fixture::new().await;
    let first_client = fixture
        .manager
        .get_client(SupportedLanguages::Python)
        .await
        .unwrap();
    let busy_client = first_client.lock().await;
    let (diagnostics, ()) = tokio::join!(fixture.manager.get_diagnostics(None), async {
        assert_ne!(next_event(&mut fixture.events).await, "client0");
        drop(busy_client);
    });
    let diagnostics = diagnostics.unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics.values().next().unwrap()[0].message, "client5");
}
