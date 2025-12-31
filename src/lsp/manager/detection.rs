// ABOUTME: Language detection for workspace files to determine which LSP servers to start
// ABOUTME: Uses single directory walk with extension matching for efficiency

use crate::api_types::SupportedLanguages;
use crate::lsp::registry::LanguageMetadata;
use crate::utils::workspace_documents::DEFAULT_EXCLUDE_PATTERNS;
use ignore::WalkBuilder;
use log::debug;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Detects languages in the workspace using a single directory walk.
/// Builds an extension-to-language map and matches files during traversal.
pub fn detect_languages_in_workspace(root_path: &str) -> Vec<SupportedLanguages> {
    let mut ext_to_lang: HashMap<&str, SupportedLanguages> = HashMap::new();
    for metadata in LanguageMetadata::all() {
        for ext in metadata.extensions {
            ext_to_lang.insert(*ext, metadata.id);
        }
    }

    let total_languages = LanguageMetadata::all().count();
    let exclude_patterns: Vec<String> = DEFAULT_EXCLUDE_PATTERNS
        .iter()
        .map(|s| s.to_string())
        .collect();

    let mut detected: HashSet<SupportedLanguages> = HashSet::new();

    let walk = WalkBuilder::new(Path::new(root_path))
        .git_ignore(true)
        .filter_entry(move |entry| {
            let path = entry.path();
            !exclude_patterns.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches_path(path))
                    .unwrap_or(false)
            })
        })
        .build();

    for entry in walk.flatten() {
        if entry.path().is_file() {
            if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                if let Some(&lang) = ext_to_lang.get(ext) {
                    detected.insert(lang);
                    if detected.len() == total_languages {
                        break;
                    }
                }
            }
        }
    }

    let result: Vec<SupportedLanguages> = detected.into_iter().collect();
    debug!("Starting LSPs: {:?}", result);
    result
}
