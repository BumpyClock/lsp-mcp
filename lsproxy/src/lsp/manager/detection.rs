// ABOUTME: Language detection for workspace files to determine which LSP servers to start
// ABOUTME: Scans workspace directories for files matching language-specific patterns

use crate::api_types::SupportedLanguages;
use crate::lsp::registry::LanguageMetadata;
use crate::utils::file_utils::search_files;
use crate::utils::workspace_documents::DEFAULT_EXCLUDE_PATTERNS;
use log::{debug, warn};
use std::path::Path;

/// Detects the languages in the workspace by searching for files that match the language server's file patterns
pub fn detect_languages_in_workspace(root_path: &str) -> Vec<SupportedLanguages> {
    let mut detected_languages = Vec::new();

    for metadata in LanguageMetadata::all() {
        let patterns: Vec<String> = metadata
            .file_patterns
            .iter()
            .map(|&s| s.to_string())
            .collect();
        let exclude_patterns: Vec<String> = DEFAULT_EXCLUDE_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect();

        let files_found = search_files(Path::new(root_path), patterns, exclude_patterns, true)
            .map_err(|e| warn!("Error searching files for {:?}: {}", metadata.id, e))
            .unwrap_or_default();

        if !files_found.is_empty() {
            detected_languages.push(metadata.id);
        }
    }

    debug!("Starting LSPs: {:?}", detected_languages);
    detected_languages
}
