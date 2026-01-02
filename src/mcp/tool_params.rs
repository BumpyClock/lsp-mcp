// ABOUTME: Parameter structs for MCP tools.
// ABOUTME: Strongly-typed parameter definitions for all 16 MCP tools with serde and JSON schema support.

use schemars::JsonSchema;
use serde::Deserialize;

use super::serde_helpers::{deserialize_flexible_u32, deserialize_flexible_u32_opt};

/// Parameters for the documentSymbol tool.
/// Returns symbols defined in a file (top-level only by default; set include_children for nesting).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocumentSymbolParams {
    pub path: String,
    /// Include local/private symbols (default: false)
    pub include_locals: Option<bool>,
    /// Include nested children in hierarchical response (default: false)
    pub include_children: Option<bool>,
    /// Maximum number of symbols to return
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub limit: Option<u32>,
    /// Number of symbols to skip for pagination
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub offset: Option<u32>,
    /// Number of context lines to include in snippets
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub context_lines: Option<u32>,
}

/// Parameters for the goToDefinition tool.
/// Returns the definition location of a symbol at a given position.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GoToDefinitionParams {
    pub path: String,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub line: u32,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub character: u32,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub limit: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub offset: Option<u32>,
}

/// Parameters for the findReferences tool.
/// Returns all references to a symbol at a given position.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindReferencesParams {
    pub path: String,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub line: u32,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub character: u32,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub context_lines: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub limit: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub offset: Option<u32>,
}

/// Parameters for the hover tool.
/// Returns hover information (documentation, type info) for a symbol at a given position.
/// Supports both single-position and batch mode via requests parameter.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct HoverParams {
    pub path: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub line: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub character: Option<u32>,
    pub include_definition: Option<bool>,
    /// JSON string containing array of hover requests for batch mode.
    pub requests: Option<String>,
}

/// Parameters for the workspaceSymbol tool.
/// Searches for symbols across the entire workspace by name.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkspaceSymbolParams {
    pub query: String,
    pub exact: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub limit: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub offset: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub context_lines: Option<u32>,
}

/// Parameters for the goToImplementation tool.
/// Returns implementations of an interface or abstract method.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GoToImplementationParams {
    pub path: String,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub line: u32,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub character: u32,
}

/// Parameters for the callHierarchy tool.
/// Returns incoming or outgoing calls for a function at a given position.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CallHierarchyParams {
    pub path: String,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub line: u32,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub character: u32,
    pub direction: String,
    pub externals: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub context_lines: Option<u32>,
}

/// Parameters for the findReferencedSymbols tool.
/// Returns all symbols referenced by a definition at a given position.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindReferencedSymbolsParams {
    pub path: String,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub line: u32,
    #[serde(deserialize_with = "deserialize_flexible_u32")]
    #[schemars(with = "u32")]
    pub character: u32,
    pub full_scan: Option<bool>,
    pub externals: Option<bool>,
}

/// Parameters for the findIdentifier tool.
/// Returns all occurrences of a specific identifier name within a file.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindIdentifierParams {
    pub path: String,
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub line: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub character: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub limit: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub offset: Option<u32>,
}

/// Parameters for the listFiles tool.
/// Returns a list of all files in the workspace.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListFilesParams {
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub limit: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub offset: Option<u32>,
}

/// Parameters for the readSourceCode tool.
/// Reads source code from a file with optional range selection.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadSourceCodeParams {
    pub path: String,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub start_line: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub start_character: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub end_line: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub end_character: Option<u32>,
}

/// Parameters for the getDiagnostics tool.
/// Returns diagnostics (errors, warnings) for a file or the entire workspace.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDiagnosticsParams {
    pub file_path: Option<String>,
}

/// Parameters for the semanticSearch tool.
/// Performs semantic code search using natural language queries over indexed code chunks.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SemanticSearchParams {
    pub query: String,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub limit: Option<u32>,
    pub path: Option<String>,
    pub file_pattern: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub min_score: Option<f32>,
    pub per_file: Option<bool>,
    pub rerank: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
    #[schemars(with = "Option<u32>")]
    pub context_lines: Option<u32>,
}
