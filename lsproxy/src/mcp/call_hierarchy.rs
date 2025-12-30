// ABOUTME: MCP tool handlers for call hierarchy and implementation operations.
// ABOUTME: Provides call_hierarchy and go_to_implementation tool logic.

use crate::api_types::{CallHierarchyDirection, Position};
use crate::config::OutputMode;
use crate::mcp_response::{format_error, format_response};
use crate::service::LspService;
use mcpkit::prelude::*;

pub async fn call_hierarchy(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    direction: String,
) -> ToolOutput {
    let dir = match direction.to_lowercase().as_str() {
        "incoming" => CallHierarchyDirection::Incoming,
        "outgoing" => CallHierarchyDirection::Outgoing,
        _ => {
            return ToolOutput::error(format!(
                "Invalid direction '{}': must be 'incoming' or 'outgoing'",
                direction
            ));
        }
    };
    let pos = Position { line, character };
    match service.call_hierarchy(&path, pos, dir).await {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}

pub async fn go_to_implementation(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
) -> ToolOutput {
    let pos = Position { line, character };
    match service
        .find_implementation(&path, pos, output_mode == OutputMode::Verbose)
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}
