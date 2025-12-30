use crate::{
    api_types::{get_mount_dir, SupportedLanguages},
    lsp::{manager::LspManagerError, registry::LanguageMetadata},
};
use ignore::WalkBuilder;
use log::{debug, error, warn};
use std::path::{Component, Path, PathBuf};
use url::Url;

/// Error returned when path normalization fails.
#[derive(Debug, Clone, PartialEq)]
pub enum PathNormalizationError {
    OutsideWorkspace { path: String, workspace: String },
}

impl std::fmt::Display for PathNormalizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutsideWorkspace { path, workspace } => {
                write!(f, "Path '{}' is outside workspace '{}'", path, workspace)
            }
        }
    }
}

impl std::error::Error for PathNormalizationError {}

/// Normalizes a file path to be relative to the workspace.
///
/// Accepts both absolute and relative paths:
/// - Relative paths: validated to not escape workspace via `..`
/// - Absolute paths within workspace: converted to relative
/// - Paths outside workspace: returns error
pub fn normalize_path(path: &str) -> Result<String, PathNormalizationError> {
    let mount_dir = get_mount_dir();
    let input_path = PathBuf::from(path);

    // Handle empty or current directory
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "." {
        return Ok(String::new());
    }

    // Compute full path
    let full_path = if input_path.is_absolute() {
        input_path
    } else {
        mount_dir.join(&input_path)
    };

    // Lexically normalize to resolve `.` and `..`
    let normalized = lexically_normalize(&full_path);

    // Validate path is within workspace and convert to relative
    normalized
        .strip_prefix(&mount_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|_| PathNormalizationError::OutsideWorkspace {
            path: path.to_string(),
            workspace: mount_dir.to_string_lossy().into_owned(),
        })
}

/// Lexically normalizes a path by resolving `.` and `..` components without filesystem access.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            _ => result.push(component),
        }
    }
    result
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{set_thread_local_mount_dir, unset_thread_local_mount_dir};
    use tempfile::TempDir;

    #[test]
    fn test_normalize_path_relative_unchanged() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let result = normalize_path("src/main.rs");
        assert_eq!(result.unwrap(), "src/main.rs");

        unset_thread_local_mount_dir();
    }

    #[test]
    fn test_normalize_path_absolute_within_workspace() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let absolute = temp.path().join("src/lib.rs");
        let result = normalize_path(absolute.to_str().unwrap());
        assert_eq!(result.unwrap(), "src/lib.rs");

        unset_thread_local_mount_dir();
    }

    #[test]
    fn test_normalize_path_outside_workspace_returns_error() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let result = normalize_path("/etc/passwd");
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, PathNormalizationError::OutsideWorkspace { .. }));

        unset_thread_local_mount_dir();
    }

    #[test]
    fn test_normalize_path_escaping_via_dotdot_returns_error() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let result = normalize_path("../../etc/passwd");
        assert!(result.is_err());

        unset_thread_local_mount_dir();
    }

    #[test]
    fn test_normalize_path_with_dot_components() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let result = normalize_path("src/../src/main.rs");
        assert_eq!(result.unwrap(), "src/main.rs");

        unset_thread_local_mount_dir();
    }

    #[test]
    fn test_normalize_path_empty_string() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let result = normalize_path("");
        assert_eq!(result.unwrap(), "");

        unset_thread_local_mount_dir();
    }

    #[test]
    fn test_normalize_path_current_dir() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let result = normalize_path(".");
        assert_eq!(result.unwrap(), "");

        unset_thread_local_mount_dir();
    }
}
