// ABOUTME: MCP tool handlers for reference-related operations.
// ABOUTME: Provides find_references and find_referenced_symbols tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_error, success_response};
use crate::service::{LspService, ServiceError};
use mcpkit::prelude::*;
use std::collections::HashMap;

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
            let data = match serde_json::to_value(&response) {
                Ok(v) => v,
                Err(e) => {
                    let err = ServiceError::Serialization(e.to_string());
                    return ToolOutput::error(format_error(&err));
                }
            };
            let mut counts = HashMap::new();
            let reference_count: usize = response.by_file.iter().map(|g| g.refs.len()).sum();
            counts.insert("references".to_string(), reference_count);
            let resp = success_response("find_references", data, output_mode, Some(counts));
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}

pub async fn find_referenced_symbols(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    full_scan: Option<bool>,
) -> ToolOutput {
    let pos = Position { line, character };
    match service
        .find_referenced_symbols(&path, pos, full_scan.unwrap_or(false))
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
            let mut counts = HashMap::new();
            counts.insert("workspace_symbols".to_string(), response.workspace_symbols.len());
            counts.insert("external_symbols".to_string(), response.external_symbols.len());
            counts.insert("not_found".to_string(), response.not_found.len());
            let resp = success_response("find_referenced_symbols", data, output_mode, Some(counts));
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}
