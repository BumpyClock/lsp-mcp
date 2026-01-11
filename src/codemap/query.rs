// ABOUTME: Query engine for codemap graph queries.
// ABOUTME: Supports overview, impact, and context query modes.

use crate::codemap::cache::{GraphCache, TraversalDirection};
use crate::codemap::store::{CodemapStore, CodemapStoreError};
use crate::codemap::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("Store error: {0}")]
    StoreError(#[from] CodemapStoreError),
    #[error("Target not found: {0}")]
    TargetNotFound(String),
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}

/// Execute a codemap query
pub async fn execute_query(
    cache: &GraphCache,
    store: &Arc<CodemapStore>,
    query: CodemapQuery,
) -> Result<CodemapResponse, QueryError> {
    match query.mode {
        QueryMode::Overview => execute_overview(cache, store, &query).await,
        QueryMode::Impact => execute_impact(cache, store, &query).await,
        QueryMode::Context => execute_context(cache, store, &query).await,
    }
}

/// Execute overview query - high-level codebase structure
async fn execute_overview(
    cache: &GraphCache,
    store: &Arc<CodemapStore>,
    query: &CodemapQuery,
) -> Result<CodemapResponse, QueryError> {
    let (file_count, symbol_count, _edge_count) = store.get_stats().await?;

    // Get all files and group by module/directory
    let files = store.get_all_files().await?;
    let symbols = store.get_all_symbols().await?;

    // Group files by top-level directory as "modules"
    let mut module_map: HashMap<String, (u32, u32)> = HashMap::new();

    for file in &files {
        if !query.include_external && file.is_external {
            continue;
        }
        if let Some(ref scope) = query.scope {
            if !file.path.starts_with(scope) {
                continue;
            }
        }

        let module_path = extract_module_path(&file.path);
        let entry = module_map.entry(module_path).or_insert((0, 0));
        entry.0 += 1; // file count
    }

    // Count symbols per module
    for symbol in &symbols {
        if let Some(ref scope) = query.scope {
            if !symbol.location.path.starts_with(scope) {
                continue;
            }
        }

        let module_path = extract_module_path(&symbol.location.path);
        if let Some(entry) = module_map.get_mut(&module_path) {
            entry.1 += 1; // symbol count
        }
    }

    let modules: Vec<ModuleSummary> = module_map
        .into_iter()
        .map(|(path, (file_count, symbol_count))| ModuleSummary {
            path,
            file_count,
            symbol_count,
        })
        .take(query.limit as usize)
        .collect();

    // Get top symbols by reference count
    let symbol_summaries = get_top_symbols_by_refs(cache, &symbols, query.limit);

    let truncated = modules.len() >= query.limit as usize;

    Ok(CodemapResponse {
        mode: QueryMode::Overview,
        file_count,
        symbol_count,
        target: None,
        modules,
        symbols: symbol_summaries,
        nodes: Vec::new(),
        edges: Vec::new(),
        limit: query.limit,
        offset: query.offset,
        truncated,
    })
}

/// Execute impact query - what depends on a target
async fn execute_impact(
    cache: &GraphCache,
    store: &Arc<CodemapStore>,
    query: &CodemapQuery,
) -> Result<CodemapResponse, QueryError> {
    let target = query
        .target
        .as_ref()
        .ok_or_else(|| QueryError::InvalidQuery("Impact query requires target".to_string()))?;

    // Find target node
    let target_node = find_target_node(cache, store, target).await?;
    let target_id = target_node.id().clone();

    // Get incoming edges (dependents)
    let mut collected_nodes: Vec<Node> = vec![target_node];
    let mut collected_edges: Vec<Edge> = Vec::new();

    // BFS to find dependents up to depth
    let edge_kinds = match query.edge_type {
        Some(kind) => vec![kind],
        None => vec![EdgeKind::Defines, EdgeKind::Imports, EdgeKind::Calls],
    };

    let traversal = cache.traverse_bfs(
        &target_id,
        &edge_kinds,
        TraversalDirection::Incoming,
        query.depth as usize,
    );

    for (node_id, _depth) in traversal {
        if node_id != target_id {
            if let Some(node) = cache.get_node(&node_id) {
                collected_nodes.push(node);
            }
        }

        // Get edges to this node
        for (kind, _from_id, edge_id) in cache.get_incoming(&node_id) {
            if edge_kinds.contains(&kind) {
                if let Some(edge) = cache.get_edge(&edge_id) {
                    if !collected_edges.iter().any(|e| e.id() == &edge_id) {
                        collected_edges.push(edge);
                    }
                }
            }
        }
    }

    // Apply limit
    let truncated = collected_nodes.len() > query.limit as usize;
    let nodes: Vec<Node> = collected_nodes
        .into_iter()
        .take(query.limit as usize)
        .collect();
    let edges: Vec<Edge> = collected_edges
        .into_iter()
        .take(query.limit as usize * 2)
        .collect();

    let (file_count, symbol_count, _) = store.get_stats().await?;

    Ok(CodemapResponse {
        mode: QueryMode::Impact,
        file_count,
        symbol_count,
        target: Some(target.clone()),
        modules: Vec::new(),
        symbols: Vec::new(),
        nodes,
        edges,
        limit: query.limit,
        offset: query.offset,
        truncated,
    })
}

