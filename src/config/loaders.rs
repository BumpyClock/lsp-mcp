// ABOUTME: Configuration loaders for the lsp-mcp server.
// ABOUTME: Loads and merges global and project-level .lsp-mcp.json config files.

use super::{
    DebugConfig, DebugConfigFile, LspMcpConfig, OutputConfigFile, OutputMode,
    SemanticSearchConfigFile, ToolsConfigFile,
};
use log::info;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// Internal config structure for deserialization.
#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct LspMcpConfigFile {
    pub languages: Option<Vec<String>>,
    pub binaries: Option<std::collections::HashMap<String, String>>,
    pub tools: Option<ToolsConfigFile>,
    pub output: Option<OutputConfigFile>,
    pub debug: Option<DebugConfigFile>,
    pub semantic_search: Option<SemanticSearchConfigFile>,
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
        Self::load_global_file().map(LspMcpConfigFile::resolve)
    }

    fn load_global_file() -> Option<LspMcpConfigFile> {
        let home = dirs::home_dir()?;
        let global_config_path = home.join(".lsp-mcp").join(".lsp-mcp.json");
        Self::load_from_path(&global_config_path)
    }

    /// Load project config from workspace root (.lsp-mcp.json)
    pub fn load_project(workspace_root: &Path) -> Option<Self> {
        let config_path = workspace_root.join(".lsp-mcp.json");
        Self::load_from_path(&config_path).map(|f| {
            let mut config = f.resolve();
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
        let global_file = Self::load_global_file();
        let project_file = Self::load_from_path(&workspace_root.join(".lsp-mcp.json"));
        let project_present = project_file.is_some();
        let merged = match (global_file, project_file) {
            (None, None) => {
                info!("No config files found, using defaults");
                None
            }
            (Some(global), None) => {
                info!("Loaded global config from ~/.lsp-mcp/.lsp-mcp.json");
                Some(global)
            }
            (None, Some(project)) => {
                info!("Loaded project config from .lsp-mcp.json");
                Some(project)
            }
            (Some(global), Some(project)) => {
                info!("Merged global and project configs");
                Some(global.merge(project))
            }
        };

        let mut config = merged.map(LspMcpConfigFile::resolve).unwrap_or_default();

        config.project_config_present = project_present;
        config
    }

    /// Merge project config over this (global) config
    #[cfg(test)]
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

impl LspMcpConfigFile {
    fn merge(self, project: Self) -> Self {
        let binaries = match (self.binaries, project.binaries) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_binaries)) => Some(project_binaries),
            (Some(mut global), Some(project_binaries)) => {
                global.extend(project_binaries);
                Some(global)
            }
        };
        let tools = match (self.tools, project.tools) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_tools)) => Some(project_tools),
            (Some(global), Some(project_tools)) => Some(global.merge(project_tools)),
        };
        let output = match (self.output, project.output) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_output)) => Some(project_output),
            (Some(global), Some(project_output)) => Some(global.merge(project_output)),
        };
        let debug = match (self.debug, project.debug) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_debug)) => Some(project_debug),
            (Some(global), Some(project_debug)) => Some(global.merge(project_debug)),
        };
        let semantic_search = match (self.semantic_search, project.semantic_search) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_search)) => Some(project_search),
            (Some(global), Some(project_search)) => Some(global.merge(project_search)),
        };

        LspMcpConfigFile {
            languages: project.languages.or(self.languages),
            binaries,
            tools,
            output,
            debug,
            semantic_search,
        }
    }

    fn resolve(self) -> LspMcpConfig {
        LspMcpConfig {
            languages: self.languages.unwrap_or_default(),
            binaries: self.binaries.unwrap_or_default(),
            tools: self.tools.unwrap_or_default().resolve(),
            output: self.output.map(OutputConfigFile::resolve),
            debug: self.debug.map(DebugConfigFile::resolve),
            semantic_search: self.semantic_search.map(SemanticSearchConfigFile::resolve),
            project_config_present: false,
        }
    }
}
