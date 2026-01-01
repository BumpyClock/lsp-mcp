// ABOUTME: MCP tool handlers for call hierarchy and implementation operations.
// ABOUTME: Provides call_hierarchy and go_to_implementation tool logic.

use crate::api_types::{CallHierarchyDirection, Position};
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_result_error, tool_result_from_error, tool_result_success};
use crate::service::LspService;
use rmcp::model::CallToolResult;

pub async fn call_hierarchy(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    direction: String,
    externals: Option<bool>,
    context_lines: Option<u32>,
) -> CallToolResult {
    let dir = match direction.to_lowercase().as_str() {
        "incoming" => CallHierarchyDirection::Incoming,
        "outgoing" => CallHierarchyDirection::Outgoing,
        _ => {
            return tool_result_error(format!(
                "Invalid direction '{}': must be 'incoming' or 'outgoing'",
                direction
            ));
        }
    };
    let pos = Position { line, character };
    let internal_only = resolve_internal_only(externals);
    let context_lines = resolve_context_lines(context_lines);
    match service
        .call_hierarchy(&path, pos, dir, internal_only, context_lines)
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            tool_result_success(resp)
        }
        Err(e) => tool_result_from_error(e),
    }
}

fn resolve_internal_only(externals: Option<bool>) -> bool {
    !externals.unwrap_or(false)
}

fn resolve_context_lines(context_lines: Option<u32>) -> u32 {
    context_lines.unwrap_or(1)
}

pub async fn go_to_implementation(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
) -> CallToolResult {
    let pos = Position { line, character };
    match service
        .find_implementation(&path, pos, output_mode == OutputMode::Verbose)
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            tool_result_success(resp)
        }
        Err(e) => tool_result_from_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn resolve_internal_only_defaults_to_true() {
        assert!(resolve_internal_only(None), "default must exclude externals");
    }

    #[test]
    fn resolve_internal_only_is_false_when_externals_true() {
        assert!(
            !resolve_internal_only(Some(true)),
            "externals=true must disable internal-only filtering"
        );
    }

    #[test]
    fn resolve_internal_only_is_true_when_externals_false() {
        assert!(
            resolve_internal_only(Some(false)),
            "externals=false must keep internal-only filtering"
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
