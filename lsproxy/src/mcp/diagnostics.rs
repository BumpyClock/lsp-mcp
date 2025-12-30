// ABOUTME: MCP tool handlers for diagnostics and health operations.
// ABOUTME: Provides get_diagnostics and health tool logic.

use crate::api_types::HealthResponse;
use crate::config::OutputMode;
use crate::mcp_response::{format_error, success_response};
use crate::service::{LspService, ServiceError};
use mcpkit::prelude::*;
use std::collections::HashMap;

pub async fn get_diagnostics(
    service: &LspService,
    output_mode: OutputMode,
    file_path: Option<String>,
) -> ToolOutput {
    match service.get_diagnostics(file_path.as_deref()).await {
        Ok(response) => {
            let data = match serde_json::to_value(&response) {
                Ok(v) => v,
                Err(e) => {
                    let err = ServiceError::Serialization(e.to_string());
                    return ToolOutput::error(format_error(&err));
                }
            };
            let mut counts = HashMap::new();
            counts.insert("diagnostics".to_string(), response.total_count);
            counts.insert("files".to_string(), response.files.len());
            let resp = success_response("get_diagnostics", data, output_mode, Some(counts));
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}

pub async fn health(service: &LspService, output_mode: OutputMode) -> ToolOutput {
    let response = HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        languages: service.health().await,
    };
    match serde_json::to_value(&response) {
        Ok(data) => {
            let resp = success_response("health", data, output_mode, None);
            ToolOutput::text(resp)
        }
        Err(e) => {
            let err = ServiceError::Serialization(e.to_string());
            ToolOutput::error(format_error(&err))
        }
    }
}
