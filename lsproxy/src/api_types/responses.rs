// ABOUTME: Response types for LSP operations.
// ABOUTME: Includes definition, references, hover, workspace symbol, and implementation responses.

use super::{CodeContext, FilePosition, Identifier, Range};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Response to a definition request.
///
/// The definition(s) of the symbol.
/// Points to the start position of the symbol's identifier.
///
/// e.g. for the definition of `User` on line 5 of `src/main.py` with the code:
/// ```
/// 0: class User:
/// _________^
/// 1:     def __init__(self, name, age):
/// 2:         self.name = name
/// 3:         self.age = age
/// 4:
/// 5: user = User("John", 30)
/// __________^
/// ```
/// The definition(s) will be `[{"path": "src/main.py", "line": 0, "character": 6}]`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct DefinitionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The raw response from the langserver.
    ///
    /// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_definition
    pub raw_response: Option<Value>,
    pub definitions: Vec<FilePosition>,
    /// The source code of symbol definitions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_code_context: Option<Vec<CodeContext>>,
    /// The identifier that was "clicked-on" to get the definition.
    pub selected_identifier: Identifier,
}

/// Response to a references request.
///
/// Points to the start position of the symbol's identifier.
///
/// e.g. for the references of `User` on line 0 character 6 of `src/main.py` with the code:
/// ```
/// 0: class User:
/// 1:     def __init__(self, name, age):
/// 2:         self.name = name
/// 3:         self.age = age
/// 4:
/// 5: user = User("John", 30)
/// _________^
/// 6:
/// 7: print(user.name)
/// ```
/// The references will be `[{"path": "src/main.py", "line": 5, "character": 7}]`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ReferencesResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The raw response from the langserver.
    ///
    /// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_references
    pub raw_response: Option<Value>,

    pub references: Vec<FilePosition>,

    /// The source code around the references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<CodeContext>>,
    /// The identifier that was "clicked-on" to get the references.
    pub selected_identifier: Identifier,
}

/// Response containing symbols referenced from the requested position
///
/// The symbols are categorized into:
/// - workspace_symbols: References to symbols that were found and have definitions in the workspace
/// - external_symbols: References to symbols from outside the workspace (built-in functions, external libraries)
/// - not_found: References where the symbol definition could not be found
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ReferencedSymbolsResponse {
    pub workspace_symbols: Vec<super::ReferenceWithSymbolDefinitions>,
    pub external_symbols: Vec<Identifier>,
    pub not_found: Vec<Identifier>,
}

/// Request for a single hover operation (used in batch mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverRequest {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

/// A single item in a batch hover response - either success or error
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HoverBatchItem {
    Success(HoverResponse),
    Error { error: String },
}

/// Minimal definition location for hover response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefinitionLocation {
    /// Path to the file containing the definition
    pub path: String,
    /// Line number (1-indexed)
    pub line: u32,
    /// True if definition is in node_modules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
}

/// Response to a hover request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverResponse {
    /// The raw response from the langserver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    /// The hover contents (documentation, type info)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<HoverContents>,
    /// The range of the symbol being hovered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    /// Definition locations (optional, when include_definition is true)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub definitions: Vec<DefinitionLocation>,
}

/// The contents of a hover response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HoverContents {
    /// Plain text or markdown content
    Markup(String),
    /// Multiple content items
    Array(Vec<String>),
}

/// Response to a workspace symbol request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSymbolResponse {
    /// The raw response from the langserver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    /// The symbols found matching the query
    pub symbols: Vec<WorkspaceSymbolInfo>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

/// Response to a go-to-implementation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationResponse {
    /// The raw response from the langserver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    /// The implementations found
    pub implementations: Vec<FilePosition>,
    /// The identifier that was queried
    pub selected_identifier: Identifier,
}

/// Information about a workspace symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSymbolInfo {
    /// The name of the symbol
    pub name: String,
    /// The kind of the symbol (function, class, etc.)
    pub kind: String,
    /// The location of the symbol
    pub location: FilePosition,
    /// The containing symbol name (e.g., class name for a method)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_score: Option<f32>,
    /// Type signature from LSP hover (e.g., "fn example() -> Result<T, E>")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_definition_location_serialization() {
        let def_loc = DefinitionLocation {
            path: "node_modules/@reduxjs/toolkit/dist/index.d.mts".to_string(),
            line: 1847,
            external: Some(true),
        };

        let json = serde_json::to_value(&def_loc).expect("failed to serialize");

        assert_eq!(json["path"], "node_modules/@reduxjs/toolkit/dist/index.d.mts");
        assert_eq!(json["line"], 1847);
        assert_eq!(json["external"], true);
    }

    #[test]
    fn test_hover_response_with_definition() {
        let hover = HoverResponse {
            raw_response: None,
            contents: Some(HoverContents::Markup("```typescript\nfunction configureStore<S>(): EnhancedStore\n```".to_string())),
            range: None,
            definitions: vec![DefinitionLocation {
                path: "node_modules/@reduxjs/toolkit/dist/index.d.mts".to_string(),
                line: 1847,
                external: Some(true),
            }],
        };

        let json = serde_json::to_value(&hover).expect("failed to serialize");

        assert!(json.get("definitions").is_some(), "definitions must be present");
        assert_eq!(json["definitions"][0]["path"], "node_modules/@reduxjs/toolkit/dist/index.d.mts");
        assert_eq!(json["definitions"][0]["line"], 1847);
        assert_eq!(json["definitions"][0]["external"], true);
    }

    #[test]
    fn test_hover_response_skips_none_definition() {
        let hover = HoverResponse {
            raw_response: None,
            contents: Some(HoverContents::Markup("some docs".to_string())),
            range: None,
            definitions: Vec::new(),
        };

        let json = serde_json::to_value(&hover).expect("failed to serialize");

        assert!(json.get("definitions").is_none(), "empty definitions must be skipped in serialization");
    }
}
