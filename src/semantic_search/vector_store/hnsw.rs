// ABOUTME: HNSW vector index implementation using hnsw_rs.
// ABOUTME: Provides efficient approximate nearest neighbor search.

use super::metadata::MetadataStore;
use super::types::{
    EnrichmentData, IndexEntry, IndexState, IndexStats, SearchOptions, SearchResult,
};
use super::{VectorStore, VectorStoreError};
use async_trait::async_trait;
use hnsw_rs::api::AnnT;
use hnsw_rs::hnswio::HnswIo;
use hnsw_rs::prelude::*;
use log::warn;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const INDEX_BASENAME: &str = "vectors";

fn normalize_path(value: &str) -> String {
    let mut normalized = value.replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    if normalized == "." {
        return String::new();
    }
    normalized.trim_start_matches('/').to_string()
}

fn path_matches_prefix(entry_path: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    if prefix.ends_with('/') {
        return entry_path.starts_with(prefix);
    }
    entry_path == prefix || entry_path.starts_with(&format!("{}/", prefix))
}

fn graph_path(index_dir: &Path, basename: &str) -> PathBuf {
    index_dir.join(format!("{}.hnsw.graph", basename))
}

fn data_path(index_dir: &Path, basename: &str) -> PathBuf {
    index_dir.join(format!("{}.hnsw.data", basename))
}

fn index_files_exist(index_dir: &Path, basename: &str) -> bool {
    graph_path(index_dir, basename).exists() && data_path(index_dir, basename).exists()
}

fn load_index(
    index_dir: &Path,
    basename: &str,
) -> Result<Hnsw<'static, f32, DistCosine>, VectorStoreError> {
    let load_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let loader = Box::new(HnswIo::new(index_dir, basename));
        let loader = Box::leak(loader);
        loader.load_hnsw::<f32, DistCosine>()
    }));

    match load_result {
        Ok(Ok(index)) => Ok(index),
        Ok(Err(err)) => Err(VectorStoreError::IndexError(err.to_string())),
        Err(_) => Err(VectorStoreError::IndexError(
            "HNSW load panicked".to_string(),
        )),
    }
}

/// HNSW-based vector store with SQLite metadata.
pub struct HnswVectorStore {
    /// HNSW index for vector search
    index: Arc<RwLock<Hnsw<'static, f32, DistCosine>>>,
    /// SQLite metadata store
    metadata: MetadataStore,
    /// Embedding dimension
    dimension: usize,
    /// Directory for index files
    index_dir: PathBuf,
    /// Current basename for index files
    index_basename: Arc<RwLock<String>>,
    /// Mapping from segment_hash to HNSW data_id
    hash_to_id: Arc<RwLock<HashMap<String, usize>>>,
    /// Next available data_id
    next_id: Arc<RwLock<usize>>,
}

impl HnswVectorStore {
    /// Create or open a vector store at the given directory.
    pub async fn new(index_dir: &Path, dimension: usize) -> Result<Self, VectorStoreError> {
        tokio::fs::create_dir_all(index_dir)
            .await
            .map_err(VectorStoreError::IoError)?;

        let metadata_path = index_dir.join("metadata.db");

        let metadata = MetadataStore::new(&metadata_path).await?;

        let mut index_basename = metadata
            .get_index_basename()
            .await?
            .unwrap_or_else(|| INDEX_BASENAME.to_string());
        if index_basename.trim().is_empty() {
            index_basename = INDEX_BASENAME.to_string();
        }
        if !index_files_exist(index_dir, &index_basename) && index_basename != INDEX_BASENAME {
            if index_files_exist(index_dir, INDEX_BASENAME) {
                index_basename = INDEX_BASENAME.to_string();
            }
        }
        metadata.set_index_basename(&index_basename).await?;

        let mappings = metadata.load_hnsw_ids().await?;
        let next_id = mappings.values().max().map(|v| v + 1).unwrap_or(0);
        let entry_count = metadata.count_entries().await?;
        let mapping_missing = entry_count > 0 && mappings.is_empty();
        if mapping_missing {
            metadata.set_state(IndexState::Empty).await?;
        }

        let index = if index_files_exist(index_dir, &index_basename) && !mapping_missing {
            match load_index(index_dir, &index_basename) {
                Ok(index) => index,
                Err(err) => {
                    warn!("Failed to load HNSW index: {}", err);
                    metadata.set_state(IndexState::Empty).await?;
                    Hnsw::<f32, DistCosine>::new(16, 50000, 16, 200, DistCosine {})
                }
            }
        } else {
            Hnsw::<f32, DistCosine>::new(16, 50000, 16, 200, DistCosine {})
        };

        Ok(Self {
            index: Arc::new(RwLock::new(index)),
            metadata,
            dimension,
            index_dir: index_dir.to_path_buf(),
            index_basename: Arc::new(RwLock::new(index_basename)),
            hash_to_id: Arc::new(RwLock::new(mappings)),
            next_id: Arc::new(RwLock::new(next_id)),
        })
    }

