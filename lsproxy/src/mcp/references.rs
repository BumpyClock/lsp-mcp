// ABOUTME: MCP tool handlers for reference-related operations.
// ABOUTME: Provides find_references and find_referenced_symbols tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_output_from_error};
use crate::service::LspService;
use mcpkit::prelude::*;

pub async fn find_references(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    context_lines: Option<u32>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    let pos = Position { line, character };
    match service
        .find_references(
            &path,
            pos,
            output_mode == OutputMode::Verbose,
            context_lines,
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

pub async fn find_referenced_symbols(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    full_scan: Option<bool>,
    externals: Option<bool>,
) -> ToolOutput {
    let pos = Position { line, character };
    match service
        .find_referenced_symbols(
            &path,
            pos,
            full_scan.unwrap_or(false),
            externals.unwrap_or(false),
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
