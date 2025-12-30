use lsp_types::{Location, LocationLink};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};
use strum_macros::{Display, EnumString};

use crate::utils::file_utils::uri_to_relative_path_string;

static GLOBAL_MOUNT_DIR: LazyLock<Arc<RwLock<PathBuf>>> =
    LazyLock::new(|| Arc::new(RwLock::new(PathBuf::from("/mnt/workspace"))));

thread_local! {
    static THREAD_LOCAL_MOUNT_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub fn get_mount_dir() -> PathBuf {
    THREAD_LOCAL_MOUNT_DIR.with(|local| {
        local
            .borrow()
            .clone()
            .unwrap_or_else(|| GLOBAL_MOUNT_DIR.read().unwrap().clone())
    })
}

pub fn set_thread_local_mount_dir(path: impl AsRef<Path>) {
    THREAD_LOCAL_MOUNT_DIR.with(|local| {
        *local.borrow_mut() = Some(path.as_ref().to_path_buf());
    });
}

pub fn unset_thread_local_mount_dir() {
    THREAD_LOCAL_MOUNT_DIR.with(|local| {
        *local.borrow_mut() = None;
    });
}

pub fn set_global_mount_dir(path: impl AsRef<Path>) {
    let mut global_dir = GLOBAL_MOUNT_DIR.write().unwrap();
    *global_dir = path.as_ref().to_path_buf();
}

/// Response returned when an API error occurs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Description of the error that occurred
    pub error: String,
}

/// Status of a language server
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LspStatus {
    /// Language server is available and ready
    Ready,
    /// Language server is starting up in the background
    Initializing,
    /// Language server is not available (not installed or failed to start)
    Unavailable,
}

/// Response returned by the health check endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Current status of the service ("ok" or error description)
    pub status: String,
    /// Version of the service
    pub version: String,
    /// Map of supported languages and their availability status
    pub languages: HashMap<SupportedLanguages, LspStatus>,
}

#[derive(Debug, EnumString, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[strum(serialize_all = "lowercase")]
pub enum SupportedLanguages {
    #[serde(rename = "python")]
    Python,
    /// TypeScript and JavaScript are handled by the same langserver
    #[serde(rename = "typescript_javascript")]
    #[strum(serialize = "typescript", serialize = "javascript", serialize = "typescriptjavascript")]
    TypeScriptJavaScript,
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "cpp")]
    CPP,
    #[serde(rename = "csharp")]
    CSharp,
    #[serde(rename = "java")]
    Java,
    #[serde(rename = "golang")]
    Golang,
    #[serde(rename = "php")]
    PHP,
    #[serde(rename = "ruby")]
    Ruby,
}

/// A position within a text document, using 1-based indexing (matching editor display)
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Position {
    /// 1-indexed line number (first line is 1).
    pub line: u32,
    /// 1-indexed character/column within the line (first column is 1).
    pub character: u32,
}

/// A position within a specific file in the workspace
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct FilePosition {
    /// Path to the file, relative to the workspace root
    pub path: String,
    /// Position within the file
    pub position: Position,
}

/// A range within a specific file, defined by start and end positions
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct FileRange {
    /// The path to the file.
    pub path: String,
    /// The range within the file
    pub range: Range,
}

impl FileRange {
    pub fn contains(&self, position: FilePosition) -> bool {
        let pos = &position.position;
        self.path == position.path
            && self.range.start.line <= pos.line
            && self.range.end.line >= pos.line
            && (self.range.start.line != pos.line || self.range.start.character <= pos.character)
            && (self.range.end.line != pos.line || self.range.end.character >= pos.character)
    }
}

impl From<FileRange> for lsp_types::Range {
    fn from(range: FileRange) -> Self {
        lsp_types::Range::new(
            lsp_types::Position::from(range.range.start),
            lsp_types::Position::from(range.range.end),
        )
    }
}

impl From<Position> for lsp_types::Position {
    fn from(position: Position) -> Self {
        // Convert from 1-indexed (user-facing) to 0-indexed (LSP internal)
        lsp_types::Position {
            line: position.line.saturating_sub(1),
            character: position.character.saturating_sub(1),
        }
    }
}

