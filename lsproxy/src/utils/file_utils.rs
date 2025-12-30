use crate::{
    api_types::{get_mount_dir, SupportedLanguages},
    lsp::{manager::LspManagerError, registry::LanguageMetadata},
};
use ignore::WalkBuilder;
use log::{debug, error, warn};
use std::path::{Path, PathBuf};
use url::Url;

pub fn search_files(
    path: &std::path::Path,
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
    respect_gitignore: bool,
) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    let walk = build_walk(path, exclude_patterns, respect_gitignore);
    // println!("Searching for {:?}",include_patterns);
    for result in walk {
        match result {
            Ok(entry) => {
                let path = entry.path();
                if !include_patterns.iter().any(|pattern| {
                    glob::Pattern::new(pattern)
                        .map(|p| p.matches_path(path))
                        .unwrap_or(false)
                }) {
                    continue;
                }
                if path.is_file() {
                    files.push(path.to_path_buf());
                }
            }
            Err(err) => error!("Error: {}", err),
        }
    }

    Ok(files)
}

pub fn search_directories(
    root_path: &std::path::Path,
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
) -> std::io::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    let walk = build_walk(root_path, exclude_patterns, true);
    for result in walk {
        match result {
            Ok(entry) => {
                let path = entry.path().to_path_buf();
                if !include_patterns.iter().any(|pattern| {
                    glob::Pattern::new(pattern)
                        .map(|p| p.matches_path(&path))
                        .unwrap_or(false)
                }) {
                    continue;
                }
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    dirs.push(path.parent().unwrap().to_path_buf());
                }
            }
            Err(err) => error!("Error: {}", err),
        }
    }
    Ok(dirs
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect())
}

fn build_walk(path: &Path, exclude_patterns: Vec<String>, respect_gitignore: bool) -> ignore::Walk {
    let walk = WalkBuilder::new(path)
        .git_ignore(respect_gitignore)
        .filter_entry(move |entry| {
            let path = entry.path();
            let is_excluded = exclude_patterns.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches_path(path))
                    .unwrap_or(false)
            });
            !is_excluded
        })
        .build();
    walk
}

pub fn uri_to_relative_path_string(uri: &Url) -> String {
    let path = uri.to_file_path().unwrap_or_else(|e| {
        warn!("Failed to convert URI to file path: {:?}", e);
        PathBuf::from(uri.path())
    });

    absolute_path_to_relative_path_string(&path)
}

pub fn absolute_path_to_relative_path_string(path: &PathBuf) -> String {
    let mount_dir = get_mount_dir();
    path.strip_prefix(mount_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|e| {
            debug!("Failed to strip prefix from {:?}: {:?}", path, e);
            path.to_string_lossy().into_owned()
        })
}

pub fn detect_language(file_path: &str) -> Result<SupportedLanguages, LspManagerError> {
    let path = PathBuf::from(file_path);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| LspManagerError::UnsupportedFileType(file_path.to_string()))?;

    LanguageMetadata::from_extension(extension)
        .map(|m| m.id)
        .ok_or_else(|| LspManagerError::UnsupportedFileType(file_path.to_string()))
}

pub fn detect_language_string(file_path: &str) -> Result<String, LspManagerError> {
    let path = PathBuf::from(file_path);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| LspManagerError::UnsupportedFileType(file_path.to_string()))?;

    // Handle TypeScript/JavaScript variants specially for LSP language IDs
    match extension {
        "ts" => Ok("typescript".to_string()),
        "tsx" => Ok("typescriptreact".to_string()),
        "js" => Ok("javascript".to_string()),
        "jsx" => Ok("javascriptreact".to_string()),
        "c" => Ok("c".to_string()),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Ok("cpp".to_string()),
        "h" => Ok("c".to_string()), // Headers are typically C
        _ => LanguageMetadata::from_extension(extension)
            .map(|m| m.name.to_lowercase().replace("/", ""))
            .ok_or_else(|| LspManagerError::UnsupportedFileType(file_path.to_string())),
    }
}
