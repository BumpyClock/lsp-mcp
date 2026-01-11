// ABOUTME: SQLite metadata store for vector index entries.
// ABOUTME: Maps segment hashes to numeric IDs and stores chunk metadata.

use super::types::{EnrichmentData, IndexEntry, IndexState};
use super::VectorStoreError;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::task;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    segment_hash TEXT UNIQUE NOT NULL,
    file_path TEXT NOT NULL,
    code TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    symbol_name TEXT,
    symbol_kind TEXT,
    indexed_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_file_path ON entries(file_path);
CREATE INDEX IF NOT EXISTS idx_segment_hash ON entries(segment_hash);
CREATE INDEX IF NOT EXISTS idx_symbol_kind ON entries(symbol_kind);

CREATE TABLE IF NOT EXISTS enrichments (
    segment_hash TEXT PRIMARY KEY,
    summary TEXT NOT NULL,
    tags_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_enrichments_updated_at ON enrichments(updated_at);

CREATE TABLE IF NOT EXISTS hnsw_ids (
    segment_hash TEXT PRIMARY KEY,
    hnsw_id INTEGER UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_hnsw_id ON hnsw_ids(hnsw_id);

CREATE TABLE IF NOT EXISTS index_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS file_hashes (
    file_path TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_file_hashes_updated_at ON file_hashes(updated_at);
"#;

/// SQLite-based metadata store for vector index entries.
pub struct MetadataStore {
    conn: Arc<Mutex<Connection>>,
    db_path: std::path::PathBuf,
}

impl MetadataStore {
    /// Create or open a metadata store at the given path.
    pub async fn new(db_path: &Path) -> Result<Self, VectorStoreError> {
        let path = db_path.to_path_buf();
        let conn = task::spawn_blocking(move || {
            let conn = Connection::open(&path)
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            conn.execute_batch(SCHEMA)
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok::<_, VectorStoreError>(conn)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))??;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: db_path.to_path_buf(),
        })
    }

    /// Get the numeric ID for a segment hash, or None if not found.
    pub async fn get_numeric_id(
        &self,
        segment_hash: &str,
    ) -> Result<Option<usize>, VectorStoreError> {
        let hash = segment_hash.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result: Result<i64, _> = conn.query_row(
                "SELECT id FROM entries WHERE segment_hash = ?",
                params![hash],
                |row| row.get(0),
            );

            match result {
                Ok(id) => Ok(Some(id as usize)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(VectorStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Insert or update an entry and return its numeric ID.
    pub async fn upsert_entry(&self, entry: &IndexEntry) -> Result<i64, VectorStoreError> {
        let entry = entry.clone();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();

            conn.execute(
                r#"
                INSERT INTO entries (segment_hash, file_path, code, start_line, end_line, symbol_name, symbol_kind, indexed_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(segment_hash) DO UPDATE SET
                    file_path = excluded.file_path,
                    code = excluded.code,
                    start_line = excluded.start_line,
                    end_line = excluded.end_line,
                    symbol_name = excluded.symbol_name,
                    symbol_kind = excluded.symbol_kind,
                    indexed_at = excluded.indexed_at
                "#,
                params![
                    entry.id,
                    entry.file_path,
                    entry.code,
                    entry.start_line,
                    entry.end_line,
                    entry.symbol_name,
                    entry.symbol_kind,
                    entry.indexed_at,
                ],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;

            // Get the ID (either newly inserted or existing)
            let id: i64 = conn
                .query_row(
                    "SELECT id FROM entries WHERE segment_hash = ?",
                    params![entry.id],
                    |row| row.get(0),
                )
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;

            Ok(id)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Insert or update a segment hash to HNSW ID mapping.
    pub async fn upsert_hnsw_id(
        &self,
        segment_hash: &str,
        hnsw_id: usize,
    ) -> Result<(), VectorStoreError> {
        let segment_hash = segment_hash.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO hnsw_ids (segment_hash, hnsw_id)
                VALUES (?1, ?2)
                ON CONFLICT(segment_hash) DO UPDATE SET
                    hnsw_id = excluded.hnsw_id
                "#,
                params![segment_hash, hnsw_id as i64],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Load all segment hash to HNSW ID mappings.
    pub async fn load_hnsw_ids(&self) -> Result<HashMap<String, usize>, VectorStoreError> {
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt = conn
                .prepare("SELECT segment_hash, hnsw_id FROM hnsw_ids")
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            let mappings = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
                .filter_map(|r| r.ok())
                .map(|(hash, id)| (hash, id as usize))
                .collect::<HashMap<_, _>>();
            Ok(mappings)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get distinct file paths whose HNSW IDs are at or above the provided minimum.
    pub async fn get_files_with_hnsw_id_at_or_above(
        &self,
        min_id: usize,
    ) -> Result<Vec<String>, VectorStoreError> {
        let min_id = min_id as i64;
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt = conn
                .prepare(
                    r#"
                    SELECT DISTINCT e.file_path
                    FROM entries e
                    JOIN hnsw_ids h ON e.segment_hash = h.segment_hash
                    WHERE h.hnsw_id >= ?1
                    ORDER BY e.file_path ASC
                    "#,
                )
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;

            let files: Vec<String> = stmt
                .query_map(params![min_id], |row| row.get(0))
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(files)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get an entry by its numeric ID.
    pub async fn get_by_numeric_id(
        &self,
        id: usize,
    ) -> Result<Option<IndexEntry>, VectorStoreError> {
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();

            let result = conn.query_row(
                "SELECT segment_hash, file_path, code, start_line, end_line, symbol_name, symbol_kind, indexed_at FROM entries WHERE id = ?",
                params![id as i64],
                |row| {
                    Ok(IndexEntry {
                        id: row.get(0)?,
                        file_path: row.get(1)?,
                        code: row.get(2)?,
                        start_line: row.get(3)?,
                        end_line: row.get(4)?,
                        symbol_name: row.get(5)?,
                        symbol_kind: row.get(6)?,
                        indexed_at: row.get(7)?,
                    })
                },
            );

            match result {
                Ok(entry) => Ok(Some(entry)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(VectorStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get all segment hashes for a file path.
    pub async fn get_file_hashes(&self, file_path: &str) -> Result<Vec<String>, VectorStoreError> {
        let path = file_path.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt = conn
                .prepare("SELECT segment_hash FROM entries WHERE file_path = ?")
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;

            let hashes: Vec<String> = stmt
                .query_map(params![path], |row| row.get(0))
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(hashes)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get enrichment data for a segment hash.
    pub async fn get_enrichment(
        &self,
        segment_hash: &str,
    ) -> Result<Option<EnrichmentData>, VectorStoreError> {
        let hash = segment_hash.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result: Result<(String, String), _> = conn.query_row(
                "SELECT summary, tags_json FROM enrichments WHERE segment_hash = ?",
                params![hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );

            match result {
                Ok((summary, tags_json)) => {
                    let tags: Vec<String> = serde_json::from_str(&tags_json)
                        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
                    Ok(Some(EnrichmentData { summary, tags }))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(VectorStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Insert or update enrichment data for a segment hash.
    pub async fn upsert_enrichment(
        &self,
        segment_hash: &str,
        summary: &str,
        tags: &[String],
        updated_at: i64,
    ) -> Result<(), VectorStoreError> {
        let hash = segment_hash.to_string();
        let summary = summary.to_string();
        let tags_json = serde_json::to_string(tags)
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO enrichments (segment_hash, summary, tags_json, updated_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(segment_hash) DO UPDATE SET
                    summary = excluded.summary,
                    tags_json = excluded.tags_json,
                    updated_at = excluded.updated_at
                "#,
                params![hash, summary, tags_json, updated_at],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get HNSW IDs for entries that match a path prefix or exact file path.
    pub async fn get_hnsw_ids_by_path_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<usize>, VectorStoreError> {
        let prefix = prefix.to_string();
        let like_prefix = if prefix.ends_with('/') {
            format!("{}%", escape_like(&prefix))
        } else {
            format!("{}/%", escape_like(&prefix))
        };
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt = conn
                .prepare(
                    r#"
                    SELECT h.hnsw_id
                    FROM hnsw_ids h
                    JOIN entries e ON e.segment_hash = h.segment_hash
                    WHERE e.file_path = ?1 OR e.file_path LIKE ?2 ESCAPE '\'
                    ORDER BY h.hnsw_id ASC
                    "#,
                )
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;

            let ids: Vec<usize> = stmt
                .query_map(params![prefix, like_prefix], |row| row.get::<_, i64>(0))
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
                .filter_map(|r| r.ok().map(|id| id as usize))
                .collect();

            Ok(ids)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get the stored hash for a file path.
    pub async fn get_file_hash(&self, file_path: &str) -> Result<Option<String>, VectorStoreError> {
        let path = file_path.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result: Result<String, _> = conn.query_row(
                "SELECT content_hash FROM file_hashes WHERE file_path = ?",
                params![path],
                |row| row.get(0),
            );

            match result {
                Ok(hash) => Ok(Some(hash)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(VectorStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Insert or update the stored hash for a file path.
    pub async fn upsert_file_hash(
        &self,
        file_path: &str,
        content_hash: &str,
        updated_at: i64,
    ) -> Result<(), VectorStoreError> {
        let path = file_path.to_string();
        let hash = content_hash.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO file_hashes (file_path, content_hash, updated_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(file_path) DO UPDATE SET
                    content_hash = excluded.content_hash,
                    updated_at = excluded.updated_at
                "#,
                params![path, hash, updated_at],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Delete the stored hash for a file path.
    pub async fn delete_file_hash(&self, file_path: &str) -> Result<(), VectorStoreError> {
        let path = file_path.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute("DELETE FROM file_hashes WHERE file_path = ?", params![path])
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Delete an entry by segment hash.
    pub async fn delete_entry(&self, segment_hash: &str) -> Result<(), VectorStoreError> {
        let hash = segment_hash.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                "DELETE FROM entries WHERE segment_hash = ?",
                params![hash.clone()],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            conn.execute("DELETE FROM hnsw_ids WHERE segment_hash = ?", params![hash])
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            conn.execute(
                "DELETE FROM enrichments WHERE segment_hash = ?",
                params![hash],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Delete all entries for a file path.
    #[allow(dead_code)]
    pub async fn delete_file(&self, file_path: &str) -> Result<usize, VectorStoreError> {
        let path = file_path.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                "DELETE FROM hnsw_ids WHERE segment_hash IN (SELECT segment_hash FROM entries WHERE file_path = ?)",
                params![path.clone()],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            conn.execute(
                "DELETE FROM enrichments WHERE segment_hash IN (SELECT segment_hash FROM entries WHERE file_path = ?)",
                params![path.clone()],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            let deleted = conn
                .execute("DELETE FROM entries WHERE file_path = ?", params![path.clone()])
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            conn.execute("DELETE FROM file_hashes WHERE file_path = ?", params![path])
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(deleted)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Check if a segment hash exists.
    pub async fn exists(&self, segment_hash: &str) -> Result<bool, VectorStoreError> {
        let hash = segment_hash.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM entries WHERE segment_hash = ?",
                    params![hash],
                    |row| row.get(0),
                )
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(count > 0)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Count total entries.
    pub async fn count_entries(&self) -> Result<usize, VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        task::spawn_blocking(move || {
            let conn = conn.lock();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(count as usize)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Count distinct files.
    pub async fn count_files(&self) -> Result<usize, VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        task::spawn_blocking(move || {
            let conn = conn.lock();
            let count: i64 = conn
                .query_row("SELECT COUNT(DISTINCT file_path) FROM entries", [], |row| {
                    row.get(0)
                })
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(count as usize)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get the last update timestamp.
    pub async fn last_updated(&self) -> Result<i64, VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        task::spawn_blocking(move || {
            let conn = conn.lock();
            let ts: Option<i64> = conn
                .query_row("SELECT MAX(indexed_at) FROM entries", [], |row| row.get(0))
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(ts.unwrap_or(0))
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get database file size in bytes.
    pub async fn size_bytes(&self) -> Result<u64, VectorStoreError> {
        let path = self.db_path.clone();
        task::spawn_blocking(move || {
            std::fs::metadata(&path)
                .map(|m| m.len())
                .map_err(VectorStoreError::IoError)
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Flush WAL to main database file.
    pub async fn flush(&self) -> Result<(), VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Set index state.
    pub async fn set_state(&self, state: IndexState) -> Result<(), VectorStoreError> {
        let state_str = match state {
            IndexState::Empty => "empty",
            IndexState::Building => "building",
            IndexState::Ready => "ready",
        };
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                "INSERT OR REPLACE INTO index_state (key, value) VALUES ('state', ?)",
                params![state_str],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get index state.
    pub async fn get_state(&self) -> Result<IndexState, VectorStoreError> {
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result: Result<String, _> = conn.query_row(
                "SELECT value FROM index_state WHERE key = 'state'",
                [],
                |row| row.get(0),
            );

            match result {
                Ok(state_str) => match state_str.as_str() {
                    "building" => Ok(IndexState::Building),
                    "ready" => Ok(IndexState::Ready),
                    _ => Ok(IndexState::Empty),
                },
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(IndexState::Empty),
                Err(e) => Err(VectorStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Store the current index basename.
    pub async fn set_index_basename(&self, basename: &str) -> Result<(), VectorStoreError> {
        let basename = basename.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                "INSERT OR REPLACE INTO index_state (key, value) VALUES ('basename', ?)",
                params![basename],
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Get the stored index basename if present.
    pub async fn get_index_basename(&self) -> Result<Option<String>, VectorStoreError> {
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result: Result<String, _> = conn.query_row(
                "SELECT value FROM index_state WHERE key = 'basename'",
                [],
                |row| row.get(0),
            );
            match result {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(VectorStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    /// Record the index start timestamp.
    pub async fn set_index_started_at(&self, timestamp: i64) -> Result<(), VectorStoreError> {
        self.set_index_value("index_started_at", Some(timestamp.to_string()))
            .await
    }

    /// Record the index completion timestamp.
    pub async fn set_index_completed_at(&self, timestamp: i64) -> Result<(), VectorStoreError> {
        self.set_index_value("index_completed_at", Some(timestamp.to_string()))
            .await
    }

    /// Clear the index completion timestamp.
    pub async fn clear_index_completed_at(&self) -> Result<(), VectorStoreError> {
        self.set_index_value("index_completed_at", None).await
    }

    /// Get the index start timestamp.
    #[allow(dead_code)]
    pub async fn get_index_started_at(&self) -> Result<Option<i64>, VectorStoreError> {
        self.get_index_timestamp("index_started_at").await
    }

    /// Get the index completion timestamp.
    #[allow(dead_code)]
    pub async fn get_index_completed_at(&self) -> Result<Option<i64>, VectorStoreError> {
        self.get_index_timestamp("index_completed_at").await
    }

    /// Record the index embedding dimension.
    pub async fn set_index_dimension(&self, dimension: usize) -> Result<(), VectorStoreError> {
        self.set_index_value("dimension", Some(dimension.to_string()))
            .await
    }

    /// Get the stored index embedding dimension.
    pub async fn get_index_dimension(&self) -> Result<Option<usize>, VectorStoreError> {
        match self.get_index_value("dimension").await? {
            Some(value) => value.parse::<usize>().map(Some).map_err(|_| {
                VectorStoreError::DatabaseError("Invalid index dimension value".to_string())
            }),
            None => Ok(None),
        }
    }

    /// Remove all index data and metadata state.
    pub async fn clear_index_data(&self) -> Result<(), VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute_batch(
                r#"
                DELETE FROM entries;
                DELETE FROM hnsw_ids;
                DELETE FROM file_hashes;
                DELETE FROM enrichments;
                DELETE FROM index_state;
                "#,
            )
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    async fn set_index_value(
        &self,
        key: &str,
        value: Option<String>,
    ) -> Result<(), VectorStoreError> {
        let key = key.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            match value {
                Some(value) => conn.execute(
                    "INSERT OR REPLACE INTO index_state (key, value) VALUES (?1, ?2)",
                    params![key, value],
                ),
                None => conn.execute("DELETE FROM index_state WHERE key = ?", params![key]),
            }
            .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    async fn get_index_value(&self, key: &str) -> Result<Option<String>, VectorStoreError> {
        let key = key.to_string();
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn.lock();
            let result: Result<String, _> = conn.query_row(
                "SELECT value FROM index_state WHERE key = ?",
                params![key],
                |row| row.get(0),
            );
            match result {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(VectorStoreError::DatabaseError(e.to_string())),
            }
        })
        .await
        .map_err(|e| VectorStoreError::DatabaseError(e.to_string()))?
    }

    async fn get_index_timestamp(&self, key: &str) -> Result<Option<i64>, VectorStoreError> {
        if let Some(value) = self.get_index_value(key).await? {
            if let Ok(timestamp) = value.parse::<i64>() {
                if timestamp > 0 {
                    return Ok(Some(timestamp));
                }
            }
        }
        Ok(None)
    }
}

fn escape_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '%' | '_' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}
