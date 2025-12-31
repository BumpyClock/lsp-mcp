// ABOUTME: Binary entrypoint for the lsp-mcp stdio server.
// ABOUTME: Initializes session, logging, workspace manager, and runs the MCP server.

use clap::Parser;
use lsproxy::config::LspMcpConfig;
use lsproxy::initialize_manager_with_workspace_root_async;
use lsproxy::logging::init_logging;
use lsproxy::mcp::run_server;
use lsproxy::session::init_session;
use std::error::Error;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Workspace root to serve; defaults to the current directory
    #[arg(long)]
    workspace_root: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 1. Initialize session ID first (before any logging)
    let session_id = init_session();

    // 2. Parse CLI args and resolve workspace root
    let cli = Cli::parse();
    let workspace_root = match cli.workspace_root {
        Some(path) => path,
        None => std::env::current_dir()?,
    };

    // 3. Load config early (before logging init) to check debug settings
    let config = LspMcpConfig::load_merged(&workspace_root);

    // 4. Initialize logging with config
    let log_file = init_logging(config.debug_config(), &workspace_root)
        .map_err(|e| -> Box<dyn Error> { e })?;

    // 5. Log startup info
    if let Some(log_path) = &log_file {
        tracing::info!(
            session_id = %session_id,
            log_file = %log_path.display(),
            workspace = %workspace_root.display(),
            "LSP-MCP server starting with debug logging"
        );
    } else {
        tracing::info!(
            workspace = %workspace_root.display(),
            "LSP-MCP server starting"
        );
    }

    // 6. Initialize manager and run server
    let (manager, config) = initialize_manager_with_workspace_root_async(&workspace_root).await?;
    run_server(manager, &config, &workspace_root).await.map_err(|e| e.to_string())?;

    Ok(())
}
