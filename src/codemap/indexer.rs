// ABOUTME: Codemap indexer for extracting symbols and edges from codebase.
// ABOUTME: Leverages LSP operations and ast-grep fallbacks for edge collection.

use crate::codemap::store::{CodemapStore, CodemapStoreError};
use crate::codemap::types::*;
use crate::lsp::manager::Manager;
use crate::api_types::{Symbol, FilePosition, Position, FileRange, Range};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub enum IndexerError {
    StoreError(CodemapStoreError),
    LspError(String),
    IoError(std::io::Error),
}

impl std::fmt::Display for IndexerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexerError::StoreError(e) => write!(f, "Store error: {}", e),
            IndexerError::LspError(e) => write!(f, "LSP error: {}", e),
            IndexerError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for IndexerError {}

impl From<CodemapStoreError> for IndexerError {
    fn from(e: CodemapStoreError) -> Self {
        IndexerError::StoreError(e)
    }
}

impl From<std::io::Error> for IndexerError {
    fn from(e: std::io::Error) -> Self {
        IndexerError::IoError(e)
    }
}

/// Indexer state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IndexerState {
    Idle,
    Indexing { progress: f32 },
    Error,
}

/// Codemap indexer
pub struct CodemapIndexer {
    store: Arc<CodemapStore>,
    manager: Arc<Manager>,
    state: Arc<RwLock<IndexerState>>,
    workspace_files: Arc<RwLock<HashSet<String>>>,
}


