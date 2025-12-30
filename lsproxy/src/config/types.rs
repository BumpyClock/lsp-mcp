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
