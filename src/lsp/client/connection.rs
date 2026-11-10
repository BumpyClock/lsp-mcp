use super::LspClientConfig;
use crate::lsp::json_rpc::{JsonRpc, JsonRpcHandler};
use crate::lsp::process::{Process, ProcessHandler};
use crate::lsp::PendingRequests;
use futures::future::BoxFuture;
use log::debug;
use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, time::Duration};

pub type LspResponse<T> = BoxFuture<'static, Result<T, Box<dyn Error + Send + Sync>>>;

struct CancelOnDrop {
    id: Option<u64>,
    pending: PendingRequests,
    process: ProcessHandler,
    rpc: JsonRpcHandler,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        let Some(id) = self.id.take() else {
            return;
        };
        let pending = self.pending.clone();
        let process = self.process.clone();
        let rpc = self.rpc.clone();
        tokio::spawn(async move {
            if let Ok(Some(_)) = pending.remove_request(id).await {
                let notification =
                    rpc.create_notification("$/cancelRequest", serde_json::json!({ "id": id }));
                let message = format!(
                    "Content-Length: {}\r\n\r\n{}",
                    notification.len(),
                    notification
                );
                if let Err(error) = process
                    .send_with_lock_timeout(&message, Duration::from_secs(1))
                    .await
                {
                    debug!("Failed to cancel request {}: {}", id, error);
                }
            }
        });
    }
}

#[derive(Clone)]
pub struct Connection {
    pub(super) rpc: JsonRpcHandler,
    pub(super) pending: PendingRequests,
    pub(super) process: ProcessHandler,
    pub(super) config: LspClientConfig,
}

impl Connection {
    pub fn request<T: DeserializeOwned + Send + 'static>(
        &self,
        method: &str,
        params: impl Serialize,
        empty: T,
    ) -> Result<LspResponse<T>, Box<dyn Error + Send + Sync>> {
        let response = self.send_request(
            method,
            Some(serde_json::to_value(params)?),
            self.config.request_timeout,
        );
        Ok(Box::pin(async move {
            let result = response.await?;
            if result.is_null() {
                Ok(empty)
            } else {
                Ok(serde_json::from_value(result)?)
            }
        }))
    }

    pub fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
        timeout: Duration,
    ) -> LspResponse<serde_json::Value> {
        let rpc = self.rpc.clone();
        let pending = self.pending.clone();
        let mut process = self.process.clone();
        let method = method.to_owned();
        let deadline = tokio::time::Instant::now() + timeout;
        Box::pin(async move {
            tokio::time::timeout_at(deadline, async {
                let _permit = pending.acquire_slot().await;
                let (id, request) = rpc.create_request(&method, params);
                let mut receiver = pending.add_request(id).await?;
                let mut cancel = CancelOnDrop {
                    id: Some(id),
                    pending: pending.clone(),
                    process: process.clone(),
                    rpc,
                };
                let message = format!("Content-Length: {}\r\n\r\n{}", request.len(), request);
                debug!("Message: {:?}", message);
                process.send(&message).await?;
                let response = receiver.recv().await?;
                cancel.id = None;
                match (response.result, response.error) {
                    (Some(result), _) => Ok(result),
                    (_, Some(error)) => Err(Box::new(error) as Box<dyn Error + Send + Sync>),
                    _ => Ok(serde_json::Value::Null),
                }
            })
            .await
            .map_err(|_| format!("Request '{}' timed out after {:?}", method, timeout))?
        })
    }
}
