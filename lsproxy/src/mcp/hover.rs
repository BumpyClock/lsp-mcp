// ABOUTME: MCP tool handler for hover information retrieval.
// ABOUTME: Supports single-position and batch hover requests.

use crate::api_types::{HoverBatchItem, HoverRequest, Position};
use crate::config::OutputMode;
use crate::mcp_response::{format_error, success_response};
use crate::service::{LspService, ServiceError};
use mcpkit::prelude::*;

pub async fn hover(
    service: &LspService,
    output_mode: OutputMode,
    path: Option<String>,
    line: Option<u32>,
    character: Option<u32>,
    include_definition: Option<bool>,
    requests: Option<String>,
) -> ToolOutput {
    let include_def = include_definition.unwrap_or(false);
    if let Some(requests_json) = requests {
        let batch_requests: Vec<HoverRequest> = match serde_json::from_str(&requests_json) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(format!("Invalid requests JSON: {}", e)),
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
        let data = match serde_json::to_value(&results) {
            Ok(v) => v,
            Err(e) => return ToolOutput::error(format!("Serialization error: {}", e)),
        };
        let resp = success_response("hover", data, output_mode, None);
        return ToolOutput::text(resp);
    }
    let (path, line, character) = match (path, line, character) {
        (Some(p), Some(l), Some(c)) => (p, l, c),
        _ => return ToolOutput::error("Single mode requires path, line, and character"),
    };
    let pos = Position { line, character };
    match service
        .hover(&path, pos, output_mode == OutputMode::Verbose, include_def)
        .await
    {
        Ok(response) => {
            let data = match serde_json::to_value(&response) {
                Ok(v) => v,
                Err(e) => {
                    let err = ServiceError::Serialization(e.to_string());
                    return ToolOutput::error(format_error(&err));
                }
            };
            let resp = success_response("hover", data, output_mode, None);
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}