impl CodemapIndexer {
    pub fn new(store: Arc<CodemapStore>, manager: Arc<Manager>) -> Self {
        Self {
            store,
            manager,
            state: Arc::new(RwLock::new(IndexerState::Idle)),
            workspace_files: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Get current indexer state
    pub async fn state(&self) -> IndexerState {
        *self.state.read().await
    }

    /// Full workspace index
    pub async fn full_index(&self) -> Result<IndexStats, IndexerError> {
        info!("Starting full codemap index");
        *self.state.write().await = IndexerState::Indexing { progress: 0.0 };

        // Get all workspace files
        let files = self.manager.list_files().await
            .map_err(|e| IndexerError::LspError(e.to_string()))?;

        let total = files.len();
        if total == 0 {
            *self.state.write().await = IndexerState::Idle;
            return Ok(IndexStats::default());
        }

        // Update workspace files set
        {
            let mut ws = self.workspace_files.write().await;
            ws.clear();
            ws.extend(files.iter().cloned());
        }

        let mut indexed_files = 0u32;
        let mut indexed_symbols = 0u32;
        let mut indexed_edges = 0u32;

        for (i, file_path) in files.iter().enumerate() {
            // Skip external files
            if self.is_external_path(file_path) {
                continue;
            }

            match self.index_file(file_path).await {
                Ok(stats) => {
                    indexed_files += 1;
                    indexed_symbols += stats.symbols;
                    indexed_edges += stats.edges;
                }
                Err(e) => {
                    warn!("Failed to index file {}: {}", file_path, e);
                }
            }

            // Update progress
            let progress = (i + 1) as f32 / total as f32;
            *self.state.write().await = IndexerState::Indexing { progress };
        }

        *self.state.write().await = IndexerState::Idle;

        info!(
            "Codemap index complete: {} files, {} symbols, {} edges",
            indexed_files, indexed_symbols, indexed_edges
        );

        Ok(IndexStats {
            files: indexed_files,
            symbols: indexed_symbols,
            edges: indexed_edges,
        })
    }

    /// Index a single file
    pub async fn index_file(&self, path: &str) -> Result<FileIndexStats, IndexerError> {
        debug!("Indexing file: {}", path);

        let now = current_timestamp();
        let file_version = now as u64;

        // Create file node
        let file_id = NodeId::for_file(path);
        let language = detect_language(path);
        let content_hash = self.compute_file_hash(path).await?;
        let line_count = self.count_lines(path).await.unwrap_or(0);

        let file_node = FileNode {
            id: file_id.clone(),
            path: path.to_string(),
            language: language.clone(),
            content_hash,
            mtime: now,
            line_count,
            is_external: false,
        };

        self.store.upsert_file(&file_node).await?;

        // Extract symbols via LSP documentSymbol
        let symbols = self.extract_symbols(path, file_version).await?;
        let symbol_count = symbols.len() as u32;

        // Store symbols and defines edges
        for symbol in &symbols {
            self.store.upsert_symbol(symbol).await?;

            let defines_edge = DefinesEdge {
                id: EdgeId::new(&file_id, &symbol.id, EdgeKind::Defines),
                file_id: file_id.clone(),
                symbol_id: symbol.id.clone(),
                metadata: EdgeMetadata {
                    confidence: Confidence::High,
                    provenance: Provenance::Lsp,
                    validated_at: now,
                    source_file_version: file_version,
                    target_file_version: None,
                    is_cross_package: false,
                },
            };
            self.store.upsert_defines_edge(&defines_edge).await?;
        }

        // Extract calls edges for callable symbols
        let mut edge_count = symbol_count; // defines edges
        for symbol in symbols.iter().filter(|s| is_callable(&s.kind)) {
            match self.extract_calls(symbol, file_version).await {
                Ok(calls) => {
                    for call_edge in calls {
                        self.store.upsert_calls_edge(&call_edge).await?;
                        edge_count += 1;
                    }
                }
                Err(e) => {
                    debug!("Failed to extract calls for {}: {}", symbol.name, e);
                }
            }
        }

        // Extract imports
        match self.extract_imports(path, &file_id, file_version).await {
            Ok(imports) => {
                for import_edge in imports {
                    self.store.upsert_imports_edge(&import_edge).await?;
                    edge_count += 1;
                }
            }
            Err(e) => {
                debug!("Failed to extract imports for {}: {}", path, e);
            }
        }

        // Update file version
        self.store.set_file_version(path, &file_node.content_hash, file_version).await?;

        Ok(FileIndexStats {
            symbols: symbol_count,
            edges: edge_count,
        })
    }

    /// Remove all data for a file
    pub async fn remove_file(&self, path: &str) -> Result<(), IndexerError> {
        let file_id = NodeId::for_file(path);
        self.store.delete_edges_for_file(&file_id).await?;
        self.store.delete_symbols_in_file(path).await?;
        self.store.delete_file(path).await?;
        Ok(())
    }

    /// Extract symbols from a file using LSP documentSymbol
    async fn extract_symbols(&self, path: &str, file_version: u64) -> Result<Vec<SymbolNode>, IndexerError> {
        let now = current_timestamp();

        // Try LSP first
        let lsp_symbols = self.manager.document_symbol(path).await
            .map_err(|e| IndexerError::LspError(e.to_string()))?;

        if let Some(symbols) = lsp_symbols {
            // Convert DocumentSymbolResponse to Symbol vector
            let api_symbols = match symbols {
                lsp_types::DocumentSymbolResponse::Flat(symbol_info) => {
                    // Convert SymbolInformation to Symbol
                    symbol_info.iter().map(|si| Symbol {
                        name: si.name.clone(),
                        kind: format!("{:?}", si.kind),
                        identifier_position: FilePosition {
                            path: path.to_string(),
                            position: Position {
                                line: si.location.range.start.line + 1,
                                character: si.location.range.start.character + 1,
                            },
                        },
                        file_range: FileRange {
                            path: path.to_string(),
                            range: Range {
                                start: Position {
                                    line: si.location.range.start.line + 1,
                                    character: si.location.range.start.character + 1,
                                },
                                end: Position {
                                    line: si.location.range.end.line + 1,
                                    character: si.location.range.end.character + 1,
                                },
                            },
                        },
                        signature: None,
                        exported: None,
                        jsdoc_summary: None,
                        dependencies: None,
                        line_count: None,
                        children: None,
                        snippet: None,
                    }).collect()
                }
                lsp_types::DocumentSymbolResponse::Nested(document_symbols) => {
                    self.flatten_document_symbols(path, &document_symbols)
                }
            };
            return Ok(self.convert_api_symbols_to_nodes(path, &api_symbols, file_version, now));
        }

        // Fallback to ast-grep would go here
        Ok(Vec::new())
    }

    /// Flatten nested DocumentSymbols into a flat Symbol list
    fn flatten_document_symbols(&self, path: &str, symbols: &[lsp_types::DocumentSymbol]) -> Vec<Symbol> {
        let mut result = Vec::new();
        for symbol in symbols {
            let api_symbol = Symbol {
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                identifier_position: FilePosition {
                    path: path.to_string(),
                    position: Position {
                        line: symbol.selection_range.start.line + 1,
                        character: symbol.selection_range.start.character + 1,
                    },
                },
                file_range: FileRange {
                    path: path.to_string(),
                    range: Range {
                        start: Position {
                            line: symbol.range.start.line + 1,
                            character: symbol.range.start.character + 1,
                        },
                        end: Position {
                            line: symbol.range.end.line + 1,
                            character: symbol.range.end.character + 1,
                        },
                    },
                },
                signature: None,
                exported: None,
                jsdoc_summary: None,
                dependencies: None,
                line_count: None,
                children: if let Some(children) = &symbol.children {
                    Some(self.flatten_document_symbols(path, children))
                } else {
                    None
                },
                snippet: None,
            };
            result.push(api_symbol);

            // Add children to the flat list
            if let Some(children) = &symbol.children {
                result.extend(self.flatten_document_symbols(path, children));
            }
        }
        result
    }

    /// Convert API Symbol to SymbolNode
    fn convert_api_symbols_to_nodes(
        &self,
        path: &str,
        symbols: &[Symbol],
        file_version: u64,
        now: i64,
    ) -> Vec<SymbolNode> {
        symbols.iter().map(|symbol| {
            let pos = &symbol.identifier_position;
            SymbolNode {
                id: NodeId::for_symbol(path, pos.position.line, pos.position.character),
                name: symbol.name.clone(),
                kind: parse_symbol_kind(&symbol.kind),
                location: pos.clone(),
                end_position: Some(FilePosition {
                    path: symbol.file_range.path.clone(),
                    position: symbol.file_range.range.end.clone(),
                }),
                signature: symbol.signature.clone(),
                container_name: None,
                file_version,
                indexed_at: now,
                is_public_api: symbol.exported.unwrap_or(false),
            }
        }).collect()
    }

    /// Extract calls from a callable symbol using call hierarchy
    async fn extract_calls(&self, symbol: &SymbolNode, file_version: u64) -> Result<Vec<CallsEdge>, IndexerError> {
        let now = current_timestamp();
        let mut calls = Vec::new();

        // First prepare call hierarchy
        let lsp_position = lsp_types::Position {
            line: symbol.location.position.line.saturating_sub(1),
            character: symbol.location.position.character.saturating_sub(1),
        };

        let items = self.manager
            .prepare_call_hierarchy(&symbol.location.path, lsp_position)
            .await
            .map_err(|e| IndexerError::LspError(e.to_string()))?;

        if let Some(hierarchy_items) = items {
            for item in hierarchy_items {
                // Get outgoing calls
                let outgoing = self.manager
                    .outgoing_calls(&symbol.location.path, &item)
                    .await
                    .map_err(|e| IndexerError::LspError(e.to_string()))?;

                for call in outgoing {
                    // Create callee NodeId
                    let callee_uri = &call.to.uri;
                    let callee_path = crate::utils::file_utils::uri_to_relative_path_string(callee_uri);
                    let callee_position = call.to.selection_range.start;

                    let callee_id = NodeId::for_symbol(
                        &callee_path,
                        callee_position.line + 1,
                        callee_position.character + 1,
                    );

                    let call_sites: Vec<CallSite> = call.from_ranges
                        .iter()
                        .map(|r| CallSite {
                            line: r.start.line + 1,
                            character: r.start.character + 1,
                            snippet: None,
                        })
                        .collect();

                    let is_cross_package = self.is_cross_package(&symbol.location.path, &callee_path);

                    let edge = CallsEdge {
                        id: EdgeId::new(&symbol.id, &callee_id, EdgeKind::Calls),
                        caller_id: symbol.id.clone(),
                        callee_id,
                        call_sites,
                        metadata: EdgeMetadata {
                            confidence: Confidence::High,
                            provenance: Provenance::Lsp,
                            validated_at: now,
                            source_file_version: file_version,
                            target_file_version: Some(file_version),
                            is_cross_package,
                        },
                    };
                    calls.push(edge);
                }
            }
        }

        Ok(calls)
    }

    /// Extract imports from a file
    async fn extract_imports(&self, _path: &str, _file_id: &NodeId, _file_version: u64) -> Result<Vec<ImportsEdge>, IndexerError> {
        // This would use ast-grep to parse import statements
        // For now, return empty - will implement with ast-grep patterns
        Ok(Vec::new())
    }

    fn is_external_path(&self, path: &str) -> bool {
        path.contains("node_modules") ||
        path.contains(".cargo/registry") ||
        path.contains("vendor/") ||
        path.contains("site-packages")
    }

    fn is_cross_package(&self, from_path: &str, to_path: &str) -> bool {
        // Check if paths are in different top-level directories
        let from_parts: Vec<&str> = from_path.split('/').collect();
        let to_parts: Vec<&str> = to_path.split('/').collect();

        if from_parts.len() < 2 || to_parts.len() < 2 {
            return false;
        }

        from_parts[0] != to_parts[0] || from_parts[1] != to_parts[1]
    }

    async fn compute_file_hash(&self, path: &str) -> Result<String, IndexerError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let content = tokio::fs::read_to_string(path).await?;
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        Ok(format!("{:016x}", hasher.finish()))
    }

    async fn count_lines(&self, path: &str) -> Result<u32, IndexerError> {
        let content = tokio::fs::read_to_string(path).await?;
        Ok(content.lines().count() as u32)
    }
}

#[derive(Debug, Default)]
pub struct IndexStats {
    pub files: u32,
    pub symbols: u32,
    pub edges: u32,
}

#[derive(Debug, Default)]
pub struct FileIndexStats {
    pub symbols: u32,
    pub edges: u32,
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn detect_language(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" | "cxx" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        _ => "unknown",
    }.to_string()
}

fn parse_symbol_kind(kind: &str) -> SymbolKind {
    match kind.to_lowercase().as_str() {
        "function" => SymbolKind::Function,
        "method" => SymbolKind::Method,
        "class" => SymbolKind::Class,
        "interface" => SymbolKind::Interface,
        "trait" => SymbolKind::Trait,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "enumvariant" | "enum variant" | "enummember" => SymbolKind::EnumVariant,
        "type" => SymbolKind::Type,
        "typealias" | "type alias" => SymbolKind::TypeAlias,
        "field" => SymbolKind::Field,
        "property" => SymbolKind::Property,
        "variable" => SymbolKind::Variable,
        "constant" => SymbolKind::Constant,
        "module" => SymbolKind::Module,
        "namespace" => SymbolKind::Namespace,
        _ => SymbolKind::Unknown,
    }
}

fn is_callable(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Function | SymbolKind::Method)
}