impl From<lsp_types::Position> for Position {
    fn from(position: lsp_types::Position) -> Self {
        // Convert from 0-indexed (LSP internal) to 1-indexed (user-facing)
        Position {
            line: position.line + 1,
            character: position.character + 1,
        }
    }
}

/// A reference to a symbol along with its definition(s) found in the workspace
///
/// e.g. for a reference to `User` in `main.py`:
/// ```python
/// user = User("John", 30)
/// _______^
/// ```
/// This would contain:
/// - The reference location and name ("User" at line 0)
/// - The symbol definition(s) (e.g. "class User" in models.py)
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ReferenceWithSymbolDefinitions {
    pub reference: Identifier,
    pub definitions: Vec<Symbol>,
}

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
    pub workspace_symbols: Vec<ReferenceWithSymbolDefinitions>,
    pub external_symbols: Vec<Identifier>,
    pub not_found: Vec<Identifier>,
}

pub type SymbolResponse = Vec<Symbol>;

impl From<Location> for FilePosition {
    fn from(location: Location) -> Self {
        // Convert from 0-indexed (LSP internal) to 1-indexed (user-facing)
        FilePosition {
            path: uri_to_relative_path_string(&location.uri),
            position: Position {
                line: location.range.start.line + 1,
                character: location.range.start.character + 1,
            },
        }
    }
}

impl From<LocationLink> for FilePosition {
    fn from(link: LocationLink) -> Self {
        // Convert from 0-indexed (LSP internal) to 1-indexed (user-facing)
        FilePosition {
            path: uri_to_relative_path_string(&link.target_uri),
            position: Position {
                line: link.target_range.start.line + 1,
                character: link.target_range.start.character + 1,
            },
        }
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IdentifierResponse {
    pub identifiers: Vec<Identifier>,
}

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Range {
    /// The start position of the range.
    pub start: Position,
    /// The end position of the range.
    pub end: Position,
}


/// Diagnostic severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl From<lsp_types::DiagnosticSeverity> for DiagnosticSeverity {
    fn from(severity: lsp_types::DiagnosticSeverity) -> Self {
        match severity {
            lsp_types::DiagnosticSeverity::ERROR => DiagnosticSeverity::Error,
            lsp_types::DiagnosticSeverity::WARNING => DiagnosticSeverity::Warning,
            lsp_types::DiagnosticSeverity::INFORMATION => DiagnosticSeverity::Information,
            lsp_types::DiagnosticSeverity::HINT => DiagnosticSeverity::Hint,
            _ => DiagnosticSeverity::Error, // Fallback for unknown severity
        }
    }
}

/// Aggregated counts of diagnostics by severity level
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeverityCounts {
    /// Number of error-level diagnostics
    pub error: u32,
    /// Number of warning-level diagnostics
    pub warning: u32,
    /// Number of informational diagnostics
    pub info: u32,
    /// Number of hint-level diagnostics
    pub hint: u32,
}

/// A diagnostic message (error, warning, etc.) for a specific location
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The range where the diagnostic applies
    pub range: Range,
    /// The severity of the diagnostic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<DiagnosticSeverity>,
    /// The diagnostic's code (e.g., error number)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The source of the diagnostic (e.g., "typescript", "eslint")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The diagnostic message
    pub message: String,
    /// Whether a quick-fix code action is available for this diagnostic
    pub has_quick_fix: bool,
}

impl From<lsp_types::Diagnostic> for Diagnostic {
    fn from(diag: lsp_types::Diagnostic) -> Self {
        // Convert from 0-indexed (LSP internal) to 1-indexed (user-facing)
        Self {
            range: Range {
                start: Position {
                    line: diag.range.start.line + 1,
                    character: diag.range.start.character + 1,
                },
                end: Position {
                    line: diag.range.end.line + 1,
                    character: diag.range.end.character + 1,
                },
            },
            severity: diag.severity.map(DiagnosticSeverity::from),
            code: diag.code.map(|c| match c {
                lsp_types::NumberOrString::Number(n) => n.to_string(),
                lsp_types::NumberOrString::String(s) => s,
            }),
            source: diag.source,
            message: diag.message,
            has_quick_fix: false,
        }
    }
}

