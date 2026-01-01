// ABOUTME: MCP tool handlers for reference-related operations.
// ABOUTME: Provides find_references and find_referenced_symbols tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_result_from_error, tool_result_success};
use crate::service::LspService;
use rmcp::model::CallToolResult;

pub async fn find_references(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    context_lines: Option<u32>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> CallToolResult {
    let pos = Position { line, character };
    let context_lines = resolve_context_lines(context_lines);
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
            tool_result_success(resp)
        }
        Err(e) => tool_result_from_error(e),
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
) -> CallToolResult {
    let pos = Position { line, character };
    let include_externals = resolve_include_externals(externals);
    match service
        .find_referenced_symbols(
            &path,
            pos,
            full_scan.unwrap_or(false),
            include_externals,
        )
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            tool_result_success(resp)
        }
        Err(e) => tool_result_from_error(e),
    }
}

fn resolve_context_lines(context_lines: Option<u32>) -> Option<u32> {
    Some(context_lines.unwrap_or(1))
}

fn resolve_include_externals(externals: Option<bool>) -> bool {
    externals.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn it_defaults_context_lines_to_one_when_omitted() {
        let context_lines = None;
        let resolved = resolve_context_lines(context_lines);
        assert_eq!(
            resolved,
            Some(1),
            "negative: omitted context lines must default to one line"
        );
    }

    #[test]
    fn it_preserves_explicit_context_line_values() {
        let mut rng = rand::rng();
        let value: u32 = rng.random_range(2..50);
        let resolved = resolve_context_lines(Some(value));
        assert_eq!(
            resolved,
            Some(value),
            "negative: explicit context lines must be preserved"
        );
    }

    #[test]
    fn it_defaults_externals_to_false_when_omitted() {
        let resolved = resolve_include_externals(None);
        assert!(
            !resolved,
            "negative: externals must default to false when omitted"
        );
    }

    #[test]
    fn it_preserves_explicit_externals_value() {
        let mut rng = rand::rng();
        let explicit = rng.random_range(0..2) == 1;
        let resolved = resolve_include_externals(Some(explicit));
        assert_eq!(
            resolved,
            explicit,
            "negative: explicit externals value must be preserved"
        );
    }
}
