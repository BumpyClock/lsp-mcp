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
pub mod mcp;
pub mod service;
mod ast_grep;
mod utils;

#[cfg(test)]
mod test_utils;

/// Initialize a workspace manager with synchronous language server startup
///
/// This function blocks until all configured language servers have initialized.
/// Use `initialize_manager_with_workspace_root_async` for non-blocking initialization.
pub async fn initialize_manager_with_workspace_root(
    workspace_root: &Path,
) -> Result<Arc<Manager>, Box<dyn Error>> {
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

    let config = LspMcpConfig::load(workspace_root);
    if config.is_some() {
        info!("Loaded .lsp-mcp.json config from workspace root");
    }

    let mut manager = Manager::new(workspace_path).await?;
    manager.start_langservers(workspace_path, config.as_ref()).await?;
    Ok(Arc::new(manager))
}

/// Initialize a workspace manager with async language server startup
///
/// This function returns immediately after spawning background tasks to initialize
/// language servers. Language servers become available as they complete initialization.
/// Check the health endpoint to see which servers are ready.
pub async fn initialize_manager_with_workspace_root_async(
    workspace_root: &Path,
) -> Result<Arc<Manager>, Box<dyn Error>> {
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

    let config = LspMcpConfig::load(workspace_root);
    if config.is_some() {
        info!("Loaded .lsp-mcp.json config from workspace root");
    }

    let manager = Arc::new(Manager::new(workspace_path).await?);
    manager.start_langservers_async(workspace_path, config).await;
    Ok(manager)
}