    fn validate_dimension(&self, embedding: &[f32]) -> Result<(), VectorStoreError> {
        if embedding.len() != self.dimension {
            return Err(VectorStoreError::DimensionMismatch {
                expected: self.dimension,
                got: embedding.len(),
            });
        }
        Ok(())
    }

    fn get_or_create_id(&self, segment_hash: &str) -> usize {
        let mut hash_to_id = self.hash_to_id.write();
        if let Some(&id) = hash_to_id.get(segment_hash) {
            return id;
        }

        let mut next_id = self.next_id.write();
        let id = *next_id;
        *next_id += 1;
        hash_to_id.insert(segment_hash.to_string(), id);
        id
    }

    pub async fn get_file_hash(&self, file_path: &str) -> Result<Option<String>, VectorStoreError> {
        self.metadata.get_file_hash(file_path).await
    }

    pub async fn upsert_file_hash(
        &self,
        file_path: &str,
        content_hash: &str,
        updated_at: i64,
    ) -> Result<(), VectorStoreError> {
        self.metadata
            .upsert_file_hash(file_path, content_hash, updated_at)
            .await
    }

    pub async fn delete_file_hash(&self, file_path: &str) -> Result<(), VectorStoreError> {
        self.metadata.delete_file_hash(file_path).await
    }

    pub async fn get_enrichment(
        &self,
        segment_hash: &str,
    ) -> Result<Option<EnrichmentData>, VectorStoreError> {
        self.metadata.get_enrichment(segment_hash).await
    }

    pub async fn upsert_enrichment(
        &self,
        segment_hash: &str,
        summary: &str,
        tags: &[String],
        updated_at: i64,
    ) -> Result<(), VectorStoreError> {
        self.metadata
            .upsert_enrichment(segment_hash, summary, tags, updated_at)
            .await
    }

    pub async fn set_index_started_at(&self, timestamp: i64) -> Result<(), VectorStoreError> {
        self.metadata.set_index_started_at(timestamp).await
    }

    pub async fn set_index_completed_at(&self, timestamp: i64) -> Result<(), VectorStoreError> {
        self.metadata.set_index_completed_at(timestamp).await
    }

    pub async fn clear_index_completed_at(&self) -> Result<(), VectorStoreError> {
        self.metadata.clear_index_completed_at().await
    }

    pub async fn get_index_started_at(&self) -> Result<Option<i64>, VectorStoreError> {
        self.metadata.get_index_started_at().await
    }

    pub async fn get_index_completed_at(&self) -> Result<Option<i64>, VectorStoreError> {
        self.metadata.get_index_completed_at().await
    }

    pub async fn set_index_dimension(&self, dimension: usize) -> Result<(), VectorStoreError> {
        self.metadata.set_index_dimension(dimension).await
    }

    pub async fn get_index_dimension(&self) -> Result<Option<usize>, VectorStoreError> {
        self.metadata.get_index_dimension().await
    }

    pub async fn files_missing_vectors(&self) -> Result<Vec<String>, VectorStoreError> {
        let nb_points = {
            let index = self.index.read();
            index.get_nb_point()
        };
        self.metadata
            .get_files_with_hnsw_id_at_or_above(nb_points)
            .await
    }

