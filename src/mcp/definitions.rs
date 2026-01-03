// ABOUTME: MCP tool handlers for definition-related operations.
// ABOUTME: Provides definitions_in_file and find_definition tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_result_from_error, tool_result_success};
use crate::service::{filter_sibling_exports, LspService};
use rmcp::model::CallToolResult;

pub async fn definitions_in_file(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    include_locals: Option<bool>,
    include_children: Option<bool>,
    limit: Option<u32>,
    offset: Option<u32>,
    context_lines: Option<u32>,
) -> CallToolResult {
    let include_locals = include_locals.unwrap_or(false);
    let include_children = include_children.unwrap_or(false);
    let context_lines = resolve_context_lines(context_lines);
    match service
        .definitions_in_file(
            &path,
            include_locals,
            include_children,
            limit,
            offset,
            context_lines,
        )
        .await
    {
        Ok(response) => {
            let markdown = format_response(&response, output_mode);
            tool_result_success(markdown)
        }
        Err(e) => tool_result_from_error(e),
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
) -> CallToolResult {
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
            tool_result_success(markdown)
        }
        Err(e) => tool_result_from_error(e),
    }
}

pub async fn get_symbol_definition(
    service: &LspService,
    output_mode: OutputMode,
    symbol_name: String,
    file_path: String,
) -> CallToolResult {
    match service
        .get_symbol_definition(&symbol_name, &file_path)
        .await
    {
        Ok(response) => {
            let markdown = format_response(&response, output_mode);
            tool_result_success(markdown)
        }
        Err(e) => tool_result_from_error(e),
    }
}

fn resolve_context_lines(context_lines: Option<u32>) -> u32 {
    context_lines.unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn it_defaults_context_lines_to_one_when_omitted() {
        let resolved = resolve_context_lines(None);
        assert_eq!(
            resolved, 1,
            "negative: context_lines must default to 1 when omitted"
        );
    }

    #[test]
    fn it_preserves_explicit_context_lines_value() {
        let mut rng = rand::rng();
        let explicit: u32 = rng.random_range(0..10);
        let resolved = resolve_context_lines(Some(explicit));
        assert_eq!(
            resolved,
            explicit,
            "negative: explicit context_lines value must be preserved"
        );
    }
}
