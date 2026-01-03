// ABOUTME: Tools configuration for the lsp-mcp server.
// ABOUTME: Manages tool presets and explicit enable/disable lists.

use crate::tool_registry::{get_preset_tools, ToolPreset};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InitialSetupMode {
    Auto,
    Enabled,
    Disabled,
}

impl Default for InitialSetupMode {
    fn default() -> Self {
        InitialSetupMode::Auto
    }
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
    /// Initial setup tool behavior ("auto", "enabled", "disabled")
    #[serde(default)]
    pub initial_setup: InitialSetupMode,
}

/// Optional tools configuration for config file merging.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct ToolsConfigFile {
    /// Preset to use as base ("minimal", "standard", "full")
    pub preset: Option<ToolPreset>,
    /// Tools to explicitly enable (added to preset)
    pub enable: Option<Vec<String>>,
    /// Tools to explicitly disable (removed from final set, overrides enable)
    pub disable: Option<Vec<String>>,
    /// Initial setup tool behavior ("auto", "enabled", "disabled")
    pub initial_setup: Option<InitialSetupMode>,
}

impl ToolsConfigFile {
    pub(crate) fn merge(self, project: Self) -> Self {
        let enable = match (self.enable, project.enable) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_enable)) => Some(project_enable),
            (Some(global), Some(project_enable)) => {
                let mut merged: HashSet<_> = global.into_iter().collect();
                merged.extend(project_enable);
                Some(merged.into_iter().collect())
            }
        };
        let disable = match (self.disable, project.disable) {
            (None, None) => None,
            (Some(global), None) => Some(global),
            (None, Some(project_disable)) => Some(project_disable),
            (Some(global), Some(project_disable)) => {
                let mut merged: HashSet<_> = global.into_iter().collect();
                merged.extend(project_disable);
                Some(merged.into_iter().collect())
            }
        };

        ToolsConfigFile {
            preset: project.preset.or(self.preset),
            enable,
            disable,
            initial_setup: project.initial_setup.or(self.initial_setup),
        }
    }

    pub(crate) fn resolve(self) -> ToolsConfig {
        ToolsConfig {
            preset: self.preset.unwrap_or_default(),
            enable: self.enable.unwrap_or_default(),
            disable: self.disable.unwrap_or_default(),
            initial_setup: self.initial_setup.unwrap_or_default(),
        }
    }
}

impl ToolsConfig {
    /// Compute the final set of enabled tools based on preset and overrides
    pub fn enabled_tools(&self, project_config_present: bool) -> HashSet<String> {
        let initial_setup_tool = "initialSetup";
        let mut tools = get_preset_tools(self.preset);

        for tool in &self.enable {
            tools.insert(tool.clone());
        }

        match self.initial_setup {
            InitialSetupMode::Enabled => {
                tools.insert(initial_setup_tool.to_string());
            }
            InitialSetupMode::Disabled => {
                tools.remove(initial_setup_tool);
            }
            InitialSetupMode::Auto => {
                if project_config_present || self.preset != ToolPreset::Standard {
                    tools.remove(initial_setup_tool);
                }
            }
        }

        for tool in &self.disable {
            tools.remove(tool);
        }

        tools
    }

    /// Merge project config over global config for tools
    #[cfg(test)]
    pub(crate) fn merge(self, project: Self) -> Self {
        let preset = project.preset;
        let initial_setup = project.initial_setup;

        let mut enable: HashSet<_> = self.enable.into_iter().collect();
        enable.extend(project.enable);

        let mut disable: HashSet<_> = self.disable.into_iter().collect();
        disable.extend(project.disable);

        ToolsConfig {
            preset,
            enable: enable.into_iter().collect(),
            disable: disable.into_iter().collect(),
            initial_setup,
        }
    }
}