    pub async fn reset_index(&self) -> Result<(), VectorStoreError> {
        let basename = self.index_basename.read().clone();
        let index_dir = self.index_dir.clone();
        let default_basename = INDEX_BASENAME.to_string();

        let _ = tokio::fs::remove_file(graph_path(&index_dir, &basename)).await;
        let _ = tokio::fs::remove_file(data_path(&index_dir, &basename)).await;
        if basename != INDEX_BASENAME {
            let _ = tokio::fs::remove_file(graph_path(&index_dir, INDEX_BASENAME)).await;
            let _ = tokio::fs::remove_file(data_path(&index_dir, INDEX_BASENAME)).await;
        }

        {
            let mut index = self.index.write();
            *index = Hnsw::<f32, DistCosine>::new(16, 50000, 16, 200, DistCosine {});
        }

        {
            let mut hash_to_id = self.hash_to_id.write();
            hash_to_id.clear();
        }

        {
            let mut next_id = self.next_id.write();
            *next_id = 0;
        }

        {
            let mut current = self.index_basename.write();
            *current = default_basename.clone();
        }

        self.metadata.clear_index_data().await?;
        self.metadata.set_index_basename(&default_basename).await?;

        Ok(())
    }

    async fn get_hnsw_ids_for_prefix(&self, prefix: &str) -> Result<Vec<usize>, VectorStoreError> {
        self.metadata.get_hnsw_ids_by_path_prefix(prefix).await
    }
}

#[async_trait]
impl VectorStore for HnswVectorStore {
    async fn upsert(
        &self,
        entries: Vec<(IndexEntry, Vec<f32>)>,
    ) -> Result<usize, VectorStoreError> {
        if entries.is_empty() {
            return Ok(0);
        }

        // Validate dimensions
        for (_, embedding) in &entries {
            self.validate_dimension(embedding)?;
        }

        let mut count = 0;

        for (entry, embedding) in entries {
            // Get or create HNSW data_id
            let data_id = self.get_or_create_id(&entry.id);

            // Update metadata
            self.metadata.upsert_entry(&entry).await?;
            self.metadata.upsert_hnsw_id(&entry.id, data_id).await?;

            // Insert into HNSW index
            {
                let index = self.index.write();
                index.insert((&embedding, data_id));
            }

            count += 1;
        }

        Ok(count)
    }

    async fn delete(&self, ids: &[String]) -> Result<usize, VectorStoreError> {
        let mut count = 0;

        for id in ids {
            // Remove from metadata
            self.metadata.delete_entry(id).await?;

            // Remove from hash mapping
            {
                let mut hash_to_id = self.hash_to_id.write();
                hash_to_id.remove(id);
            }

            count += 1;
        }

        // Note: HNSW doesn't support true deletion, vectors remain until rebuild
        // This is acceptable for our use case

        Ok(count)
    }

