// ABOUTME: Workspace documents handler for caching and managing workspace file contents
// ABOUTME: Provides eager file caching, pattern-based filtering, and file change notifications

use super::range::extract_range;
use crate::utils::file_utils::search_files;
use log::{debug, error, warn};
use lsp_types::Range;
use notify_debouncer_mini::DebouncedEvent;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs::read,
    sync::{broadcast::Receiver, RwLock},
};
use url::Url;

#[derive(Clone, PartialEq)]
pub enum DidOpenConfiguration {
    Lazy,
    None,
}

#[async_trait::async_trait]
pub trait WorkspaceDocuments: Send + Sync {
    async fn read_text_document(
        &self,
        full_file_path: &PathBuf,
        range: Option<Range>,
    ) -> Result<String, Box<dyn Error + Send + Sync>>;
    async fn list_files(&self) -> Vec<PathBuf>;
    fn get_did_open_configuration(&self) -> DidOpenConfiguration;
    fn is_did_open_document(&self, file_path: &str) -> bool;
    fn add_did_open_document(&mut self, file_path: &str);
}

pub struct WorkspaceDocumentsHandler {
    cache: Arc<RwLock<HashMap<PathBuf, Option<String>>>>,
    patterns: Arc<RwLock<(Vec<String>, Vec<String>)>>,
    root_path: PathBuf,
    did_open_text_documents: HashSet<Url>,
    did_open_configuration: DidOpenConfiguration,
}

impl WorkspaceDocumentsHandler {
    pub fn new(
        root_path: &Path,
        include_patterns: Vec<String>,
        exclude_patterns: Vec<String>,
        watch_events_rx: Receiver<DebouncedEvent>,
        did_open_configuration: DidOpenConfiguration,
    ) -> Self {
        let root_path = root_path.to_path_buf();

        // Eagerly populate cache with workspace files before wrapping in Arc<RwLock>
        let mut initial_cache = HashMap::new();
        let initial_files =
            search_files(&root_path, include_patterns.clone(), exclude_patterns.clone(), true)
                .unwrap_or_else(|err| {
                    error!("Error searching files during init: {}", err);
                    Vec::new()
                });
        for file_path in initial_files {
            initial_cache.insert(file_path, None);
        }
        debug!("Eagerly populated cache with {} files", initial_cache.len());

        let cache = Arc::new(RwLock::new(initial_cache));
        let patterns = Arc::new(RwLock::new((include_patterns, exclude_patterns)));
        let cache_clone = Arc::clone(&cache);
        let patterns_clone = Arc::clone(&patterns);

        tokio::spawn(async move {
            let mut watch_events_rx = watch_events_rx;
            while let Ok(event) = watch_events_rx.recv().await {
                debug!("Received event: {:?}", event);
                if WorkspaceDocumentsHandler::matches_patterns(&event.path, &patterns_clone).await {
                    cache_clone.write().await.clear();
                    debug!("Cache cleared for {:?}", event.path);
                }
            }
        });

        Self {
            cache,
            patterns,
            root_path,
            did_open_text_documents: HashSet::new(),
            did_open_configuration,
        }
    }

    async fn matches_patterns(
        path: &Path,
        patterns: &Arc<RwLock<(Vec<String>, Vec<String>)>>,
    ) -> bool {
        let patterns_guard = patterns.read().await;
        let (include, exclude) = &*patterns_guard;
        let path_str = path.to_string_lossy();

        include
            .iter()
            .any(|pat| glob::Pattern::new(pat).unwrap().matches(&path_str))
            && !exclude
                .iter()
                .any(|pat| glob::Pattern::new(pat).unwrap().matches(&path_str))
    }

    async fn get_content(
        &self,
        full_file_path: &PathBuf,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut cache = self.cache.write().await;
        match cache.get(full_file_path) {
            Some(Some(content)) => Ok(content.clone()),
            _ => {
                let bytes = read(full_file_path).await?;

                if String::from_utf8(bytes.clone()).is_err() {
                    warn!("File {:?} contains invalid UTF-8", full_file_path);
                }

                let content = String::from_utf8_lossy(&bytes).into_owned();
                cache.insert(full_file_path.clone(), Some(content.clone()));
                Ok(content)
            }
        }
    }
}

#[async_trait::async_trait]
impl WorkspaceDocuments for WorkspaceDocumentsHandler {
    async fn read_text_document(
        &self,
        full_file_path: &PathBuf,
        range: Option<Range>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        let content = self.get_content(full_file_path).await?;
        match range {
            Some(range) => extract_range(&content, range),
            None => Ok(content),
        }
    }

    async fn list_files(&self) -> Vec<PathBuf> {
        let cache_read = self.cache.read().await;
        if cache_read.is_empty() {
            drop(cache_read);
            let (include_patterns, exclude_patterns) = self.patterns.read().await.clone();
            let file_paths =
                search_files(&self.root_path, include_patterns, exclude_patterns, true)
                    .unwrap_or_else(|err| {
                        error!("Error searching files: {}", err);
                        Vec::new()
                    });
            let mut cache_write = self.cache.write().await;
            for file_path in file_paths {
                cache_write.insert(file_path, None);
            }
            cache_write.keys().cloned().collect()
        } else {
            cache_read.keys().cloned().collect()
        }
    }

    fn get_did_open_configuration(&self) -> DidOpenConfiguration {
        self.did_open_configuration.clone()
    }

    fn is_did_open_document(&self, file_path: &str) -> bool {
        self.did_open_text_documents
            .contains(&Url::from_file_path(file_path).unwrap())
    }

    fn add_did_open_document(&mut self, file_path: &str) {
        self.did_open_text_documents
            .insert(Url::from_file_path(file_path).unwrap());
    }
}
