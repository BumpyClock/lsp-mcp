// ABOUTME: Binary entrypoint for the lsp-mcp stdio server.
// ABOUTME: Initializes session, logging, workspace manager, and runs the MCP server.

use clap::Parser;
use lsproxy::config::{LspMcpConfig, SemanticSearchConfig};
use lsproxy::initialize_manager_with_workspace_root_async;
use lsproxy::logging::init_logging;
use lsproxy::mcp::run_server;
use lsproxy::semantic_search::{SemanticSearchManager, SemanticSearchState};
use lsproxy::session::init_session;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Workspace root to serve; defaults to the current directory
    #[arg(long)]
    workspace_root: Option<PathBuf>,

    /// Build semantic search index and exit (does not start MCP server)
    #[arg(long)]
    index: bool,

    /// Force rebuild of index from scratch (requires --index)
    #[arg(long, requires = "index")]
    force: bool,
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

    // 4. Handle index-only mode (no logging, no server)
    if cli.index {
        return run_indexing(&workspace_root, &config, cli.force).await;
    }

    // 5. Initialize logging with config
    let log_file = init_logging(config.debug_config(), &workspace_root)
        .map_err(|e| -> Box<dyn Error> { e })?;

    // 6. Log startup info
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

    // 7. Initialize manager and run server
    let (manager, config) = initialize_manager_with_workspace_root_async(&workspace_root).await?;
    run_server(manager, &config, &workspace_root)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

async fn run_indexing(
    workspace_root: &Path,
    config: &LspMcpConfig,
    force: bool,
) -> Result<(), Box<dyn Error>> {
    let ss_config = config
        .semantic_search
        .clone()
        .map(|mut c| {
            c.enabled = true;
            c
        })
        .unwrap_or_else(SemanticSearchConfig::default_enabled);

    println!(
        "Building semantic search index for: {}",
        workspace_root.display()
    );
    let start_time = Instant::now();

    let mut manager = SemanticSearchManager::new(ss_config, workspace_root.to_path_buf());

    if force {
        println!("Forcing full rebuild...");
        manager.force_rebuild();
    }

    manager.start().await.map_err(|e| e.to_string())?;

    let mut last_progress = String::new();
    loop {
        let state = manager.state().await;
        match state {
            SemanticSearchState::Indexing {
                indexed_count,
                total_count,
            } => {
                let pct = if total_count > 0 {
                    (indexed_count as f64 / total_count as f64 * 100.0) as u32
                } else {
                    0
                };
                let progress = format!(
                    "Indexing: {}/{} files ({}%)",
                    indexed_count, total_count, pct
                );
                if progress != last_progress {
                    println!("{}", progress);
                    last_progress = progress;
                }
            }
            SemanticSearchState::Ready { total_chunks } => {
                let elapsed = start_time.elapsed();
                println!("\nIndexing complete!");
                println!("  Chunks indexed: {}", total_chunks);
                println!("  Time elapsed: {:.1}s", elapsed.as_secs_f64());
                return Ok(());
            }
            SemanticSearchState::Error { message } => {
                eprintln!("Indexing failed: {}", message);
                std::process::exit(1);
            }
            SemanticSearchState::Initializing => {
                if last_progress != "Initializing..." {
                    println!("Initializing...");
                    last_progress = "Initializing...".to_string();
                }
            }
            SemanticSearchState::Disabled => {
                eprintln!("Semantic search is disabled");
                std::process::exit(1);
            }
            SemanticSearchState::Updating { .. } => {}
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
