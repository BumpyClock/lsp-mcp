// ABOUTME: Thread-safe storage for LSP diagnostics received via publishDiagnostics notifications
// ABOUTME: Provides file-keyed storage with concurrent read/write access

use lsp_types::{Diagnostic, Url};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Thread-safe storage for diagnostics received via publishDiagnostics notifications
#[derive(Clone)]
pub struct DiagnosticsStore {
    diagnostics: Arc<RwLock<HashMap<Url, Vec<Diagnostic>>>>,
}

impl DiagnosticsStore {
    pub fn new() -> Self {
        Self {
            diagnostics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update diagnostics for a file (replaces existing)
    pub async fn update(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        let mut store = self.diagnostics.write().await;
        if diagnostics.is_empty() {
            store.remove(&uri);
        } else {
            store.insert(uri, diagnostics);
        }
    }

    /// Get diagnostics for a specific file
    pub async fn get(&self, uri: &Url) -> Option<Vec<Diagnostic>> {
        self.diagnostics.read().await.get(uri).cloned()
    }

    /// Get all diagnostics (for workspace-wide query)
    pub async fn get_all(&self) -> HashMap<Url, Vec<Diagnostic>> {
        self.diagnostics.read().await.clone()
    }

    /// Clear all diagnostics
    pub async fn clear(&self) {
        self.diagnostics.write().await.clear();
    }
}

impl Default for DiagnosticsStore {
    fn default() -> Self {
        Self::new()
    }
}
