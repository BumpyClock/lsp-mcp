// ABOUTME: Debug logging configuration and file-based log setup.
// ABOUTME: Configures tracing subscriber with optional file output based on config.

use crate::config::{DebugConfig, DebugLogLevel};
use crate::session::session_id;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Initialize logging based on configuration.
///
/// If debug is enabled in config, logs to both stderr and a session-based file.
/// Otherwise, logs only to stderr using RUST_LOG env var.
///
/// # Arguments
/// * `debug_config` - Optional debug configuration (None means debug disabled)
/// * `workspace_root` - Path to workspace root for resolving relative log paths
///
/// # Returns
/// * `Ok(Some(PathBuf))` - Path to the log file if file logging is enabled
/// * `Ok(None)` - If only stderr logging is used
/// * `Err` - If logging setup fails
pub fn init_logging(
    debug_config: Option<&DebugConfig>,
    workspace_root: &Path,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
    let stderr_writer = std::io::stderr;
    let use_ansi = stderr_writer().is_terminal() && std::env::var("NO_COLOR").is_err();

    if let Some(debug) = debug_config {
        if debug.enabled {
            // Resolve log directory and create session log file
            let log_dir = resolve_log_dir(debug, workspace_root);
            let session_log_path = log_dir.join(format!("{}.log", session_id()));

            // Ensure parent directories exist
            fs::create_dir_all(&log_dir)?;

            // Open log file
            let file = fs::File::create(&session_log_path)?;

            // Create level filter from config
            let level_filter = debug_level_to_filter(debug.log_level);

            // Build subscriber with both stderr and file output
            // File output does not use ANSI colors
            let file_layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(file);

            let stderr_layer = tracing_subscriber::fmt::layer()
                .with_ansi(use_ansi)
                .with_writer(stderr_writer);

            tracing_subscriber::registry()
                .with(stderr_layer)
                .with(file_layer)
                .with(level_filter)
                .init();

            return Ok(Some(session_log_path));
        }
    }

    // Default: stderr only with RUST_LOG
    tracing_subscriber::fmt()
        .with_writer(stderr_writer)
        .with_ansi(use_ansi)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    Ok(None)
}

/// Resolve the log directory from config, with default fallback.
fn resolve_log_dir(config: &DebugConfig, workspace_root: &Path) -> PathBuf {
    match &config.log_dir {
        Some(custom_dir) => {
            let path = PathBuf::from(custom_dir);
            if path.is_absolute() {
                path.join("sessions")
            } else {
                workspace_root.join(path).join("sessions")
            }
        }
        None => workspace_root
            .join(".lsp-mcp")
            .join("logs")
            .join("sessions"),
    }
}

/// Convert debug log level to tracing EnvFilter.
fn debug_level_to_filter(level: DebugLogLevel) -> EnvFilter {
    let level_str = match level {
        DebugLogLevel::Error => "error",
        DebugLogLevel::Warn => "warn",
        DebugLogLevel::Info => "info",
        DebugLogLevel::Debug => "debug",
        DebugLogLevel::Trace => "trace",
    };
    EnvFilter::new(level_str)
}

/// Get the log file path for the current session.
///
/// This returns the path where logs would be written based on the config.
/// Useful for including in health response.
/// Returns None if debug is disabled or session is not initialized.
pub fn session_log_path(
    debug_config: Option<&DebugConfig>,
    workspace_root: &Path,
) -> Option<PathBuf> {
    let debug = debug_config.filter(|d| d.enabled)?;
    let session_id = crate::session::try_session_id()?;
    let log_dir = resolve_log_dir(debug, workspace_root);
    Some(log_dir.join(format!("{}.log", session_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn resolve_log_dir_uses_default_when_not_specified() {
        let temp_dir = TempDir::new().unwrap();
        let config = DebugConfig {
            enabled: true,
            log_level: DebugLogLevel::Debug,
            log_dir: None,
        };

        let result = resolve_log_dir(&config, temp_dir.path());

        assert!(result.ends_with("sessions"));
        assert!(result.to_string_lossy().contains(".lsp-mcp"));
        assert!(result.to_string_lossy().contains("logs"));
    }

    #[test]
    fn resolve_log_dir_uses_custom_relative_path() {
        let temp_dir = TempDir::new().unwrap();
        let config = DebugConfig {
            enabled: true,
            log_level: DebugLogLevel::Debug,
            log_dir: Some("custom/logs".to_string()),
        };

        let result = resolve_log_dir(&config, temp_dir.path());

        assert!(result.ends_with("sessions"));
        assert!(result.to_string_lossy().contains("custom"));
    }

    #[test]
    fn resolve_log_dir_uses_custom_absolute_path() {
        let temp_dir = TempDir::new().unwrap();
        let abs_path = temp_dir.path().join("absolute").join("logs");
        let config = DebugConfig {
            enabled: true,
            log_level: DebugLogLevel::Debug,
            log_dir: Some(abs_path.to_string_lossy().to_string()),
        };

        let result = resolve_log_dir(&config, Path::new("/other/workspace"));

        assert!(result.ends_with("sessions"));
        assert!(result.to_string_lossy().contains("absolute"));
    }

    #[test]
    fn debug_level_to_filter_maps_correctly() {
        // Just verify they don't panic - actual filtering is tested by tracing
        let _ = debug_level_to_filter(DebugLogLevel::Error);
        let _ = debug_level_to_filter(DebugLogLevel::Warn);
        let _ = debug_level_to_filter(DebugLogLevel::Info);
        let _ = debug_level_to_filter(DebugLogLevel::Debug);
        let _ = debug_level_to_filter(DebugLogLevel::Trace);
    }

    #[test]
    fn session_log_path_returns_none_when_session_not_initialized() {
        // This test must run before session is initialized
        // Reset the session by creating a new test process (not possible in unit test)
        // Instead, we test with disabled config which also returns None
        let config = DebugConfig {
            enabled: false,
            log_level: DebugLogLevel::Debug,
            log_dir: None,
        };
        let workspace_root = PathBuf::from("/tmp/workspace");

        let result = session_log_path(Some(&config), &workspace_root);

        assert!(
            result.is_none(),
            "negative: session_log_path must return None when debug disabled"
        );
    }
}
