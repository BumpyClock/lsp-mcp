// ABOUTME: Configuration loaders for the lsp-mcp server.
// ABOUTME: Loads and merges global and project-level .lsp-mcp.json config files.

use super::{
    DebugConfig, DebugConfigFile, LanguageSelection, LspMcpConfig, OutputConfigFile, OutputMode,
    SemanticSearchConfigFile, ToolsConfigFile,
};
use log::info;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// Internal config structure for deserialization.
#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct LspMcpConfigFile {
    #[serde(default, deserialize_with = "deserialize_languages")]
    pub languages: Option<Vec<crate::api_types::SupportedLanguages>>,
    pub binaries: Option<std::collections::HashMap<String, String>>,
    pub tools: Option<ToolsConfigFile>,
    pub output: Option<OutputConfigFile>,
    pub debug: Option<DebugConfigFile>,
    pub semantic_search: Option<SemanticSearchConfigFile>,
}

fn deserialize_languages<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<crate::api_types::SupportedLanguages>>, D::Error> {
    use serde::de::Error;
    let values = Vec::<String>::deserialize(deserializer)?;
    values
        .into_iter()
        .map(|value| {
            value.parse().map_err(|_| {
                D::Error::custom(format!(
                    "unknown language identifier '{value}'; use a supported language name or alias"
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

impl LspMcpConfig {
    fn load_from_path(path: &Path) -> Result<Option<LspMcpConfigFile>, Box<dyn std::error::Error>> {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("Failed to read config {}: {e}", path.display()).into()),
        };
        let config = serde_json::from_str(&content)
            .map_err(|e| format!("Invalid config {}: {e}", path.display()))?;
        Ok(Some(config))
    }

    pub fn load_project(workspace_root: &Path) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        Ok(
            Self::load_from_path(&workspace_root.join(".lsp-mcp.json"))?.map(|f| {
                let mut config = f.resolve();
                config.project_config_present = true;
                config
            }),
        )
    }

    /// Load merged config: global config with project config overrides
    ///
    /// Returns default config if neither global nor project config exists.
    pub fn load_merged(workspace_root: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let global_path = dirs::home_dir().map(|home| home.join(".lsp-mcp").join(".lsp-mcp.json"));
        Self::load_merged_with_global(workspace_root, global_path.as_deref())
    }

    pub(super) fn load_merged_with_global(
        workspace_root: &Path,
        global_path: Option<&Path>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let global_file = global_path.map(Self::load_from_path).transpose()?.flatten();
        let project_file = Self::load_from_path(&workspace_root.join(".lsp-mcp.json"))?;
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
        Ok(config)
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
            languages: self
                .languages
                .map(LanguageSelection::Explicit)
                .unwrap_or_default(),
            binaries: self.binaries.unwrap_or_default(),
            tools: self.tools.unwrap_or_default().resolve(),
            output: self.output.map(OutputConfigFile::resolve),
            debug: self.debug.map(DebugConfigFile::resolve),
            semantic_search: self.semantic_search.map(SemanticSearchConfigFile::resolve),
            project_config_present: false,
        }
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use crate::api_types::SupportedLanguages;

    #[test]
    fn file_merge_preserves_settings_and_project_language_selection() {
        let home = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let global = home.path().join("global.json");
        std::fs::write(
            &global,
            r#"{
            "languages":["rust"],
            "binaries":{"rust":"/global/rust", "golang":"/global/gopls"},
            "tools":{"enable":["health"],"disable":["documentSymbol"]},
            "debug":{"enabled":true,"log_level":"trace"},
            "output":{"mode":"verbose"}
        }"#,
        )
        .unwrap();
        let project = workspace.path().join(".lsp-mcp.json");
        std::fs::write(
            &project,
            r#"{
            "languages":[], "binaries":{"rust":"/project/rust"},
            "tools":{"preset":"minimal","enable":["readSourceCode"],"disable":["health"]},
            "debug":{"log_level":"error"}
        }"#,
        )
        .unwrap();
        let config =
            LspMcpConfig::load_merged_with_global(workspace.path(), Some(&global)).unwrap();
        assert_eq!(config.languages, LanguageSelection::Explicit(vec![]));
        assert_eq!(config.get_binary("rust").unwrap(), "/project/rust");
        assert_eq!(config.get_binary("golang").unwrap(), "/global/gopls");
        let tools = config.enabled_tools();
        assert!(tools.contains("readSourceCode"));
        assert!(!tools.contains("health"));
        assert!(!tools.contains("documentSymbol"));
        assert!(config.debug.as_ref().unwrap().enabled);
        assert_eq!(
            config.debug.as_ref().unwrap().log_level,
            crate::config::DebugLogLevel::Error
        );
        assert_eq!(config.output_mode(), OutputMode::Verbose);
        assert!(config.project_config_present);
        std::fs::write(&project, "{}").unwrap();
        assert_eq!(
            LspMcpConfig::load_merged_with_global(workspace.path(), Some(&global))
                .unwrap()
                .languages,
            LanguageSelection::Explicit(vec![SupportedLanguages::Rust])
        );
        std::fs::write(&project, r#"{"languages":null}"#).unwrap();
        assert!(
            LspMcpConfig::load_merged_with_global(workspace.path(), None)
                .unwrap_err()
                .to_string()
                .contains(".lsp-mcp.json")
        );
    }

    #[test]
    fn selection_and_precedence() {
        let global: LspMcpConfigFile = serde_json::from_str(r#"{"languages":["rust"]}"#).unwrap();
        let settings: LspMcpConfigFile = serde_json::from_str(r#"{"binaries":{}}"#).unwrap();
        assert_eq!(
            global.clone().merge(settings).resolve().languages,
            LanguageSelection::Explicit(vec![SupportedLanguages::Rust])
        );
        let empty: LspMcpConfigFile = serde_json::from_str(r#"{"languages":[]}"#).unwrap();
        assert_eq!(
            global.merge(empty).resolve().languages,
            LanguageSelection::Explicit(vec![])
        );
        let alias: LspMcpConfigFile =
            serde_json::from_str(r#"{"languages":["javascript"]}"#).unwrap();
        assert_eq!(
            alias.resolve().languages,
            LanguageSelection::Explicit(vec![SupportedLanguages::TypeScriptJavaScript])
        );
        assert!(
            serde_json::from_str::<LspMcpConfigFile>(r#"{"languages":["bogus"]}"#)
                .unwrap_err()
                .to_string()
                .contains("bogus")
        );
    }
}
