// ABOUTME: Tool filtering wrapper for MCP server configuration.
// ABOUTME: Allows dynamic enabling/disabling of tools based on configuration.

use crate::mcp::LspMcpServer;
use crate::mcp_response::tool_disabled_message;
use crate::session::new_request_id;
use rmcp::{
    ServerHandler,
    model::{
        CallToolResult, Content, ListToolsResult, ServerInfo, Tool,
        CallToolRequestParam, PaginatedRequestParam,
    },
    service::{RequestContext, RoleServer},
    ErrorData as McpError,
};
use std::collections::HashSet;

/// Wrapper that filters tools based on configuration.
///
/// This allows dynamically enabling/disabling tools at runtime based on
/// the configuration file without modifying the underlying tool implementations.
pub struct FilteredLspMcpServer {
    inner: LspMcpServer,
    enabled_tools: HashSet<String>,
}

impl FilteredLspMcpServer {
    /// Create a new filtered handler wrapping the inner handler.
    pub fn new(inner: LspMcpServer, enabled_tools: HashSet<String>) -> Self {
        Self {
            inner,
            enabled_tools,
        }
    }

    fn filter_tools(&self, tools: Vec<Tool>) -> Vec<Tool> {
        let enabled = &self.enabled_tools;
        tools
            .into_iter()
            .filter(|tool| enabled.contains(&tool.name.to_string()))
            .collect()
    }

    fn disabled_tool_result(&self, tool_name: &str) -> CallToolResult {
        let result = CallToolResult::error(vec![Content::text(tool_disabled_message(tool_name))]);
        let request_id = new_request_id();
        self.inner.wrap_output(request_id, result)
    }
}

impl ServerHandler for FilteredLspMcpServer {
    fn get_info(&self) -> ServerInfo {
        self.inner.get_info()
    }

    fn list_tools(
        &self,
        request: Option<PaginatedRequestParam>,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            let result = self.inner.list_tools(request, context).await?;
            let filtered_tools = self.filter_tools(result.tools);
            Ok(ListToolsResult {
                tools: filtered_tools,
                next_cursor: result.next_cursor,
                meta: result.meta,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let enabled = &self.enabled_tools;
        let tool_name = request.name.to_string();
        async move {
            if !enabled.contains(&tool_name) {
                return Ok(self.disabled_tool_result(&tool_name));
            }
            self.inner.call_tool(request, context).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DebugConfig, LspMcpConfig};
    use crate::lsp::manager::Manager;
    use rmcp::model::RawContent;
    use std::borrow::Cow;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn create_test_server(
        debug_enabled: bool,
        enabled_tools: HashSet<String>,
    ) -> (FilteredLspMcpServer, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workspace_root = temp_dir.path();

        let test_file = workspace_root.join("test.rs");
        std::fs::write(&test_file, "fn main() {}").expect("Failed to write test file");

        let manager = Manager::new(workspace_root.to_str().unwrap())
            .await
            .expect("Failed to create manager");

        let config = if debug_enabled {
            LspMcpConfig {
                debug: Some(DebugConfig {
                    enabled: true,
                    ..Default::default()
                }),
                ..Default::default()
            }
        } else {
            LspMcpConfig::default()
        };

        let server = LspMcpServer::new(Arc::new(manager), &config, workspace_root);
        let filtered_server = FilteredLspMcpServer::new(server, enabled_tools);
        (filtered_server, temp_dir)
    }

    fn extract_text_content(result: &CallToolResult) -> String {
        for content in &result.content {
            if let RawContent::Text(text_content) = &content.raw {
                return text_content.text.clone();
            }
        }
        String::new()
    }

    #[tokio::test]
    async fn test_filter_tools_excludes_disabled_tools() {
        let enabled_tools: HashSet<String> = ["hover".to_string()].into_iter().collect();
        let (server, _temp) = create_test_server(false, enabled_tools).await;

        let schema = Arc::new(serde_json::Map::new());
        let tools = vec![
            Tool {
                name: Cow::Borrowed("hover"),
                title: None,
                description: None,
                input_schema: schema.clone(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: Cow::Borrowed("health"),
                title: None,
                description: None,
                input_schema: schema,
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
        ];

        let filtered = server.filter_tools(tools);
        assert_eq!(filtered.len(), 1, "Disabled tools still present after filtering");
        assert_eq!(filtered[0].name.as_ref(), "hover", "Enabled hover tool was removed");
    }

    #[tokio::test]
    async fn test_disabled_tool_result_includes_request_id_when_debug_enabled() {
        let enabled_tools: HashSet<String> = HashSet::new();
        let (server, _temp) = create_test_server(true, enabled_tools).await;

        let result = server.disabled_tool_result("health");
        assert_eq!(result.is_error, Some(true), "Disabled tool did not return error result");
        let text = extract_text_content(&result);
        assert!(text.contains("<!-- request:"), "Request ID header missing in debug mode");
        assert!(
            text.contains(&tool_disabled_message("health")),
            "Disabled tool message missing"
        );
    }
}
