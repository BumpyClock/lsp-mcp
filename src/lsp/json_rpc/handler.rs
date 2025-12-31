// ABOUTME: JSON-RPC handler for creating and parsing LSP protocol messages
// ABOUTME: Manages request ID counter and provides message serialization

use super::message::{JsonRpcError, JsonRpcMessage};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub trait JsonRpc: Send + Sync {
    fn create_success_response(&self, id: u64) -> String;
    fn create_request(&self, method: &str, params: Option<Value>) -> (u64, String);
    fn create_notification(&self, method: &str, params: Value) -> String;
    fn parse_message(&self, data: &str) -> Result<JsonRpcMessage, JsonRpcError>;
}

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
