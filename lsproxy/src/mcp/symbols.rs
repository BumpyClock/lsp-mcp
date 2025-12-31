// ABOUTME: MCP tool handlers for symbol search operations.
// ABOUTME: Provides workspace_symbol and find_identifier tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_output_from_error};
use crate::service::LspService;
use mcpkit::prelude::*;

pub async fn workspace_symbol(
    service: &LspService,
    output_mode: OutputMode,
    query: String,
    exact: Option<bool>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    let exact = resolve_exact(exact);
    match service
        .workspace_symbol(
            &query,
            output_mode == OutputMode::Verbose,
            exact,
            limit,
            offset,
        )
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => tool_output_from_error(e),
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
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => tool_output_from_error(e),
    }
}

fn resolve_exact(exact: Option<bool>) -> bool {
    exact.unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn it_defaults_exact_to_true_when_omitted() {
        let resolved = resolve_exact(None);
        assert!(
            resolved,
            "negative: exact must default to true when omitted"
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
}
