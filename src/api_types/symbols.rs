// ABOUTME: Symbol and identifier types for code navigation.
// ABOUTME: Includes Symbol, Identifier, RelatedSymbols, and associated context types.

use super::{FilePosition, FileRange};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct CodeContext {
    pub range: FileRange,
    pub source_code: String,
}

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// The name of the symbol.
    pub name: String,
    /// The kind of the symbol (e.g., function, class).
    pub kind: String,

    /// The start position of the symbol's identifier.
    pub identifier_position: FilePosition,

    /// The full range of the symbol.
    pub file_range: FileRange,

    /// The signature of the symbol (from LSP hover or source).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,

    /// Whether the symbol is exported (best-effort heuristic).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exported: Option<bool>,

    /// JSDoc/docstring summary (from LSP hover or source comments).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsdoc_summary: Option<String>,

    /// List of imports/dependencies used in the symbol body (best-effort).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,

    /// Number of lines in the symbol's context range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u32>,

    /// Nested child symbols (for hierarchical structure from LSP documentSymbol).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Symbol>>,

    /// Source code snippet around the symbol definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<CodeContext>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Identifier {
    pub name: String,
    pub file_range: FileRange,
    pub kind: Option<String>,
}

impl Identifier {
    /// Ensures the kind field is never None by providing a default fallback.
    /// Returns the kind if present, otherwise "identifier".
    pub fn kind_or_default(&self) -> &str {
        self.kind.as_deref().unwrap_or("identifier")
    }

    /// Creates a new Identifier with a guaranteed non-None kind.
    /// If kind is None, sets it to "identifier".
    pub fn with_kind_defaulted(mut self) -> Self {
        if self.kind.is_none() {
            self.kind = Some("identifier".to_string());
        }
        self
    }
}

/// Related symbols for a definition (interfaces, parent classes, siblings)
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct RelatedSymbols {
    /// Interfaces or traits this symbol implements
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<Symbol>,
    /// Parent classes this symbol extends
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extends: Vec<Symbol>,
    /// Types that reference this symbol
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub used_by_types: Vec<Symbol>,
    /// Other exports from the same module/file
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sibling_exports: Vec<Symbol>,
}

/// A reference to a symbol along with its definition(s) found in the workspace
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ReferenceWithSymbolDefinitions {
    pub reference: Identifier,
    pub definitions: Vec<Symbol>,
}

