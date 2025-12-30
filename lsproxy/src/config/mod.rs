// ABOUTME: Configuration module for the lsp-mcp server.
// ABOUTME: Re-exports configuration types and loaders from submodules.

mod loaders;
mod tools_config;
mod types;

use std::collections::HashMap;

pub use tools_config::ToolsConfig;
pub use types::{OutputConfig, OutputMode};

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
///     "disable": ["call_hierarchy"]
///   },
///   "output": {
///     "mode": "verbose"
///   }
/// }
/// ```
#[derive(Debug, Default, Clone)]
pub struct LspMcpConfig {
    pub languages: Vec<String>,
    pub binaries: HashMap<String, String>,
    pub tools: ToolsConfig,
    pub output: Option<OutputConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_registry::ToolPreset;
    use rand::distr::Alphanumeric;
    use rand::Rng;
    use std::collections::HashSet;
    use tempfile::TempDir;

    fn random_suffix() -> String {
        rand::rng()
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
                "disable": ["call_hierarchy"]
            }
        }"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load(temp_dir.path());
        let config = result.expect("Expected config to load");

        assert_eq!(config.tools.preset, ToolPreset::Standard);
        assert!(config.tools.enable.contains(&"find_referenced_symbols".to_string()));
        assert!(config.tools.disable.contains(&"call_hierarchy".to_string()));
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

        assert_eq!(tools.len(), 7);
        assert!(tools.contains("find_definition"));
        assert!(tools.contains("find_references"));
        assert!(tools.contains("hover"));
        assert!(tools.contains("get_diagnostics"));
        assert!(tools.contains("workspace_symbol"));
        assert!(tools.contains("definitions_in_file"));
        assert!(tools.contains("call_hierarchy"));
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

        assert_eq!(tools.len(), 8);
        assert!(tools.contains("find_referenced_symbols"));
    }

    #[test]
    fn test_enabled_tools_with_explicit_disable() {
        let config = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Standard,
                enable: vec![],
                disable: vec!["call_hierarchy".to_string()],
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        assert_eq!(tools.len(), 6);
        assert!(!tools.contains("call_hierarchy"));
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

        let disable_set: HashSet<_> = merged.tools.disable.into_iter().collect();
        assert!(disable_set.contains("tool_a"));
        assert!(disable_set.contains("tool_b"));
    }

    #[test]
    fn test_load_merged_returns_defaults_when_no_config() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config = LspMcpConfig::load_merged(temp_dir.path());

        assert!(config.languages.is_empty());
        assert!(config.binaries.is_empty());
        assert_eq!(config.tools.preset, ToolPreset::Standard);
    }

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

        assert_eq!(merged.output_mode(), OutputMode::Verbose);
    }
}
