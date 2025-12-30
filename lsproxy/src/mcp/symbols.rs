// ABOUTME: MCP tool handlers for symbol search operations.
// ABOUTME: Provides workspace_symbol and find_identifier tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_output_from_error};
use crate::service::LspService;
use mcpkit::prelude::*;

pub async fn workspace_symbol(
    service: &LspService,
    output_mode: OutputMode,
    query: String,
    exact: Option<bool>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    match service
        .workspace_symbol(
            &query,
            output_mode == OutputMode::Verbose,
            exact.unwrap_or(false),
            limit,
            offset,
        )
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => tool_output_from_error(e),
    }
}

pub async fn find_identifier(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    name: String,
    line: Option<u32>,
    character: Option<u32>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    let position = match (line, character) {
        (Some(l), Some(c)) => Some(Position {
            line: l,
            character: c,
        }),
        _ => None,
    };
    match service
        .find_identifier(&path, &name, position, limit, offset)
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => tool_output_from_error(e),
    }
}
