// ABOUTME: Pending request tracking for LSP client request/response correlation
// ABOUTME: Manages request channels, notification routing, and method handlers

use super::message::{JsonRpcError, JsonRpcMessage};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio::sync::mpsc;
use tokio::sync::Mutex;

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

    #[cfg(test)]
    pub(crate) fn request_channels_for_test(
        &self,
    ) -> Arc<Mutex<HashMap<u64, Sender<JsonRpcMessage>>>> {
        self.request_channels.clone()
    }
}
