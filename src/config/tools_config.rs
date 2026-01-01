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
