// ABOUTME: MCP tool handlers for symbol search operations.
// ABOUTME: Provides workspace_symbol and find_identifier tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_result_from_error, tool_result_success};
use crate::service::LspService;
use rmcp::model::CallToolResult;

pub async fn workspace_symbol(
    service: &LspService,
    output_mode: OutputMode,
    query: String,
    exact: Option<bool>,
    limit: Option<u32>,
    offset: Option<u32>,
    context_lines: Option<u32>,
) -> CallToolResult {
    let exact = resolve_exact(exact);
    let context_lines = resolve_context_lines(context_lines);
    match service
        .workspace_symbol(
            &query,
            output_mode == OutputMode::Verbose,
            exact,
            limit,
            offset,
            context_lines,
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

pub async fn find_identifier(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    name: String,
    line: Option<u32>,
    character: Option<u32>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> CallToolResult {
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
            let resp = format_response(&response, output_mode);
            tool_result_success(resp)
        }
        Err(e) => tool_result_from_error(e),
    }
}

fn resolve_exact(exact: Option<bool>) -> bool {
    exact.unwrap_or(false)
}

fn resolve_context_lines(context_lines: Option<u32>) -> u32 {
    context_lines.unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn it_defaults_exact_to_false_when_omitted() {
        let resolved = resolve_exact(None);
        assert!(
            !resolved,
            "negative: exact must default to false when omitted"
        );
    }

    #[test]
    fn it_preserves_explicit_exact_value() {
        let mut rng = rand::rng();
        let explicit = rng.random_range(0..2) == 1;
        let resolved = resolve_exact(Some(explicit));
        assert_eq!(
            resolved,
            explicit,
            "negative: explicit exact value must be preserved"
        );
    }

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
