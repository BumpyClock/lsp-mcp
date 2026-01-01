// ABOUTME: Tool filtering wrapper for MCP server configuration.
// ABOUTME: Allows dynamic enabling/disabling of tools based on configuration.

use crate::mcp::LspMcpServer;
use crate::mcp_response::tool_disabled_message;
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
        let enabled = &self.enabled_tools;
        async move {
            let result = self.inner.list_tools(request, context).await?;
            let filtered_tools: Vec<Tool> = result
                .tools
                .into_iter()
                .filter(|tool| enabled.contains(&tool.name.to_string()))
                .collect();
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
                return Ok(CallToolResult::error(vec![Content::text(tool_disabled_message(&tool_name))]));
            }
            self.inner.call_tool(request, context).await
        }
    }
}
