// ABOUTME: JSON-RPC module for LSP protocol message handling
// ABOUTME: Re-exports message types, handlers, pending requests, and diagnostics storage

mod diagnostics;
mod handler;
mod message;
mod pending;

pub use diagnostics::DiagnosticsStore;
pub use handler::{JsonRpc, JsonRpcHandler};
pub use message::{InnerMessage, JsonRpcError, JsonRpcMessage};
pub use pending::{ExpectedMessageKey, PendingRequests};

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{DiagnosticSeverity, Position, Range};
    use std::time::Duration;
    use tokio::time::timeout;

    fn create_test_diagnostic(message: &str) -> lsp_types::Diagnostic {
        lsp_types::Diagnostic {
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

    fn create_test_url(path: &str) -> lsp_types::Url {
        lsp_types::Url::parse(&format!("file://{}", path)).expect("test URL should be valid")
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

        let binding = pending.request_channels_for_test();
        let channels = binding.lock().await;
        assert!(
            channels.is_empty(),
            "expected request_channels to be empty after fail_all_requests"
        );
    }
}
