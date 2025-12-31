// ABOUTME: External file handling utilities for reading files outside workspace.
// ABOUTME: Provides graceful fallback for type definitions and external dependencies.

use crate::api_types::get_mount_dir;
use std::path::{Path, PathBuf};

/// Determines if a path is external to the workspace.
///
/// A path is considered external if:
/// - It's an absolute path outside the workspace mount directory
/// - It's a relative path but the file doesn't exist in the workspace (will be handled by caller)
///
/// Note: This function checks path prefixes, not file existence.
pub fn is_external_path(path: &str) -> bool {
    let path_obj = Path::new(path);
    if path_obj.is_absolute() {
        let mount_dir = get_mount_dir();
        !path.starts_with(mount_dir.to_str().unwrap_or(""))
    } else {
        false
    }
}

/// Resolves a path to an absolute path for reading.
///
/// - For absolute paths: returns as-is
/// - For relative paths: joins with mount_dir
pub fn resolve_file_path(path: &str) -> PathBuf {
    let path_buf = PathBuf::from(path);
    if path_buf.is_absolute() {
        path_buf
    } else {
        get_mount_dir().join(path)
    }
}

/// Reads file content directly from filesystem.
///
/// Works for both workspace and external files by resolving the path appropriately.
pub async fn read_file_content(path: &str) -> Result<String, std::io::Error> {
    let full_path = resolve_file_path(path);
    let bytes = tokio::fs::read(&full_path).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Reads a range of lines from a file.
///
/// Returns the content between start_line and end_line (0-indexed, inclusive).
/// If start_line is beyond the file, returns empty string.
/// If end_line exceeds file length, returns up to end of file.
pub async fn read_file_range(
    path: &str,
    start_line: u32,
    end_line: u32,
) -> Result<String, std::io::Error> {
    let content = read_file_content(path).await?;
    let lines: Vec<&str> = content.lines().collect();
    let start = start_line as usize;
    let end = std::cmp::min(end_line as usize + 1, lines.len());

    if start >= lines.len() {
        return Ok(String::new());
    }

    Ok(lines[start..end].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{set_thread_local_mount_dir, unset_thread_local_mount_dir};
    use tempfile::TempDir;

    #[test]
    fn is_external_path_returns_true_for_absolute_path_outside_workspace() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let result = is_external_path("/etc/passwd");
        assert!(result, "Absolute path outside workspace should be external");

        unset_thread_local_mount_dir();
    }

    #[test]
    fn is_external_path_returns_false_for_absolute_path_inside_workspace() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let path_inside = temp.path().join("src/main.rs");
        let result = is_external_path(path_inside.to_str().unwrap());
        assert!(!result, "Absolute path inside workspace should not be external");

        unset_thread_local_mount_dir();
    }

    #[test]
    fn is_external_path_returns_false_for_relative_path() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let result = is_external_path("src/main.rs");
        assert!(!result, "Relative paths should not be considered external");

        unset_thread_local_mount_dir();
    }

    #[test]
    fn resolve_file_path_returns_absolute_path_unchanged() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let absolute = "/usr/lib/something.so";
        let result = resolve_file_path(absolute);
        assert_eq!(result, PathBuf::from(absolute), "Absolute path should be returned unchanged");

        unset_thread_local_mount_dir();
    }

    #[test]
    fn resolve_file_path_joins_relative_path_with_mount_dir() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let relative = "src/lib.rs";
        let result = resolve_file_path(relative);
        let expected = temp.path().join(relative);
        assert_eq!(result, expected, "Relative path should be joined with mount dir");

        unset_thread_local_mount_dir();
    }

    #[tokio::test]
    async fn read_file_content_reads_entire_file() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let file_path = temp.path().join("test.txt");
        let content = "line1\nline2\nline3";
        std::fs::write(&file_path, content).unwrap();

        let result = read_file_content(file_path.to_str().unwrap()).await;
        assert!(result.is_ok(), "Should successfully read file");
        assert_eq!(result.unwrap(), content, "Content should match");

        unset_thread_local_mount_dir();
    }

    #[tokio::test]
    async fn read_file_content_handles_non_ascii() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let file_path = temp.path().join("unicode.txt");
        let content = "日本語\n中文\n한글";
        std::fs::write(&file_path, content).unwrap();

        let result = read_file_content(file_path.to_str().unwrap()).await;
        assert!(result.is_ok(), "Should handle non-ASCII content");
        assert_eq!(result.unwrap(), content, "Unicode content should match");

        unset_thread_local_mount_dir();
    }

    #[tokio::test]
    async fn read_file_content_returns_error_for_nonexistent_file() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let result = read_file_content("/nonexistent/path/file.txt").await;
        assert!(result.is_err(), "Should return error for nonexistent file");

        unset_thread_local_mount_dir();
    }

    #[tokio::test]
    async fn read_file_range_returns_specified_lines() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let file_path = temp.path().join("lines.txt");
        let content = "line0\nline1\nline2\nline3\nline4";
        std::fs::write(&file_path, content).unwrap();

        let result = read_file_range(file_path.to_str().unwrap(), 1, 3).await;
        assert!(result.is_ok(), "Should successfully read line range");
        assert_eq!(result.unwrap(), "line1\nline2\nline3", "Should return lines 1-3");

        unset_thread_local_mount_dir();
    }

    #[tokio::test]
    async fn read_file_range_returns_empty_when_start_beyond_file() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let file_path = temp.path().join("short.txt");
        std::fs::write(&file_path, "only one line").unwrap();

        let result = read_file_range(file_path.to_str().unwrap(), 10, 20).await;
        assert!(result.is_ok(), "Should not error when start is beyond file");
        assert_eq!(result.unwrap(), "", "Should return empty string");

        unset_thread_local_mount_dir();
    }

    #[tokio::test]
    async fn read_file_range_clamps_end_to_file_length() {
        let temp = TempDir::new().unwrap();
        set_thread_local_mount_dir(temp.path());

        let file_path = temp.path().join("short2.txt");
        let content = "line0\nline1\nline2";
        std::fs::write(&file_path, content).unwrap();

        let result = read_file_range(file_path.to_str().unwrap(), 1, 100).await;
        assert!(result.is_ok(), "Should handle end beyond file length");
        assert_eq!(result.unwrap(), "line1\nline2", "Should clamp to actual file length");

        unset_thread_local_mount_dir();
    }
}
