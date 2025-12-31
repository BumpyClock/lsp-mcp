// ABOUTME: Tools configuration for the lsp-mcp server.
// ABOUTME: Manages tool presets and explicit enable/disable lists.

use crate::tool_registry::{get_preset_tools, ToolPreset};
use serde::Deserialize;
use std::collections::HashSet;

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

        for tool in &self.enable {
            tools.insert(tool.clone());
        }

        for tool in &self.disable {
            tools.remove(tool);
        }

        tools
    }

    /// Merge project config over global config for tools
    pub(crate) fn merge(self, project: Self) -> Self {
        let preset = project.preset;

        let mut enable: HashSet<_> = self.enable.into_iter().collect();
        enable.extend(project.enable);

        let mut disable: HashSet<_> = self.disable.into_iter().collect();
        disable.extend(project.disable);

        ToolsConfig {
            preset,
            enable: enable.into_iter().collect(),
            disable: disable.into_iter().collect(),
        }
    }
}
