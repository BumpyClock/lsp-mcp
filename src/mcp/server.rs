// ABOUTME: MCP server initialization and runtime entry point.
// ABOUTME: Creates and runs the LSP MCP server over stdio transport.

use crate::config::LspMcpConfig;
use crate::lsp::manager::Manager;
use crate::mcp::filter::FilteredLspMcpServer;
use crate::mcp::LspMcpServer;
use crate::semantic_search::SemanticSearchManager;
use crate::stats::StatsStore;
use log::info;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Create and run the LSP MCP server over stdio
pub async fn run_server(
    manager: Arc<Manager>,
    config: &LspMcpConfig,
    workspace_root: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let enabled_tools = config.enabled_tools();

    // Initialize semantic search manager if enabled
    let semantic_search_manager = match &config.semantic_search {
        Some(ss_config) if ss_config.enabled => {
            info!("Initializing semantic search manager...");
            let manager =
                SemanticSearchManager::new(ss_config.clone(), workspace_root.to_path_buf());
            let manager_arc = Arc::new(RwLock::new(manager));
            // Start indexing in the background
            let manager_clone = Arc::clone(&manager_arc);
            tokio::spawn(async move {
                let mut manager = manager_clone.write().await;
                if let Err(e) = manager.start().await {
                    tracing::error!(error = %e, "Failed to start semantic search indexing");
                }
            });
            Some(manager_arc)
        }
        _ => None,
    };

    // Create server instance
    let mut server = LspMcpServer::new(manager, config, workspace_root);
    if let Some(ss_manager) = semantic_search_manager {
        server = server.with_semantic_search(ss_manager);
    }

    info!(
        "Starting MCP server with {} tools enabled (preset: {:?})",
        enabled_tools.len(),
        config.tools.preset
    );

    // Initialize stats store
    let stats_store = Arc::new(StatsStore::new(workspace_root).await);

    // Wrap with filtered handler to apply tool enable/disable filtering
    let filtered_server = FilteredLspMcpServer::new(server, enabled_tools, stats_store);

    // Create stdio transport and serve
    let transport = stdio();
    let server = filtered_server.serve(transport).await?;

    // Wait for the server to complete
    server.waiting().await?;

    Ok(())
}