/// Diagnostics for a single file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDiagnostics {
    /// Path to the file, relative to workspace root
    pub path: String,
    /// The diagnostics for this file
    pub diagnostics: Vec<Diagnostic>,
}

/// Response containing diagnostics for one or more files
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticsResponse {
    /// Total number of diagnostics across all files
    pub total_count: usize,
    /// Counts of diagnostics aggregated by severity level
    pub by_severity: SeverityCounts,
    /// Diagnostics grouped by file
    pub files: Vec<FileDiagnostics>,
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
    /// Definition location (optional, when include_definition is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<DefinitionLocation>,
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

/// A call hierarchy item representing a function/method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHierarchyItemInfo {
    /// The name of the function/method
    pub name: String,
    /// The kind (function, method, constructor, etc.)
    pub kind: String,
    /// Location of the function/method identifier
    pub location: FilePosition,
    /// The full range of the function/method
    pub range: Range,
    /// Detail information (e.g., signature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Response to prepareCallHierarchy request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareCallHierarchyResponse {
    /// The raw response from the langserver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    /// The call hierarchy items at the position
    pub items: Vec<CallHierarchyItemInfo>,
}

/// An incoming call (caller) in the call hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingCallInfo {
    /// The calling function/method
    pub from: CallHierarchyItemInfo,
    /// The ranges where the call occurs within the calling function
    pub from_ranges: Vec<Range>,
}

/// Response to incomingCalls request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingCallsResponse {
    /// The raw response from the langserver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    /// The incoming calls (callers)
    pub calls: Vec<IncomingCallInfo>,
}

/// An outgoing call (callee) in the call hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingCallInfo {
    /// The called function/method
    pub to: CallHierarchyItemInfo,
    /// The ranges where the call occurs
    pub from_ranges: Vec<Range>,
}

/// Response to outgoingCalls request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingCallsResponse {
    /// The raw response from the langserver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    /// The outgoing calls (callees)
    pub calls: Vec<OutgoingCallInfo>,
}

/// Direction for call hierarchy traversal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallHierarchyDirection {
    /// Find callers of the function (incoming calls)
    Incoming,
    /// Find callees of the function (outgoing calls)
    Outgoing,
}

/// A call in the call hierarchy (either incoming or outgoing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallInfo {
    /// The function/method involved in the call (caller for incoming, callee for outgoing)
    pub item: CallHierarchyItemInfo,
    /// The ranges where the call occurs
    pub call_ranges: Vec<Range>,
}

