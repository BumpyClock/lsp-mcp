// ABOUTME: MCP tool handlers for file-related operations.
// ABOUTME: Provides list_files and read_source_code tool logic.

use crate::api_types::{Position, Range};
use crate::config::OutputMode;
use crate::mcp_response::{format_error, success_response};
use crate::service::{LspService, ServiceError};
use mcpkit::prelude::*;
use serde_json::json;
use std::collections::HashMap;

pub async fn list_files(
    service: &LspService,
    output_mode: OutputMode,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    match service.list_files(limit, offset).await {
        Ok(response) => {
            let data = match serde_json::to_value(&response) {
                Ok(v) => v,
                Err(e) => {
                    let err = ServiceError::Serialization(e.to_string());
                    return ToolOutput::error(format_error(&err));
                }
            };
            let mut counts = HashMap::new();
            counts.insert("files".to_string(), response.files.len());
            let resp = success_response("list_files", data, output_mode, Some(counts));
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}

pub async fn read_source_code(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    start_line: Option<u32>,
    start_character: Option<u32>,
    end_line: Option<u32>,
    end_character: Option<u32>,
) -> ToolOutput {
    let range = match (start_line, start_character, end_line, end_character) {
        (Some(sl), Some(sc), Some(el), Some(ec)) => Some(Range {
            start: Position {
                line: sl,
                character: sc,
            },
            end: Position {
                line: el,
                character: ec,
            },
        }),
        _ => None,
    };
    match service.read_source_code(&path, range).await {
        Ok(source_code) => {
            let data = json!({"source": source_code});
            let mut counts = HashMap::new();
            counts.insert("chars".to_string(), source_code.len());
            let resp = success_response("read_source_code", data, output_mode, Some(counts));
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}
