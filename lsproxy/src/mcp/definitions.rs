// ABOUTME: MCP tool handlers for definition-related operations.
// ABOUTME: Provides definitions_in_file and find_definition tool logic.

use crate::api_types::Position;
use crate::config::OutputMode;
use crate::mcp_response::{format_error, format_response};
use crate::service::{filter_sibling_exports, LspService, ServiceError};
use mcpkit::prelude::*;

fn tool_output_from_error(error: ServiceError) -> ToolOutput {
    let message = format_error(&error);
    match error {
        ServiceError::IdentifierSelection(_) => ToolOutput::text(message),
        _ => ToolOutput::error(message),
    }
}

pub async fn definitions_in_file(
    service: &LspService,
    output_mode: OutputMode,
    path: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ToolOutput {
    match service.definitions_in_file(&path, limit, offset).await {
        Ok(response) => {
            let markdown = format_response(&response, output_mode);
            ToolOutput::text(markdown)
        }
        Err(e) => tool_output_from_error(e),
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
) -> ToolOutput {
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
            ToolOutput::text(markdown)
        }
        Err(e) => tool_output_from_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{FileRange, Identifier, Position, Range};
    use crate::service::{PositionError, ServiceError};
    use mcpkit::types::Content;

    fn extract_text_content(tool_output: &ToolOutput) -> String {
        let content = match tool_output {
            ToolOutput::Success(result) => &result.content,
            ToolOutput::RecoverableError { message, .. } => return message.clone(),
        };

        for item in content {
            if let Content::Text(text_content) = item {
                return text_content.text.clone();
            }
        }

        String::new()
    }

    fn is_error_output(tool_output: &ToolOutput) -> bool {
        matches!(tool_output, ToolOutput::RecoverableError { .. })
    }

    #[test]
    fn test_identifier_selection_returns_message_output() {
        let identifier = Identifier {
            name: "Button".to_string(),
            file_range: FileRange {
                path: "src/test.rs".to_string(),
                range: Range {
                    start: Position { line: 1, character: 1 },
                    end: Position { line: 1, character: 6 },
                },
            },
            kind: None,
        };
        let error = ServiceError::IdentifierSelection(PositionError::IdentifierNotFound {
            closest: vec![identifier],
        });

        let output = tool_output_from_error(error);
        assert!(
            !is_error_output(&output),
            "negative: identifier selection should not be an error output"
        );

        let text = extract_text_content(&output);
        assert!(
            text.contains("Identifier selection failed because"),
            "negative: identifier message should be included"
        );
        assert!(
            text.contains("Nearby identifiers"),
            "negative: nearby identifier list should be included"
        );
    }
}
