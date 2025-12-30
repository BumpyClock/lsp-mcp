// ABOUTME: MCP tool handlers for diagnostics and health operations.
// ABOUTME: Provides get_diagnostics and health tool logic.

use crate::api_types::HealthResponse;
use crate::config::OutputMode;
use crate::mcp_response::{format_error, format_response};
use crate::service::LspService;
use mcpkit::prelude::*;

pub async fn get_diagnostics(
    service: &LspService,
    output_mode: OutputMode,
    file_path: Option<String>,
) -> ToolOutput {
    match service.get_diagnostics(file_path.as_deref()).await {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
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
    let resp = format_response(&response, output_mode);
    ToolOutput::text(resp)
}
