// ABOUTME: Configuration loaders for the lsp-mcp server.
// ABOUTME: Loads and merges global and project-level .lsp-mcp.json config files.

use super::{DebugConfig, LspMcpConfig, OutputConfig, OutputMode, SemanticSearchConfig, ToolsConfig};
use log::info;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// Internal config structure for deserialization.
#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct LspMcpConfigFile {
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub binaries: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub output: Option<OutputConfig>,
    #[serde(default)]
    pub debug: Option<DebugConfig>,
    #[serde(default)]
    pub semantic_search: Option<SemanticSearchConfig>,
}

impl LspMcpConfig {
    /// Load config from a specific path
    fn load_from_path(path: &Path) -> Option<LspMcpConfigFile> {
        if path.exists() {
            let content = std::fs::read_to_string(path).ok()?;
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    }

    /// Load global config from $HOME/.lsp-mcp/.lsp-mcp.json
    pub fn load_global() -> Option<Self> {
        let home = dirs::home_dir()?;
        let global_config_path = home.join(".lsp-mcp").join(".lsp-mcp.json");
        Self::load_from_path(&global_config_path).map(|f| f.into())
    }

    /// Load project config from workspace root (.lsp-mcp.json)
    pub fn load_project(workspace_root: &Path) -> Option<Self> {
        let config_path = workspace_root.join(".lsp-mcp.json");
        Self::load_from_path(&config_path).map(|f| {
            let mut config: LspMcpConfig = f.into();
            config.project_config_present = true;
            config
        })
    }

    /// Load config from workspace root (legacy method, prefer load_merged)
    pub fn load(workspace_root: &Path) -> Option<Self> {
        Self::load_project(workspace_root)
    }

    /// Load merged config: global config with project config overrides
    ///
    /// Returns default config if neither global nor project config exists.
    pub fn load_merged(workspace_root: &Path) -> Self {
        let global = Self::load_global();
        let project_file = Self::load_from_path(&workspace_root.join(".lsp-mcp.json"));
        let project_present = project_file.is_some();
        let project = project_file.map(|f| {
            let mut config: LspMcpConfig = f.into();
            config.project_config_present = true;
            config
        });

        let mut config = match (global, project) {
            (None, None) => {
                info!("No config files found, using defaults");
                Self::default()
            }
            (Some(g), None) => {
                info!("Loaded global config from ~/.lsp-mcp/.lsp-mcp.json");
                g
            }
            (None, Some(p)) => {
                info!("Loaded project config from .lsp-mcp.json");
                p
            }
            (Some(g), Some(p)) => {
                info!("Merged global and project configs");
                g.merge(p)
            }
        };

        config.project_config_present = project_present;
        config
    }

    /// Merge project config over this (global) config
    pub(crate) fn merge(self, project: Self) -> Self {
        LspMcpConfig {
            languages: if project.languages.is_empty() {
                self.languages
            } else {
                project.languages
            },
            binaries: {
                let mut merged = self.binaries;
                merged.extend(project.binaries);
                merged
            },
            tools: self.tools.merge(project.tools),
            output: project.output.or(self.output),
            debug: project.debug.or(self.debug),
            semantic_search: project.semantic_search.or(self.semantic_search),
            project_config_present: self.project_config_present || project.project_config_present,
        }
    }

    pub fn get_binary(&self, language: &str) -> Option<&String> {
        self.binaries.get(language)
    }

    /// Get the set of enabled tools based on config
    pub fn enabled_tools(&self) -> HashSet<String> {
        self.tools.enabled_tools(self.project_config_present)
    }

    /// Get the resolved output mode, defaulting when not configured
    pub fn output_mode(&self) -> OutputMode {
        self.output
            .as_ref()
            .map(|output| output.mode)
            .unwrap_or_default()
    }

    /// Get the debug config if enabled
    pub fn debug_config(&self) -> Option<&DebugConfig> {
        self.debug.as_ref().filter(|d| d.enabled)
    }
}

impl From<LspMcpConfigFile> for LspMcpConfig {
    fn from(file: LspMcpConfigFile) -> Self {
        LspMcpConfig {
            languages: file.languages,
            binaries: file.binaries,
            tools: file.tools,
            output: file.output,
            debug: file.debug,
            semantic_search: file.semantic_search,
            project_config_present: false,
        }
    }
}
