// ABOUTME: MCP tool handler for hover information retrieval.
// ABOUTME: Accepts JSON array of {path, line, character} requests.

use crate::api_types::{HoverBatchItem, HoverRequest, Position};
use crate::config::OutputMode;
use crate::markdown_formatter::HoverBatchResponse;
use crate::mcp_response::{format_error, format_response, tool_result_error, tool_result_success};
use crate::service::LspService;
use rmcp::model::CallToolResult;

pub async fn hover(
    service: &LspService,
    output_mode: OutputMode,
    requests: String,
    include_definition: Option<bool>,
) -> CallToolResult {
    let include_def = include_definition.unwrap_or(false);
    let batch_requests: Vec<HoverRequest> = match serde_json::from_str(&requests) {
        Ok(r) => r,
        Err(e) => return tool_result_error(format!("Invalid requests JSON: {}", e)),
    };
    let mut results: Vec<HoverBatchItem> = Vec::with_capacity(batch_requests.len());
    for req in batch_requests {
        let pos = Position {
            line: req.line,
            character: req.character,
        };
        match service
            .hover(
                &req.path,
                pos,
                output_mode == OutputMode::Verbose,
                include_def,
            )
            .await
        {
            Ok(response) => results.push(HoverBatchItem::Success(response)),
            Err(e) => results.push(HoverBatchItem::Error {
                error: format_error(&e),
            }),
        }
    }
    let batch_response = HoverBatchResponse { results };
    let resp = format_response(&batch_response, output_mode);
    tool_result_success(resp)
}
