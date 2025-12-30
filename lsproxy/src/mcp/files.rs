// ABOUTME: MCP tool handlers for file-related operations.
// ABOUTME: Provides list_files and read_source_code tool logic.

use crate::api_types::{Position, Range};
use crate::config::OutputMode;
use crate::markdown_formatter::SourceCodeResponse;
use crate::mcp_response::{format_error, format_response};
use crate::service::LspService;
use mcpkit::prelude::*;

pub async fn list_files(
    service: &LspService,
    output_mode: OutputMode,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    match service.list_files(limit, offset).await {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
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
    // Capture range info before moving into the service call
    let range_info = range.as_ref().map(|r| (r.start.line, r.end.line));
    match service.read_source_code(&path, range).await {
        Ok(source_code) => {
            let line_count = source_code.lines().count() as u32;
            let (actual_start, actual_end, total) = match range_info {
                Some((start, end)) => (start, end, line_count + start - 1),
                None => (1, line_count, line_count),
            };
            let response = SourceCodeResponse {
                path: path.clone(),
                content: source_code,
                start_line: actual_start,
                end_line: actual_end,
                total_lines: total,
            };
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}
