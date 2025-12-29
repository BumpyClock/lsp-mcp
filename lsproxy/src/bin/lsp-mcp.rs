// ABOUTME: Binary entrypoint for the lsp-mcp stdio server.
// ABOUTME: Initializes a workspace manager and runs the MCP server over stdio.
use clap::Parser;
use lsproxy::initialize_manager_with_workspace_root_async;
use lsproxy::mcp::run_server;
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
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let workspace_root = match cli.workspace_root {
        Some(path) => path,
        None => std::env::current_dir()?,
    };

    let (manager, config) = initialize_manager_with_workspace_root_async(&workspace_root).await?;
    run_server(manager, &config).await.map_err(|e| e.to_string())?;
    Ok(())
}