pub type SymbolResponse = Vec<Symbol>;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IdentifierResponse {
    pub identifiers: Vec<Identifier>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{Position, Range};

    #[test]
    fn test_identifier_kind_or_default_returns_kind_when_present() {
        let identifier = Identifier {
            name: "test_func".to_string(),
            file_range: FileRange {
                path: "test.rs".to_string(),
                range: Range {
                    start: Position { line: 1, character: 1 },
                    end: Position { line: 1, character: 10 },
                },
            },
            kind: Some("function".to_string()),
        };

        assert_eq!(
            identifier.kind_or_default(),
            "function",
            "kind_or_default must return the actual kind when present"
        );
    }

    #[test]
    fn test_identifier_kind_or_default_returns_identifier_when_none() {
        let identifier = Identifier {
            name: "unknown_symbol".to_string(),
            file_range: FileRange {
                path: "test.rs".to_string(),
                range: Range {
                    start: Position { line: 5, character: 3 },
                    end: Position { line: 5, character: 17 },
                },
            },
            kind: None,
        };

        assert_eq!(
            identifier.kind_or_default(),
            "identifier",
            "kind_or_default must return 'identifier' when kind is None"
        );
    }

    #[test]
    fn test_identifier_with_kind_defaulted_preserves_existing_kind() {
        let identifier = Identifier {
            name: "my_class".to_string(),
            file_range: FileRange {
                path: "module.py".to_string(),
                range: Range {
                    start: Position { line: 10, character: 6 },
                    end: Position { line: 10, character: 14 },
                },
            },
            kind: Some("class".to_string()),
        };

        let result = identifier.with_kind_defaulted();

        assert_eq!(
            result.kind,
            Some("class".to_string()),
            "with_kind_defaulted must preserve existing kind"
        );
    }

    #[test]
    fn test_identifier_with_kind_defaulted_sets_default_when_none() {
        let identifier = Identifier {
            name: "some_var".to_string(),
            file_range: FileRange {
                path: "script.js".to_string(),
                range: Range {
                    start: Position { line: 2, character: 4 },
                    end: Position { line: 2, character: 12 },
                },
            },
            kind: None,
        };

        let result = identifier.with_kind_defaulted();

        assert_eq!(
            result.kind,
            Some("identifier".to_string()),
            "with_kind_defaulted must set kind to 'identifier' when None"
        );
    }

    #[test]
    fn test_related_symbols_default_has_empty_vecs() {
        let related = RelatedSymbols::default();

        assert!(
            related.implements.is_empty(),
            "default implements must be empty"
        );
        assert!(
            related.extends.is_empty(),
            "default extends must be empty"
        );
        assert!(
            related.used_by_types.is_empty(),
            "default used_by_types must be empty"
        );
        assert!(
            related.sibling_exports.is_empty(),
            "default sibling_exports must be empty"
        );
    }

    #[test]
    fn test_related_symbols_serialization_skips_empty_vecs() {
        let related = RelatedSymbols::default();
        let json = serde_json::to_value(&related).expect("serialization failed");

        assert!(
            json.get("implements").is_none(),
            "empty implements must be skipped in serialization"
        );
        assert!(
            json.get("extends").is_none(),
            "empty extends must be skipped in serialization"
        );
        assert!(
            json.get("used_by_types").is_none(),
            "empty used_by_types must be skipped in serialization"
        );
        assert!(
            json.get("sibling_exports").is_none(),
            "empty sibling_exports must be skipped in serialization"
        );
    }

    #[test]
    fn test_related_symbols_serializes_non_empty_vec() {
        let sibling = Symbol {
            name: "sibling_fn".to_string(),
            kind: "function".to_string(),
            identifier_position: FilePosition {
                path: "module.rs".to_string(),
                position: Position { line: 20, character: 4 },
            },
            file_range: FileRange {
                path: "module.rs".to_string(),
                range: Range {
                    start: Position { line: 20, character: 1 },
                    end: Position { line: 25, character: 1 },
                },
            },
            ..Default::default()
        };

        let related = RelatedSymbols {
            sibling_exports: vec![sibling],
            ..Default::default()
        };

        let json = serde_json::to_value(&related).expect("serialization failed");

        assert!(
            json.get("sibling_exports").is_some(),
            "non-empty sibling_exports must be serialized"
        );
        assert!(
            json.get("implements").is_none(),
            "empty implements must still be skipped"
        );
    }

    #[test]
    fn test_symbol_snippet_serializes_when_present() {
        let snippet = CodeContext {
            range: FileRange {
                path: "test.rs".to_string(),
                range: Range {
                    start: Position { line: 9, character: 1 },
                    end: Position { line: 11, character: 1 },
                },
            },
            source_code: "fn foo\tbar() {\n    bar()\n}".to_string(),
        };

        let symbol = Symbol {
            name: "foo\tbar".to_string(),
            kind: "function".to_string(),
            identifier_position: FilePosition {
                path: "test.rs".to_string(),
                position: Position { line: 10, character: 4 },
            },
            file_range: FileRange {
                path: "test.rs".to_string(),
                range: Range {
                    start: Position { line: 10, character: 1 },
                    end: Position { line: 12, character: 1 },
                },
            },
            snippet: Some(snippet),
            ..Default::default()
        };

        let json = serde_json::to_value(&symbol).expect("negative: serialization failed");

        assert!(
            json.get("snippet").is_some(),
            "negative: snippet must be serialized when present"
        );
        assert!(
            json["snippet"]["source_code"]
                .as_str()
                .unwrap()
                .contains("fn foo\tbar()"),
            "negative: snippet must contain source code"
        );
    }

    #[test]
    fn test_symbol_snippet_skipped_when_none() {
        let symbol = Symbol {
            name: "bar\tvalue".to_string(),
            kind: "variable".to_string(),
            identifier_position: FilePosition {
                path: "test.rs".to_string(),
                position: Position { line: 5, character: 5 },
            },
            file_range: FileRange {
                path: "test.rs".to_string(),
                range: Range {
                    start: Position { line: 5, character: 1 },
                    end: Position { line: 5, character: 20 },
                },
            },
            snippet: None,
            ..Default::default()
        };

        let json = serde_json::to_value(&symbol).expect("negative: serialization failed");

        assert!(
            json.get("snippet").is_none(),
            "negative: snippet must be omitted when None"
        );
    }
}
