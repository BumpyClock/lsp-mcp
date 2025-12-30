// ABOUTME: MCP tool handlers for definition-related operations.
// ABOUTME: Provides definitions_in_file and find_definition tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_output_from_error};
use crate::service::{filter_sibling_exports, LspService};
use mcpkit::prelude::*;

pub async fn definitions_in_file(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    match service.definitions_in_file(&path, limit, offset).await {
        Ok(response) => {
            let markdown = format_response(&response, output_mode);
            ToolOutput::text(markdown)
        }
        Err(e) => tool_output_from_error(e),
    }
}

pub async fn find_definition(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    let pos = Position { line, character };
    match service
        .find_definition(
            &path,
            pos,
            true, // always include source code
            output_mode == OutputMode::Verbose,
            limit,
            offset,
        )
        .await
    {
        Ok(mut response) => {
            // Always include siblings (max 5)
            if let Some(ref mut related) = response.related {
                related.sibling_exports = filter_sibling_exports(
                    std::mem::take(&mut related.sibling_exports),
                    5,
                );
            }

            let markdown = format_response(&response, output_mode);
            ToolOutput::text(markdown)
        }
        Err(e) => tool_output_from_error(e),
    }
}
