// ABOUTME: Response types for MCP service layer operations.
// ABOUTME: Defines structured responses for definitions, references, symbols, and files.

use crate::api_types::{CodeContext, Identifier, Position, Range, Symbol};
use crate::service::utils::external::PackageInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReferenceType {
    Definition,
    Import,
    Call,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpDefinitionLocation {
    pub path: String,
    pub position: Position,
    pub definition_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<CodeContext>,
    /// Type signature from LSP hover (e.g., "(arg: Type) => ReturnType")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Documentation string (JSDoc, docstring, etc.) from LSP hover
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// True if definition is in node_modules (external dependency)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    /// Package info if external (name and version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageInfo>,
    /// Number of references to this symbol in the workspace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_count: Option<u32>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpDefinitionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    pub definitions: Vec<McpDefinitionLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_code_context: Option<Vec<CodeContext>>,
    pub selected_identifier: Identifier,
    /// Related symbols (interfaces implemented, parent classes, sibling exports)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related: Option<crate::api_types::RelatedSymbols>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpReferenceLocation {
    /// Path to file (omitted when grouped by file, as FileGroup provides it)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub position: Position,
    pub symbol_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<CodeContext>,
    pub reference_type: ReferenceType,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct FileGroup {
    pub path: String,
    pub count: u32,
    pub refs: Vec<McpReferenceLocation>,
}

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct TypeCounts {
    pub definition: u32,
    pub import: u32,
    pub call: u32,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpReferencesResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    pub selected_identifier: Identifier,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
    pub total_count: u32,
    pub by_file: Vec<FileGroup>,
    pub by_type: TypeCounts,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpSymbolsResponse {
    /// Path to the file, relative to workspace root
    pub path: String,
    /// File modification time in RFC3339 UTC format
    pub mtime: String,
    pub symbols: Vec<Symbol>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpIdentifierResponse {
    pub identifiers: Vec<Identifier>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpListFilesResponse {
    pub files: Vec<String>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

/// Ultra-compact response format for find_definition (~180 chars)
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct CompactDefinitionResponse {
    pub name: String,
    pub sig: String,
    pub loc: String,
    pub ext: bool,
}
