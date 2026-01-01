// ABOUTME: MCP tool handler for hover information retrieval.
// ABOUTME: Supports single-position and batch hover requests.

use crate::api_types::{HoverBatchItem, HoverRequest, Position};
use crate::config::OutputMode;
use crate::markdown_formatter::HoverBatchResponse;
use crate::mcp_response::{format_error, format_response, tool_result_error, tool_result_from_error, tool_result_success};
use crate::service::LspService;
use rmcp::model::CallToolResult;

pub async fn hover(
    service: &LspService,
    output_mode: OutputMode,
    path: Option<String>,
    line: Option<u32>,
    character: Option<u32>,
    include_definition: Option<bool>,
    requests: Option<String>,
) -> CallToolResult {
    let include_def = include_definition.unwrap_or(false);
    if let Some(requests_json) = requests {
        let batch_requests: Vec<HoverRequest> = match serde_json::from_str(&requests_json) {
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
                .hover(&req.path, pos, output_mode == OutputMode::Verbose, include_def)
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
        return tool_result_success(resp);
    }
    let (path, line, character) = match (path, line, character) {
        (Some(p), Some(l), Some(c)) => (p, l, c),
        _ => return tool_result_error("Single mode requires path, line, and character".to_string()),
    };
    let pos = Position { line, character };
    match service
        .hover(&path, pos, output_mode == OutputMode::Verbose, include_def)
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            tool_result_success(resp)
        }
        Err(e) => tool_result_from_error(e),
    }
}
