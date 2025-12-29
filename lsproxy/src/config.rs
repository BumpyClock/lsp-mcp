// ABOUTME: Configuration file support for the lsp-mcp server.
// ABOUTME: Loads .lsp-mcp.json from workspace root and global config to configure language servers and tools.

use crate::tool_registry::{get_preset_tools, ToolPreset};
use log::info;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

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

/// Configuration for which MCP tools are enabled/disabled.
///
/// Tools are selected based on a preset, then refined with explicit enable/disable lists.
/// The disable list takes precedence over enable.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ToolsConfig {
    /// Preset to use as base ("minimal", "standard", "full")
    #[serde(default)]
    pub preset: ToolPreset,
    /// Tools to explicitly enable (added to preset)
    #[serde(default)]
    pub enable: Vec<String>,
    /// Tools to explicitly disable (removed from final set, overrides enable)
    #[serde(default)]
    pub disable: Vec<String>,
}

impl ToolsConfig {
    /// Compute the final set of enabled tools based on preset and overrides
    pub fn enabled_tools(&self) -> HashSet<String> {
        let mut tools = get_preset_tools(self.preset);

        // Add explicitly enabled tools
        for tool in &self.enable {
            tools.insert(tool.clone());
        }

        // Remove explicitly disabled tools (disable wins over enable)
        for tool in &self.disable {
            tools.remove(tool);
        }

        tools
    }

    /// Merge project config over global config for tools
    fn merge(self, project: Self) -> Self {
        // Project preset wins if set (we check by comparing to default)
        // Since we can't easily detect "was this field present", we always use project preset
        let preset = project.preset;

        // Merge enable lists (union)
        let mut enable: HashSet<_> = self.enable.into_iter().collect();
        enable.extend(project.enable);

        // Merge disable lists (union)
        let mut disable: HashSet<_> = self.disable.into_iter().collect();
        disable.extend(project.disable);

        ToolsConfig {
            preset,
            enable: enable.into_iter().collect(),
            disable: disable.into_iter().collect(),
        }
    }
}

/// Configuration for the lsp-mcp server loaded from .lsp-mcp.json.
///
/// Supports both project-level config (workspace root) and global config (~/.lsp-mcp/).
/// Project config overrides global config.
///
/// # Example
/// ```json
/// {
///   "languages": ["rust", "python"],
///   "binaries": {
///     "rust": "/opt/rust-analyzer"
///   },
///   "tools": {
///     "preset": "standard",
///     "enable": ["find_referenced_symbols"],
///     "disable": ["incoming_calls"]
///   },
///   "output": {
///     "mode": "verbose"
///   }
/// }
/// ```
#[derive(Debug, Deserialize, Default, Clone)]
pub struct LspMcpConfig {
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub binaries: HashMap<String, String>,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub output: Option<OutputConfig>,
}

impl LspMcpConfig {
    /// Load config from a specific path
    fn load_from_path(path: &Path) -> Option<Self> {
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
        Self::load_from_path(&global_config_path)
    }

    /// Load project config from workspace root (.lsp-mcp.json)
    pub fn load_project(workspace_root: &Path) -> Option<Self> {
        let config_path = workspace_root.join(".lsp-mcp.json");
        Self::load_from_path(&config_path)
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
        let project = Self::load_project(workspace_root);

        match (global, project) {
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
        }
    }

    /// Merge project config over this (global) config
    fn merge(self, project: Self) -> Self {
        LspMcpConfig {
            // Project languages replace global if non-empty
            languages: if project.languages.is_empty() {
                self.languages
            } else {
                project.languages
            },
            // Merge binaries (project overrides)
            binaries: {
                let mut merged = self.binaries;
                merged.extend(project.binaries);
                merged
            },
            // Merge tools config
            tools: self.tools.merge(project.tools),
            // Project output config overrides global
            output: project.output.or(self.output),
        }
    }

    pub fn get_binary(&self, language: &str) -> Option<&String> {
        self.binaries.get(language)
    }

    /// Get the set of enabled tools based on config
    pub fn enabled_tools(&self) -> HashSet<String> {
        self.tools.enabled_tools()
    }

    /// Get the resolved output mode, defaulting when not configured
    pub fn output_mode(&self) -> OutputMode {
        self.output
            .as_ref()
            .map(|output| output.mode)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::distributions::Alphanumeric;
    use rand::Rng;
    use tempfile::TempDir;

    fn random_suffix() -> String {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect()
    }

    #[test]
    fn it_returns_none_when_config_file_does_not_exist() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let result = LspMcpConfig::load(temp_dir.path());
        assert!(
            result.is_none(),
            "Expected None when .lsp-mcp.json does not exist"
        );
    }