/// Unified response for call hierarchy requests (both incoming and outgoing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHierarchyResponse {
    /// The direction of the call hierarchy traversal
    pub direction: CallHierarchyDirection,
    /// The raw response from the langserver (always None at service layer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<Value>,
    /// The calls found
    pub calls: Vec<CallInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_multi_line_range() {
        let range = FileRange {
            path: "test.rs".to_string(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 12,
                    character: 10,
                },
            },
        };

        // Test positions within the range
        assert!(
            range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 11,
                    character: 0
                }
            }),
            "middle line should be contained"
        );
        assert!(
            range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 10,
                    character: 5
                }
            }),
            "start position should be contained"
        );
        assert!(
            range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 12,
                    character: 10
                }
            }),
            "end position should be contained"
        );
    }

    #[test]
    fn test_contains_multi_line_range_outside_positions() {
        let range = FileRange {
            path: "test.rs".to_string(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 12,
                    character: 10,
                },
            },
        };

        assert!(
            !range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 9,
                    character: 0
                }
            }),
            "line before start should not be contained"
        );
        assert!(
            !range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 13,
                    character: 0
                }
            }),
            "line after end should not be contained"
        );
        assert!(
            !range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 10,
                    character: 4
                }
            }),
            "position before start on first line should not be contained"
        );
        assert!(
            !range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 12,
                    character: 11
                }
            }),
            "position after end on last line should not be contained"
        );
    }

    #[test]
    fn test_contains_single_line_range() {
        let single_line_range = FileRange {
            path: "test.rs".to_string(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 10,
                    character: 10,
                },
            },
        };

        assert!(
            single_line_range.contains(FilePosition {
                path: single_line_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 7
                }
            }),
            "position within single line range should be contained"
        );
        assert!(
            !single_line_range.contains(FilePosition {
                path: single_line_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 4
                }
            }),
            "position before single line range should not be contained"
        );
        assert!(
            !single_line_range.contains(FilePosition {
                path: single_line_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 11
                }
            }),
            "position after single line range should not be contained"
        );
    }

    #[test]
    fn test_contains_zero_width_range() {
        let zero_width_range = FileRange {
            path: "test.rs".to_string(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 10,
                    character: 5,
                },
            },
        };

        assert!(
            zero_width_range.contains(FilePosition {
                path: zero_width_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 5
                }
            }),
            "position at zero-width range should be contained"
        );
        assert!(
            !zero_width_range.contains(FilePosition {
                path: zero_width_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 4
                }
            }),
            "position before zero-width range should not be contained"
        );
        assert!(
            !zero_width_range.contains(FilePosition {
                path: zero_width_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 6
                }
            }),
            "position after zero-width range should not be contained"
        );
    }

    #[test]
    fn test_diagnostic_severity_from_lsp() {
        assert_eq!(
            DiagnosticSeverity::from(lsp_types::DiagnosticSeverity::ERROR),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticSeverity::from(lsp_types::DiagnosticSeverity::WARNING),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            DiagnosticSeverity::from(lsp_types::DiagnosticSeverity::INFORMATION),
            DiagnosticSeverity::Information
        );
        assert_eq!(
            DiagnosticSeverity::from(lsp_types::DiagnosticSeverity::HINT),
            DiagnosticSeverity::Hint
        );
    }

    #[test]
    fn test_diagnostic_from_lsp_full() {
        let lsp_diag = lsp_types::Diagnostic {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: 10,
                    character: 5,
                },
                end: lsp_types::Position {
                    line: 10,
                    character: 15,
                },
            },
            severity: Some(lsp_types::DiagnosticSeverity::ERROR),
            code: Some(lsp_types::NumberOrString::String("E0001".to_string())),
            source: Some("rustc".to_string()),
            message: "mismatched types".to_string(),
            ..Default::default()
        };

        let diag = Diagnostic::from(lsp_diag);

        // Expect 1-indexed output (LSP input 10 -> output 11)
        assert_eq!(diag.range.start.line, 11);
        assert_eq!(diag.range.start.character, 6);
        assert_eq!(diag.range.end.line, 11);
        assert_eq!(diag.range.end.character, 16);
        assert_eq!(diag.severity, Some(DiagnosticSeverity::Error));
        assert_eq!(diag.code, Some("E0001".to_string()));
        assert_eq!(diag.source, Some("rustc".to_string()));
        assert_eq!(diag.message, "mismatched types");
    }

    #[test]
    fn test_diagnostic_from_lsp_with_numeric_code() {
        let lsp_diag = lsp_types::Diagnostic {
            range: lsp_types::Range::default(),
            severity: Some(lsp_types::DiagnosticSeverity::WARNING),
            code: Some(lsp_types::NumberOrString::Number(42)),
            source: None,
            message: "test warning".to_string(),
            ..Default::default()
        };

        let diag = Diagnostic::from(lsp_diag);

        assert_eq!(diag.code, Some("42".to_string()));
        assert_eq!(diag.severity, Some(DiagnosticSeverity::Warning));
    }

    #[test]
    fn test_diagnostic_from_lsp_minimal() {
        let lsp_diag = lsp_types::Diagnostic {
            range: lsp_types::Range::default(),
            severity: None,
            code: None,
            source: None,
            message: "simple error".to_string(),
            ..Default::default()
        };

        let diag = Diagnostic::from(lsp_diag);

        assert_eq!(diag.severity, None);
        assert_eq!(diag.code, None);
        assert_eq!(diag.source, None);
        assert_eq!(diag.message, "simple error");
    }

    #[test]
    fn test_diagnostics_response_structure() {
        let response = DiagnosticsResponse {
            total_count: 2,
            by_severity: SeverityCounts {
                error: 1,
                warning: 1,
                info: 0,
                hint: 0,
            },
            files: vec![
                FileDiagnostics {
                    path: "src/main.rs".to_string(),
                    diagnostics: vec![Diagnostic {
                        range: Range {
                            start: Position { line: 1, character: 0 },
                            end: Position { line: 1, character: 10 },
                        },
                        severity: Some(DiagnosticSeverity::Error),
                        code: Some("E0001".to_string()),
                        source: Some("rustc".to_string()),
                        message: "error 1".to_string(),
                        has_quick_fix: false,
                    }],
                },
                FileDiagnostics {
                    path: "src/lib.rs".to_string(),
                    diagnostics: vec![Diagnostic {
                        range: Range {
                            start: Position { line: 5, character: 0 },
                            end: Position { line: 5, character: 20 },
                        },
                        severity: Some(DiagnosticSeverity::Warning),
                        code: None,
                        source: None,
                        message: "warning 1".to_string(),
                        has_quick_fix: true,
                    }],
                },
            ],
        };

        assert_eq!(response.total_count, 2);
        assert_eq!(response.by_severity.error, 1);
        assert_eq!(response.by_severity.warning, 1);
        assert_eq!(response.files.len(), 2);
        assert_eq!(response.files[0].path, "src/main.rs");
        assert_eq!(response.files[0].diagnostics.len(), 1);
        assert_eq!(response.files[1].path, "src/lib.rs");
        assert_eq!(response.files[1].diagnostics.len(), 1);
    }

    #[test]
    fn test_call_hierarchy_direction_serializes_to_lowercase() {
        let incoming = CallHierarchyDirection::Incoming;
        let outgoing = CallHierarchyDirection::Outgoing;

        let incoming_json = serde_json::to_string(&incoming).expect("failed to serialize incoming");
        let outgoing_json = serde_json::to_string(&outgoing).expect("failed to serialize outgoing");

        assert_eq!(
            incoming_json, "\"incoming\"",
            "incoming direction must serialize to lowercase"
        );
        assert_eq!(
            outgoing_json, "\"outgoing\"",
            "outgoing direction must serialize to lowercase"
        );
    }

    #[test]
    fn test_call_hierarchy_direction_deserializes_from_lowercase() {
        let incoming: CallHierarchyDirection =
            serde_json::from_str("\"incoming\"").expect("failed to deserialize incoming");
        let outgoing: CallHierarchyDirection =
            serde_json::from_str("\"outgoing\"").expect("failed to deserialize outgoing");

        assert_eq!(
            incoming,
            CallHierarchyDirection::Incoming,
            "incoming string must deserialize to Incoming variant"
        );
        assert_eq!(
            outgoing,
            CallHierarchyDirection::Outgoing,
            "outgoing string must deserialize to Outgoing variant"
        );
    }

    #[test]
    fn test_call_hierarchy_direction_equality() {
        let incoming1 = CallHierarchyDirection::Incoming;
        let incoming2 = CallHierarchyDirection::Incoming;
        let outgoing = CallHierarchyDirection::Outgoing;

        assert_eq!(incoming1, incoming2, "same variants must be equal");
        assert_ne!(
            incoming1, outgoing,
            "different variants must not be equal"
        );
    }

    #[test]
    fn test_call_info_serializes_with_item_and_call_ranges() {
        let call_info = CallInfo {
            item: CallHierarchyItemInfo {
                name: "test_function".to_string(),
                kind: "function".to_string(),
                location: FilePosition {
                    path: "src/test.rs".to_string(),
                    position: Position {
                        line: 10,
                        character: 5,
                    },
                },
                range: Range {
                    start: Position {
                        line: 10,
                        character: 1,
                    },
                    end: Position {
                        line: 20,
                        character: 1,
                    },
                },
                detail: Some("fn test_function()".to_string()),
            },
            call_ranges: vec![Range {
                start: Position {
                    line: 15,
                    character: 10,
                },
                end: Position {
                    line: 15,
                    character: 25,
                },
            }],
        };

        let json = serde_json::to_value(&call_info).expect("failed to serialize call info");

        assert!(json.get("item").is_some(), "item field must be present");
        assert!(
            json.get("call_ranges").is_some(),
            "call_ranges field must be present"
        );
        assert_eq!(
            json["item"]["name"], "test_function",
            "item name must match"
        );
    }

    #[test]
    fn test_call_hierarchy_response_serializes_incoming_calls() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Incoming,
            raw_response: None,
            calls: vec![CallInfo {
                item: CallHierarchyItemInfo {
                    name: "caller_fn".to_string(),
                    kind: "function".to_string(),
                    location: FilePosition {
                        path: "src/caller.rs".to_string(),
                        position: Position { line: 5, character: 1 },
                    },
                    range: Range {
                        start: Position { line: 5, character: 1 },
                        end: Position { line: 10, character: 1 },
                    },
                    detail: None,
                },
                call_ranges: vec![Range {
                    start: Position { line: 7, character: 5 },
                    end: Position { line: 7, character: 20 },
                }],
            }],
        };

        let json = serde_json::to_value(&response).expect("failed to serialize response");

        assert_eq!(
            json["direction"], "incoming",
            "direction must be lowercase incoming"
        );
        assert!(
            json.get("raw_response").is_none(),
            "raw_response must be skipped when None"
        );
        assert_eq!(json["calls"].as_array().unwrap().len(), 1, "must have one call");
    }

    #[test]
    fn test_call_hierarchy_response_serializes_outgoing_calls() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Outgoing,
            raw_response: Some(serde_json::json!({"test": "data"})),
            calls: vec![],
        };

        let json = serde_json::to_value(&response).expect("failed to serialize response");

        assert_eq!(
            json["direction"], "outgoing",
            "direction must be lowercase outgoing"
        );
        assert!(
            json.get("raw_response").is_some(),
            "raw_response must be present when Some"
        );
        assert_eq!(
            json["raw_response"]["test"], "data",
            "raw_response content must match"
        );
    }

    #[test]
    fn test_call_hierarchy_response_deserializes_correctly() {
        let json_str = r#"{
            "direction": "incoming",
            "calls": [{
                "item": {
                    "name": "test_fn",
                    "kind": "function",
                    "location": {"path": "test.rs", "position": {"line": 1, "character": 1}},
                    "range": {"start": {"line": 1, "character": 1}, "end": {"line": 5, "character": 1}}
                },
                "call_ranges": []
            }]
        }"#;

        let response: CallHierarchyResponse =
            serde_json::from_str(json_str).expect("failed to deserialize response");

        assert_eq!(
            response.direction,
            CallHierarchyDirection::Incoming,
            "direction must be Incoming"
        );
        assert_eq!(response.calls.len(), 1, "must have one call");
        assert_eq!(
            response.calls[0].item.name, "test_fn",
            "item name must match"
        );
    }

    #[test]
    fn test_call_info_with_multiple_call_ranges() {
        let call_info = CallInfo {
            item: CallHierarchyItemInfo {
                name: "multiply_called".to_string(),
                kind: "method".to_string(),
                location: FilePosition {
                    path: "src/lib.rs".to_string(),
                    position: Position { line: 100, character: 10 },
                },
                range: Range {
                    start: Position { line: 100, character: 1 },
                    end: Position { line: 110, character: 1 },
                },
                detail: None,
            },
            call_ranges: vec![
                Range {
                    start: Position { line: 102, character: 5 },
                    end: Position { line: 102, character: 20 },
                },
                Range {
                    start: Position { line: 105, character: 5 },
                    end: Position { line: 105, character: 20 },
                },
                Range {
                    start: Position { line: 108, character: 5 },
                    end: Position { line: 108, character: 20 },
                },
            ],
        };

        let json = serde_json::to_value(&call_info).expect("failed to serialize");
        let call_ranges = json["call_ranges"].as_array().unwrap();

        assert_eq!(
            call_ranges.len(),
            3,
            "must serialize all three call ranges"
        );
    }

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
    fn test_severity_counts_default_has_all_zeros() {
        let counts = SeverityCounts::default();

        assert_eq!(counts.error, 0, "default error count must be zero");
        assert_eq!(counts.warning, 0, "default warning count must be zero");
        assert_eq!(counts.info, 0, "default info count must be zero");
        assert_eq!(counts.hint, 0, "default hint count must be zero");
    }

    #[test]
    fn test_severity_counts_serialization_roundtrip() {
        let counts = SeverityCounts {
            error: 3,
            warning: 5,
            info: 2,
            hint: 1,
        };

        let json = serde_json::to_string(&counts).expect("failed to serialize severity counts");
        let deserialized: SeverityCounts =
            serde_json::from_str(&json).expect("failed to deserialize severity counts");

        assert_eq!(
            counts, deserialized,
            "severity counts must survive serialization roundtrip"
        );
    }

    #[test]
    fn test_diagnostics_response_includes_by_severity() {
        let response = DiagnosticsResponse {
            total_count: 4,
            by_severity: SeverityCounts {
                error: 2,
                warning: 1,
                info: 1,
                hint: 0,
            },
            files: vec![],
        };

        let json = serde_json::to_value(&response).expect("failed to serialize diagnostics response");

        assert!(
            json.get("by_severity").is_some(),
            "by_severity field must be present in serialized output"
        );
        assert_eq!(
            json["by_severity"]["error"], 2,
            "error count must match"
        );
        assert_eq!(
            json["by_severity"]["warning"], 1,
            "warning count must match"
        );
    }

    #[test]
    fn test_diagnostic_has_quick_fix_field_serializes() {
        let diag_with_fix = Diagnostic {
            range: Range {
                start: Position { line: 1, character: 1 },
                end: Position { line: 1, character: 10 },
            },
            severity: Some(DiagnosticSeverity::Error),
            code: Some("E0001".to_string()),
            source: Some("rustc".to_string()),
            message: "unused variable".to_string(),
            has_quick_fix: true,
        };

        let diag_without_fix = Diagnostic {
            range: Range {
                start: Position { line: 2, character: 1 },
                end: Position { line: 2, character: 15 },
            },
            severity: Some(DiagnosticSeverity::Warning),
            code: None,
            source: None,
            message: "some warning".to_string(),
            has_quick_fix: false,
        };

        let json_with = serde_json::to_value(&diag_with_fix).expect("failed to serialize");
        let json_without = serde_json::to_value(&diag_without_fix).expect("failed to serialize");

        assert_eq!(
            json_with["has_quick_fix"], true,
            "has_quick_fix must be true when set"
        );
        assert_eq!(
            json_without["has_quick_fix"], false,
            "has_quick_fix must be false when not set"
        );
    }

    #[test]
    fn test_diagnostic_from_lsp_sets_has_quick_fix_false() {
        let lsp_diag = lsp_types::Diagnostic {
            range: lsp_types::Range::default(),
            severity: Some(lsp_types::DiagnosticSeverity::ERROR),
            code: None,
            source: None,
            message: "test error".to_string(),
            ..Default::default()
        };

        let diag = Diagnostic::from(lsp_diag);

        assert_eq!(
            diag.has_quick_fix, false,
            "diagnostic from LSP must have has_quick_fix set to false by default"
        );
    }

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
            definition: Some(DefinitionLocation {
                path: "node_modules/@reduxjs/toolkit/dist/index.d.mts".to_string(),
                line: 1847,
                external: Some(true),
            }),
        };

        let json = serde_json::to_value(&hover).expect("failed to serialize");

        assert!(json.get("definition").is_some(), "definition must be present");
        assert_eq!(json["definition"]["path"], "node_modules/@reduxjs/toolkit/dist/index.d.mts");
        assert_eq!(json["definition"]["line"], 1847);
        assert_eq!(json["definition"]["external"], true);
    }

    #[test]
    fn test_hover_response_skips_none_definition() {
        let hover = HoverResponse {
            raw_response: None,
            contents: Some(HoverContents::Markup("some docs".to_string())),
            range: None,
            definition: None,
        };

        let json = serde_json::to_value(&hover).expect("failed to serialize");

        assert!(json.get("definition").is_none(), "None definition must be skipped in serialization");
    }
}
