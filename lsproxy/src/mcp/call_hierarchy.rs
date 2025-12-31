// ABOUTME: MCP tool handlers for call hierarchy and implementation operations.
// ABOUTME: Provides call_hierarchy and go_to_implementation tool logic.

use crate::api_types::{CallHierarchyDirection, Position};
use crate::config::OutputMode;
use crate::mcp_response::{format_response, tool_output_from_error};
use crate::service::LspService;
use mcpkit::prelude::*;

pub async fn call_hierarchy(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
    direction: String,
    externals: Option<bool>,
) -> ToolOutput {
    let dir = match direction.to_lowercase().as_str() {
        "incoming" => CallHierarchyDirection::Incoming,
        "outgoing" => CallHierarchyDirection::Outgoing,
        _ => {
            return ToolOutput::error(format!(
                "Invalid direction '{}': must be 'incoming' or 'outgoing'",
                direction
            ));
        }
    };
    let pos = Position { line, character };
    let internal_only = resolve_internal_only(externals);
    match service.call_hierarchy(&path, pos, dir, internal_only).await {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => tool_output_from_error(e),
    }
}

fn resolve_internal_only(externals: Option<bool>) -> bool {
    !externals.unwrap_or(true)
}

pub async fn go_to_implementation(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    line: u32,
    character: u32,
) -> ToolOutput {
    let pos = Position { line, character };
    match service
        .find_implementation(&path, pos, output_mode == OutputMode::Verbose)
        .await
    {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => tool_output_from_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_internal_only_defaults_to_false() {
        assert!(!resolve_internal_only(None), "default must include externals");
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
}