    #[test]
    fn it_parses_languages_from_config_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{"languages": ["rust", "python"]}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());

        assert!(result.is_some(), "Expected Some when config file exists");
        let config = result.unwrap();
        assert_eq!(
            config.languages.len(),
            2,
            "Expected two languages in config"
        );
        assert!(
            config.languages.contains(&"rust".to_string()),
            "Expected rust in languages"
        );
        assert!(
            config.languages.contains(&"python".to_string()),
            "Expected python in languages"
        );
    }

    #[test]
    fn it_parses_custom_binary_paths_from_config_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let random_path = format!("/custom/path/{}/rust-analyzer", random_suffix());
        let config_content = format!(
            r#"{{"languages": ["rust"], "binaries": {{"rust": "{}"}}}}"#,
            random_path
        );
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());

        assert!(result.is_some(), "Expected Some when config file exists");
        let config = result.unwrap();
        assert_eq!(
            config.get_binary("rust"),
            Some(&random_path),
            "Expected custom binary path for rust"
        );
    }

    #[test]
    fn it_returns_none_for_invalid_json() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, "not valid json {{{").expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());

        assert!(result.is_none(), "Expected None for invalid JSON config");
    }

    #[test]
    fn it_handles_empty_config_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, "{}").expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());

        assert!(result.is_some(), "Expected Some for empty but valid JSON");
        let config = result.unwrap();
        assert!(
            config.languages.is_empty(),
            "Expected empty languages for empty config"
        );
        assert!(
            config.binaries.is_empty(),
            "Expected empty binaries for empty config"
        );
    }

    #[test]
    fn it_returns_none_when_binary_not_specified() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{"languages": ["rust"]}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());
        let config = result.expect("Expected config to load");

        assert!(
            config.get_binary("rust").is_none(),
            "Expected None for unspecified binary"
        );
    }

    #[test]
    fn it_handles_unicode_paths_in_binaries() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let unicode_path = format!("/路径/αβγ/{}/rust-analyzer", random_suffix());
        let config_content = format!(
            r#"{{"languages": ["rust"], "binaries": {{"rust": "{}"}}}}"#,
            unicode_path
        );
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());

        assert!(result.is_some(), "Expected Some for unicode paths");
        let config = result.unwrap();
        assert_eq!(
            config.get_binary("rust"),
            Some(&unicode_path),
            "Expected unicode path to be preserved"
        );
    }

    // ========== Tools Config Tests ==========

    #[test]
    fn it_parses_tools_config_with_preset() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{"tools": {"preset": "minimal"}}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());
        let config = result.expect("Expected config to load");

        assert_eq!(config.tools.preset, ToolPreset::Minimal);
    }

    #[test]
    fn it_parses_tools_config_with_enable_disable() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{
            "tools": {
                "preset": "standard",
                "enable": ["find_referenced_symbols"],
                "disable": ["incoming_calls"]
            }
        }"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());
        let config = result.expect("Expected config to load");

        assert_eq!(config.tools.preset, ToolPreset::Standard);
        assert!(config.tools.enable.contains(&"find_referenced_symbols".to_string()));
        assert!(config.tools.disable.contains(&"incoming_calls".to_string()));
    }

    #[test]
    fn it_defaults_to_standard_preset_when_tools_missing() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{"languages": ["rust"]}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());
        let config = result.expect("Expected config to load");

        assert_eq!(config.tools.preset, ToolPreset::Standard);
        assert!(config.tools.enable.is_empty());
        assert!(config.tools.disable.is_empty());
    }

    #[test]
    fn test_enabled_tools_standard_preset() {
        let config = LspMcpConfig::default();
        let tools = config.enabled_tools();

        // Standard preset should have 8 tools
        assert_eq!(tools.len(), 8);
        assert!(tools.contains("find_definition"));
        assert!(tools.contains("find_references"));
        assert!(tools.contains("hover"));
        assert!(tools.contains("get_diagnostics"));
        assert!(tools.contains("workspace_symbol"));
        assert!(tools.contains("definitions_in_file"));
        assert!(tools.contains("incoming_calls"));
        assert!(tools.contains("outgoing_calls"));
    }

    #[test]
    fn test_enabled_tools_with_explicit_enable() {
        let config = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Standard,
                enable: vec!["find_referenced_symbols".to_string()],
                disable: vec![],
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        // Should have 8 + 1 = 9 tools
        assert_eq!(tools.len(), 9);
        assert!(tools.contains("find_referenced_symbols"));
    }

    #[test]
    fn test_enabled_tools_with_explicit_disable() {
        let config = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Standard,
                enable: vec![],
                disable: vec!["incoming_calls".to_string()],
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        // Should have 8 - 1 = 7 tools
        assert_eq!(tools.len(), 7);
        assert!(!tools.contains("incoming_calls"));
    }

    #[test]
    fn test_disable_wins_over_enable() {
        let config = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Minimal,
                enable: vec!["list_files".to_string()],
                disable: vec!["list_files".to_string()],
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        // Disable should win
        assert!(!tools.contains("list_files"));
    }

    #[test]
    fn test_config_merge_languages() {
        let global = LspMcpConfig {
            languages: vec!["rust".to_string()],
            ..Default::default()
        };
        let project = LspMcpConfig {
            languages: vec!["python".to_string()],
            ..Default::default()
        };

        let merged = global.merge(project);

        // Project languages should replace global
        assert_eq!(merged.languages, vec!["python".to_string()]);
    }

    #[test]
    fn test_config_merge_languages_empty_project() {
        let global = LspMcpConfig {
            languages: vec!["rust".to_string()],
            ..Default::default()
        };
        let project = LspMcpConfig::default();

        let merged = global.merge(project);

        // Global languages should be kept when project is empty
        assert_eq!(merged.languages, vec!["rust".to_string()]);
    }

    #[test]
    fn test_config_merge_binaries() {
        let global = LspMcpConfig {
            binaries: [("rust".to_string(), "/global/rust-analyzer".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let project = LspMcpConfig {
            binaries: [("python".to_string(), "/project/jedi".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };

        let merged = global.merge(project);

        // Both binaries should be present
        assert_eq!(merged.binaries.len(), 2);
        assert_eq!(merged.get_binary("rust"), Some(&"/global/rust-analyzer".to_string()));
        assert_eq!(merged.get_binary("python"), Some(&"/project/jedi".to_string()));
    }

    #[test]
    fn test_config_merge_binaries_project_overrides() {
        let global = LspMcpConfig {
            binaries: [("rust".to_string(), "/global/rust-analyzer".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let project = LspMcpConfig {
            binaries: [("rust".to_string(), "/project/rust-analyzer".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };

        let merged = global.merge(project);

        // Project binary should override global
        assert_eq!(merged.get_binary("rust"), Some(&"/project/rust-analyzer".to_string()));
    }

    #[test]
    fn test_config_merge_tools_preset() {
        let global = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Full,
                ..Default::default()
            },
            ..Default::default()
        };
        let project = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Minimal,
                ..Default::default()
            },
            ..Default::default()
        };

        let merged = global.merge(project);

        // Project preset should win
        assert_eq!(merged.tools.preset, ToolPreset::Minimal);
    }

    #[test]
    fn test_config_merge_tools_enable_union() {
        let global = LspMcpConfig {
            tools: ToolsConfig {
                enable: vec!["tool_a".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let project = LspMcpConfig {
            tools: ToolsConfig {
                enable: vec!["tool_b".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        let merged = global.merge(project);

        // Enable lists should be unioned
        let enable_set: HashSet<_> = merged.tools.enable.into_iter().collect();
        assert!(enable_set.contains("tool_a"));
        assert!(enable_set.contains("tool_b"));
    }

    #[test]
    fn test_config_merge_tools_disable_union() {
        let global = LspMcpConfig {
            tools: ToolsConfig {
                disable: vec!["tool_a".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let project = LspMcpConfig {
            tools: ToolsConfig {
                disable: vec!["tool_b".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        let merged = global.merge(project);

        // Disable lists should be unioned
        let disable_set: HashSet<_> = merged.tools.disable.into_iter().collect();
        assert!(disable_set.contains("tool_a"));
        assert!(disable_set.contains("tool_b"));
    }

    #[test]
    fn test_load_merged_returns_defaults_when_no_config() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config = LspMcpConfig::load_merged(temp_dir.path());

        // Should return default config
        assert!(config.languages.is_empty());
        assert!(config.binaries.is_empty());
        assert_eq!(config.tools.preset, ToolPreset::Standard);
    }

    // ========== Output Config Tests ==========

    #[test]
    fn it_parses_output_mode_from_config_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{"output": {"mode": "verbose"}}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());
        let config = result.expect("Expected config to load");

        assert_eq!(
            config
                .output
                .as_ref()
                .expect("Expected output config to be present")
                .mode,
            OutputMode::Verbose
        );
    }

    #[test]
    fn it_defaults_to_default_output_mode() {
        let config = LspMcpConfig::default();
        assert_eq!(config.output_mode(), OutputMode::Default);
    }

    #[test]
    fn it_parses_default_output_mode_from_config_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{"output": {"mode": "default"}}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());
        let config = result.expect("Expected config to load");

        assert_eq!(
            config
                .output
                .as_ref()
                .expect("Expected output config to be present")
                .mode,
            OutputMode::Default
        );
    }

    #[test]
    fn test_config_merge_output_mode_project_overrides() {
        let global = LspMcpConfig {
            output: Some(OutputConfig {
                mode: OutputMode::Default,
            }),
            ..Default::default()
        };
        let project = LspMcpConfig {
            output: Some(OutputConfig {
                mode: OutputMode::Verbose,
            }),
            ..Default::default()
        };

        let merged = global.merge(project);

        // Project output mode should override global
        assert_eq!(merged.output_mode(), OutputMode::Verbose);
    }

    #[test]
    fn test_config_merge_output_mode_uses_global_when_project_missing() {
        let global = LspMcpConfig {
            output: Some(OutputConfig {
                mode: OutputMode::Verbose,
            }),
            ..Default::default()
        };
        let project = LspMcpConfig {
            output: None,
            ..Default::default()
        };

        let merged = global.merge(project);

        // Global output mode should be preserved when project does not set output
        assert_eq!(merged.output_mode(), OutputMode::Verbose);
    }
}
