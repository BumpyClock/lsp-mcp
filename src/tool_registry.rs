// ABOUTME: Tool tier definitions and preset computation for configurable MCP tools.
// ABOUTME: Defines which tools are core vs optional and provides preset-based tool selection.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// All available MCP tool names (must match #[tool] names in mcp.rs)
pub const ALL_TOOLS: &[&str] = &[
    "documentSymbol",
    "goToDefinition",
    "findReferences",
    "hover",
    "workspaceSymbol",
    "goToImplementation",
    "callHierarchy",
    "findReferencedSymbols",
    "findIdentifier",
    "listFiles",
    "readSourceCode",
    "health",
    "getDiagnostics",
    "initialInstructions",
    "initialSetup",
];

/// Core tools (Tier 1+2): Enabled by default in "standard" preset
pub const CORE_TOOLS: &[&str] = &[
    "goToDefinition",
    "findReferences",
    "hover",
    "getDiagnostics",
    "workspaceSymbol",
    "documentSymbol",
    "callHierarchy",
    "findReferencedSymbols",
    "initialInstructions",
    "initialSetup",
];

/// Minimal tools (Tier 1): Essential navigation only
pub const MINIMAL_TOOLS: &[&str] = &[
    "goToDefinition",
    "findReferences",
    "hover",
    "getDiagnostics",
];

/// Tool preset configurations
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolPreset {
    /// Minimal: Only essential navigation tools (4 tools)
    Minimal,
    /// Standard (default): Core tools for productive development (10 tools)
    #[default]
    Standard,
    /// Full: All available tools (15 tools)
    Full,
}

/// Get the set of enabled tools for a given preset
pub fn get_preset_tools(preset: ToolPreset) -> HashSet<String> {
    match preset {
        ToolPreset::Minimal => MINIMAL_TOOLS.iter().map(|s| (*s).to_string()).collect(),
        ToolPreset::Standard => CORE_TOOLS.iter().map(|s| (*s).to_string()).collect(),
        ToolPreset::Full => ALL_TOOLS.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// Validate that a tool name is known
pub fn is_valid_tool(name: &str) -> bool {
    ALL_TOOLS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tools_count() {
        assert_eq!(ALL_TOOLS.len(), 15, "Expected 15 total tools");
    }

    #[test]
    fn test_core_tools_count() {
        assert_eq!(CORE_TOOLS.len(), 10, "Expected 10 core tools");
    }

    #[test]
    fn test_minimal_tools_count() {
        assert_eq!(MINIMAL_TOOLS.len(), 4, "Expected 4 minimal tools");
    }

    #[test]
    fn test_get_preset_tools_minimal() {
        let tools = get_preset_tools(ToolPreset::Minimal);
        assert_eq!(tools.len(), 4);
        assert!(tools.contains("goToDefinition"));
        assert!(tools.contains("findReferences"));
        assert!(tools.contains("hover"));
        assert!(tools.contains("getDiagnostics"));
    }

    #[test]
    fn test_get_preset_tools_standard() {
        let tools = get_preset_tools(ToolPreset::Standard);
        assert_eq!(tools.len(), 10);
        // Should include all minimal tools
        assert!(tools.contains("goToDefinition"));
        assert!(tools.contains("findReferences"));
        assert!(tools.contains("hover"));
        assert!(tools.contains("getDiagnostics"));
        // Plus additional core tools
        assert!(tools.contains("workspaceSymbol"));
        assert!(tools.contains("documentSymbol"));
        assert!(tools.contains("callHierarchy"));
        assert!(tools.contains("findReferencedSymbols"));
        assert!(tools.contains("initialInstructions"));
        assert!(tools.contains("initialSetup"));
    }

    #[test]
    fn test_get_preset_tools_full() {
        let tools = get_preset_tools(ToolPreset::Full);
        assert_eq!(tools.len(), 15);
        // Should include all tools
        for tool in ALL_TOOLS {
            assert!(tools.contains(*tool), "Missing tool: {}", tool);
        }
    }

    #[test]
    fn test_minimal_is_subset_of_standard() {
        let minimal = get_preset_tools(ToolPreset::Minimal);
        let standard = get_preset_tools(ToolPreset::Standard);
        for tool in &minimal {
            assert!(standard.contains(tool), "Minimal tool {} not in standard", tool);
        }
    }

    #[test]
    fn test_standard_is_subset_of_full() {
        let standard = get_preset_tools(ToolPreset::Standard);
        let full = get_preset_tools(ToolPreset::Full);
        for tool in &standard {
            assert!(full.contains(tool), "Standard tool {} not in full", tool);
        }
    }

    #[test]
    fn test_is_valid_tool() {
        assert!(is_valid_tool("goToDefinition"));
        assert!(is_valid_tool("hover"));
        assert!(!is_valid_tool("nonexistent_tool"));
        assert!(!is_valid_tool(""));
    }

    #[test]
    fn test_preset_default() {
        let preset = ToolPreset::default();
        assert_eq!(preset, ToolPreset::Standard);
    }

    #[test]
    fn test_preset_serialization() {
        assert_eq!(
            serde_json::to_string(&ToolPreset::Minimal).unwrap(),
            "\"minimal\""
        );
        assert_eq!(
            serde_json::to_string(&ToolPreset::Standard).unwrap(),
            "\"standard\""
        );
        assert_eq!(
            serde_json::to_string(&ToolPreset::Full).unwrap(),
            "\"full\""
        );
    }

    #[test]
    fn test_preset_deserialization() {
        assert_eq!(
            serde_json::from_str::<ToolPreset>("\"minimal\"").unwrap(),
            ToolPreset::Minimal
        );
        assert_eq!(
            serde_json::from_str::<ToolPreset>("\"standard\"").unwrap(),
            ToolPreset::Standard
        );
        assert_eq!(
            serde_json::from_str::<ToolPreset>("\"full\"").unwrap(),
            ToolPreset::Full
        );
    }
}
