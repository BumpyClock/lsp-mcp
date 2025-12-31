// ABOUTME: Tool filtering wrapper for MCP server configuration.
// ABOUTME: Allows dynamic enabling/disabling of tools based on configuration.

use crate::mcp_response::tool_disabled_message;
use mcpkit::prelude::*;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;

/// Wrapper that filters tools based on configuration.
///
/// This allows dynamically enabling/disabling tools at runtime based on
/// the configuration file without modifying the underlying tool implementations.
pub struct FilteredToolHandler<T> {
    inner: Arc<T>,
    enabled_tools: HashSet<String>,
}

impl<T> FilteredToolHandler<T> {
    /// Create a new filtered handler wrapping the inner handler.
    pub fn new(inner: Arc<T>, enabled_tools: HashSet<String>) -> Self {
        Self {
            inner,
            enabled_tools,
        }
    }
}

impl<T: ToolHandler + Send + Sync> ToolHandler for FilteredToolHandler<T> {
    fn list_tools(
        &self,
        ctx: &Context<'_>,
    ) -> impl std::future::Future<Output = Result<Vec<Tool>, McpError>> + Send {
        let inner = Arc::clone(&self.inner);
        let enabled = self.enabled_tools.clone();
        async move {
            let all_tools = inner.list_tools(ctx).await?;
            Ok(all_tools
                .into_iter()
                .filter(|tool| enabled.contains(&tool.name))
                .collect())
        }
    }

    fn call_tool(
        &self,
        name: &str,
        args: Value,
        ctx: &Context<'_>,
    ) -> impl std::future::Future<Output = Result<ToolOutput, McpError>> + Send {
        let inner = Arc::clone(&self.inner);
        let enabled = self.enabled_tools.clone();
        let name = name.to_string();
        async move {
            if !enabled.contains(&name) {
                return Ok(ToolOutput::error(tool_disabled_message(&name)));
            }
            inner.call_tool(&name, args, ctx).await
        }
    }
}
