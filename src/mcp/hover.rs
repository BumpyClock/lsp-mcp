// ABOUTME: MCP tool handler for hover information retrieval.
// ABOUTME: Accepts JSON array of {path, line, character} requests.

use crate::api_types::{HoverBatchItem, HoverRequest, Position};
use crate::config::OutputMode;
use crate::markdown_formatter::HoverBatchResponse;
use crate::mcp_response::{format_error, format_response, tool_result_error, tool_result_success};
use crate::service::LspService;
use futures::stream::{self, StreamExt};
use rmcp::model::CallToolResult;

pub async fn hover(
    service: &LspService,
    output_mode: OutputMode,
    requests: String,
    include_definition: Option<bool>,
) -> CallToolResult {
    let include_def = include_definition.unwrap_or(false);
    let batch_requests: Vec<HoverRequest> = match serde_json::from_str(&requests) {
        Ok(r) => r,
        Err(e) => return tool_result_error(format!("Invalid requests JSON: {}", e)),
    };
    let results = stream::iter(batch_requests)
        .map(|req| async move {
            let pos = Position {
                line: req.line,
                character: req.character,
            };
            match service
                .hover(
                    &req.path,
                    pos,
                    output_mode == OutputMode::Verbose,
                    include_def,
                )
                .await
            {
                Ok(response) => HoverBatchItem::Success(response),
                Err(e) => HoverBatchItem::Error {
                    error: format_error(&e),
                },
            }
        })
        .buffered(8)
        .collect()
        .await;
    format_batch_result(results, output_mode)
}

fn format_batch_result(results: Vec<HoverBatchItem>, output_mode: OutputMode) -> CallToolResult {
    let all_failed = !results.is_empty()
        && results
            .iter()
            .all(|item| matches!(item, HoverBatchItem::Error { .. }));
    let batch_response = HoverBatchResponse { results };
    let resp = format_response(&batch_response, output_mode);
    if all_failed {
        tool_result_error(resp)
    } else {
        tool_result_success(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::manager::Manager;
    use crate::service::create_service;
    use rmcp::model::RawContent;
    use std::sync::Arc;

    #[tokio::test]
    async fn batch_errors_remain_in_request_order_across_multiple_windows() {
        let workspace = tempfile::tempdir().unwrap();
        let service = create_service(Arc::new(
            Manager::new(workspace.path().to_str().unwrap())
                .await
                .unwrap(),
        ));
        let requests: Vec<_> = (0..19)
            .map(|index| {
                serde_json::json!({
                    "path": format!("request_{index}.unsupported"),
                    "line": 1,
                    "character": 1,
                })
            })
            .collect();
        let result = hover(
            &service,
            OutputMode::Default,
            serde_json::to_string(&requests).unwrap(),
            None,
        )
        .await;
        assert_eq!(result.is_error, Some(true));
        let RawContent::Text(text) = &result.content[0].raw else {
            panic!("hover batch must return text");
        };
        let mut remaining = text.text.as_str();
        for index in 0..19 {
            let expected = format!("request_{index}.unsupported");
            let offset = remaining
                .find(&expected)
                .expect("missing or reordered error");
            remaining = &remaining[offset + expected.len()..];
        }
        assert_eq!(text.text.matches("Error: ").count(), 19);
    }

    #[tokio::test]
    async fn missing_rust_server_is_reported_as_mcp_error() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("source.rs"), "fn example() {}\n").unwrap();
        let _context =
            crate::test_utils::TestContext::setup(workspace.path().to_str().unwrap(), false)
                .await
                .unwrap();
        let manager = Arc::new(
            Manager::new(workspace.path().to_str().unwrap())
                .await
                .unwrap(),
        );
        let mut config = crate::config::LspMcpConfig::default();
        config.binaries.insert(
            "rust".into(),
            workspace
                .path()
                .join("uninstalled-rust-analyzer")
                .to_string_lossy()
                .into_owned(),
        );
        manager
            .start_langservers_async(workspace.path().to_str().unwrap(), Some(config))
            .await;
        let service = create_service(manager);
        let result = hover(
            &service,
            OutputMode::Default,
            r#"[{"path":"source.rs","line":1,"character":4}]"#.into(),
            None,
        )
        .await;
        assert_eq!(result.is_error, Some(true));
        let RawContent::Text(text) = &result.content[0].raw else {
            panic!("hover batch must return text");
        };
        assert!(text.text.contains("Rust"));
        assert!(text.text.contains("rust-analyzer"));
        assert!(text.text.contains("startup logs"));
    }

    #[test]
    fn mixed_batches_keep_success_and_errors_in_order() {
        use crate::api_types::{HoverContents, HoverResponse};

        let result = format_batch_result(
            vec![
                HoverBatchItem::Error {
                    error: "first failure".into(),
                },
                HoverBatchItem::Success(HoverResponse {
                    raw_response: None,
                    contents: Some(HoverContents::Markup("successful hover".into())),
                    range: None,
                    definitions: vec![],
                    active_signature: None,
                    active_parameter: None,
                    nearby_symbols: vec![],
                }),
                HoverBatchItem::Error {
                    error: "last failure".into(),
                },
            ],
            OutputMode::Default,
        );
        assert_ne!(result.is_error, Some(true));
        let RawContent::Text(text) = &result.content[0].raw else {
            panic!("hover batch must return text");
        };
        let first = text.text.find("first failure").unwrap();
        let success = text.text.find("successful hover").unwrap();
        let last = text.text.find("last failure").unwrap();
        assert!(first < success && success < last);
    }

    #[test]
    fn empty_batch_is_not_an_error() {
        let result = format_batch_result(vec![], OutputMode::Default);
        assert_ne!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn malformed_batch_json_returns_one_tool_error() {
        let workspace = tempfile::tempdir().unwrap();
        let service = create_service(Arc::new(
            Manager::new(workspace.path().to_str().unwrap())
                .await
                .unwrap(),
        ));
        let result = hover(&service, OutputMode::Default, "[".into(), None).await;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.content.len(), 1);
        let RawContent::Text(text) = &result.content[0].raw else {
            panic!("invalid JSON must return text");
        };
        assert!(text.text.starts_with("Invalid requests JSON: "));
    }
}
