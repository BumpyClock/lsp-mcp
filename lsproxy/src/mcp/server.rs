// ABOUTME: MCP server initialization and runtime entry point.
// ABOUTME: Creates and runs the LSP MCP server over stdio transport.

use crate::config::LspMcpConfig;
use crate::lsp::manager::Manager;
use crate::mcp::filter::FilteredToolHandler;
use crate::mcp::LspMcpServer;
use log::info;
use mcpkit::prelude::*;
use mcpkit::transport::stdio::StdioTransport;
use std::sync::Arc;

/// Create and run the LSP MCP server over stdio
pub async fn run_server(manager: Arc<Manager>, config: &LspMcpConfig) -> Result<(), McpError> {
    let server_instance = Arc::new(LspMcpServer::new(manager, config));
    let enabled_tools = config.enabled_tools();

    info!(
        "Starting MCP server with {} tools enabled (preset: {:?})",
        enabled_tools.len(),
        config.tools.preset
    );

    let filtered_handler = FilteredToolHandler::new(Arc::clone(&server_instance), enabled_tools);

    let transport = StdioTransport::new();
    let server = ServerBuilder::new(Arc::clone(&server_instance))
        .with_tools(filtered_handler)
        .build();
    server.serve(transport).await
}
