// ABOUTME: SQLite persistence store for the codemap graph.
// ABOUTME: Stores nodes (files, symbols, modules) and edges (defines, imports, calls).

use crate::api_types::{FilePosition, Position};
use crate::codemap::types::*;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Arc;
use tokio::task;

const SCHEMA: &str = r#"
-- Files table
CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    path TEXT UNIQUE NOT NULL,
    language TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    line_count INTEGER NOT NULL,
    is_external INTEGER NOT NULL DEFAULT 0,
    indexed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);

-- Symbols table
CREATE TABLE IF NOT EXISTS symbols (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    file_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    start_character INTEGER NOT NULL,
    end_line INTEGER,
    end_character INTEGER,
    signature TEXT,
    container_name TEXT,
    file_version INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL,
    is_public_api INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path);
CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);

-- Modules table
CREATE TABLE IF NOT EXISTS modules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    entry_file TEXT,
    is_external INTEGER NOT NULL DEFAULT 0,
    indexed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_modules_path ON modules(path);

-- Defines edges: File -> Symbol
CREATE TABLE IF NOT EXISTS edges_defines (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL,
    symbol_id TEXT NOT NULL,
    confidence TEXT NOT NULL,
    provenance TEXT NOT NULL,
    validated_at INTEGER NOT NULL,
    source_file_version INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_defines_file ON edges_defines(file_id);
CREATE INDEX IF NOT EXISTS idx_defines_symbol ON edges_defines(symbol_id);

-- Imports edges: File -> File/Module
CREATE TABLE IF NOT EXISTS edges_imports (
    id TEXT PRIMARY KEY,
    from_file_id TEXT NOT NULL,
    to_target_id TEXT NOT NULL,
    import_path TEXT NOT NULL,
    confidence TEXT NOT NULL,
    provenance TEXT NOT NULL,
    validated_at INTEGER NOT NULL,
    source_file_version INTEGER NOT NULL,
    target_file_version INTEGER,
    is_cross_package INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_imports_from ON edges_imports(from_file_id);
CREATE INDEX IF NOT EXISTS idx_imports_to ON edges_imports(to_target_id);

-- Calls edges: Symbol -> Symbol
CREATE TABLE IF NOT EXISTS edges_calls (
    id TEXT PRIMARY KEY,
    caller_id TEXT NOT NULL,
    callee_id TEXT NOT NULL,
    confidence TEXT NOT NULL,
    provenance TEXT NOT NULL,
    validated_at INTEGER NOT NULL,
    source_file_version INTEGER NOT NULL,
    target_file_version INTEGER NOT NULL,
    is_cross_package INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_calls_caller ON edges_calls(caller_id);
CREATE INDEX IF NOT EXISTS idx_calls_callee ON edges_calls(callee_id);

-- Call sites (many per calls edge)
CREATE TABLE IF NOT EXISTS call_sites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    edge_id TEXT NOT NULL,
    line INTEGER NOT NULL,
    character INTEGER NOT NULL,
    snippet TEXT,
    FOREIGN KEY (edge_id) REFERENCES edges_calls(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_call_sites_edge ON call_sites(edge_id);

-- File versions for incremental updates
CREATE TABLE IF NOT EXISTS file_versions (
    file_path TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    version INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Codemap state
CREATE TABLE IF NOT EXISTS codemap_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// Errors from codemap store operations
#[derive(Debug)]
pub enum CodemapStoreError {
    DatabaseError(String),
    #[allow(dead_code)]
    SerializationError(String),
}

impl std::fmt::Display for CodemapStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodemapStoreError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            CodemapStoreError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for CodemapStoreError {}

/// SQLite-based persistence store for codemap graph data
pub struct CodemapStore {
    conn: Arc<Mutex<Connection>>,
    #[allow(dead_code)]
    db_path: std::path::PathBuf,
}

impl CodemapStore {
    /// Create or open a codemap store at the given path
    pub async fn new(db_path: &Path) -> Result<Self, CodemapStoreError> {
        let path = db_path.to_path_buf();
        let conn = task::spawn_blocking(move || {
            let conn = Connection::open(&path)
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            conn.execute_batch("PRAGMA foreign_keys = ON;")
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            conn.execute_batch(SCHEMA)
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            Ok::<_, CodemapStoreError>(conn)
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))??;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: db_path.to_path_buf(),
        })
    }

    // ================================================================
    // FILE OPERATIONS
    // ================================================================

    /// Insert or update a file node
    pub async fn upsert_file(&self, file: &FileNode) -> Result<(), CodemapStoreError> {
        let file = file.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            conn.execute(
                r#"
                INSERT INTO files (id, path, language, content_hash, mtime, line_count, is_external, indexed_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(id) DO UPDATE SET
                    path = excluded.path,
                    language = excluded.language,
                    content_hash = excluded.content_hash,
                    mtime = excluded.mtime,
                    line_count = excluded.line_count,
                    is_external = excluded.is_external,
                    indexed_at = excluded.indexed_at
                "#,
                params![
                    file.id.as_str(),
                    file.path,
                    file.language,
                    file.content_hash,
                    file.mtime,
                    file.line_count,
                    file.is_external as i32,
                    now,
                ],
            )
            .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Get a file node by path
    pub async fn get_file(&self, path: &str) -> Result<Option<FileNode>, CodemapStoreError> {
        let path = path.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result = conn.query_row(
                "SELECT id, path, language, content_hash, mtime, line_count, is_external FROM files WHERE path = ?",
                params![path],
                |row| {
                    Ok(FileNode {
                        id: NodeId(row.get(0)?),
                        path: row.get(1)?,
                        language: row.get(2)?,
                        content_hash: row.get(3)?,
                        mtime: row.get(4)?,
                        line_count: row.get(5)?,
                        is_external: row.get::<_, i32>(6)? != 0,
                    })
                },
            );

            match result {
                Ok(file) => Ok(Some(file)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(CodemapStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Delete a file node by path
    pub async fn delete_file(&self, path: &str) -> Result<(), CodemapStoreError> {
        let path = path.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute("DELETE FROM files WHERE path = ?", params![path])
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Get all file nodes
    pub async fn get_all_files(&self) -> Result<Vec<FileNode>, CodemapStoreError> {
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt = conn
                .prepare("SELECT id, path, language, content_hash, mtime, line_count, is_external FROM files")
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            let files = stmt
                .query_map([], |row| {
                    Ok(FileNode {
                        id: NodeId(row.get(0)?),
                        path: row.get(1)?,
                        language: row.get(2)?,
                        content_hash: row.get(3)?,
                        mtime: row.get(4)?,
                        line_count: row.get(5)?,
                        is_external: row.get::<_, i32>(6)? != 0,
                    })
                })
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(files)
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    // ================================================================
    // SYMBOL OPERATIONS
    // ================================================================

    /// Insert or update a symbol node
    pub async fn upsert_symbol(&self, symbol: &SymbolNode) -> Result<(), CodemapStoreError> {
        let symbol = symbol.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let kind_str = format!("{:?}", symbol.kind).to_lowercase();

            conn.execute(
                r#"
                INSERT INTO symbols (id, name, kind, file_path, start_line, start_character, end_line, end_character, signature, container_name, file_version, indexed_at, is_public_api)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    kind = excluded.kind,
                    file_path = excluded.file_path,
                    start_line = excluded.start_line,
                    start_character = excluded.start_character,
                    end_line = excluded.end_line,
                    end_character = excluded.end_character,
                    signature = excluded.signature,
                    container_name = excluded.container_name,
                    file_version = excluded.file_version,
                    indexed_at = excluded.indexed_at,
                    is_public_api = excluded.is_public_api
                "#,
                params![
                    symbol.id.as_str(),
                    symbol.name,
                    kind_str,
                    symbol.location.path,
                    symbol.location.position.line,
                    symbol.location.position.character,
                    symbol.end_position.as_ref().map(|p| p.position.line),
                    symbol.end_position.as_ref().map(|p| p.position.character),
                    symbol.signature,
                    symbol.container_name,
                    symbol.file_version as i64,
                    symbol.indexed_at,
                    symbol.is_public_api as i32,
                ],
            )
            .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Get a symbol by ID
    pub async fn get_symbol(&self, id: &NodeId) -> Result<Option<SymbolNode>, CodemapStoreError> {
        let id = id.0.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result = conn.query_row(
                "SELECT id, name, kind, file_path, start_line, start_character, end_line, end_character, signature, container_name, file_version, indexed_at, is_public_api FROM symbols WHERE id = ?",
                params![id],
                |row| {
                    let kind_str: String = row.get(2)?;
                    let kind = parse_symbol_kind(&kind_str);
                    let file_path: String = row.get(3)?;
                    let end_line: Option<u32> = row.get(6)?;
                    let end_char: Option<u32> = row.get(7)?;
                    let end_position = match (end_line, end_char) {
                        (Some(line), Some(character)) => Some(FilePosition {
                            path: file_path.clone(),
                            position: Position { line, character },
                        }),
                        _ => None,
                    };

                    Ok(SymbolNode {
                        id: NodeId(row.get(0)?),
                        name: row.get(1)?,
                        kind,
                        location: FilePosition {
                            path: file_path,
                            position: Position {
                                line: row.get(4)?,
                                character: row.get(5)?,
                            },
                        },
                        end_position,
                        signature: row.get(8)?,
                        container_name: row.get(9)?,
                        file_version: row.get::<_, i64>(10)? as u64,
                        indexed_at: row.get(11)?,
                        is_public_api: row.get::<_, i32>(12)? != 0,
                    })
                },
            );

            match result {
                Ok(symbol) => Ok(Some(symbol)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(CodemapStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Get all symbols in a file
    pub async fn get_symbols_in_file(
        &self,
        path: &str,
    ) -> Result<Vec<SymbolNode>, CodemapStoreError> {
        let path = path.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt = conn
                .prepare("SELECT id, name, kind, file_path, start_line, start_character, end_line, end_character, signature, container_name, file_version, indexed_at, is_public_api FROM symbols WHERE file_path = ?")
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            let symbols = stmt
                .query_map(params![path], |row| {
                    let kind_str: String = row.get(2)?;
                    let kind = parse_symbol_kind(&kind_str);
                    let file_path: String = row.get(3)?;
                    let end_line: Option<u32> = row.get(6)?;
                    let end_char: Option<u32> = row.get(7)?;
                    let end_position = match (end_line, end_char) {
                        (Some(line), Some(character)) => Some(FilePosition {
                            path: file_path.clone(),
                            position: Position { line, character },
                        }),
                        _ => None,
                    };

                    Ok(SymbolNode {
                        id: NodeId(row.get(0)?),
                        name: row.get(1)?,
                        kind,
                        location: FilePosition {
                            path: file_path,
                            position: Position {
                                line: row.get(4)?,
                                character: row.get(5)?,
                            },
                        },
                        end_position,
                        signature: row.get(8)?,
                        container_name: row.get(9)?,
                        file_version: row.get::<_, i64>(10)? as u64,
                        indexed_at: row.get(11)?,
                        is_public_api: row.get::<_, i32>(12)? != 0,
                    })
                })
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(symbols)
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Delete all symbols in a file
    pub async fn delete_symbols_in_file(&self, path: &str) -> Result<(), CodemapStoreError> {
        let path = path.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute("DELETE FROM symbols WHERE file_path = ?", params![path])
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Get all symbols
    pub async fn get_all_symbols(&self) -> Result<Vec<SymbolNode>, CodemapStoreError> {
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt = conn
                .prepare("SELECT id, name, kind, file_path, start_line, start_character, end_line, end_character, signature, container_name, file_version, indexed_at, is_public_api FROM symbols")
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            let symbols = stmt
                .query_map([], |row| {
                    let kind_str: String = row.get(2)?;
                    let kind = parse_symbol_kind(&kind_str);
                    let file_path: String = row.get(3)?;
                    let end_line: Option<u32> = row.get(6)?;
                    let end_char: Option<u32> = row.get(7)?;
                    let end_position = match (end_line, end_char) {
                        (Some(line), Some(character)) => Some(FilePosition {
                            path: file_path.clone(),
                            position: Position { line, character },
                        }),
                        _ => None,
                    };

                    Ok(SymbolNode {
                        id: NodeId(row.get(0)?),
                        name: row.get(1)?,
                        kind,
                        location: FilePosition {
                            path: file_path,
                            position: Position {
                                line: row.get(4)?,
                                character: row.get(5)?,
                            },
                        },
                        end_position,
                        signature: row.get(8)?,
                        container_name: row.get(9)?,
                        file_version: row.get::<_, i64>(10)? as u64,
                        indexed_at: row.get(11)?,
                        is_public_api: row.get::<_, i32>(12)? != 0,
                    })
                })
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(symbols)
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Search symbols by name (LIKE query)
    pub async fn search_symbols(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SymbolNode>, CodemapStoreError> {
        let query = format!("%{}%", query);
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt = conn
                .prepare("SELECT id, name, kind, file_path, start_line, start_character, end_line, end_character, signature, container_name, file_version, indexed_at, is_public_api FROM symbols WHERE name LIKE ? LIMIT ?")
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            let symbols = stmt
                .query_map(params![query, limit], |row| {
                    let kind_str: String = row.get(2)?;
                    let kind = parse_symbol_kind(&kind_str);
                    let file_path: String = row.get(3)?;
                    let end_line: Option<u32> = row.get(6)?;
                    let end_char: Option<u32> = row.get(7)?;
                    let end_position = match (end_line, end_char) {
                        (Some(line), Some(character)) => Some(FilePosition {
                            path: file_path.clone(),
                            position: Position { line, character },
                        }),
                        _ => None,
                    };

                    Ok(SymbolNode {
                        id: NodeId(row.get(0)?),
                        name: row.get(1)?,
                        kind,
                        location: FilePosition {
                            path: file_path,
                            position: Position {
                                line: row.get(4)?,
                                character: row.get(5)?,
                            },
                        },
                        end_position,
                        signature: row.get(8)?,
                        container_name: row.get(9)?,
                        file_version: row.get::<_, i64>(10)? as u64,
                        indexed_at: row.get(11)?,
                        is_public_api: row.get::<_, i32>(12)? != 0,
                    })
                })
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(symbols)
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    // ================================================================
    // EDGE OPERATIONS
    // ================================================================

    /// Insert or update a defines edge
    pub async fn upsert_defines_edge(&self, edge: &DefinesEdge) -> Result<(), CodemapStoreError> {
        let edge = edge.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let confidence_str = format!("{:?}", edge.metadata.confidence).to_lowercase();
            let provenance_str = format!("{:?}", edge.metadata.provenance).to_lowercase();

            conn.execute(
                r#"
                INSERT INTO edges_defines (id, file_id, symbol_id, confidence, provenance, validated_at, source_file_version)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(id) DO UPDATE SET
                    file_id = excluded.file_id,
                    symbol_id = excluded.symbol_id,
                    confidence = excluded.confidence,
                    provenance = excluded.provenance,
                    validated_at = excluded.validated_at,
                    source_file_version = excluded.source_file_version
                "#,
                params![
                    edge.id.as_str(),
                    edge.file_id.as_str(),
                    edge.symbol_id.as_str(),
                    confidence_str,
                    provenance_str,
                    edge.metadata.validated_at,
                    edge.metadata.source_file_version as i64,
                ],
            )
            .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Insert or update an imports edge
    pub async fn upsert_imports_edge(&self, edge: &ImportsEdge) -> Result<(), CodemapStoreError> {
        let edge = edge.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let confidence_str = format!("{:?}", edge.metadata.confidence).to_lowercase();
            let provenance_str = format!("{:?}", edge.metadata.provenance).to_lowercase();

            conn.execute(
                r#"
                INSERT INTO edges_imports (id, from_file_id, to_target_id, import_path, confidence, provenance, validated_at, source_file_version, target_file_version, is_cross_package)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(id) DO UPDATE SET
                    from_file_id = excluded.from_file_id,
                    to_target_id = excluded.to_target_id,
                    import_path = excluded.import_path,
                    confidence = excluded.confidence,
                    provenance = excluded.provenance,
                    validated_at = excluded.validated_at,
                    source_file_version = excluded.source_file_version,
                    target_file_version = excluded.target_file_version,
                    is_cross_package = excluded.is_cross_package
                "#,
                params![
                    edge.id.as_str(),
                    edge.from_file_id.as_str(),
                    edge.to_target_id.as_str(),
                    edge.import_path,
                    confidence_str,
                    provenance_str,
                    edge.metadata.validated_at,
                    edge.metadata.source_file_version as i64,
                    edge.metadata.target_file_version.map(|v| v as i64),
                    edge.metadata.is_cross_package as i32,
                ],
            )
            .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Insert or update a calls edge (including call sites)
    pub async fn upsert_calls_edge(&self, edge: &CallsEdge) -> Result<(), CodemapStoreError> {
        let edge = edge.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let confidence_str = format!("{:?}", edge.metadata.confidence).to_lowercase();
            let provenance_str = format!("{:?}", edge.metadata.provenance).to_lowercase();

            conn.execute(
                r#"
                INSERT INTO edges_calls (id, caller_id, callee_id, confidence, provenance, validated_at, source_file_version, target_file_version, is_cross_package)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(id) DO UPDATE SET
                    caller_id = excluded.caller_id,
                    callee_id = excluded.callee_id,
                    confidence = excluded.confidence,
                    provenance = excluded.provenance,
                    validated_at = excluded.validated_at,
                    source_file_version = excluded.source_file_version,
                    target_file_version = excluded.target_file_version,
                    is_cross_package = excluded.is_cross_package
                "#,
                params![
                    edge.id.as_str(),
                    edge.caller_id.as_str(),
                    edge.callee_id.as_str(),
                    confidence_str,
                    provenance_str,
                    edge.metadata.validated_at,
                    edge.metadata.source_file_version as i64,
                    edge.metadata.target_file_version.unwrap_or(0) as i64,
                    edge.metadata.is_cross_package as i32,
                ],
            )
            .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            conn.execute(
                "DELETE FROM call_sites WHERE edge_id = ?",
                params![edge.id.as_str()],
            )
            .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            for site in &edge.call_sites {
                conn.execute(
                    "INSERT INTO call_sites (edge_id, line, character, snippet) VALUES (?1, ?2, ?3, ?4)",
                    params![edge.id.as_str(), site.line, site.character, site.snippet],
                )
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            }

            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Get all edges originating from a node
    pub async fn get_edges_from(&self, node_id: &NodeId) -> Result<Vec<Edge>, CodemapStoreError> {
        let id = node_id.0.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut edges = Vec::new();

            {
                let mut stmt = conn
                    .prepare("SELECT id, file_id, symbol_id, confidence, provenance, validated_at, source_file_version FROM edges_defines WHERE file_id = ?")
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

                let defines: Vec<Edge> = stmt
                    .query_map(params![id], |row| {
                        Ok(Edge::Defines(DefinesEdge {
                            id: EdgeId(row.get(0)?),
                            file_id: NodeId(row.get(1)?),
                            symbol_id: NodeId(row.get(2)?),
                            metadata: EdgeMetadata {
                                confidence: parse_confidence(&row.get::<_, String>(3)?),
                                provenance: parse_provenance(&row.get::<_, String>(4)?),
                                validated_at: row.get(5)?,
                                source_file_version: row.get::<_, i64>(6)? as u64,
                                target_file_version: None,
                                is_cross_package: false,
                            },
                        }))
                    })
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                    .filter_map(|r| r.ok())
                    .collect();
                edges.extend(defines);
            }

            {
                let mut stmt = conn
                    .prepare("SELECT id, from_file_id, to_target_id, import_path, confidence, provenance, validated_at, source_file_version, target_file_version, is_cross_package FROM edges_imports WHERE from_file_id = ?")
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

                let imports: Vec<Edge> = stmt
                    .query_map(params![id], |row| {
                        Ok(Edge::Imports(ImportsEdge {
                            id: EdgeId(row.get(0)?),
                            from_file_id: NodeId(row.get(1)?),
                            to_target_id: NodeId(row.get(2)?),
                            import_path: row.get(3)?,
                            metadata: EdgeMetadata {
                                confidence: parse_confidence(&row.get::<_, String>(4)?),
                                provenance: parse_provenance(&row.get::<_, String>(5)?),
                                validated_at: row.get(6)?,
                                source_file_version: row.get::<_, i64>(7)? as u64,
                                target_file_version: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                                is_cross_package: row.get::<_, i32>(9)? != 0,
                            },
                        }))
                    })
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                    .filter_map(|r| r.ok())
                    .collect();
                edges.extend(imports);
            }

            {
                let mut stmt = conn
                    .prepare("SELECT id, caller_id, callee_id, confidence, provenance, validated_at, source_file_version, target_file_version, is_cross_package FROM edges_calls WHERE caller_id = ?")
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

                let calls: Vec<Edge> = stmt
                    .query_map(params![id], |row| {
                        let edge_id: String = row.get(0)?;
                        Ok((edge_id, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, i64>(5)?, row.get::<_, i64>(6)?, row.get::<_, i64>(7)?, row.get::<_, i32>(8)?))
                    })
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                    .filter_map(|r| r.ok())
                    .map(|(edge_id, caller_id, callee_id, confidence, provenance, validated_at, source_version, target_version, is_cross)| {
                        let call_sites = get_call_sites_sync(&conn, &edge_id);
                        Edge::Calls(CallsEdge {
                            id: EdgeId(edge_id),
                            caller_id: NodeId(caller_id),
                            callee_id: NodeId(callee_id),
                            call_sites,
                            metadata: EdgeMetadata {
                                confidence: parse_confidence(&confidence),
                                provenance: parse_provenance(&provenance),
                                validated_at,
                                source_file_version: source_version as u64,
                                target_file_version: Some(target_version as u64),
                                is_cross_package: is_cross != 0,
                            },
                        })
                    })
                    .collect();
                edges.extend(calls);
            }

            Ok(edges)
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Get all edges pointing to a node
    pub async fn get_edges_to(&self, node_id: &NodeId) -> Result<Vec<Edge>, CodemapStoreError> {
        let id = node_id.0.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut edges = Vec::new();

            {
                let mut stmt = conn
                    .prepare("SELECT id, file_id, symbol_id, confidence, provenance, validated_at, source_file_version FROM edges_defines WHERE symbol_id = ?")
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

                let defines: Vec<Edge> = stmt
                    .query_map(params![id], |row| {
                        Ok(Edge::Defines(DefinesEdge {
                            id: EdgeId(row.get(0)?),
                            file_id: NodeId(row.get(1)?),
                            symbol_id: NodeId(row.get(2)?),
                            metadata: EdgeMetadata {
                                confidence: parse_confidence(&row.get::<_, String>(3)?),
                                provenance: parse_provenance(&row.get::<_, String>(4)?),
                                validated_at: row.get(5)?,
                                source_file_version: row.get::<_, i64>(6)? as u64,
                                target_file_version: None,
                                is_cross_package: false,
                            },
                        }))
                    })
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                    .filter_map(|r| r.ok())
                    .collect();
                edges.extend(defines);
            }

            {
                let mut stmt = conn
                    .prepare("SELECT id, from_file_id, to_target_id, import_path, confidence, provenance, validated_at, source_file_version, target_file_version, is_cross_package FROM edges_imports WHERE to_target_id = ?")
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

                let imports: Vec<Edge> = stmt
                    .query_map(params![id], |row| {
                        Ok(Edge::Imports(ImportsEdge {
                            id: EdgeId(row.get(0)?),
                            from_file_id: NodeId(row.get(1)?),
                            to_target_id: NodeId(row.get(2)?),
                            import_path: row.get(3)?,
                            metadata: EdgeMetadata {
                                confidence: parse_confidence(&row.get::<_, String>(4)?),
                                provenance: parse_provenance(&row.get::<_, String>(5)?),
                                validated_at: row.get(6)?,
                                source_file_version: row.get::<_, i64>(7)? as u64,
                                target_file_version: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                                is_cross_package: row.get::<_, i32>(9)? != 0,
                            },
                        }))
                    })
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                    .filter_map(|r| r.ok())
                    .collect();
                edges.extend(imports);
            }

            {
                let mut stmt = conn
                    .prepare("SELECT id, caller_id, callee_id, confidence, provenance, validated_at, source_file_version, target_file_version, is_cross_package FROM edges_calls WHERE callee_id = ?")
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

                let calls: Vec<Edge> = stmt
                    .query_map(params![id], |row| {
                        let edge_id: String = row.get(0)?;
                        Ok((edge_id, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, i64>(5)?, row.get::<_, i64>(6)?, row.get::<_, i64>(7)?, row.get::<_, i32>(8)?))
                    })
                    .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
                    .filter_map(|r| r.ok())
                    .map(|(edge_id, caller_id, callee_id, confidence, provenance, validated_at, source_version, target_version, is_cross)| {
                        let call_sites = get_call_sites_sync(&conn, &edge_id);
                        Edge::Calls(CallsEdge {
                            id: EdgeId(edge_id),
                            caller_id: NodeId(caller_id),
                            callee_id: NodeId(callee_id),
                            call_sites,
                            metadata: EdgeMetadata {
                                confidence: parse_confidence(&confidence),
                                provenance: parse_provenance(&provenance),
                                validated_at,
                                source_file_version: source_version as u64,
                                target_file_version: Some(target_version as u64),
                                is_cross_package: is_cross != 0,
                            },
                        })
                    })
                    .collect();
                edges.extend(calls);
            }

            Ok(edges)
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Delete all edges for a file (defines edges where file is source, imports edges where file is source)
    pub async fn delete_edges_for_file(&self, file_id: &NodeId) -> Result<(), CodemapStoreError> {
        let id = file_id.0.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();

            conn.execute("DELETE FROM edges_defines WHERE file_id = ?", params![id])
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            conn.execute(
                "DELETE FROM edges_imports WHERE from_file_id = ?",
                params![id],
            )
            .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    // ================================================================
    // FILE VERSION TRACKING
    // ================================================================

    /// Get the version for a file path
    pub async fn get_file_version(&self, path: &str) -> Result<Option<u64>, CodemapStoreError> {
        let path = path.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result: Result<i64, _> = conn.query_row(
                "SELECT version FROM file_versions WHERE file_path = ?",
                params![path],
                |row| row.get(0),
            );

            match result {
                Ok(version) => Ok(Some(version as u64)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(CodemapStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Set the version for a file path
    pub async fn set_file_version(
        &self,
        path: &str,
        hash: &str,
        version: u64,
    ) -> Result<(), CodemapStoreError> {
        let path = path.to_string();
        let hash = hash.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            conn.execute(
                r#"
                INSERT INTO file_versions (file_path, content_hash, version, updated_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(file_path) DO UPDATE SET
                    content_hash = excluded.content_hash,
                    version = excluded.version,
                    updated_at = excluded.updated_at
                "#,
                params![path, hash, version as i64, now],
            )
            .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    // ================================================================
    // STATS
    // ================================================================

    /// Get counts of files, symbols, and edges
    pub async fn get_stats(&self) -> Result<(u32, u32, u32), CodemapStoreError> {
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();

            let file_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            let symbol_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            let defines_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM edges_defines", [], |row| row.get(0))
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            let imports_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM edges_imports", [], |row| row.get(0))
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            let calls_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM edges_calls", [], |row| row.get(0))
                .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;

            let edge_count = defines_count + imports_count + calls_count;

            Ok((file_count as u32, symbol_count as u32, edge_count as u32))
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    // ================================================================
    // STATE MANAGEMENT
    // ================================================================

    /// Set a state value
    pub async fn set_state(&self, key: &str, value: &str) -> Result<(), CodemapStoreError> {
        let key = key.to_string();
        let value = value.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                "INSERT OR REPLACE INTO codemap_state (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }

    /// Get a state value
    pub async fn get_state(&self, key: &str) -> Result<Option<String>, CodemapStoreError> {
        let key = key.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result: Result<String, _> = conn.query_row(
                "SELECT value FROM codemap_state WHERE key = ?",
                params![key],
                |row| row.get(0),
            );

            match result {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(CodemapStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| CodemapStoreError::DatabaseError(e.to_string()))?
    }
}

fn get_call_sites_sync(conn: &Connection, edge_id: &str) -> Vec<CallSite> {
    let mut stmt = match conn.prepare("SELECT line, character, snippet FROM call_sites WHERE edge_id = ?") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map(params![edge_id], |row| {
        Ok(CallSite {
            line: row.get(0)?,
            character: row.get(1)?,
            snippet: row.get(2)?,
        })
    })
    .ok()
    .map(|iter| iter.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

fn parse_symbol_kind(s: &str) -> SymbolKind {
    match s.to_lowercase().as_str() {
        "function" => SymbolKind::Function,
        "method" => SymbolKind::Method,
        "class" => SymbolKind::Class,
        "interface" => SymbolKind::Interface,
        "trait" => SymbolKind::Trait,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "enumvariant" => SymbolKind::EnumVariant,
        "type" => SymbolKind::Type,
        "typealias" => SymbolKind::TypeAlias,
        "field" => SymbolKind::Field,
        "property" => SymbolKind::Property,
        "variable" => SymbolKind::Variable,
        "constant" => SymbolKind::Constant,
        "module" => SymbolKind::Module,
        "namespace" => SymbolKind::Namespace,
        _ => SymbolKind::Unknown,
    }
}

fn parse_confidence(s: &str) -> Confidence {
    match s.to_lowercase().as_str() {
        "high" => Confidence::High,
        "medium" => Confidence::Medium,
        "low" => Confidence::Low,
        _ => Confidence::Medium,
    }
}

fn parse_provenance(s: &str) -> Provenance {
    match s.to_lowercase().as_str() {
        "lsp" => Provenance::Lsp,
        "ast" => Provenance::Ast,
        "heuristic" => Provenance::Heuristic,
        _ => Provenance::Ast,
    }
}