    async fn delete_file(&self, file_path: &str) -> Result<usize, VectorStoreError> {
        let hashes = self.metadata.get_file_hashes(file_path).await?;
        self.delete(&hashes).await
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        options: SearchOptions,
    ) -> Result<Vec<SearchResult>, VectorStoreError> {
        self.validate_dimension(query_embedding)?;

        let limit = options.limit.max(1).min(1000);
        let ef_search = (limit * 2).max(100);

        let normalized_prefix = options
            .path_prefix
            .as_ref()
            .map(|prefix| normalize_path(prefix))
            .filter(|prefix| !prefix.is_empty());
        let filter_ids = match &normalized_prefix {
            Some(prefix) => {
                let ids = self.get_hnsw_ids_for_prefix(prefix).await?;
                if ids.is_empty() {
                    return Ok(Vec::new());
                }
                Some(ids)
            }
            None => None,
        };

        // Perform HNSW search
        let neighbors = {
            let index = self.index.read();
            if let Some(ref ids) = filter_ids {
                index.search_filter(query_embedding, limit, ef_search, Some(ids))
            } else {
                index.search(query_embedding, limit, ef_search)
            }
        };

        let mut results = Vec::with_capacity(neighbors.len());

        // Build reverse mapping from data_id to segment_hash
        let id_to_hash: HashMap<usize, String> = {
            let hash_to_id = self.hash_to_id.read();
            hash_to_id
                .iter()
                .map(|(hash, &id)| (id, hash.clone()))
                .collect()
        };

        for (rank, neighbor) in neighbors.into_iter().enumerate() {
            let data_id = neighbor.d_id;
            let score = 1.0 - neighbor.distance; // Convert distance to similarity

            // Skip low scores
            if let Some(min_score) = options.min_score {
                if score < min_score {
                    continue;
                }
            }

            // Look up segment_hash from data_id
            let segment_hash = match id_to_hash.get(&data_id) {
                Some(hash) => hash,
                None => continue, // Orphaned vector, skip
            };

            // Look up metadata by segment_hash
            if let Some(numeric_id) = self.metadata.get_numeric_id(segment_hash).await? {
                if let Some(entry) = self.metadata.get_by_numeric_id(numeric_id).await? {
                    // Apply path prefix filter
                    if let Some(ref prefix) = normalized_prefix {
                        let entry_path = normalize_path(&entry.file_path);
                        if !path_matches_prefix(&entry_path, prefix) {
                            continue;
                        }
                    }

                    // Apply symbol kind filter
                    if let Some(ref kinds) = options.symbol_kinds {
                        if let Some(ref entry_kind) = entry.symbol_kind {
                            if !kinds.contains(entry_kind) {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }

                    results.push(SearchResult {
                        entry,
                        score,
                        rank: (rank + 1) as u32,
                    });
                }
            }
        }

        // Re-rank after filtering
        for (i, result) in results.iter_mut().enumerate() {
            result.rank = (i + 1) as u32;
        }

        Ok(results)
    }

    async fn stats(&self) -> Result<IndexStats, VectorStoreError> {
        let chunk_count = self.metadata.count_entries().await?;
        let file_count = self.metadata.count_files().await?;

        let basename = self.index_basename.read().clone();
        let graph_size = tokio::fs::metadata(graph_path(&self.index_dir, &basename))
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let data_size = tokio::fs::metadata(data_path(&self.index_dir, &basename))
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let index_size_bytes = graph_size.saturating_add(data_size);

        let metadata_size_bytes = self.metadata.size_bytes().await?;
        let last_updated = self.metadata.last_updated().await?;

        Ok(IndexStats {
            chunk_count,
            file_count,
            dimension: self.dimension,
            index_size_bytes,
            metadata_size_bytes,
            last_updated,
        })
    }

    async fn contains(&self, segment_hash: &str) -> Result<bool, VectorStoreError> {
        self.metadata.exists(segment_hash).await
    }

    async fn get_file_hashes(&self, file_path: &str) -> Result<Vec<String>, VectorStoreError> {
        self.metadata.get_file_hashes(file_path).await
    }

    async fn flush(&self) -> Result<(), VectorStoreError> {
        let basename = self.index_basename.read().clone();
        let index_dir = self.index_dir.clone();
        let index = Arc::clone(&self.index);
        let dump_basename = basename.clone();
        let dump_result = tokio::task::spawn_blocking(move || {
            let index = index.read();
            index
                .file_dump(&index_dir, &dump_basename)
                .map_err(|err| err.to_string())
        })
        .await
        .map_err(|err| VectorStoreError::IndexError(err.to_string()))?;

        let new_basename = dump_result.map_err(VectorStoreError::IndexError)?;
        if new_basename != basename {
            {
                let mut current = self.index_basename.write();
                *current = new_basename.clone();
            }
            self.metadata.set_index_basename(&new_basename).await?;
            let _ = tokio::fs::remove_file(graph_path(&self.index_dir, &basename)).await;
            let _ = tokio::fs::remove_file(data_path(&self.index_dir, &basename)).await;
        }

        self.metadata.flush().await
    }

    async fn set_state(&self, state: IndexState) -> Result<(), VectorStoreError> {
        self.metadata.set_state(state).await
    }

    async fn get_state(&self) -> Result<IndexState, VectorStoreError> {
        self.metadata.get_state().await
    }
}