/// Execute context query - local subgraph around a target
async fn execute_context(
    cache: &GraphCache,
    store: &Arc<CodemapStore>,
    query: &CodemapQuery,
) -> Result<CodemapResponse, QueryError> {
    let target = query
        .target
        .as_ref()
        .ok_or_else(|| QueryError::InvalidQuery("Context query requires target".to_string()))?;

    // Find target node
    let target_node = find_target_node(cache, store, target).await?;
    let target_id = target_node.id().clone();

    let mut collected_nodes: Vec<Node> = vec![target_node];
    let mut collected_edges: Vec<Edge> = Vec::new();

    let edge_kinds = match query.edge_type {
        Some(kind) => vec![kind],
        None => vec![EdgeKind::Defines, EdgeKind::Imports, EdgeKind::Calls],
    };

    // BFS in both directions
    let traversal = cache.traverse_bfs(
        &target_id,
        &edge_kinds,
        TraversalDirection::Both,
        query.depth as usize,
    );

    for (node_id, _depth) in traversal {
        if node_id != target_id {
            if let Some(node) = cache.get_node(&node_id) {
                collected_nodes.push(node);
            }
        }

        // Get both incoming and outgoing edges
        for (kind, _, edge_id) in cache.get_incoming(&node_id) {
            if edge_kinds.contains(&kind) {
                if let Some(edge) = cache.get_edge(&edge_id) {
                    if !collected_edges.iter().any(|e| e.id() == &edge_id) {
                        collected_edges.push(edge);
                    }
                }
            }
        }
        for (kind, _, edge_id) in cache.get_outgoing(&node_id) {
            if edge_kinds.contains(&kind) {
                if let Some(edge) = cache.get_edge(&edge_id) {
                    if !collected_edges.iter().any(|e| e.id() == &edge_id) {
                        collected_edges.push(edge);
                    }
                }
            }
        }
    }

    // Apply limit
    let truncated = collected_nodes.len() > query.limit as usize;
    let nodes: Vec<Node> = collected_nodes
        .into_iter()
        .take(query.limit as usize)
        .collect();
    let edges: Vec<Edge> = collected_edges
        .into_iter()
        .take(query.limit as usize * 2)
        .collect();

    let (file_count, symbol_count, _) = store.get_stats().await?;

    Ok(CodemapResponse {
        mode: QueryMode::Context,
        file_count,
        symbol_count,
        target: Some(target.clone()),
        modules: Vec::new(),
        symbols: Vec::new(),
        nodes,
        edges,
        limit: query.limit,
        offset: query.offset,
        truncated,
    })
}

/// Find a node by target string (symbol name or file path)
async fn find_target_node(
    cache: &GraphCache,
    store: &Arc<CodemapStore>,
    target: &str,
) -> Result<Node, QueryError> {
    // Try as file path first
    if target.contains('/') || target.contains('.') {
        if let Some(file_id) = cache.get_file_by_path(target) {
            if let Some(node) = cache.get_node(&file_id) {
                return Ok(node);
            }
        }
    }

    // Try as symbol name
    let symbol_ids = cache.search_symbols(target, 1);
    if let Some(id) = symbol_ids.first() {
        if let Some(node) = cache.get_node(id) {
            return Ok(node);
        }
    }

    // Try store as fallback
    let symbols = store.search_symbols(target, 1).await?;
    if let Some(symbol) = symbols.first() {
        return Ok(Node::Symbol(symbol.clone()));
    }

    Err(QueryError::TargetNotFound(target.to_string()))
}

/// Extract module path from file path
fn extract_module_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 2 {
        format!("{}/{}/", parts[0], parts[1])
    } else if !parts.is_empty() {
        format!("{}/", parts[0])
    } else {
        "/".to_string()
    }
}

/// Get top symbols by incoming reference count
fn get_top_symbols_by_refs(
    cache: &GraphCache,
    symbols: &[SymbolNode],
    limit: u32,
) -> Vec<SymbolSummary> {
    let mut symbol_refs: Vec<(&SymbolNode, u32)> = symbols
        .iter()
        .map(|s| {
            let ref_count = cache
                .get_incoming(&s.id)
                .iter()
                .filter(|(k, _, _)| matches!(k, EdgeKind::Calls))
                .count() as u32;
            (s, ref_count)
        })
        .collect();

    symbol_refs.sort_by(|a, b| b.1.cmp(&a.1));

    symbol_refs
        .into_iter()
        .take(limit as usize)
        .map(|(s, ref_count)| SymbolSummary {
            name: s.name.clone(),
            kind: s.kind,
            location: s.location.clone(),
            reference_count: ref_count,
        })
        .collect()
}
