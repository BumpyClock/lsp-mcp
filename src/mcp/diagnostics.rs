// ABOUTME: MCP tool handlers for diagnostics and health operations.
// ABOUTME: Provides get_diagnostics and health tool logic.

use crate::api_types::HealthResponse;
use crate::config::{DebugConfig, OutputMode};
use crate::logging::session_log_path;
use crate::mcp_response::{format_error, format_response};
use crate::service::LspService;
use crate::session::try_session_id;
use mcpkit::prelude::*;
use std::path::Path;

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

pub async fn health(
    service: &LspService,
    output_mode: OutputMode,
    debug_config: Option<&DebugConfig>,
    workspace_root: &Path,
) -> ToolOutput {
    let session_id = if debug_config.is_some() {
        try_session_id().map(|id| id.to_string())
    } else {
        None
    };

    let log_file = session_log_path(debug_config, workspace_root)
        .map(|p| p.to_string_lossy().to_string());

    let response = HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        languages: service.health().await,
        session_id,
        log_file,
    };
    let resp = format_response(&response, output_mode);
    ToolOutput::text(resp)
}
