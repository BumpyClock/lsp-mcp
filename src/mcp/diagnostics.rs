// ABOUTME: MCP tool handlers for diagnostics and health operations.
// ABOUTME: Provides get_diagnostics and health tool logic.

use crate::api_types::{HealthResponse, SemanticSearchHealth};
use crate::config::{DebugConfig, OutputMode};
use crate::logging::session_log_path;
use crate::mcp_response::{format_error, format_response};
use crate::service::LspService;
use crate::semantic_search::{SemanticSearchHealthSnapshot, SemanticSearchManager, SemanticSearchState};
use crate::session::try_session_id;
use mcpkit::prelude::*;
use std::sync::Arc;
use std::path::Path;
use tokio::sync::RwLock;

pub async fn get_diagnostics(
    service: &LspService,
    output_mode: OutputMode,
    file_path: Option<String>,
) -> ToolOutput {
    match service.get_diagnostics(file_path.as_deref()).await {
        Ok(response) => {
            let resp = format_response(&response, output_mode);
            ToolOutput::text(resp)
        }
        Err(e) => ToolOutput::error(format_error(&e)),
    }
}

pub async fn health(
    service: &LspService,
    output_mode: OutputMode,
    debug_config: Option<&DebugConfig>,
    workspace_root: &Path,
    semantic_search_manager: Option<Arc<RwLock<SemanticSearchManager>>>,
) -> ToolOutput {
    let debug_enabled = debug_config.is_some();

    let session_id = if debug_enabled {
        try_session_id().map(|id| id.to_string())
    } else {
        None
    };

    let log_file = session_log_path(debug_config, workspace_root)
        .map(|p| p.to_string_lossy().to_string());

    let semantic_search = match semantic_search_manager {
        Some(manager) => {
            let snapshot = manager.read().await.health_snapshot().await;
            Some(to_semantic_search_health(snapshot))
        }
        None => None,
    };

    let response = HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        languages: service.health().await,
        debug_mode: if debug_enabled { Some(true) } else { None },
        session_id,
        log_file,
        semantic_search,
    };
    let resp = format_response(&response, output_mode);
    ToolOutput::text(resp)
}

fn to_semantic_search_health(snapshot: SemanticSearchHealthSnapshot) -> SemanticSearchHealth {
    let state = match snapshot.state {
        SemanticSearchState::Disabled => "disabled",
        SemanticSearchState::Initializing => "initializing",
        SemanticSearchState::Indexing { .. } => "indexing",
        SemanticSearchState::Ready { .. } => "ready",
        SemanticSearchState::Updating { .. } => "updating",
        SemanticSearchState::Error { .. } => "error",
    }
    .to_string();

    SemanticSearchHealth {
        enabled: snapshot.enabled,
        state: Some(state),
        embedder_provider: Some(snapshot.embedder_provider),
        embedder_model: snapshot.embedder_model,
        embedder_dimension: Some(snapshot.embedder_dimension),
        stored_dimension: snapshot.stored_dimension,
        dimension_mismatch: snapshot.dimension_mismatch,
    }
}
