// ABOUTME: MCP tool handlers for definition-related operations.
// ABOUTME: Provides definitions_in_file and find_definition tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_error, success_response};
use crate::service::{filter_sibling_exports, LspService, ServiceError};
use mcpkit::prelude::*;
use std::collections::HashMap;

pub async fn definitions_in_file(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    match service.definitions_in_file(&path, limit, offset).await {
        Ok(response) => {
            let data = match serde_json::to_value(&response) {
                Ok(v) => v,
                Err(e) => {
                    let err = ServiceError::Serialization(e.to_string());
                    return ToolOutput::error(format_error(&err));
                }
            };
            let mut counts = HashMap::new();
            counts.insert("symbols".to_string(), response.symbols.len());
            let response = success_response("definitions_in_file", data, output_mode, Some(counts));
            ToolOutput::text(response)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}

pub async fn find_definition(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    include_source_code: Option<bool>,
    context_lines: Option<u32>,
    include_siblings: Option<bool>,
    siblings_limit: Option<u32>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    let pos = Position { line, character };
    match service
        .find_definition(
            &path,
            pos,
            include_source_code.unwrap_or(false),
            output_mode == OutputMode::Verbose,
            context_lines,
            limit,
            offset,
        )
        .await
    {
        Ok(mut response) => {
            if !include_siblings.unwrap_or(false) {
                if let Some(ref mut related) = response.related {
                    related.sibling_exports.clear();
                }
            } else if let Some(ref mut related) = response.related {
                let limit = siblings_limit.unwrap_or(5);
                related.sibling_exports = filter_sibling_exports(
                    std::mem::take(&mut related.sibling_exports),
                    limit,
                );
            }

            let data = match serde_json::to_value(&response) {
                Ok(v) => v,
                Err(e) => {
                    let err = ServiceError::Serialization(e.to_string());
                    return ToolOutput::error(format_error(&err));
                }
            };
            let mut counts = HashMap::new();
            counts.insert("definitions".to_string(), response.definitions.len());
            let resp = success_response("find_definition", data, output_mode, Some(counts));
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}
