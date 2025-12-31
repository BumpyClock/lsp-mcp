// ABOUTME: MCP server initialization and runtime entry point.
// ABOUTME: Creates and runs the LSP MCP server over stdio transport.

use crate::config::LspMcpConfig;
use crate::lsp::manager::Manager;
use crate::mcp::filter::FilteredToolHandler;
use crate::mcp::LspMcpServer;
use log::info;
use mcpkit::prelude::*;
use mcpkit::transport::stdio::StdioTransport;
use std::path::Path;
use std::sync::Arc;

/// Wrapper that provides custom instructions based on debug mode.
///
/// The `#[mcp_server]` macro generates a static ServerHandler impl, but we need
/// dynamic instructions based on whether debug mode is enabled at startup.
/// This wrapper delegates server_info to the inner handler but provides custom instructions.
struct InstructionsWrapper {
    inner: Arc<LspMcpServer>,
}

impl ServerHandler for InstructionsWrapper {
    fn server_info(&self) -> ServerInfo {
        // Delegate to the macro-generated implementation
        ServerHandler::server_info(self.inner.as_ref())
    }

    fn instructions(&self) -> Option<String> {
        let instructions = self.inner.get_instructions();
        tracing::info!(
            instructions_length = instructions.len(),
            debug_enabled = self.inner.debug_enabled(),
            "Providing server instructions"
        );
        Some(instructions)
    }

    fn capabilities(&self) -> ServerCapabilities {
        ServerHandler::capabilities(self.inner.as_ref())
    }
}

/// Create and run the LSP MCP server over stdio
pub async fn run_server(
    manager: Arc<Manager>,
    config: &LspMcpConfig,
    workspace_root: &Path,
) -> Result<(), McpError> {
    let server_instance = Arc::new(LspMcpServer::new(manager, config, workspace_root));
    let enabled_tools = config.enabled_tools();

    info!(
        "Starting MCP server with {} tools enabled (preset: {:?})",
        enabled_tools.len(),
        config.tools.preset
    );

    let filtered_handler = FilteredToolHandler::new(Arc::clone(&server_instance), enabled_tools);

    // Use wrapper to provide custom instructions
    let handler = Arc::new(InstructionsWrapper {
        inner: Arc::clone(&server_instance),
    });

    let transport = StdioTransport::new();
    let server = ServerBuilder::new(handler)
        .with_tools(filtered_handler)
        .build();
    server.serve(transport).await
}
