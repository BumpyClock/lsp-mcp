// ABOUTME: Configuration module for the lsp-mcp server.
// ABOUTME: Re-exports configuration types and loaders from submodules.

mod loaders;
mod semantic_search_config;
mod tools_config;
mod types;

use crate::api_types::SupportedLanguages;
use std::collections::HashMap;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum LanguageSelection {
    #[default]
    Auto,
    Explicit(Vec<SupportedLanguages>),
}

pub use semantic_search_config::{
    EmbedderConfig, EnrichmentConfig, ExcludeConfig, SemanticSearchConfig, VectorStoreConfig,
};
pub use tools_config::{InitialSetupMode, ToolsConfig};
pub use types::{DebugConfig, DebugLogLevel, OutputConfig, OutputMode};

pub(crate) use semantic_search_config::SemanticSearchConfigFile;
pub(crate) use tools_config::ToolsConfigFile;
pub(crate) use types::{DebugConfigFile, OutputConfigFile};

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
///     "enable": ["findReferencedSymbols"],
///     "disable": ["callHierarchy"]
///   },
///   "output": {
///     "mode": "verbose"
///   }
/// }
/// ```
#[derive(Debug, Default, Clone)]
pub struct LspMcpConfig {
    pub languages: LanguageSelection,
    pub binaries: HashMap<String, String>,
    pub tools: ToolsConfig,
    pub output: Option<OutputConfig>,
    pub debug: Option<DebugConfig>,
    pub semantic_search: Option<SemanticSearchConfig>,
    pub project_config_present: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_registry::ToolPreset;
    use rand::distr::Alphanumeric;
    use rand::Rng;
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
        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();
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

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();

        assert!(result.is_some(), "Expected Some when config file exists");
        let config = result.unwrap();
        assert_eq!(
            config.languages,
            LanguageSelection::Explicit(vec![SupportedLanguages::Rust, SupportedLanguages::Python])
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

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();

        assert!(result.is_some(), "Expected Some when config file exists");
        let config = result.unwrap();
        assert_eq!(
            config.get_binary("rust"),
            Some(&random_path),
            "Expected custom binary path for rust"
        );
    }

