// ABOUTME: Configuration file support for the lsp-mcp server.
// ABOUTME: Loads .lsp-mcp.json from workspace root to configure language servers.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Configuration for the lsp-mcp server loaded from .lsp-mcp.json.
///
/// When present in the workspace root, this configuration replaces
/// automatic language detection and allows specifying custom binary paths.
///
/// # Example
/// ```json
/// {
///   "languages": ["rust", "python"],
///   "binaries": {
///     "rust": "/opt/rust-analyzer"
///   }
/// }
/// ```
#[derive(Debug, Deserialize, Default, Clone)]
pub struct LspMcpConfig {
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub binaries: HashMap<String, String>,
}

impl LspMcpConfig {
    pub fn load(workspace_root: &Path) -> Option<Self> {
        let config_path = workspace_root.join(".lsp-mcp.json");
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path).ok()?;
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    }

    pub fn get_binary(&self, language: &str) -> Option<&String> {
        self.binaries.get(language)
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
}
