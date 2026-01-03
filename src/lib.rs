// ABOUTME: Core library for MCP-based code navigation over LSP-managed workspaces.
// ABOUTME: Exposes manager initialization and domain services for the lsp-mcp binary.
use crate::api_types::set_global_mount_dir;
use crate::config::LspMcpConfig;
use crate::lsp::manager::Manager;
use log::info;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

pub mod api_types;
pub mod config;
pub mod lsp;
pub mod logging;
pub mod markdown_formatter;
pub mod mcp;
pub mod mcp_response;
pub mod semantic_search;
pub mod session;
pub mod service;
pub mod stats;
pub mod tool_registry;
mod ast_grep;
mod utils;
pub mod shared;

#[cfg(test)]
mod test_utils;

/// Initialize a workspace manager with synchronous language server startup
///
/// This function blocks until all configured language servers have initialized.
/// Use `initialize_manager_with_workspace_root_async` for non-blocking initialization.
///
/// Returns both the manager and the merged configuration (for tool filtering).
pub async fn initialize_manager_with_workspace_root(
    workspace_root: &Path,
) -> Result<(Arc<Manager>, LspMcpConfig), Box<dyn Error>> {
    set_global_mount_dir(workspace_root);
    let workspace_path = workspace_root.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "Workspace root is not valid unicode: {}",
                workspace_root.display()
            ),
        )
    })?;

    let config = LspMcpConfig::load_merged(workspace_root);
    info!(
        "Config loaded: {} languages, {} tools enabled",
        config.languages.len(),
        config.enabled_tools().len()
    );

    let mut manager = Manager::new(workspace_path).await?;
    manager.start_langservers(workspace_path, Some(&config)).await?;
    Ok((Arc::new(manager), config))
}

/// Initialize a workspace manager with async language server startup
///
/// This function returns immediately after spawning background tasks to initialize
/// language servers. Language servers become available as they complete initialization.
/// Use the health MCP tool to see which servers are ready.
///
/// Returns both the manager and the merged configuration (for tool filtering).
pub async fn initialize_manager_with_workspace_root_async(
    workspace_root: &Path,
) -> Result<(Arc<Manager>, LspMcpConfig), Box<dyn Error>> {
    set_global_mount_dir(workspace_root);
    let workspace_path = workspace_root.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "Workspace root is not valid unicode: {}",
                workspace_root.display()
            ),
        )
    })?;

    let config = LspMcpConfig::load_merged(workspace_root);
    info!(
        "Config loaded: {} languages, {} tools enabled",
        config.languages.len(),
        config.enabled_tools().len()
    );

    let manager = Arc::new(Manager::new(workspace_path).await?);
    manager.start_langservers_async(workspace_path, Some(config.clone())).await;
    Ok((manager, config))
}
