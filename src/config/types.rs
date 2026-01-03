// ABOUTME: Configuration type definitions for the lsp-mcp server.
// ABOUTME: Contains OutputMode and OutputConfig for MCP tool response formatting.

use serde::Deserialize;

/// Output formatting mode for MCP tool responses.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Compact output format (default)
    Default,
    /// Verbose output format with all details
    Verbose,
}

impl Default for OutputMode {
    fn default() -> Self {
        OutputMode::Default
    }
}

/// Configuration for MCP output formatting.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OutputConfig {
    /// Output mode (default or verbose)
    #[serde(default)]
    pub mode: OutputMode,
}

/// Optional output configuration for config file merging.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct OutputConfigFile {
    /// Output mode (default or verbose)
    pub mode: Option<OutputMode>,
}

impl OutputConfigFile {
    pub(crate) fn merge(self, project: Self) -> Self {
        OutputConfigFile {
            mode: project.mode.or(self.mode),
        }
    }

    pub(crate) fn resolve(self) -> OutputConfig {
        OutputConfig {
            mode: self.mode.unwrap_or_default(),
        }
    }
}

/// Log level for debug output.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DebugLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for DebugLogLevel {
    fn default() -> Self {
        DebugLogLevel::Debug
    }
}

/// Debug configuration for logging.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DebugConfig {
    /// Enable debug logging to file
    #[serde(default)]
    pub enabled: bool,

    /// Log level (default: debug)
    #[serde(default)]
    pub log_level: DebugLogLevel,

    /// Custom log directory (default: .lsp-mcp/logs)
    #[serde(default)]
    pub log_dir: Option<String>,
}

/// Optional debug configuration for config file merging.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct DebugConfigFile {
    /// Enable debug logging to file
    pub enabled: Option<bool>,
    /// Log level (default: debug)
    pub log_level: Option<DebugLogLevel>,
    /// Custom log directory (default: .lsp-mcp/logs)
    pub log_dir: Option<String>,
}

impl DebugConfigFile {
    pub(crate) fn merge(self, project: Self) -> Self {
        DebugConfigFile {
            enabled: project.enabled.or(self.enabled),
            log_level: project.log_level.or(self.log_level),
            log_dir: project.log_dir.or(self.log_dir),
        }
    }

    pub(crate) fn resolve(self) -> DebugConfig {
        DebugConfig {
            enabled: self.enabled.unwrap_or_default(),
            log_level: self.log_level.unwrap_or_default(),
            log_dir: self.log_dir,
        }
    }
}
