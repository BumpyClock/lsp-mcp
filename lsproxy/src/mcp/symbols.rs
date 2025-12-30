// ABOUTME: MCP tool handlers for symbol search operations.
// ABOUTME: Provides workspace_symbol and find_identifier tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_error, success_response};
use crate::service::{LspService, ServiceError};
use mcpkit::prelude::*;
use std::collections::HashMap;

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
            let data = match serde_json::to_value(&response) {
                Ok(v) => v,
                Err(e) => {
                    let err = ServiceError::Serialization(e.to_string());
                    return ToolOutput::error(format_error(&err));
                }
            };
            let mut counts = HashMap::new();
            counts.insert("symbols".to_string(), response.symbols.len());
            let resp = success_response("workspace_symbol", data, output_mode, Some(counts));
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
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
            let data = match serde_json::to_value(&response) {
                Ok(v) => v,
                Err(e) => {
                    let err = ServiceError::Serialization(e.to_string());
                    return ToolOutput::error(format_error(&err));
                }
            };
            let mut counts = HashMap::new();
            counts.insert("identifiers".to_string(), response.identifiers.len());
            let resp = success_response("find_identifier", data, output_mode, Some(counts));
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}