    #[test]
    fn it_rejects_invalid_json_with_config_path() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, "not valid json {{{").expect("Failed to write config");

        let result = LspMcpConfig::load_project(temp_dir.path());

        assert!(result
            .unwrap_err()
            .to_string()
            .contains(&config_path.display().to_string()));
    }

    #[test]
    fn it_handles_empty_config_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, "{}").expect("Failed to write config");

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();

        assert!(result.is_some(), "Expected Some for empty but valid JSON");
        let config = result.unwrap();
        assert!(
            config.languages == LanguageSelection::Auto,
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

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();
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

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();

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

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();
        let config = result.expect("Expected config to load");

        assert_eq!(config.tools.preset, ToolPreset::Minimal);
    }

    #[test]
    fn it_parses_tools_config_with_enable_disable() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{
            "tools": {
                "preset": "standard",
                "enable": ["findReferencedSymbols"],
                "disable": ["callHierarchy"]
            }
        }"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();
        let config = result.expect("Expected config to load");

        assert_eq!(config.tools.preset, ToolPreset::Standard);
        assert!(config
            .tools
            .enable
            .contains(&"findReferencedSymbols".to_string()));
        assert!(config.tools.disable.contains(&"callHierarchy".to_string()));
    }

    #[test]
    fn it_defaults_to_standard_preset_when_tools_missing() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{"languages": ["rust"]}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();
        let config = result.expect("Expected config to load");

        assert_eq!(config.tools.preset, ToolPreset::Standard);
        assert!(config.tools.enable.is_empty());
        assert!(config.tools.disable.is_empty());
    }

    #[test]
    fn test_enabled_tools_standard_preset() {
        let config = LspMcpConfig::default();
        let tools = config.enabled_tools();

        assert_eq!(tools.len(), 11);
        assert!(tools.contains("goToDefinition"));
        assert!(tools.contains("getSymbolDefinition"));
        assert!(tools.contains("findReferences"));
        assert!(tools.contains("hover"));
        assert!(tools.contains("getDiagnostics"));
        assert!(tools.contains("findSymbol"));
        assert!(tools.contains("documentSymbol"));
        assert!(tools.contains("callHierarchy"));
        assert!(tools.contains("findReferencedSymbols"));
        assert!(tools.contains("initialSetup"));
    }

    #[test]
    fn test_enabled_tools_with_explicit_enable() {
        let config = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Standard,
                enable: vec!["findReferencedSymbols".to_string()],
                disable: vec![],
                ..Default::default()
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        assert_eq!(tools.len(), 11);
        assert!(tools.contains("findReferencedSymbols"));
    }

    #[test]
    fn test_enabled_tools_with_explicit_disable() {
        let config = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Standard,
                enable: vec![],
                disable: vec!["callHierarchy".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        assert_eq!(tools.len(), 10);
        assert!(!tools.contains("callHierarchy"));
    }

    #[test]
    fn test_disable_wins_over_enable() {
        let config = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Minimal,
                enable: vec!["listFiles".to_string()],
                disable: vec!["listFiles".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        assert!(!tools.contains("listFiles"));
    }

    #[test]
    fn test_initial_setup_auto_disabled_when_project_config_present() {
        let config = LspMcpConfig {
            project_config_present: true,
            ..Default::default()
        };
        let tools = config.enabled_tools();

        assert!(!tools.contains("initialSetup"));
        assert_eq!(tools.len(), 10);
    }

    #[test]
    fn test_initial_setup_enabled_overrides_auto() {
        let config = LspMcpConfig {
            project_config_present: true,
            tools: ToolsConfig {
                initial_setup: InitialSetupMode::Enabled,
                ..Default::default()
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        assert!(tools.contains("initialSetup"));
        assert_eq!(tools.len(), 11);
    }

    #[test]
    fn test_initial_setup_disabled_when_no_project_config() {
        let config = LspMcpConfig {
            project_config_present: false,
            tools: ToolsConfig {
                initial_setup: InitialSetupMode::Disabled,
                ..Default::default()
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        assert!(!tools.contains("initialSetup"));
        assert_eq!(tools.len(), 10);
    }

    #[test]
    fn test_initial_setup_auto_disabled_when_preset_not_standard() {
        let config = LspMcpConfig {
            tools: ToolsConfig {
                preset: ToolPreset::Full,
                ..Default::default()
            },
            ..Default::default()
        };
        let tools = config.enabled_tools();

        assert!(!tools.contains("initialSetup"));
        // Full preset is 15 tools (16 ALL_TOOLS - 1 opt-in semanticSearch), minus initialSetup = 14
        assert_eq!(tools.len(), 14);
    }

    #[test]
    fn test_load_merged_returns_defaults_when_no_config() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config = LspMcpConfig::load_merged_with_global(temp_dir.path(), None).unwrap();

        assert!(config.languages == LanguageSelection::Auto);
        assert!(config.binaries.is_empty());
        assert_eq!(config.tools.preset, ToolPreset::Standard);
    }

    #[test]
    fn test_load_merged_semantic_search_deep_merge() {
        let home_dir = TempDir::new().expect("Failed to create temp directory");
        let workspace_dir = TempDir::new().expect("Failed to create temp directory");
        let global_config_dir = home_dir.path().join(".lsp-mcp");
        std::fs::create_dir_all(&global_config_dir).expect("Failed to create global config dir");

        let global_config = r#"{
            "semantic_search": {
                "enabled": true,
                "embedder": {
                    "provider": "openai",
                    "model": "global-model",
                    "api_key_env": "GLOBAL_KEY",
                    "base_url": "https://global"
                },
                "vector_store": { "path": ".global/semantic" },
                "enrichment": {
                    "enabled": true,
                    "model": "global-enrich"
                }
            }
        }"#;
        let global_config_path = global_config_dir.join(".lsp-mcp.json");
        std::fs::write(&global_config_path, global_config).expect("Failed to write global config");

        let project_config = r#"{
            "semantic_search": {
                "embedder": {
                    "provider": "openai",
                    "model": "project-model"
                },
                "enrichment": { "max_concurrency": 4 },
                "default_context_lines": null
            }
        }"#;
        let project_config_path = workspace_dir.path().join(".lsp-mcp.json");
        std::fs::write(&project_config_path, project_config)
            .expect("Failed to write project config");

        let config =
            LspMcpConfig::load_merged_with_global(workspace_dir.path(), Some(&global_config_path))
                .unwrap();

        let semantic = config
            .semantic_search
            .expect("Expected semantic_search to be present");
        assert!(semantic.enabled);
        assert_eq!(
            semantic.vector_store.path.as_deref(),
            Some(".global/semantic")
        );
        assert!(semantic.enrichment.enabled);
        assert_eq!(semantic.enrichment.model, "global-enrich");
        assert_eq!(semantic.enrichment.max_concurrency, 4);
        assert_eq!(semantic.default_context_lines, None);

        match semantic.embedder {
            EmbedderConfig::OpenAI {
                model,
                base_url,
                api_key_env,
                ..
            } => {
                assert_eq!(model, "project-model");
                assert_eq!(base_url, "https://global");
                assert_eq!(api_key_env, "GLOBAL_KEY");
            }
            _ => panic!("Expected OpenAI embedder"),
        }
    }

    #[test]
    fn it_parses_output_mode_from_config_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{"output": {"mode": "verbose"}}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();
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

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();
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
    fn it_parses_debug_config_from_config_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content =
            r#"{"debug": {"enabled": true, "log_level": "trace", "log_dir": ".custom/logs"}}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();
        let config = result.expect("Expected config to load");

        let debug = config.debug.expect("Expected debug config");
        assert!(debug.enabled);
        assert_eq!(debug.log_level, DebugLogLevel::Trace);
        assert_eq!(debug.log_dir, Some(".custom/logs".to_string()));
    }

    #[test]
    fn it_defaults_debug_config_when_not_specified() {
        let config = LspMcpConfig::default();
        assert!(config.debug.is_none());
        assert!(config.debug_config().is_none());
    }

    #[test]
    fn debug_config_returns_none_when_disabled() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_content = r#"{"debug": {"enabled": false}}"#;
        let config_path = temp_dir.path().join(".lsp-mcp.json");
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let result = LspMcpConfig::load_project(temp_dir.path()).unwrap();
        let config = result.expect("Expected config to load");

        assert!(config.debug_config().is_none());
    }
}
