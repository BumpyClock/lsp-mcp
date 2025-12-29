// ABOUTME: JSON-RPC message handling for LSP communication
// ABOUTME: Provides request/response routing, notification handling, and diagnostics storage

use lsp_types::{Diagnostic, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;

pub trait JsonRpc: Send + Sync {
    fn create_success_response(&self, id: u64) -> String;
    fn create_request(&self, method: &str, params: Option<Value>) -> (u64, String);
    fn create_notification(&self, method: &str, params: Value) -> String;
    fn parse_message(&self, data: &str) -> Result<JsonRpcMessage, JsonRpcError>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub method: Option<String>,
    pub params: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InnerMessage {
    pub message: String,
    pub r#type: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for JsonRpcError {}

#[derive(Clone)]
pub struct JsonRpcHandler {
    id_counter: Arc<AtomicU64>,
}

impl JsonRpcHandler {
    pub fn new() -> Self {
        Self {
            id_counter: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl JsonRpc for JsonRpcHandler {
    fn create_success_response(&self, id: u64) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": null
        })
        .to_string()
    }

    fn create_request(&self, method: &str, params: Option<Value>) -> (u64, String) {
        let id = self.id_counter.fetch_add(1, Ordering::Relaxed);
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params.unwrap_or(serde_json::Value::Null)
        })
        .to_string();
        (id, request)
    }

    fn create_notification(&self, method: &str, params: Value) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        })
        .to_string()
    }

    fn parse_message(&self, data: &str) -> Result<JsonRpcMessage, JsonRpcError> {
        serde_json::from_str(data).map_err(|e| JsonRpcError {
            code: -32700,
            message: e.to_string(),
            data: None,
        })
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ExpectedMessageKey {
    pub method: String,
    pub params: Value,
}

#[derive(Clone)]
pub struct PendingRequests {
    request_channels: Arc<Mutex<HashMap<u64, Sender<JsonRpcMessage>>>>,
    notification_channels: Arc<Mutex<HashMap<ExpectedMessageKey, Sender<JsonRpcMessage>>>>,
    method_handlers: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<JsonRpcMessage>>>>,
}

impl PendingRequests {
    pub fn new() -> Self {
        Self {
            request_channels: Arc::new(Mutex::new(HashMap::new())),
            notification_channels: Arc::new(Mutex::new(HashMap::new())),
            method_handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn add_request(
        &self,
        id: u64,
    ) -> Result<Receiver<JsonRpcMessage>, Box<dyn Error + Send + Sync>> {
        let (tx, rx) = channel::<JsonRpcMessage>(16);
        self.request_channels.lock().await.insert(id, tx);
        Ok(rx)
    }

    pub async fn remove_request(
        &self,
        id: u64,
    ) -> Result<Option<Sender<JsonRpcMessage>>, Box<dyn Error + Send + Sync>> {
        Ok(self.request_channels.lock().await.remove(&id))
    }

    pub async fn add_notification(
        &self,
        expected_message: ExpectedMessageKey,
    ) -> Result<Receiver<JsonRpcMessage>, Box<dyn Error + Send + Sync>> {
        let (tx, rx) = channel::<JsonRpcMessage>(16);
        self.notification_channels
            .lock()
            .await
            .insert(expected_message, tx);
        Ok(rx)
    }

    pub async fn remove_notification(
        &self,
        pattern: ExpectedMessageKey,
    ) -> Option<Sender<JsonRpcMessage>> {
        self.notification_channels.lock().await.remove(&pattern)
    }

    /// Register a handler for all notifications of a specific method
    pub async fn register_method_handler(
        &self,
        method: String,
    ) -> mpsc::UnboundedReceiver<JsonRpcMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.method_handlers.lock().await.insert(method, tx);
        rx
    }

    /// Route a notification to a method handler if one exists
    pub async fn route_to_method_handler(&self, method: &str, message: JsonRpcMessage) -> bool {
        if let Some(sender) = self.method_handlers.lock().await.get(method) {
            sender.send(message).is_ok()
        } else {
            false
        }
    }

    /// Fail all pending requests with an error message
    /// Called when the LSP process dies to notify all waiting requests
    pub async fn fail_all_requests(&self, error_message: String) {
        let mut channels = self.request_channels.lock().await;
        for (id, sender) in channels.drain() {
            let error_response = JsonRpcMessage {
                jsonrpc: "2.0".to_string(),
                id: Some(id),
                method: None,
                params: None,
                result: None,
                error: Some(JsonRpcError {
                    code: -32099,
                    message: error_message.clone(),
                    data: None,
                }),
            };
            let _ = sender.send(error_response);
        }
    }
}

/// Thread-safe storage for diagnostics received via publishDiagnostics notifications
#[derive(Clone)]
pub struct DiagnosticsStore {
    diagnostics: Arc<RwLock<HashMap<Url, Vec<Diagnostic>>>>,
}

impl DiagnosticsStore {
    pub fn new() -> Self {
        Self {
            diagnostics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update diagnostics for a file (replaces existing)
    pub async fn update(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        let mut store = self.diagnostics.write().await;
        if diagnostics.is_empty() {
            store.remove(&uri);
        } else {
            store.insert(uri, diagnostics);
        }
    }

    /// Get diagnostics for a specific file
    pub async fn get(&self, uri: &Url) -> Option<Vec<Diagnostic>> {
        self.diagnostics.read().await.get(uri).cloned()
    }

    /// Get all diagnostics (for workspace-wide query)
    pub async fn get_all(&self) -> HashMap<Url, Vec<Diagnostic>> {
        self.diagnostics.read().await.clone()
    }

    /// Clear all diagnostics
    pub async fn clear(&self) {
        self.diagnostics.write().await.clear();
    }
}

impl Default for DiagnosticsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{DiagnosticSeverity, Position, Range};
    use std::time::Duration;
    use tokio::time::timeout;

    fn create_test_diagnostic(message: &str) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 10,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("test".to_string()),
            message: message.to_string(),
            related_information: None,
            tags: None,
            data: None,
        }
    }

    fn create_test_url(path: &str) -> Url {
        Url::parse(&format!("file://{}", path)).expect("test URL should be valid")
    }

    #[tokio::test]
    async fn diagnostics_store_returns_none_for_unknown_file() {
        let store = DiagnosticsStore::new();
        let uri = create_test_url("/test/unknown_file.rs");

        let result = store.get(&uri).await;

        assert!(
            result.is_none(),
            "expected None for unknown file, got Some"
        );
    }

    #[tokio::test]
    async fn diagnostics_store_stores_and_retrieves_diagnostics() {
        let store = DiagnosticsStore::new();
        let uri = create_test_url("/test/file.rs");
        let diag = create_test_diagnostic("test error");

        store.update(uri.clone(), vec![diag.clone()]).await;
        let result = store.get(&uri).await;

        assert!(result.is_some(), "expected Some, got None");
        let diagnostics = result.unwrap();
        assert_eq!(
            diagnostics.len(),
            1,
            "expected one diagnostic, got {}",
            diagnostics.len()
        );
        assert_eq!(
            diagnostics[0].message, "test error",
            "expected 'test error', got '{}'",
            diagnostics[0].message
        );
    }

    #[tokio::test]
    async fn diagnostics_store_replaces_existing_diagnostics() {
        let store = DiagnosticsStore::new();
        let uri = create_test_url("/test/file.rs");
        let diag1 = create_test_diagnostic("first error");
        let diag2 = create_test_diagnostic("second error");

        store.update(uri.clone(), vec![diag1]).await;
        store.update(uri.clone(), vec![diag2.clone()]).await;
        let result = store.get(&uri).await.unwrap();

        assert_eq!(
            result.len(),
            1,
            "expected one diagnostic after replacement, got {}",
            result.len()
        );
        assert_eq!(
            result[0].message, "second error",
            "expected 'second error', got '{}'",
            result[0].message
        );
    }

    #[tokio::test]
    async fn diagnostics_store_removes_entry_when_empty_diagnostics_provided() {
        let store = DiagnosticsStore::new();
        let uri = create_test_url("/test/file.rs");
        let diag = create_test_diagnostic("test error");

        store.update(uri.clone(), vec![diag]).await;
        store.update(uri.clone(), vec![]).await;
        let result = store.get(&uri).await;

        assert!(
            result.is_none(),
            "expected None after clearing with empty vec, got Some"
        );
    }

    #[tokio::test]
    async fn diagnostics_store_get_all_returns_all_files() {
        let store = DiagnosticsStore::new();
        let uri1 = create_test_url("/test/file1.rs");
        let uri2 = create_test_url("/test/file2.rs");
        let diag1 = create_test_diagnostic("error 1");
        let diag2 = create_test_diagnostic("error 2");

        store.update(uri1.clone(), vec![diag1]).await;
        store.update(uri2.clone(), vec![diag2]).await;
        let all = store.get_all().await;

        assert_eq!(all.len(), 2, "expected 2 files, got {}", all.len());
        assert!(all.contains_key(&uri1), "expected uri1 to be present");
        assert!(all.contains_key(&uri2), "expected uri2 to be present");
    }

    #[tokio::test]
    async fn diagnostics_store_clear_removes_all_entries() {
        let store = DiagnosticsStore::new();
        let uri1 = create_test_url("/test/file1.rs");
        let uri2 = create_test_url("/test/file2.rs");
        let diag = create_test_diagnostic("error");

        store.update(uri1, vec![diag.clone()]).await;
        store.update(uri2, vec![diag]).await;
        store.clear().await;
        let all = store.get_all().await;

        assert!(all.is_empty(), "expected empty after clear, got {} entries", all.len());
    }

    #[tokio::test]
    async fn diagnostics_store_default_creates_empty_store() {
        let store = DiagnosticsStore::default();
        let all = store.get_all().await;

        assert!(
            all.is_empty(),
            "expected empty store from default, got {} entries",
            all.len()
        );
    }

    #[tokio::test]
    async fn diagnostics_store_clone_shares_state() {
        let store1 = DiagnosticsStore::new();
        let store2 = store1.clone();
        let uri = create_test_url("/test/file.rs");
        let diag = create_test_diagnostic("shared error");

        store1.update(uri.clone(), vec![diag]).await;
        let result = store2.get(&uri).await;

        assert!(
            result.is_some(),
            "expected cloned store to see updates from original"
        );
    }

    #[tokio::test]
    async fn pending_requests_method_handler_receives_routed_messages() {
        let pending = PendingRequests::new();
        let method = "textDocument/publishDiagnostics";
        let mut receiver = pending.register_method_handler(method.to_string()).await;

        let message = JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some(method.to_string()),
            params: Some(serde_json::json!({"uri": "file:///test.rs"})),
            result: None,
            error: None,
        };

        let routed = pending
            .route_to_method_handler(method, message.clone())
            .await;
        assert!(routed, "expected message to be routed successfully");

        let received = timeout(Duration::from_millis(100), receiver.recv())
            .await
            .expect("expected to receive message within timeout")
            .expect("expected channel to have message");

        assert_eq!(
            received.method,
            Some(method.to_string()),
            "expected method to match"
        );
    }

    #[tokio::test]
    async fn pending_requests_route_to_method_handler_returns_false_for_unknown_method() {
        let pending = PendingRequests::new();

        let message = JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some("unknown/method".to_string()),
            params: None,
            result: None,
            error: None,
        };

        let routed = pending
            .route_to_method_handler("unknown/method", message)
            .await;

        assert!(
            !routed,
            "expected false when no handler registered for method"
        );
    }

    #[tokio::test]
    async fn pending_requests_method_handler_can_be_replaced() {
        let pending = PendingRequests::new();
        let method = "test/method";

        let _receiver1 = pending.register_method_handler(method.to_string()).await;
        let mut receiver2 = pending.register_method_handler(method.to_string()).await;

        let message = JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some(method.to_string()),
            params: None,
            result: None,
            error: None,
        };

        pending.route_to_method_handler(method, message).await;

        let received = timeout(Duration::from_millis(100), receiver2.recv())
            .await
            .expect("expected second receiver to get message within timeout");

        assert!(
            received.is_some(),
            "expected second handler to receive message after replacement"
        );
    }

    #[tokio::test]
    async fn pending_requests_fail_all_requests_notifies_all_waiting_receivers() {
        let pending = PendingRequests::new();

        let mut receiver1 = pending.add_request(1).await.expect("add_request should succeed");
        let mut receiver2 = pending.add_request(2).await.expect("add_request should succeed");

        pending.fail_all_requests("LSP process died".to_string()).await;

        let response1 = timeout(Duration::from_millis(100), receiver1.recv())
            .await
            .expect("expected receiver1 to get message within timeout")
            .expect("expected channel to have message");

        let response2 = timeout(Duration::from_millis(100), receiver2.recv())
            .await
            .expect("expected receiver2 to get message within timeout")
            .expect("expected channel to have message");

        assert!(
            response1.error.is_some(),
            "expected response1 to have error"
        );
        assert!(
            response2.error.is_some(),
            "expected response2 to have error"
        );
        assert_eq!(
            response1.error.as_ref().unwrap().message,
            "LSP process died",
            "expected error message to match"
        );
    }

    #[tokio::test]
    async fn pending_requests_fail_all_requests_clears_request_channels() {
        let pending = PendingRequests::new();

        let _receiver = pending.add_request(1).await.expect("add_request should succeed");

        pending.fail_all_requests("test error".to_string()).await;

        let channels = pending.request_channels.lock().await;
        assert!(
            channels.is_empty(),
            "expected request_channels to be empty after fail_all_requests"
        );
    }
}
