// ABOUTME: Domain service layer for LSP-backed code navigation operations.
// ABOUTME: Provides async methods for symbol lookup, references, and file access.
use crate::api_types::{
    CallHierarchyDirection, CallHierarchyItemInfo, CallHierarchyResponse, CallInfo, CodeContext,
    DefinitionLocation, Diagnostic, DiagnosticSeverity, DiagnosticsResponse, FileDiagnostics,
    FilePosition, FileRange, HoverContents, HoverResponse, Identifier, ImplementationResponse,
    IncomingCallInfo, IncomingCallsResponse, LspStatus, OutgoingCallInfo, OutgoingCallsResponse,
    Position, PrepareCallHierarchyResponse, Range, ReferenceWithSymbolDefinitions,
    ReferencedSymbolsResponse, RelatedSymbols, SeverityCounts, SupportedLanguages, Symbol,
    WorkspaceSymbolInfo, WorkspaceSymbolResponse,
};
use crate::lsp::manager::{LspManagerError, Manager};
use crate::mcp_response::normalize_kind;
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{GotoDefinitionResponse, Location, Position as LspPosition, Range as LspRange};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Provides code navigation operations over a workspace manager.
///
/// # Example
/// ```
/// use std::sync::Arc;
/// use lsproxy::lsp::manager::Manager;
/// use lsproxy::service::create_service;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = Arc::new(Manager::new("/tmp").await?);
/// let service = create_service(manager);
/// let _files = service.list_files(None, None).await?;
/// # Ok(())
/// # }
/// ```
pub struct LspService {
    manager: Arc<Manager>,
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
    pub related: Option<RelatedSymbols>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct McpReferenceLocation {
    pub path: String,
    pub position: Position,
    pub symbol_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<CodeContext>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct FileGroup {
    pub path: String,
    pub count: u32,
    pub refs: Vec<McpReferenceLocation>,
}

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct TypeCounts {
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

#[derive(Debug, PartialEq, Clone)]
struct Pagination {
    limit: u32,
    offset: u32,
    truncated: bool,
}

/// Information about a package from node_modules
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
}

/// Information about external (node_modules) code
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ExternalInfo {
    pub external: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageInfo>,
}

impl ExternalInfo {
    /// Parses external package info from a file path.
    /// Returns Some if the path contains node_modules, None otherwise.
    pub fn from_path(path: &str) -> Option<Self> {
        if !path.contains("node_modules") {
            return None;
        }

        let package = parse_pnpm_package_info(path)
            .or_else(|| parse_standard_package_info(path));

        Some(ExternalInfo {
            external: true,
            package,
        })
    }
}

/// Parses package info from pnpm-style paths like:
/// node_modules/.pnpm/@reduxjs+toolkit@2.0.0/node_modules/@reduxjs/toolkit/...
fn parse_pnpm_package_info(path: &str) -> Option<PackageInfo> {
    if !path.contains(".pnpm/") {
        return None;
    }

    // Find the pnpm package segment: .pnpm/{package}@{version}/
    let pnpm_start = path.find(".pnpm/")?;
    let after_pnpm = &path[pnpm_start + 6..];
    let segment_end = after_pnpm.find("/node_modules/")?;
    let package_segment = &after_pnpm[..segment_end];

    // Parse format: @scope+name@version or name@version
    let at_version_pos = package_segment.rfind('@')?;
    if at_version_pos == 0 {
        return None;
    }

    let name_part = &package_segment[..at_version_pos];
    let version = &package_segment[at_version_pos + 1..];

    // Convert + back to / for scoped packages
    let name = name_part.replace('+', "/");

    Some(PackageInfo {
        name,
        version: version.to_string(),
    })
}

/// Parses package info from standard npm paths like:
/// node_modules/react/index.js or node_modules/@scope/package/index.js
fn parse_standard_package_info(path: &str) -> Option<PackageInfo> {
    let nm_pos = path.find("node_modules/")?;
    let after_nm = &path[nm_pos + 13..];

    let (name, _rest) = if after_nm.starts_with('@') {
        // Scoped package: @scope/name/rest
        let first_slash = after_nm.find('/')?;
        let second_slash = after_nm[first_slash + 1..].find('/').map(|p| p + first_slash + 1)?;
        (&after_nm[..second_slash], &after_nm[second_slash..])
    } else {
        // Regular package: name/rest
        let slash_pos = after_nm.find('/')?;
        (&after_nm[..slash_pos], &after_nm[slash_pos..])
    };

    Some(PackageInfo {
        name: name.to_string(),
        version: "unknown".to_string(),
    })
}

/// Ultra-compact response format for find_definition (~180 chars)
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct CompactDefinitionResponse {
    pub name: String,
    pub sig: String,
    pub loc: String,
    pub ext: bool,
}

/// Parameters for find_definition with optimization options
#[derive(Debug, Clone, Default)]
pub struct FindDefinitionParams {
    pub compact: bool,
    pub include_siblings: bool,
    pub siblings_limit: Option<u32>,
}

/// Checks if a symbol name is an internal builder symbol that should be filtered from siblings.
/// This includes RTK Query builder functions, underscore-prefixed internals, etc.
pub fn is_internal_builder_symbol(name: &str) -> bool {
    // Underscore prefix indicates internal/private
    if name.starts_with('_') {
        return true;
    }

    // RTK Query builder methods and common framework internals
    matches!(
        name,
        "query"
            | "mutation"
            | "endpoints"
            | "providesTags"
            | "invalidatesTags"
            | "transformResponse"
            | "transformErrorResponse"
            | "onQueryStarted"
            | "onCacheEntryAdded"
            | "baseQuery"
            | "reducerPath"
            | "tagTypes"
            | "keepUnusedDataFor"
    )
}

/// Filters sibling exports to remove internal builder symbols and respect limit
pub fn filter_sibling_exports(siblings: Vec<Symbol>, limit: u32) -> Vec<Symbol> {
    siblings
        .into_iter()
        .filter(|s| !is_internal_builder_symbol(&s.name))
        .take(limit as usize)
        .collect()
}

/// Default maximum length for signatures in responses
pub const DEFAULT_MAX_SIGNATURE_LENGTH: usize = 100;

/// Truncates a signature with semantic awareness:
/// 1. Normalizes whitespace (collapses newlines/spaces)
/// 2. Truncates at generic opener `<` for complex types
/// 3. Falls back to byte truncation with char-boundary safety
pub fn truncate_signature(sig: &str, max_length: Option<usize>) -> String {
    let limit = max_length.unwrap_or(DEFAULT_MAX_SIGNATURE_LENGTH);

    // Step 1: Normalize - collapse newlines and excess whitespace
    let normalized: String = sig.split_whitespace().collect::<Vec<_>>().join(" ");

    if normalized.len() <= limit {
        return normalized;
    }

    // Step 2: Try semantic truncation at generic opener
    // For complex types like `EnhancedStore<{...}>`, truncate at `<`
    if let Some(angle_pos) = normalized.find('<') {
        if angle_pos > 0 && angle_pos < limit {
            return format!("{}...", &normalized[..angle_pos]);
        }
    }

    // Step 3: Fallback to byte truncation with char-boundary safety
    let truncate_at = limit.saturating_sub(3);
    let end = normalized
        .char_indices()
        .take_while(|(i, _)| *i < truncate_at)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(truncate_at);
    format!("{}...", &normalized[..end])
}

pub fn create_service(manager: Arc<Manager>) -> LspService {
    LspService { manager }
}

#[derive(Debug)]
pub enum ServiceError {
    Lsp(LspManagerError),
    IdentifierSelection(PositionError),
    CallHierarchy(CallHierarchyError),
    Serialization(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::Lsp(e) => write!(f, "Operation failed because {e}"),
            ServiceError::IdentifierSelection(e) => {
                write!(f, "Identifier selection failed because {e}")
            }
            ServiceError::CallHierarchy(e) => {
                write!(f, "Call hierarchy failed because {e}")
            }
            ServiceError::Serialization(message) => {
                write!(f, "Serialization failed because {message}")
            }
        }
    }
}

impl ServiceError {
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            ServiceError::IdentifierSelection(e) => e.suggestions(),
            ServiceError::CallHierarchy(e) => e.suggestions(),
            ServiceError::Lsp(_) | ServiceError::Serialization(_) => vec![],
        }
    }
}

impl Error for ServiceError {}

impl From<LspManagerError> for ServiceError {
    fn from(err: LspManagerError) -> Self {
        ServiceError::Lsp(err)
    }
}

impl From<PositionError> for ServiceError {
    fn from(err: PositionError) -> Self {
        ServiceError::IdentifierSelection(err)
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(err: serde_json::Error) -> Self {
        ServiceError::Serialization(err.to_string())
    }
}

impl From<CallHierarchyError> for ServiceError {
    fn from(err: CallHierarchyError) -> Self {
        ServiceError::CallHierarchy(err)
    }
}

#[derive(Debug)]
pub enum PositionError {
    IdentifierNotFound { closest: Vec<Identifier> },
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PositionError::IdentifierNotFound { closest } => write!(
                f,
                "No identifier found at position with {} nearby matches",
                closest.len()
            ),
        }
    }
}

impl Error for PositionError {}

impl PositionError {
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            PositionError::IdentifierNotFound { closest } => {
                let mut suggestions = vec![
                    "Use definitions_in_file to see available symbols in this file".to_string(),
                ];
                if !closest.is_empty() {
                    let names: Vec<&str> = closest.iter().take(3).map(|id| id.name.as_str()).collect();
                    suggestions.push(format!("Nearby identifiers: {}", names.join(", ")));
                }
                suggestions
            }
        }
    }
}

#[derive(Debug)]
pub enum CallHierarchyError {
    NoItemAtPosition { nearby_callables: Vec<Symbol> },
}

impl fmt::Display for CallHierarchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallHierarchyError::NoItemAtPosition { nearby_callables } => write!(
                f,
                "No call hierarchy item at position with {} nearby callables",
                nearby_callables.len()
            ),
        }
    }
}

impl Error for CallHierarchyError {}

impl CallHierarchyError {
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            CallHierarchyError::NoItemAtPosition { nearby_callables } => {
                let mut suggestions = vec![
                    "Position must be on a function or method name".to_string(),
                ];
                if !nearby_callables.is_empty() {
                    let names: Vec<&str> = nearby_callables
                        .iter()
                        .take(3)
                        .map(|s| s.name.as_str())
                        .collect();
                    suggestions.push(format!("Nearby callables: {}", names.join(", ")));
                }
                suggestions
            }
        }
    }

    pub fn nearby_callables(&self) -> &[Symbol] {
        match self {
            CallHierarchyError::NoItemAtPosition { nearby_callables } => nearby_callables,
        }
    }
}

/// Enriches a symbol with LSP hover data and source-based heuristics
async fn enrich_symbol(manager: &Manager, file_path: &str, symbol: &mut Symbol) {
    // Calculate line_count from file_range
    symbol.line_count = Some(
        symbol.file_range.range.end.line
            .saturating_sub(symbol.file_range.range.start.line)
            .saturating_add(1)
    );

    // Try to get LSP hover for signature and jsdoc_summary
    let hover_position = lsp_types::Position {
        line: symbol.identifier_position.position.line.saturating_sub(1),
        character: symbol.identifier_position.position.character.saturating_sub(1),
    };

    if let Ok(Some(hover)) = manager.hover(file_path, hover_position).await {
        // Extract signature and jsdoc from hover
        let (sig, jsdoc) = extract_signature_and_docs(&hover.contents);
        if sig.is_some() {
            symbol.signature = sig;
        }
        if jsdoc.is_some() {
            symbol.jsdoc_summary = jsdoc;
        }
    }

    // Fallback to source-based extraction if LSP didn't provide signature/jsdoc
    if symbol.signature.is_none() || symbol.jsdoc_summary.is_none() {
        if let Ok(source_code) = manager.read_source_code(
            file_path,
            Some(LspRange::new(
                LspPosition {
                    line: symbol.file_range.range.start.line.saturating_sub(1),
                    character: 0,
                },
                LspPosition {
                    line: symbol.file_range.range.end.line.saturating_sub(1),
                    character: 0,
                },
            )),
        ).await {
            if symbol.signature.is_none() {
                symbol.signature = extract_signature_from_source(&source_code, &symbol.name);
            }
            if symbol.jsdoc_summary.is_none() {
                symbol.jsdoc_summary = extract_docs_from_source(&source_code);
            }
        }
    }

    // Truncate signature if too long
    if let Some(ref sig) = symbol.signature {
        symbol.signature = Some(truncate_signature(sig, None));
    }

    // Best-effort: detect if exported
    symbol.exported = detect_exported(&symbol.kind);

    // Best-effort: detect dependencies from source code
    if let Ok(source_code) = manager.read_source_code(
        file_path,
        Some(LspRange::new(
            LspPosition {
                line: symbol.file_range.range.start.line.saturating_sub(1),
                character: 0,
            },
            LspPosition {
                line: symbol.file_range.range.end.line.saturating_sub(1),
                character: 0,
            },
        )),
    ).await {
        symbol.dependencies = extract_dependencies_from_source(&source_code);
    }
}

/// Detects if a symbol is exported based on its kind (best-effort heuristic)
fn detect_exported(kind: &str) -> Option<bool> {
    // Heuristic: Certain kinds like "export-function", "pub-function" suggest exported
    // For now, return None to indicate "unknown" - implementations can be refined later
    match kind {
        k if k.contains("export") => Some(true),
        k if k.contains("pub") => Some(true),
        k if k.starts_with("public-") => Some(true),
        _ => Some(false), // Default to false for best-effort
    }
}

/// Extracts signature and documentation from LSP hover contents
fn extract_signature_and_docs(contents: &lsp_types::HoverContents) -> (Option<String>, Option<String>) {
    use lsp_types::{HoverContents, MarkedString, MarkupContent};

    let text = match contents {
        HoverContents::Scalar(MarkedString::String(s)) => s.clone(),
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => {
            format!("```{}\n{}\n```", ls.language, ls.value)
        }
        HoverContents::Markup(MarkupContent { value, .. }) => value.clone(),
        HoverContents::Array(arr) => {
            arr.iter()
                .map(|m| match m {
                    MarkedString::String(s) => s.clone(),
                    MarkedString::LanguageString(ls) => {
                        format!("```{}\n{}\n```", ls.language, ls.value)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        }
    };

    // Simple heuristic: First code block is signature, rest is docs
    let lines: Vec<&str> = text.lines().collect();
    let mut signature = None;
    let mut docs = Vec::new();
    let mut in_code_block = false;
    let mut code_lines = Vec::new();

    for line in lines {
        if line.starts_with("```") {
            if in_code_block {
                // End of code block - this might be the signature
                if signature.is_none() && !code_lines.is_empty() {
                    signature = Some(code_lines.join("\n"));
                    code_lines.clear();
                }
                in_code_block = false;
            } else {
                in_code_block = true;
            }
        } else if in_code_block {
            code_lines.push(line);
        } else if !line.is_empty() {
            docs.push(line);
        }
    }

    let jsdoc = if docs.is_empty() {
        None
    } else {
        Some(docs.join(" ").trim().to_string())
    };

    (signature, jsdoc)
}

/// Extracts signature from source code (fallback when LSP unavailable)
fn extract_signature_from_source(source: &str, symbol_name: &str) -> Option<String> {
    // Find the first line containing the symbol name
    // This is a best-effort heuristic - looks for function/class/struct definitions
    for line in source.lines() {
        let trimmed = line.trim();
        // Skip comments and empty lines
        if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with("/*") || trimmed.is_empty() {
            continue;
        }
        // Check if this line contains the symbol name as a definition
        if trimmed.contains(symbol_name) {
            // Common patterns: "fn name", "function name", "class name", "def name", "struct name"
            if trimmed.contains("fn ") || trimmed.contains("function ") ||
               trimmed.contains("class ") || trimmed.contains("def ") ||
               trimmed.contains("struct ") || trimmed.contains("enum ") ||
               trimmed.contains("interface ") || trimmed.contains("type ") {
                // Extract up to opening brace or semicolon
                let sig = if let Some(brace_pos) = trimmed.find('{') {
                    trimmed[..brace_pos].trim()
                } else if let Some(semi_pos) = trimmed.find(';') {
                    trimmed[..semi_pos].trim()
                } else {
                    trimmed
                };
                return Some(sig.to_string());
            }
        }
    }
    None
}

/// Extracts documentation from source code (fallback when LSP unavailable)
fn extract_docs_from_source(source: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut docs = Vec::new();

    // Look for comment blocks at the start of the source
    for line in &lines {
        let trimmed = line.trim();

        // Rust-style doc comments: ///
        if let Some(doc) = trimmed.strip_prefix("///") {
            docs.push(doc.trim());
        }
        // Python/Ruby docstrings: """...""" or '''...'''
        else if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let content = trimmed.trim_start_matches("\"\"\"").trim_start_matches("'''")
                                .trim_end_matches("\"\"\"").trim_end_matches("'''");
            if !content.is_empty() {
                docs.push(content);
            }
        }
        // C-style doc comments: /** ... */ or //!
        else if let Some(doc) = trimmed.strip_prefix("/**") {
            let content = doc.trim_end_matches("*/").trim();
            if !content.is_empty() {
                docs.push(content);
            }
        }
        // JSDoc style: * ... in multiline
        else if let Some(doc) = trimmed.strip_prefix("*") {
            let content = doc.trim();
            if !content.is_empty() && !content.starts_with("*/") {
                docs.push(content);
            }
        }
        // Regular comments at the start: // or #
        else if let Some(doc) = trimmed.strip_prefix("//") {
            docs.push(doc.trim());
        }
        else if let Some(doc) = trimmed.strip_prefix("#") {
            // Only if not a shebang
            if !doc.starts_with("!") {
                docs.push(doc.trim());
            }
        }
        // Stop at first non-comment line
        else if !trimmed.is_empty() {
            break;
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join(" ").trim().to_string())
    }
}

/// Extracts dependencies/imports used in source code (best-effort)
fn extract_dependencies_from_source(source: &str) -> Option<Vec<String>> {
    use std::collections::HashSet;
    let mut deps = HashSet::new();

    // Pattern: look for common identifiers that appear to be function calls or type references
    // This is very simplified - just looks for PascalCase and camelCase identifiers
    // that might be external dependencies

    let identifier_regex = regex::Regex::new(r"\b([A-Z][a-zA-Z0-9]*|[a-z][a-zA-Z0-9]*\.[a-z][a-zA-Z0-9]*)\b").ok()?;

    for line in source.lines() {
        let trimmed = line.trim();
        // Skip comments and import/use statements (we want usage, not declarations)
        if trimmed.starts_with("//") || trimmed.starts_with("#") ||
           trimmed.starts_with("import ") || trimmed.starts_with("use ") ||
           trimmed.starts_with("from ") {
            continue;
        }

        // Find all identifiers in the line
        for cap in identifier_regex.captures_iter(trimmed) {
            if let Some(ident) = cap.get(1) {
                let id = ident.as_str();
                // Skip very common keywords and short names
                if id.len() > 2 && !is_common_keyword(id) {
                    deps.insert(id.to_string());
                }
            }
        }
    }

    if deps.is_empty() {
        None
    } else {
        let mut result: Vec<String> = deps.into_iter().collect();
        result.sort();
        Some(result)
    }
}

/// Check if a word is a common language keyword to avoid including in dependencies
fn is_common_keyword(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(),
        "if" | "else" | "for" | "while" | "return" | "let" | "const" | "var" |
        "fn" | "func" | "function" | "def" | "class" | "struct" | "enum" |
        "pub" | "public" | "private" | "protected" | "static" | "async" |
        "await" | "try" | "catch" | "finally" | "throw" | "new" | "this" |
        "self" | "super" | "true" | "false" | "null" | "undefined" | "None" |
        "Some" | "Ok" | "Err" | "String" | "Vec" | "Option" | "Result"
    )
}

impl LspService {
    pub async fn definitions_in_file(
        &self,
        file_path: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpSymbolsResponse, ServiceError> {
        use crate::api_types::get_mount_dir;

        // Get file mtime
        let full_path = get_mount_dir().join(file_path);
        let metadata = tokio::fs::metadata(&full_path).await
            .map_err(|e| ServiceError::Lsp(crate::lsp::manager::LspManagerError::FileNotFound(
                format!("{}: {}", file_path, e)
            )))?;
        let mtime = metadata.modified()
            .map_err(|e| ServiceError::Lsp(crate::lsp::manager::LspManagerError::InternalError(
                format!("Failed to get mtime: {}", e)
            )))?;
        let mtime_rfc3339 = chrono::DateTime::<chrono::Utc>::from(mtime)
            .to_rfc3339();

        // Get symbols from ast-grep
        let ast_symbols = self.manager.definitions_in_file_ast_grep(file_path).await?;
        let mut symbols: Vec<Symbol> = ast_symbols
            .into_iter()
            .filter(|s| s.rule_id != "local-variable")
            .map(Symbol::from)
            .collect();

        // Enrich each symbol
        for symbol in &mut symbols {
            enrich_symbol(&self.manager, file_path, symbol).await;
        }

        let (symbols, pagination) = paginate_items(symbols, limit, offset);
        Ok(McpSymbolsResponse {
            path: file_path.to_string(),
            mtime: mtime_rfc3339,
            symbols,
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
        })
    }

    pub async fn find_definition(
        &self,
        file_path: &str,
        position: Position,
        include_source_code: bool,
        include_raw_response: bool,
        context_lines: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpDefinitionResponse, ServiceError> {
        let file_identifiers = self.manager.get_file_identifiers(file_path).await?;
        let selected_identifier = find_identifier_at_position(
            file_identifiers,
            &FilePosition {
                path: file_path.to_string(),
                position: position.clone(),
            },
        )
        .await?;

        let definitions = self
            .manager
            .find_definition(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let definition_locations = definition_locations_lsp(&definitions);
        let (definition_locations, pagination) =
            paginate_items(definition_locations, limit, offset);
        let source_code_context = if include_source_code {
            Some(fetch_definition_source_code(&self.manager, &definition_locations).await?)
        } else {
            None
        };
        let snippet_contexts = match context_lines {
            Some(lines) => Some(fetch_code_context(&self.manager, definition_locations.clone(), lines).await?),
            None => None,
        };

        let raw_response = if include_raw_response {
            Some(serde_json::to_value(&definitions)?)
        } else {
            None
        };

        let mut definition_items = Vec::with_capacity(definition_locations.len());
        let mut first_definition_path: Option<String> = None;
        for (index, location) in definition_locations.into_iter().enumerate() {
            let path = uri_to_relative_path_string(&location.uri);
            let is_external = path.contains("node_modules");

            if index == 0 {
                first_definition_path = Some(path.clone());
            }

            // Skip workspace-dependent operations for external files
            // (ast-grep and find_references require workspace files)
            let (symbol, snippet, reference_count) = if is_external {
                (None, None, None)
            } else {
                let symbol = self
                    .manager
                    .get_symbol_from_position(&path, &location.range.start)
                    .await
                    .ok();
                let snippet = snippet_contexts
                    .as_ref()
                    .and_then(|contexts| contexts.get(index).cloned());
                let ref_count = self.count_references(&path, &location.range.start).await;
                (symbol, snippet, ref_count)
            };

            // Hover may still work for external files (LSP handles it)
            let (signature, doc) = self.fetch_hover_info(&path, &location.range.start).await;

            definition_items.push(definition_item_from_location(&location, symbol, snippet, signature, doc, reference_count));
        }

        let related = compute_related_symbols(
            &self.manager,
            first_definition_path.as_deref(),
            &selected_identifier,
        )
        .await;

        Ok(McpDefinitionResponse {
            raw_response,
            definitions: definition_items,
            source_code_context,
            selected_identifier,
            related: Some(related),
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
        })
    }

    pub async fn find_references(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
        context_lines: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpReferencesResponse, ServiceError> {
        let file_identifiers = self.manager.get_file_identifiers(file_path).await?;
        let selected_identifier = find_identifier_at_position(
            file_identifiers,
            &FilePosition {
                path: file_path.to_string(),
                position: position.clone(),
            },
        )
        .await?;

        let all_references = find_and_filter_references(
            &self.manager,
            &FilePosition {
                path: file_path.to_string(),
                position: position.clone(),
            },
        )
        .await?;

        let total_count = all_references.len() as u32;

        // Build by_type counts before pagination
        let by_type = classify_references_by_type(&self.manager, &all_references).await;

        let raw_response = if include_raw_response {
            serde_json::to_value(&all_references).ok()
        } else {
            None
        };
        let (references, pagination) = paginate_items(all_references, limit, offset);
        let code_contexts = get_code_contexts(&self.manager, &references, context_lines).await?;

        let mut reference_items = Vec::with_capacity(references.len());
        for (index, reference) in references.iter().enumerate() {
            let snippet = code_contexts
                .as_ref()
                .and_then(|contexts| contexts.get(index).cloned());
            reference_items.push(reference_item_from_location(reference, snippet));
        }

        // Build by_file groups from paginated references
        let by_file = group_references_by_file(&reference_items);

        Ok(McpReferencesResponse {
            raw_response,
            selected_identifier,
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
            total_count,
            by_file,
            by_type,
        })
    }

    pub async fn find_referenced_symbols(
        &self,
        file_path: &str,
        position: Position,
        full_scan: bool,
    ) -> Result<ReferencedSymbolsResponse, ServiceError> {
        let referenced_symbols = self
            .manager
            .find_referenced_symbols(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
                full_scan,
            )
            .await?;

        let unwrapped_definitions: Vec<(Identifier, Vec<FilePosition>)> = referenced_symbols
            .into_iter()
            .map(|(ast_grep_result, definition_response)| {
                let definitions = definition_locations(&definition_response);
                (Identifier::from(ast_grep_result), definitions)
            })
            .collect();

        let files = self.manager.list_files().await?;

        let mut workspace_symbols = Vec::new();
        let mut external_symbols = Vec::new();
        let mut not_found = Vec::new();

        for (identifier, definitions) in unwrapped_definitions {
            if definitions.is_empty() {
                not_found.push(identifier);
            } else {
                let has_internal_definition =
                    definitions.iter().any(|def| files.contains(&def.path));
                if has_internal_definition {
                    let mut symbols_with_definitions = Vec::new();
                    for def in definitions.iter().filter(|def| files.contains(&def.path)) {
                        if let Ok(symbol) = self
                            .manager
                            .get_symbol_from_position(
                                &def.path,
                                &lsp_types::Position {
                                    line: def.position.line.saturating_sub(1),
                                    character: def.position.character.saturating_sub(1),
                                },
                            )
                            .await
                        {
                            symbols_with_definitions.push(symbol);
                        }
                    }
                    if !symbols_with_definitions.is_empty() {
                        workspace_symbols.push(ReferenceWithSymbolDefinitions {
                            reference: identifier.clone(),
                            definitions: symbols_with_definitions,
                        });
                    } else {
                        not_found.push(identifier.clone());
                    }
                } else {
                    external_symbols.push(identifier.clone());
                }
            }
        }

        workspace_symbols.sort_by(|a, b| {
            let path_cmp = a
                .reference
                .file_range
                .path
                .cmp(&b.reference.file_range.path);
            if path_cmp.is_eq() {
                a.reference
                    .file_range
                    .range
                    .start
                    .line
                    .cmp(&b.reference.file_range.range.start.line)
            } else {
                path_cmp
            }
        });

        external_symbols.sort_by(|a, b| {
            let path_cmp = a.file_range.path.cmp(&b.file_range.path);
            if path_cmp.is_eq() {
                a.file_range
                    .range
                    .start
                    .line
                    .cmp(&b.file_range.range.start.line)
            } else {
                path_cmp
            }
        });

        not_found.sort_by(|a, b| {
            let path_cmp = a.file_range.path.cmp(&b.file_range.path);
            if path_cmp.is_eq() {
                a.file_range
                    .range
                    .start
                    .line
                    .cmp(&b.file_range.range.start.line)
            } else {
                path_cmp
            }
        });

        Ok(ReferencedSymbolsResponse {
            workspace_symbols,
            external_symbols,
            not_found,
        })
    }

    pub async fn find_identifier(
        &self,
        file_path: &str,
        name: &str,
        position: Option<Position>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpIdentifierResponse, ServiceError> {
        let file_identifiers = self.manager.get_file_identifiers(file_path).await?;
        let name_matched: Vec<Identifier> = file_identifiers
            .into_iter()
            .filter(|id| id.name == name)
            .collect();

        let identifiers = if name_matched.is_empty() {
            vec![]
        } else if let Some(position) = position {
            let lookup_position = FilePosition {
                path: file_path.to_string(),
                position,
            };
            match find_identifier_at_position(name_matched.clone(), &lookup_position).await {
                Ok(identifier) => vec![identifier],
                Err(PositionError::IdentifierNotFound { closest }) => closest,
            }
        } else {
            name_matched
        };
        let (identifiers, pagination) = paginate_items(identifiers, limit, offset);
        Ok(McpIdentifierResponse {
            identifiers,
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
        })
    }

    pub async fn list_files(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpListFilesResponse, ServiceError> {
        let files = self.manager.list_files().await?;
        let (files, pagination) = paginate_items(files, limit, offset);
        Ok(McpListFilesResponse {
            files,
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
        })
    }

    pub async fn read_source_code(
        &self,
        file_path: &str,
        range: Option<Range>,
    ) -> Result<String, ServiceError> {
        let lsp_range = range.map(|range| LspRange::new(range.start.into(), range.end.into()));
        Ok(self.manager.read_source_code(file_path, lsp_range).await?)
    }

    pub async fn health(&self) -> HashMap<SupportedLanguages, LspStatus> {
        let mut languages = HashMap::new();
        for lang in [
            SupportedLanguages::Python,
            SupportedLanguages::TypeScriptJavaScript,
            SupportedLanguages::Rust,
            SupportedLanguages::CPP,
            SupportedLanguages::CSharp,
            SupportedLanguages::Java,
            SupportedLanguages::Golang,
            SupportedLanguages::PHP,
        ] {
            let status = if self.manager.get_client(lang).await.is_some() {
                LspStatus::Ready
            } else if self.manager.is_language_pending(lang).await {
                LspStatus::Initializing
            } else {
                LspStatus::Unavailable
            };
            languages.insert(lang, status);
        }
        languages
    }

    /// Get diagnostics (errors, warnings, hints) for a file or the entire workspace.
    ///
    /// If `file_path` is provided (relative to workspace root), returns diagnostics for that file only.
    /// If None, returns all diagnostics from all language clients.
    pub async fn get_diagnostics(
        &self,
        file_path: Option<&str>,
    ) -> Result<DiagnosticsResponse, ServiceError> {
        let raw_diagnostics = self.manager.get_diagnostics(file_path).await?;

        let mut files: Vec<FileDiagnostics> = Vec::new();
        let mut by_severity = SeverityCounts::default();

        for (path, lsp_diagnostics) in raw_diagnostics {
            let mut diagnostics: Vec<Diagnostic> = Vec::new();

            for lsp_diag in lsp_diagnostics {
                let lsp_range = lsp_diag.range;
                let lsp_diag_clone = lsp_diag.clone();
                let mut diag = Diagnostic::from(lsp_diag);

                match diag.severity {
                    Some(DiagnosticSeverity::Error) => by_severity.error += 1,
                    Some(DiagnosticSeverity::Warning) => by_severity.warning += 1,
                    Some(DiagnosticSeverity::Information) => by_severity.info += 1,
                    Some(DiagnosticSeverity::Hint) => by_severity.hint += 1,
                    None => {}
                }

                if let Ok(Some(actions)) = self
                    .manager
                    .code_action(&path, lsp_range, vec![lsp_diag_clone])
                    .await
                {
                    diag.has_quick_fix = actions.iter().any(|action| {
                        match action {
                            lsp_types::CodeActionOrCommand::CodeAction(ca) => ca
                                .kind
                                .as_ref()
                                .is_some_and(|k| k.as_str().starts_with("quickfix")),
                            lsp_types::CodeActionOrCommand::Command(_) => false,
                        }
                    });
                }

                diagnostics.push(diag);
            }

            files.push(FileDiagnostics { path, diagnostics });
        }

        files.sort_by(|a, b| a.path.cmp(&b.path));

        let total_count: usize = files.iter().map(|f| f.diagnostics.len()).sum();

        Ok(DiagnosticsResponse {
            total_count,
            by_severity,
            files,
        })
    }

    /// Get hover information (documentation, type info) for a symbol at a given position.
    pub async fn hover(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
        include_definition: bool,
    ) -> Result<HoverResponse, ServiceError> {
        let hover = self
            .manager
            .hover(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let (contents, range, raw_response) = match hover {
            Some(h) => {
                let contents = extract_hover_contents(&h.contents);
                let range = h.range.map(|r| Range {
                    start: Position {
                        line: r.start.line + 1,
                        character: r.start.character + 1,
                    },
                    end: Position {
                        line: r.end.line + 1,
                        character: r.end.character + 1,
                    },
                });
                let raw = if include_raw_response {
                    serde_json::to_value(&h).ok()
                } else {
                    None
                };
                (Some(contents), range, raw)
            }
            None => (None, None, None),
        };

        // Optionally fetch definition location
        let definition = if include_definition {
            self.fetch_definition_location(file_path, position).await
        } else {
            None
        };

        Ok(HoverResponse {
            raw_response,
            contents,
            range,
            definition,
        })
    }

    /// Fetches minimal definition location for hover response
    async fn fetch_definition_location(
        &self,
        file_path: &str,
        position: Position,
    ) -> Option<DefinitionLocation> {
        let lsp_position = LspPosition {
            line: position.line.saturating_sub(1),
            character: position.character.saturating_sub(1),
        };

        let definitions = self.manager.find_definition(file_path, lsp_position).await.ok()?;
        let locations = match definitions {
            GotoDefinitionResponse::Scalar(loc) => vec![loc],
            GotoDefinitionResponse::Array(locs) => locs,
            GotoDefinitionResponse::Link(links) => {
                links.into_iter().map(|l| Location {
                    uri: l.target_uri,
                    range: l.target_selection_range,
                }).collect()
            }
        };

        let first = locations.first()?;
        let path = uri_to_relative_path_string(&first.uri);
        let external = if path.contains("node_modules") { Some(true) } else { None };

        Some(DefinitionLocation {
            path,
            line: first.range.start.line + 1,
            external,
        })
    }

    /// Fetches signature and documentation from hover info for a definition position.
    /// Used internally by find_definition to enrich response with type info.
    async fn fetch_hover_info(
        &self,
        file_path: &str,
        position: &LspPosition,
    ) -> (Option<String>, Option<String>) {
        let hover_result = self.manager.hover(file_path, *position).await;

        match hover_result {
            Ok(Some(hover)) => extract_signature_and_docs(&hover.contents),
            _ => (None, None),
        }
    }

    /// Counts references to a symbol at the given position.
    /// Used to populate reference_count in find_definition responses.
    async fn count_references(
        &self,
        file_path: &str,
        position: &LspPosition,
    ) -> Option<u32> {
        let references = self.manager.find_references(file_path, *position).await.ok()?;
        // Don't include the definition itself in the count
        let count = references.len().saturating_sub(1) as u32;
        Some(count)
    }

    pub async fn workspace_symbol(
        &self,
        query: &str,
        include_raw_response: bool,
        exact: bool,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<WorkspaceSymbolResponse, ServiceError> {
        let symbols = self.manager.workspace_symbol(query).await?;

        let workspace_files = self.manager.list_files().await?;

        let mut filtered_symbols = Vec::new();
        for sym in symbols {
            let path = uri_to_relative_path_string(&sym.location.uri);
            if !workspace_files.contains(&path) {
                continue;
            }
            let mut info = workspace_symbol_info_from_lsp(sym, path);
            let (match_kind, match_score) = match_kind_and_score(query, &info.name);
            if exact && match_kind != "exact" {
                continue;
            }
            info.match_kind = Some(match_kind);
            info.match_score = Some(match_score);
            filtered_symbols.push(info);
        }

        let raw_response = if include_raw_response {
            serde_json::to_value(&filtered_symbols).ok()
        } else {
            None
        };

        let (symbols, pagination) = paginate_items(filtered_symbols, limit, offset);
        Ok(WorkspaceSymbolResponse {
            raw_response,
            symbols,
            limit: pagination.limit,
            offset: pagination.offset,
            truncated: pagination.truncated,
        })
    }

    pub async fn find_implementation(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<ImplementationResponse, ServiceError> {
        let file_identifiers = self.manager.get_file_identifiers(file_path).await?;
        let selected_identifier = find_identifier_at_position(
            file_identifiers,
            &FilePosition {
                path: file_path.to_string(),
                position: position.clone(),
            },
        )
        .await?;

        let implementations = self
            .manager
            .find_implementation(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let raw_response = if include_raw_response {
            Some(serde_json::to_value(&implementations)?)
        } else {
            None
        };

        Ok(ImplementationResponse {
            raw_response,
            implementations: definition_locations(&implementations),
            selected_identifier,
        })
    }

    pub async fn prepare_call_hierarchy(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<PrepareCallHierarchyResponse, ServiceError> {
        let items = self
            .manager
            .prepare_call_hierarchy(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let converted_items: Vec<CallHierarchyItemInfo> = items
            .unwrap_or_default()
            .iter()
            .map(call_hierarchy_item_to_info)
            .collect();

        let raw_response = if include_raw_response {
            serde_json::to_value(&converted_items).ok()
        } else {
            None
        };

        Ok(PrepareCallHierarchyResponse {
            raw_response,
            items: converted_items,
        })
    }

    pub async fn incoming_calls(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<IncomingCallsResponse, ServiceError> {
        // First prepare the call hierarchy to get the item
        let items = self
            .manager
            .prepare_call_hierarchy(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let item = items
            .and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) })
            .ok_or_else(|| {
                ServiceError::Lsp(LspManagerError::InternalError(
                    "No call hierarchy item at position".to_string(),
                ))
            })?;

        let calls = self.manager.incoming_calls(file_path, &item).await?;

        let workspace_files = self.manager.list_files().await?;

        let converted_calls: Vec<IncomingCallInfo> = calls
            .into_iter()
            .filter(|call| {
                let path = uri_to_relative_path_string(&call.from.uri);
                workspace_files.contains(&path)
            })
            .map(|call| IncomingCallInfo {
                from: call_hierarchy_item_to_info(&call.from),
                from_ranges: call
                    .from_ranges
                    .into_iter()
                    .map(|r| Range {
                        start: Position {
                            line: r.start.line + 1,
                            character: r.start.character + 1,
                        },
                        end: Position {
                            line: r.end.line + 1,
                            character: r.end.character + 1,
                        },
                    })
                    .collect(),
            })
            .collect();

        let raw_response = if include_raw_response {
            serde_json::to_value(&converted_calls).ok()
        } else {
            None
        };

        Ok(IncomingCallsResponse {
            raw_response,
            calls: converted_calls,
        })
    }

    pub async fn outgoing_calls(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<OutgoingCallsResponse, ServiceError> {
        // First prepare the call hierarchy to get the item
        let items = self
            .manager
            .prepare_call_hierarchy(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let item = items
            .and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) })
            .ok_or_else(|| {
                ServiceError::Lsp(LspManagerError::InternalError(
                    "No call hierarchy item at position".to_string(),
                ))
            })?;

        let calls = self.manager.outgoing_calls(file_path, &item).await?;

        let workspace_files = self.manager.list_files().await?;

        let converted_calls: Vec<OutgoingCallInfo> = calls
            .into_iter()
            .filter(|call| {
                let path = uri_to_relative_path_string(&call.to.uri);
                workspace_files.contains(&path)
            })
            .map(|call| OutgoingCallInfo {
                to: call_hierarchy_item_to_info(&call.to),
                from_ranges: call
                    .from_ranges
                    .into_iter()
                    .map(|r| Range {
                        start: Position {
                            line: r.start.line + 1,
                            character: r.start.character + 1,
                        },
                        end: Position {
                            line: r.end.line + 1,
                            character: r.end.character + 1,
                        },
                    })
                    .collect(),
            })
            .collect();

        let raw_response = if include_raw_response {
            serde_json::to_value(&converted_calls).ok()
        } else {
            None
        };

        Ok(OutgoingCallsResponse {
            raw_response,
            calls: converted_calls,
        })
    }

    /// Unified method for call hierarchy traversal in either direction.
    ///
    /// This method handles both incoming (callers) and outgoing (callees) call hierarchy
    /// requests based on the `direction` parameter.
    ///
    /// # Arguments
    /// * `file_path` - Path to the file containing the function/method
    /// * `position` - Position within the file (1-indexed)
    /// * `direction` - Whether to find incoming (callers) or outgoing (callees) calls
    ///
    /// # Returns
    /// A `CallHierarchyResponse` containing the calls found and the direction.
    /// The `raw_response` field is always `None` (MCP layer handles verbose mode).
    pub async fn call_hierarchy(
        &self,
        file_path: &str,
        position: Position,
        direction: CallHierarchyDirection,
    ) -> Result<CallHierarchyResponse, ServiceError> {
        // First prepare the call hierarchy to get the item
        let items = self
            .manager
            .prepare_call_hierarchy(
                file_path,
                LspPosition {
                    line: position.line.saturating_sub(1),
                    character: position.character.saturating_sub(1),
                },
            )
            .await?;

        let item = items
            .and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) })
            .ok_or_else(|| {
                ServiceError::Lsp(LspManagerError::InternalError(
                    "No call hierarchy item at position".to_string(),
                ))
            })?;

        let workspace_files = self.manager.list_files().await?;

        let calls = match direction {
            CallHierarchyDirection::Incoming => {
                let lsp_calls = self.manager.incoming_calls(file_path, &item).await?;
                lsp_calls
                    .into_iter()
                    .filter(|call| {
                        let path = uri_to_relative_path_string(&call.from.uri);
                        workspace_files.contains(&path)
                    })
                    .map(|call| CallInfo {
                        item: call_hierarchy_item_to_info(&call.from),
                        call_ranges: call
                            .from_ranges
                            .into_iter()
                            .map(|r| Range {
                                start: Position {
                                    line: r.start.line + 1,
                                    character: r.start.character + 1,
                                },
                                end: Position {
                                    line: r.end.line + 1,
                                    character: r.end.character + 1,
                                },
                            })
                            .collect(),
                    })
                    .collect()
            }
            CallHierarchyDirection::Outgoing => {
                let lsp_calls = self.manager.outgoing_calls(file_path, &item).await?;
                lsp_calls
                    .into_iter()
                    .filter(|call| {
                        let path = uri_to_relative_path_string(&call.to.uri);
                        workspace_files.contains(&path)
                    })
                    .map(|call| CallInfo {
                        item: call_hierarchy_item_to_info(&call.to),
                        call_ranges: call
                            .from_ranges
                            .into_iter()
                            .map(|r| Range {
                                start: Position {
                                    line: r.start.line + 1,
                                    character: r.start.character + 1,
                                },
                                end: Position {
                                    line: r.end.line + 1,
                                    character: r.end.character + 1,
                                },
                            })
                            .collect(),
                    })
                    .collect()
            }
        };

        Ok(CallHierarchyResponse {
            direction,
            raw_response: None, // MCP layer handles verbose mode
            calls,
        })
    }
}

fn workspace_symbol_info_from_lsp(
    sym: lsp_types::SymbolInformation,
    path: String,
) -> WorkspaceSymbolInfo {
    WorkspaceSymbolInfo {
        name: sym.name,
        kind: normalize_kind(&format!("{:?}", sym.kind)),
        location: FilePosition {
            path,
            position: Position {
                line: sym.location.range.start.line + 1,
                character: sym.location.range.start.character + 1,
            },
        },
        container_name: sym.container_name,
        match_kind: None,
        match_score: None,
    }
}

fn call_hierarchy_item_to_info(item: &lsp_types::CallHierarchyItem) -> CallHierarchyItemInfo {
    CallHierarchyItemInfo {
        name: item.name.clone(),
        kind: normalize_kind(&format!("{:?}", item.kind)),
        location: FilePosition {
            path: uri_to_relative_path_string(&item.uri),
            position: Position {
                line: item.selection_range.start.line + 1,
                character: item.selection_range.start.character + 1,
            },
        },
        range: Range {
            start: Position {
                line: item.range.start.line + 1,
                character: item.range.start.character + 1,
            },
            end: Position {
                line: item.range.end.line + 1,
                character: item.range.end.character + 1,
            },
        },
        detail: item.detail.clone(),
    }
}

fn definition_locations(definitions: &GotoDefinitionResponse) -> Vec<FilePosition> {
    match definitions {
        GotoDefinitionResponse::Scalar(location) => vec![FilePosition {
            path: uri_to_relative_path_string(&location.uri),
            position: Position {
                line: location.range.start.line + 1,
                character: location.range.start.character + 1,
            },
        }],
        GotoDefinitionResponse::Array(locations) => locations
            .iter()
            .map(|location| FilePosition {
                path: uri_to_relative_path_string(&location.uri),
                position: Position {
                    line: location.range.start.line + 1,
                    character: location.range.start.character + 1,
                },
            })
            .collect(),
        GotoDefinitionResponse::Link(links) => links
            .iter()
            .map(|link| FilePosition {
                path: uri_to_relative_path_string(&link.target_uri),
                position: Position {
                    line: link.target_range.start.line + 1,
                    character: link.target_range.start.character + 1,
                },
            })
            .collect(),
    }
}

fn definition_locations_lsp(definitions: &GotoDefinitionResponse) -> Vec<Location> {
    match definitions {
        GotoDefinitionResponse::Scalar(location) => vec![location.clone()],
        GotoDefinitionResponse::Array(locations) => locations.clone(),
        GotoDefinitionResponse::Link(links) => links
            .iter()
            .map(|link| Location::new(link.target_uri.clone(), link.target_range))
            .collect(),
    }
}

async fn fetch_definition_source_code(
    manager: &Manager,
    definitions: &[Location],
) -> Result<Vec<CodeContext>, ServiceError> {
    let mut code_contexts = Vec::new();
    for definition in definitions.iter() {
        let relative_path = uri_to_relative_path_string(&definition.uri);
        let file_symbols = manager.definitions_in_file_ast_grep(&relative_path).await?;
        let symbol = file_symbols.iter().find(|s| {
            s.get_identifier_range().start.line == definition.range.start.line
                && s.get_identifier_range().start.column == definition.range.start.character
        });

        let source_code_context = match symbol {
            Some(ast_grep_match) => CodeContext {
                range: FileRange {
                    path: relative_path,
                    range: Range {
                        start: Position {
                            line: ast_grep_match.get_context_range().start.line + 1,
                            character: ast_grep_match.get_context_range().start.column + 1,
                        },
                        end: Position {
                            line: ast_grep_match.get_context_range().end.line + 1,
                            character: ast_grep_match.get_context_range().end.column + 1,
                        },
                    },
                },
                source_code: ast_grep_match.get_source_code(),
            },
            None => {
                let range = LspRange {
                    start: LspPosition {
                        line: definition.range.start.line.saturating_sub(3),
                        character: 0,
                    },
                    end: LspPosition {
                        line: definition.range.end.line + 3,
                        character: 0,
                    },
                };
                let source_code = manager.read_source_code(&relative_path, Some(range)).await?;
                CodeContext {
                    range: FileRange {
                        path: relative_path,
                        range: Range {
                            start: Position {
                                line: definition.range.start.line.saturating_sub(3) + 1,
                                character: 1,
                            },
                            end: Position {
                                line: definition.range.end.line + 3 + 1,
                                character: 1,
                            },
                        },
                    },
                    source_code,
                }
            }
        };

        code_contexts.push(source_code_context);
    }
    Ok(code_contexts)
}

async fn find_identifier_at_position(
    identifiers: Vec<Identifier>,
    position: &FilePosition,
) -> Result<Identifier, PositionError> {
    if let Some(exact_match) = identifiers
        .iter()
        .find(|i| i.file_range.contains(position.clone()))
    {
        return Ok(exact_match.clone().with_kind_defaulted());
    }

    let mut with_distances: Vec<_> = identifiers
        .iter()
        .map(|id| {
            let start_line_diff =
                (id.file_range.range.start.line as i32 - position.position.line as i32).abs();
            let start_char_diff = (id.file_range.range.start.character as i32
                - position.position.character as i32)
                .abs();
            let start_distance = start_line_diff * 100 + start_char_diff;

            let end_line_diff =
                (id.file_range.range.end.line as i32 - position.position.line as i32).abs();
            let end_char_diff = (id.file_range.range.end.character as i32
                - position.position.character as i32)
                .abs();
            let end_distance = end_line_diff * 100 + end_char_diff;

            (id.clone().with_kind_defaulted(), (start_distance.min(end_distance)) as f64)
        })
        .collect();

    with_distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let closest = with_distances
        .into_iter()
        .take(3)
        .map(|(id, _)| id)
        .collect();

    Err(PositionError::IdentifierNotFound { closest })
}

async fn find_and_filter_references(
    manager: &Manager,
    position: &FilePosition,
) -> Result<Vec<Location>, ServiceError> {
    let references = manager
        .find_references(
            &position.path,
            LspPosition {
                line: position.position.line.saturating_sub(1),
                character: position.position.character.saturating_sub(1),
            },
        )
        .await?;

    let files = manager.list_files().await?;
    let mut filtered_refs: Vec<_> = references
        .into_iter()
        .filter(|reference| {
            let path = uri_to_relative_path_string(&reference.uri);
            files.contains(&path)
        })
        .collect();

    filtered_refs.sort_by(|a, b| {
        let uri_cmp = a.uri.to_string().cmp(&b.uri.to_string());
        if uri_cmp.is_eq() {
            a.range.start.line.cmp(&b.range.start.line)
        } else {
            uri_cmp
        }
    });

    Ok(filtered_refs)
}

async fn get_code_contexts(
    manager: &Manager,
    references: &Vec<Location>,
    context_lines: Option<u32>,
) -> Result<Option<Vec<CodeContext>>, ServiceError> {
    match context_lines {
        Some(lines) => fetch_code_context(manager, references.clone(), lines)
            .await
            .map(Some),
        None => Ok(None),
    }
}

async fn fetch_code_context(
    manager: &Manager,
    references: Vec<Location>,
    context_lines: u32,
) -> Result<Vec<CodeContext>, ServiceError> {
    let mut code_contexts = Vec::new();
    for reference in references {
        let range = LspRange {
            start: LspPosition {
                line: reference.range.start.line.saturating_sub(context_lines),
                character: 0,
            },
            end: LspPosition {
                line: reference.range.end.line.saturating_add(context_lines),
                character: 0,
            },
        };
        let relative_path = uri_to_relative_path_string(&reference.uri);
        let source_code = manager.read_source_code(&relative_path, Some(range)).await?;
        code_contexts.push(CodeContext {
            range: FileRange {
                path: relative_path,
                range: Range {
                    start: Position {
                        line: reference.range.start.line.saturating_sub(context_lines) + 1,
                        character: 1,
                    },
                    end: Position {
                        line: reference.range.end.line.saturating_add(context_lines) + 1,
                        character: 1,
                    },
                },
            },
            source_code,
        });
    }
    Ok(code_contexts)
}

const DEFAULT_LIST_LIMIT: u32 = 200;

fn paginate_items<T>(
    items: Vec<T>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> (Vec<T>, Pagination) {
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let offset = offset.unwrap_or(0);
    let start = offset as usize;
    let end = std::cmp::min(start.saturating_add(limit as usize), items.len());
    let truncated = end < items.len();
    let paginated = items.into_iter().skip(start).take(limit as usize).collect();
    (
        paginated,
        Pagination {
            limit,
            offset,
            truncated,
        },
    )
}

fn match_kind_and_score(query: &str, name: &str) -> (String, f32) {
    if query.is_empty() {
        return ("none".to_string(), 0.0);
    }
    let query_lower = query.to_ascii_lowercase();
    let name_lower = name.to_ascii_lowercase();
    if name_lower == query_lower {
        return ("exact".to_string(), 1.0);
    }
    if name_lower.starts_with(&query_lower) {
        return ("prefix".to_string(), 0.8);
    }
    if name_lower.contains(&query_lower) {
        return ("substring".to_string(), 0.6);
    }
    if is_fuzzy_match(&query_lower, &name_lower) {
        return ("fuzzy".to_string(), 0.4);
    }
    ("none".to_string(), 0.0)
}

fn is_fuzzy_match(query: &str, name: &str) -> bool {
    let mut iter = name.chars();
    for target in query.chars() {
        if !iter.any(|candidate| candidate == target) {
            return false;
        }
    }
    true
}

fn range_from_lsp(range: &LspRange) -> Range {
    Range {
        start: Position {
            line: range.start.line + 1,
            character: range.start.character + 1,
        },
        end: Position {
            line: range.end.line + 1,
            character: range.end.character + 1,
        },
    }
}

fn definition_item_from_location(
    location: &Location,
    symbol: Option<Symbol>,
    snippet: Option<CodeContext>,
    signature: Option<String>,
    doc: Option<String>,
    reference_count: Option<u32>,
) -> McpDefinitionLocation {
    let path = uri_to_relative_path_string(&location.uri);
    let position = Position {
        line: location.range.start.line + 1,
        character: location.range.start.character + 1,
    };
    let (definition_range, symbol_kind) = match &symbol {
        Some(symbol) => (symbol.file_range.range.clone(), Some(symbol.kind.clone())),
        None => (range_from_lsp(&location.range), None),
    };

    // Derive external info from path
    let external_info = ExternalInfo::from_path(&path);
    let (external, package) = match external_info {
        Some(info) => (Some(info.external), info.package),
        None => (None, None),
    };

    McpDefinitionLocation {
        path,
        position,
        definition_range,
        symbol_kind,
        snippet,
        signature,
        doc,
        external,
        package,
        reference_count,
    }
}

fn reference_item_from_location(
    location: &Location,
    snippet: Option<CodeContext>,
) -> McpReferenceLocation {
    let path = uri_to_relative_path_string(&location.uri);
    let position = Position {
        line: location.range.start.line + 1,
        character: location.range.start.character + 1,
    };
    McpReferenceLocation {
        path,
        position,
        symbol_range: range_from_lsp(&location.range),
        snippet,
    }
}

fn extract_hover_contents(contents: &lsp_types::HoverContents) -> HoverContents {
    match contents {
        lsp_types::HoverContents::Scalar(marked) => {
            HoverContents::Markup(extract_marked_string(marked))
        }
        lsp_types::HoverContents::Array(arr) => {
            HoverContents::Array(arr.iter().map(extract_marked_string).collect())
        }
        lsp_types::HoverContents::Markup(markup) => HoverContents::Markup(markup.value.clone()),
    }
}

fn extract_marked_string(marked: &lsp_types::MarkedString) -> String {
    match marked {
        lsp_types::MarkedString::String(s) => s.clone(),
        lsp_types::MarkedString::LanguageString(ls) => {
            format!("```{}\n{}\n```", ls.language, ls.value)
        }
    }
}

/// Groups references by file path
fn group_references_by_file(references: &[McpReferenceLocation]) -> Vec<FileGroup> {
    use std::collections::HashMap;

    let mut groups: HashMap<String, Vec<McpReferenceLocation>> = HashMap::new();

    for reference in references {
        groups.entry(reference.path.clone())
            .or_insert_with(Vec::new)
            .push(reference.clone());
    }

    let mut file_groups: Vec<FileGroup> = groups.into_iter()
        .map(|(path, refs)| FileGroup {
            count: refs.len() as u32,
            path,
            refs,
        })
        .collect();

    // Sort by path for consistent output
    file_groups.sort_by(|a, b| a.path.cmp(&b.path));

    file_groups
}

/// Classifies references by type (import vs call)
async fn classify_references_by_type(manager: &Manager, references: &[Location]) -> TypeCounts {
    let mut counts = TypeCounts::default();

    for reference in references {
        let path = uri_to_relative_path_string(&reference.uri);
        let line_num = reference.range.start.line;

        // Try to read the line containing this reference
        if let Ok(source) = manager.read_source_code(
            &path,
            Some(LspRange::new(
                LspPosition { line: line_num, character: 0 },
                LspPosition { line: line_num + 1, character: 0 },
            )),
        ).await {
            if is_import_line(&source) {
                counts.import += 1;
            } else {
                counts.call += 1;
            }
        } else {
            // If we can't read the line, assume it's a call
            counts.call += 1;
        }
    }

    counts
}

/// Detects if a line is an import statement
fn is_import_line(line: &str) -> bool {
    let trimmed = line.trim();

    // Skip comments
    if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("/*") {
        return false;
    }

    trimmed.starts_with("import ")
        || trimmed.starts_with("use ")
        || trimmed.contains("require(")
        || trimmed.starts_with("from \"")
        || trimmed.starts_with("from '")
        || trimmed.starts_with("from ")
}

/// Computes related symbols for a definition (sibling exports, implements, extends)
async fn compute_related_symbols(
    manager: &Manager,
    definition_file_path: Option<&str>,
    selected_identifier: &Identifier,
) -> RelatedSymbols {
    let mut related = RelatedSymbols::default();

    let Some(def_path) = definition_file_path else {
        return related;
    };

    if let Ok(file_symbols) = manager.definitions_in_file_ast_grep(def_path).await {
        let sibling_exports: Vec<Symbol> = file_symbols
            .into_iter()
            .filter(|s| s.rule_id != "local-variable" && s.rule_id != "all-identifiers")
            .filter(|s| s.meta_variables.single.name.text != selected_identifier.name)
            .map(Symbol::from)
            .collect();

        related.sibling_exports = sibling_exports;
    }

    related
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{
        CallHierarchyItem, Location, Position as LspPosition, Range as LspRange, SymbolInformation,
        SymbolKind, Url,
    };
    use rand::{distr::Alphanumeric, Rng};
    use std::thread;
    use tempfile::TempDir;

    fn random_irregular_string() -> String {
        let mut rng = rand::rng();
        let len: usize = rng.random_range(6..20);
        let mut value: String = rng
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect();
        value.push('_');
        value.push('\t');
        value
    }

    fn retry_with<T, F>(mut op: F) -> T
    where
        F: FnMut() -> Option<T>,
    {
        let mut rng = rand::rng();
        let attempts: usize = rng.random_range(2..5);
        for _ in 0..attempts {
            let result = op();
            if result.is_some() {
                return result.unwrap();
            }
        }
        let message = random_irregular_string();
        panic!("{}", message);
    }

    #[allow(deprecated)]
    #[test]
    fn test_workspace_symbol_info_kind_normalized() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let range = LspRange {
            start: LspPosition {
                line: 1,
                character: 0,
            },
            end: LspPosition {
                line: 1,
                character: 4,
            },
        };
        let sym = SymbolInformation {
            name: "Example".to_string(),
            kind: SymbolKind::ENUM_MEMBER,
            tags: None,
            deprecated: None,
            location: Location { uri, range },
            container_name: None,
        };

        let info = workspace_symbol_info_from_lsp(sym, "src/main.rs".to_string());

        assert_eq!(info.kind, "enum-member");
        assert_eq!(info.location.path, "src/main.rs");
    }

    #[test]
    fn test_call_hierarchy_kind_normalized() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let range = LspRange {
            start: LspPosition {
                line: 2,
                character: 1,
            },
            end: LspPosition {
                line: 2,
                character: 6,
            },
        };
        let item = CallHierarchyItem {
            name: "Thing".to_string(),
            kind: SymbolKind::TYPE_PARAMETER,
            tags: None,
            detail: None,
            uri,
            range: range.clone(),
            selection_range: range,
            data: None,
        };

        let info = call_hierarchy_item_to_info(&item);

        assert_eq!(info.kind, "type-parameter");
    }

    #[tokio::test]
    async fn it_reports_language_servers_unavailable_without_startup(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let random_suffix: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(10)
            .map(char::from)
            .collect();
        let workspace_root = temp_dir.path().join(format!("ñ{}", random_suffix));
        tokio::fs::create_dir_all(&workspace_root).await?;
        let manager = Manager::new(
            workspace_root
                .to_str()
                .ok_or("Workspace root path must be valid utf8")?,
        )
        .await?;
        let service = create_service(Arc::new(manager));

        let mut attempts_remaining = 3;
        let mut results = tokio::join!(service.health(), service.health());
        while attempts_remaining > 0
            && (results.0.values().any(|status| *status == LspStatus::Ready) || results.0 != results.1)
        {
            attempts_remaining -= 1;
            results = tokio::join!(service.health(), service.health());
        }

        let all_unavailable = results.0.values().all(|status| *status == LspStatus::Unavailable);
        let consistent = results.0 == results.1;
        assert!(
            all_unavailable && consistent,
            "Expected language servers to be unavailable and consistent but they were not"
        );

        Ok(())
    }

    #[tokio::test]
    async fn it_cannot_crash_when_language_servers_are_missing(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_left = TempDir::new()?;
        let temp_right = TempDir::new()?;
        let random_left: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(9)
            .map(char::from)
            .collect();
        let random_right: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(9)
            .map(char::from)
            .collect();
        let irregular_left = format!("ñ{}", random_left);
        let irregular_right = format!("ñ{}", random_right);
        let workspace_left = temp_left.path().join(format!("workspace_{}", random_left));
        let workspace_right = temp_right.path().join(format!("workspace_{}", random_right));
        tokio::fs::create_dir_all(&workspace_left).await?;
        tokio::fs::create_dir_all(&workspace_right).await?;
        let file_left = workspace_left.join(format!("sample_{}.py", random_left));
        let file_right = workspace_right.join(format!("sample_{}.py", random_right));
        tokio::fs::write(&file_left, format!("print('{}')", irregular_left)).await?;
        tokio::fs::write(&file_right, format!("print('{}')", irregular_right)).await?;

        let path_override_dir = TempDir::new()?;
        let path_override = path_override_dir
            .path()
            .join(format!("path_{}", rand::rng().random::<u32>()));
        tokio::fs::create_dir_all(&path_override).await?;
        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &path_override);

        let workspace_left_str = workspace_left.to_str().ok_or(irregular_left.clone())?;
        let workspace_right_str = workspace_right.to_str().ok_or(irregular_right.clone())?;
        let mut manager_left = Manager::new(workspace_left_str).await?;
        let mut manager_right = Manager::new(workspace_right_str).await?;

        let (result_left, result_right) = tokio::join!(
            retry_start(&mut manager_left, workspace_left_str),
            retry_start(&mut manager_right, workspace_right_str)
        );

        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        } else {
            std::env::remove_var("PATH");
        }

        let left_ok = result_left.is_ok();
        let right_ok = result_right.is_ok();
        assert!(
            left_ok && right_ok,
            "Did not ignore missing language servers on startup"
        );

        Ok(())
    }

    #[test]
    fn it_paginates_items_with_offset_and_truncation() {
        let mut rng = rand::rng();
        let total_len: usize = rng.random_range(6..20);
        let offset: u32 = rng.random_range(0..(total_len as u32 / 2 + 1));
        let limit: u32 = rng.random_range(1..(total_len as u32 / 2 + 2));
        let mut items = Vec::with_capacity(total_len);
        for _ in 0..total_len {
            items.push(random_irregular_string());
        }
        let expected_items = items.clone();
        let response = retry_with(|| {
            let items = items.clone();
            let handle = thread::spawn(move || paginate_items(items, Some(limit), Some(offset)));
            handle.join().ok()
        });
        let (actual_items, pagination) = response;
        let start = offset as usize;
        let end = std::cmp::min(start.saturating_add(limit as usize), expected_items.len());
        let expected_slice = expected_items[start..end].to_vec();
        assert_eq!(
            actual_items,
            expected_slice,
            "negative: paginated items mismatch"
        );
        assert_eq!(pagination.limit, limit, "negative: limit mismatch");
        assert_eq!(pagination.offset, offset, "negative: offset mismatch");
        assert_eq!(
            pagination.truncated,
            end < expected_items.len(),
            "negative: truncation mismatch"
        );
    }

    #[test]
    fn it_scores_prefix_matches_for_workspace_symbols() {
        let mut rng = rand::rng();
        let base = random_irregular_string();
        let prefix_len = rng.random_range(1..(base.len().saturating_sub(1).max(2)));
        let query: String = base.chars().take(prefix_len).collect();
        let name = format!("{}{}", base, random_irregular_string());
        let response = retry_with(|| {
            let query = query.clone();
            let name = name.clone();
            let handle = thread::spawn(move || match_kind_and_score(&query, &name));
            handle.join().ok()
        });
        let expected_kind = String::from("prefix");
        let (kind, score) = response;
        assert_eq!(kind, expected_kind, "negative: match kind mismatch");
        assert!(
            score > 0.7,
            "negative: match score did not exceed expected threshold"
        );
    }

    #[test]
    fn it_builds_definition_location_with_symbol_range_and_snippet() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        let file_name = format!("file_{}.rs", random_irregular_string());
        let file_path = temp_dir.path().join(file_name);
        let uri = Url::from_file_path(&file_path).expect("negative: uri creation failed");
        let mut rng = rand::rng();
        let start_line: u32 = rng.random_range(1..100);
        let start_char: u32 = rng.random_range(0..20);
        let end_line: u32 = start_line + rng.random_range(0..5);
        let end_char: u32 = start_char + rng.random_range(1..5);
        let location = Location {
            uri,
            range: LspRange {
                start: LspPosition {
                    line: start_line,
                    character: start_char,
                },
                end: LspPosition {
                    line: end_line,
                    character: end_char,
                },
            },
        };
        let expected_path = file_path.to_string_lossy().into_owned();
        let symbol_range = Range {
            start: Position {
                line: start_line + 1,
                character: 0,
            },
            end: Position {
                line: end_line + 2,
                character: 3,
            },
        };
        let symbol = Symbol {
            name: random_irregular_string(),
            kind: random_irregular_string(),
            identifier_position: FilePosition {
                path: expected_path.clone(),
                position: Position {
                    line: start_line,
                    character: start_char,
                },
            },
            file_range: FileRange {
                path: expected_path.clone(),
                range: symbol_range.clone(),
            },
            signature: None,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
        };
        let snippet = CodeContext {
            range: FileRange {
                path: expected_path.clone(),
                range: symbol_range.clone(),
            },
            source_code: random_irregular_string(),
        };
        let expected_signature = Some("fn test_function()".to_string());
        let expected_jsdoc = Some("Test documentation".to_string());
        let response = retry_with(|| {
            let location = location.clone();
            let symbol = symbol.clone();
            let snippet = snippet.clone();
            let sig = expected_signature.clone();
            let doc = expected_jsdoc.clone();
            let handle = thread::spawn(move || {
                Some(definition_item_from_location(
                    &location,
                    Some(symbol),
                    Some(snippet),
                    sig,
                    doc,
                    None,
                ))
            });
            handle.join().ok().flatten()
        });
        assert_eq!(response.path, expected_path, "negative: path mismatch");
        // Output is 1-indexed: LSP 0-indexed input + 1
        assert_eq!(
            response.position.line, start_line + 1,
            "negative: line mismatch"
        );
        assert_eq!(
            response.position.character, start_char + 1,
            "negative: character mismatch"
        );
        assert_eq!(
            response.definition_range, symbol_range,
            "negative: definition range mismatch"
        );
        assert_eq!(
            response.symbol_kind,
            Some(symbol.kind.clone()),
            "negative: symbol kind mismatch"
        );
        assert_eq!(
            response.signature, expected_signature,
            "negative: signature mismatch"
        );
        assert_eq!(
            response.doc, expected_jsdoc,
            "negative: doc mismatch"
        );
        assert_eq!(
            response.snippet,
            Some(snippet),
            "negative: snippet mismatch"
        );
    }

    #[test]
    fn it_builds_reference_location_with_symbol_range_and_snippet() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        let file_name = format!("ref_{}.rs", random_irregular_string());
        let file_path = temp_dir.path().join(file_name);
        let uri = Url::from_file_path(&file_path).expect("negative: uri creation failed");
        let mut rng = rand::rng();
        let start_line: u32 = rng.random_range(1..100);
        let start_char: u32 = rng.random_range(0..20);
        let end_line: u32 = start_line + rng.random_range(0..5);
        let end_char: u32 = start_char + rng.random_range(1..5);
        let location = Location {
            uri,
            range: LspRange {
                start: LspPosition {
                    line: start_line,
                    character: start_char,
                },
                end: LspPosition {
                    line: end_line,
                    character: end_char,
                },
            },
        };
        let expected_path = file_path.to_string_lossy().into_owned();
        // Expected range is 1-indexed (LSP 0-indexed + 1)
        let expected_range = Range {
            start: Position {
                line: start_line + 1,
                character: start_char + 1,
            },
            end: Position {
                line: end_line + 1,
                character: end_char + 1,
            },
        };
        let snippet = CodeContext {
            range: FileRange {
                path: expected_path.clone(),
                range: expected_range.clone(),
            },
            source_code: random_irregular_string(),
        };
        let response = retry_with(|| {
            let location = location.clone();
            let snippet = snippet.clone();
            let handle =
                thread::spawn(move || Some(reference_item_from_location(&location, Some(snippet))));
            handle.join().ok().flatten()
        });
        assert_eq!(response.path, expected_path, "negative: path mismatch");
        // Output is 1-indexed: LSP 0-indexed input + 1
        assert_eq!(
            response.position.line, start_line + 1,
            "negative: line mismatch"
        );
        assert_eq!(
            response.position.character, start_char + 1,
            "negative: character mismatch"
        );
        assert_eq!(
            response.symbol_range, expected_range,
            "negative: reference range mismatch"
        );
        assert_eq!(
            response.snippet,
            Some(snippet),
            "negative: snippet mismatch"
        );
    }

    async fn retry_start(
        manager: &mut Manager,
        workspace_root: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut attempts_remaining = 2;
        let mut result = manager.start_langservers(workspace_root, None).await;
        while attempts_remaining > 0 && result.is_err() {
            attempts_remaining -= 1;
            result = manager.start_langservers(workspace_root, None).await;
        }
        result
    }

    #[tokio::test]
    async fn test_definitions_in_file_includes_mtime_and_path() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        let test_file = workspace_root.join("test.rs");
        tokio::fs::write(&test_file, "fn example() {}").await?;

        let manager = Manager::new(workspace_root.to_str().unwrap()).await?;
        let service = create_service(Arc::new(manager));

        let response = service.definitions_in_file("test.rs", None, None).await?;

        // Verify path is populated
        assert_eq!(response.path, "test.rs");

        // Verify mtime is populated and is RFC3339 format
        assert!(!response.mtime.is_empty());
        assert!(chrono::DateTime::parse_from_rfc3339(&response.mtime).is_ok(),
            "mtime should be valid RFC3339: {}", response.mtime);

        // Verify pagination fields exist
        assert_eq!(response.limit, 200); // DEFAULT_LIST_LIMIT
        assert_eq!(response.offset, 0);
        assert_eq!(response.truncated, false);

        Ok(())
    }

    #[tokio::test]
    async fn test_definitions_in_file_enriches_symbols() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        // Create a Rust file with a documented function
        let test_file = workspace_root.join("test.rs");
        let source = r#"/// This function does something
pub fn example(x: i32) -> String {
    format!("{}", x)
}

fn internal_helper() {
    println!("internal");
}"#;
        tokio::fs::write(&test_file, source).await?;

        let manager = Manager::new(workspace_root.to_str().unwrap()).await?;
        let service = create_service(Arc::new(manager));

        let response = service.definitions_in_file("test.rs", None, None).await?;

        // Find the public function symbol
        let pub_fn = response.symbols.iter()
            .find(|s| s.name == "example")
            .expect("Should find 'example' function");

        // Verify line_count is populated
        assert!(pub_fn.line_count.is_some(), "line_count should be populated");
        let line_count = pub_fn.line_count.unwrap();
        assert!(line_count >= 3, "Function should span at least 3 lines, got {}", line_count);

        // Verify exported is populated (best-effort)
        // For Rust, 'pub' should be detected
        assert!(pub_fn.exported.is_some(), "exported should be populated");

        // Find the internal function
        let internal_fn = response.symbols.iter()
            .find(|s| s.name == "internal_helper")
            .expect("Should find 'internal_helper' function");

        // Verify internal function has line_count
        assert!(internal_fn.line_count.is_some(), "line_count should be populated for all symbols");

        Ok(())
    }

    #[test]
    fn test_group_references_by_file() {
        let refs = vec![
            McpReferenceLocation {
                path: "src/main.rs".to_string(),
                position: Position { line: 1, character: 5 },
                symbol_range: Range {
                    start: Position { line: 1, character: 5 },
                    end: Position { line: 1, character: 9 },
                },
                snippet: None,
            },
            McpReferenceLocation {
                path: "src/lib.rs".to_string(),
                position: Position { line: 2, character: 10 },
                symbol_range: Range {
                    start: Position { line: 2, character: 10 },
                    end: Position { line: 2, character: 14 },
                },
                snippet: None,
            },
            McpReferenceLocation {
                path: "src/main.rs".to_string(),
                position: Position { line: 5, character: 3 },
                symbol_range: Range {
                    start: Position { line: 5, character: 3 },
                    end: Position { line: 5, character: 7 },
                },
                snippet: None,
            },
        ];

        let groups = group_references_by_file(&refs);

        assert_eq!(groups.len(), 2);

        // Should be sorted by path
        assert_eq!(groups[0].path, "src/lib.rs");
        assert_eq!(groups[0].count, 1);
        assert_eq!(groups[0].refs.len(), 1);

        assert_eq!(groups[1].path, "src/main.rs");
        assert_eq!(groups[1].count, 2);
        assert_eq!(groups[1].refs.len(), 2);
    }

    #[test]
    fn test_is_import_line() {
        assert!(is_import_line("import os"));
        assert!(is_import_line("  import { useState } from 'react'"));
        assert!(is_import_line("use std::collections::HashMap;"));
        assert!(is_import_line("const fs = require('fs');"));
        assert!(is_import_line("from datetime import datetime"));
        assert!(is_import_line("from \"@/lib/utils\" import { cn }"));

        assert!(!is_import_line("let x = greet('hello')"));
        assert!(!is_import_line("const result = calculate()"));
        assert!(!is_import_line("// import this later"));
    }

    #[test]
    fn test_type_counts_default() {
        let counts = TypeCounts::default();
        assert_eq!(counts.import, 0);
        assert_eq!(counts.call, 0);
    }

    #[test]
    fn test_mcp_references_response_contains_by_file_with_snippets() {
        let snippet = CodeContext {
            range: FileRange {
                path: "src/main.rs".to_string(),
                range: Range {
                    start: Position { line: 10, character: 5 },
                    end: Position { line: 12, character: 10 },
                },
            },
            source_code: "fn example() {}".to_string(),
        };

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: Identifier {
                name: "test".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 5 },
                    },
                },
                kind: Some("function".to_string()),
            },
            limit: 200,
            offset: 0,
            truncated: false,
            total_count: 15,
            by_file: vec![
                FileGroup {
                    path: "src/main.rs".to_string(),
                    count: 10,
                    refs: vec![
                        McpReferenceLocation {
                            path: "src/main.rs".to_string(),
                            position: Position { line: 10, character: 5 },
                            symbol_range: Range {
                                start: Position { line: 10, character: 5 },
                                end: Position { line: 10, character: 9 },
                            },
                            snippet: Some(snippet.clone()),
                        },
                    ],
                },
                FileGroup {
                    path: "src/lib.rs".to_string(),
                    count: 5,
                    refs: vec![],
                },
            ],
            by_type: TypeCounts {
                import: 3,
                call: 12,
            },
        };

        // Verify by_file contains references with snippets
        assert_eq!(response.by_file.len(), 2);
        assert_eq!(response.by_file[0].count, 10);
        assert_eq!(response.by_file[0].refs.len(), 1);
        assert!(response.by_file[0].refs[0].snippet.is_some());

        // Verify the snippet is correctly attached to the reference
        let ref_snippet = response.by_file[0].refs[0].snippet.as_ref().unwrap();
        assert_eq!(ref_snippet.source_code, "fn example() {}");

        // Verify counts
        assert_eq!(response.total_count, 15);
        assert_eq!(response.by_type.import, 3);
        assert_eq!(response.by_type.call, 12);

        // Verify invariants
        assert_eq!(
            response.by_file[0].count + response.by_file[1].count,
            15,
            "by_file counts should sum to total_count"
        );
        assert_eq!(
            response.by_type.import + response.by_type.call,
            15,
            "by_type counts should sum to total_count"
        );
    }

    #[test]
    fn test_mcp_definition_response_has_related_field() {
        let response = McpDefinitionResponse {
            raw_response: None,
            definitions: vec![],
            source_code_context: None,
            selected_identifier: Identifier {
                name: "test_fn".to_string(),
                file_range: FileRange {
                    path: "src/lib.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 8 },
                    },
                },
                kind: Some("function".to_string()),
            },
            related: Some(RelatedSymbols::default()),
            limit: 200,
            offset: 0,
            truncated: false,
        };

        assert!(
            response.related.is_some(),
            "related field must be present"
        );
        let related = response.related.unwrap();
        assert!(
            related.sibling_exports.is_empty(),
            "default sibling_exports must be empty"
        );
    }

    #[test]
    fn test_mcp_definition_response_related_with_siblings() {
        let sibling = Symbol {
            name: "helper_fn".to_string(),
            kind: "function".to_string(),
            identifier_position: FilePosition {
                path: "src/lib.rs".to_string(),
                position: Position { line: 20, character: 4 },
            },
            file_range: FileRange {
                path: "src/lib.rs".to_string(),
                range: Range {
                    start: Position { line: 20, character: 1 },
                    end: Position { line: 25, character: 1 },
                },
            },
            ..Default::default()
        };

        let related = RelatedSymbols {
            sibling_exports: vec![sibling.clone()],
            ..Default::default()
        };

        let response = McpDefinitionResponse {
            raw_response: None,
            definitions: vec![],
            source_code_context: None,
            selected_identifier: Identifier {
                name: "main_fn".to_string(),
                file_range: FileRange {
                    path: "src/lib.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 8 },
                    },
                },
                kind: Some("function".to_string()),
            },
            related: Some(related),
            limit: 200,
            offset: 0,
            truncated: false,
        };

        let related = response.related.expect("related field must be present");
        assert_eq!(
            related.sibling_exports.len(),
            1,
            "sibling_exports must have one entry"
        );
        assert_eq!(
            related.sibling_exports[0].name,
            "helper_fn",
            "sibling name must match"
        );
    }

    #[test]
    fn test_mcp_definition_response_serialization_skips_empty_related() {
        let response = McpDefinitionResponse {
            raw_response: None,
            definitions: vec![],
            source_code_context: None,
            selected_identifier: Identifier {
                name: "test".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 5 },
                    },
                },
                kind: Some("identifier".to_string()),
            },
            related: None,
            limit: 200,
            offset: 0,
            truncated: false,
        };

        let json = serde_json::to_value(&response).expect("serialization failed");

        assert!(
            json.get("related").is_none(),
            "None related must be skipped in serialization"
        );
    }

    #[test]
    fn it_creates_position_error_with_suggestions() {
        let closest = vec![
            Identifier {
                name: "my_function".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 5, character: 1 },
                        end: Position { line: 5, character: 12 },
                    },
                },
                kind: Some("function".to_string()),
            },
        ];

        let error = PositionError::IdentifierNotFound { closest: closest.clone() };
        let suggestions = error.suggestions();

        assert!(
            !suggestions.is_empty(),
            "negative: IdentifierNotFound should provide suggestions"
        );
        assert!(
            suggestions.iter().any(|s| s.contains("definitions_in_file")),
            "negative: suggestions should mention definitions_in_file tool"
        );
    }

    #[test]
    fn it_creates_position_error_with_closest_identifiers_in_suggestions() {
        let closest = vec![
            Identifier {
                name: "nearby_fn".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 10, character: 1 },
                        end: Position { line: 10, character: 10 },
                    },
                },
                kind: Some("function".to_string()),
            },
            Identifier {
                name: "another_fn".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 15, character: 1 },
                        end: Position { line: 15, character: 11 },
                    },
                },
                kind: Some("function".to_string()),
            },
        ];

        let error = PositionError::IdentifierNotFound { closest: closest.clone() };
        let suggestions = error.suggestions();

        assert!(
            suggestions.iter().any(|s| s.contains("nearby_fn")),
            "negative: suggestions should include closest identifier names"
        );
    }

    #[test]
    fn it_creates_call_hierarchy_error_with_suggestions() {
        let nearby = vec![
            Symbol {
                name: "some_function".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "test.rs".to_string(),
                    position: Position { line: 10, character: 4 },
                },
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 10, character: 1 },
                        end: Position { line: 15, character: 1 },
                    },
                },
                ..Default::default()
            },
        ];

        let error = CallHierarchyError::NoItemAtPosition { nearby_callables: nearby };
        let suggestions = error.suggestions();

        assert!(
            !suggestions.is_empty(),
            "negative: NoItemAtPosition should provide suggestions"
        );
        assert!(
            suggestions.iter().any(|s| s.contains("function") || s.contains("method")),
            "negative: suggestions should mention function/method positioning"
        );
    }

    #[test]
    fn it_includes_nearby_callables_in_call_hierarchy_error_suggestions() {
        let nearby = vec![
            Symbol {
                name: "callable_fn".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "test.rs".to_string(),
                    position: Position { line: 5, character: 4 },
                },
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 5, character: 1 },
                        end: Position { line: 10, character: 1 },
                    },
                },
                ..Default::default()
            },
        ];

        let error = CallHierarchyError::NoItemAtPosition { nearby_callables: nearby.clone() };
        let suggestions = error.suggestions();

        assert!(
            suggestions.iter().any(|s| s.contains("callable_fn")),
            "negative: suggestions should include nearby callable names"
        );
    }

    #[test]
    fn it_formats_service_error_with_suggestions() {
        let closest = vec![
            Identifier {
                name: "test_id".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 8 },
                    },
                },
                kind: Some("identifier".to_string()),
            },
        ];

        let error = ServiceError::IdentifierSelection(
            PositionError::IdentifierNotFound { closest }
        );

        let suggestions = error.suggestions();
        assert!(
            !suggestions.is_empty(),
            "negative: ServiceError should expose suggestions from inner error"
        );
    }

    #[test]
    fn it_formats_call_hierarchy_service_error_with_suggestions() {
        let nearby = vec![
            Symbol {
                name: "method_name".to_string(),
                kind: "method".to_string(),
                identifier_position: FilePosition {
                    path: "test.rs".to_string(),
                    position: Position { line: 20, character: 8 },
                },
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 20, character: 1 },
                        end: Position { line: 25, character: 1 },
                    },
                },
                ..Default::default()
            },
        ];

        let error = ServiceError::CallHierarchy(
            CallHierarchyError::NoItemAtPosition { nearby_callables: nearby }
        );

        let suggestions = error.suggestions();
        assert!(
            !suggestions.is_empty(),
            "negative: ServiceError should expose suggestions from CallHierarchyError"
        );
    }

    // ==================== find_definition optimization tests ====================

    #[test]
    fn test_mcp_definition_location_includes_signature() {
        let def_location = McpDefinitionLocation {
            path: "src/service.ts".to_string(),
            position: Position { line: 82, character: 5 },
            definition_range: Range {
                start: Position { line: 82, character: 1 },
                end: Position { line: 90, character: 1 },
            },
            symbol_kind: Some("function".to_string()),
            snippet: None,
            signature: Some("(args: {classId: number}) => UseQueryResult<ClassDetails>".to_string()),
            doc: Some("Query hook for fetching class details by ID".to_string()),
            external: None,
            package: None,
            reference_count: None,
        };

        assert!(
            def_location.signature.is_some(),
            "definition location must include signature"
        );
        assert!(
            def_location.doc.is_some(),
            "definition location must include doc"
        );
    }

    #[test]
    fn test_mcp_definition_location_serializes_signature_and_doc() {
        let def_location = McpDefinitionLocation {
            path: "src/api.ts".to_string(),
            position: Position { line: 10, character: 5 },
            definition_range: Range {
                start: Position { line: 10, character: 1 },
                end: Position { line: 15, character: 1 },
            },
            symbol_kind: Some("function".to_string()),
            snippet: None,
            signature: Some("fn example(x: i32) -> String".to_string()),
            doc: Some("Example function documentation".to_string()),
            external: None,
            package: None,
            reference_count: None,
        };

        let json = serde_json::to_value(&def_location).expect("serialization failed");

        assert!(
            json.get("signature").is_some(),
            "signature must be present in serialization"
        );
        assert!(
            json.get("doc").is_some(),
            "doc must be present in serialization"
        );
        assert_eq!(
            json["signature"],
            "fn example(x: i32) -> String",
            "signature content must match"
        );
    }

    #[test]
    fn test_mcp_definition_location_skips_none_signature_and_doc() {
        let def_location = McpDefinitionLocation {
            path: "src/api.ts".to_string(),
            position: Position { line: 10, character: 5 },
            definition_range: Range {
                start: Position { line: 10, character: 1 },
                end: Position { line: 15, character: 1 },
            },
            symbol_kind: Some("function".to_string()),
            snippet: None,
            signature: None,
            doc: None,
            external: None,
            package: None,
            reference_count: None,
        };

        let json = serde_json::to_value(&def_location).expect("serialization failed");

        assert!(
            json.get("signature").is_none(),
            "None signature must be skipped in serialization"
        );
        assert!(
            json.get("doc").is_none(),
            "None doc must be skipped in serialization"
        );
    }

    #[test]
    fn test_mcp_definition_location_includes_external_fields() {
        let def_location = McpDefinitionLocation {
            path: "node_modules/.pnpm/@reduxjs+toolkit@2.0.0/node_modules/@reduxjs/toolkit/dist/index.d.ts".to_string(),
            position: Position { line: 100, character: 5 },
            definition_range: Range {
                start: Position { line: 100, character: 1 },
                end: Position { line: 110, character: 1 },
            },
            symbol_kind: Some("function".to_string()),
            snippet: None,
            signature: Some("fn configureStore<S>() -> Store<S>".to_string()),
            doc: None,
            external: Some(true),
            package: Some(PackageInfo {
                name: "@reduxjs/toolkit".to_string(),
                version: "2.0.0".to_string(),
            }),
            reference_count: Some(42),
        };

        let json = serde_json::to_value(&def_location).expect("serialization failed");

        assert_eq!(json["external"], true, "external must be true");
        assert_eq!(json["package"]["name"], "@reduxjs/toolkit", "package name must match");
        assert_eq!(json["package"]["version"], "2.0.0", "package version must match");
        assert_eq!(json["reference_count"], 42, "reference_count must match");
    }

    #[test]
    fn test_external_info_creation_for_node_modules_path() {
        let path = "node_modules/.pnpm/@reduxjs+toolkit@2.0.0/node_modules/@reduxjs/toolkit/dist/query/react/buildHooks.d.ts";
        let external_info = ExternalInfo::from_path(path);

        assert!(
            external_info.is_some(),
            "external info must be detected for node_modules path"
        );

        let info = external_info.unwrap();
        assert!(info.external, "external flag must be true");
        assert!(info.package.is_some(), "package info must be present");

        let pkg = info.package.unwrap();
        assert_eq!(pkg.name, "@reduxjs/toolkit", "package name must be parsed");
        assert_eq!(pkg.version, "2.0.0", "package version must be parsed");
    }

    #[test]
    fn test_external_info_none_for_workspace_path() {
        let path = "src/components/Button.tsx";
        let external_info = ExternalInfo::from_path(path);

        assert!(
            external_info.is_none(),
            "external info must be None for workspace paths"
        );
    }

    #[test]
    fn test_external_info_serialization() {
        let info = ExternalInfo {
            external: true,
            package: Some(PackageInfo {
                name: "react".to_string(),
                version: "18.2.0".to_string(),
            }),
        };

        let json = serde_json::to_value(&info).expect("serialization failed");

        assert_eq!(json["external"], true, "external flag must serialize");
        assert!(json.get("package").is_some(), "package must be present");
        assert_eq!(json["package"]["name"], "react", "package name must match");
        assert_eq!(json["package"]["version"], "18.2.0", "package version must match");
    }

    #[test]
    fn test_compact_definition_response_format() {
        let compact = CompactDefinitionResponse {
            name: "useGetClassDetailsQuery".to_string(),
            sig: "(args: {classId: number}) => UseQueryResult".to_string(),
            loc: "src/app/service/classManagementService.ts:82".to_string(),
            ext: false,
        };

        let json = serde_json::to_string(&compact).expect("serialization failed");

        // Compact format should be small (~180 chars or less)
        assert!(
            json.len() < 250,
            "compact format must be under 250 chars, got {} chars",
            json.len()
        );

        // Verify all fields are present
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse failed");
        assert!(parsed.get("name").is_some(), "name must be present");
        assert!(parsed.get("sig").is_some(), "sig must be present");
        assert!(parsed.get("loc").is_some(), "loc must be present");
        assert!(parsed.get("ext").is_some(), "ext must be present");
    }

    #[test]
    fn test_compact_definition_response_abbreviations() {
        let compact = CompactDefinitionResponse {
            name: "myFunction".to_string(),
            sig: "(x: number) => string".to_string(),
            loc: "src/lib.ts:42".to_string(),
            ext: true,
        };

        let json = serde_json::to_value(&compact).expect("serialization failed");

        // Uses abbreviated field names
        assert!(json.get("sig").is_some(), "must use 'sig' not 'signature'");
        assert!(json.get("loc").is_some(), "must use 'loc' not 'location'");
        assert!(json.get("ext").is_some(), "must use 'ext' not 'external'");
    }

    #[test]
    fn test_find_definition_params_include_siblings_default_false() {
        let params = FindDefinitionParams::default();

        assert!(
            !params.include_siblings,
            "include_siblings must default to false"
        );
    }

    #[test]
    fn test_find_definition_params_include_compact_default_false() {
        let params = FindDefinitionParams::default();

        assert!(
            !params.compact,
            "compact mode must default to false"
        );
    }

    #[test]
    fn test_find_definition_params_siblings_limit_default() {
        let params = FindDefinitionParams::default();

        assert_eq!(
            params.siblings_limit.unwrap_or(5),
            5,
            "siblings_limit must default to 5"
        );
    }

    #[test]
    fn test_is_internal_builder_symbol() {
        // RTK Query internal builder functions that should be filtered
        assert!(is_internal_builder_symbol("_baseEndpointQuery"), "underscore prefix indicates internal");
        assert!(is_internal_builder_symbol("providesTags"), "RTK builder function");
        assert!(is_internal_builder_symbol("invalidatesTags"), "RTK builder function");
        assert!(is_internal_builder_symbol("query"), "generic builder method");
        assert!(is_internal_builder_symbol("mutation"), "generic builder method");
        assert!(is_internal_builder_symbol("endpoints"), "RTK builder config");

        // User-defined exports that should NOT be filtered
        assert!(!is_internal_builder_symbol("useGetUserQuery"), "user hook export");
        assert!(!is_internal_builder_symbol("UserService"), "user service export");
        assert!(!is_internal_builder_symbol("getUserById"), "user function export");
    }

    #[test]
    fn test_filter_sibling_exports() {
        let siblings = vec![
            Symbol {
                name: "useGetUserQuery".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "src/api.ts".to_string(),
                    position: Position { line: 10, character: 5 },
                },
                file_range: FileRange {
                    path: "src/api.ts".to_string(),
                    range: Range {
                        start: Position { line: 10, character: 1 },
                        end: Position { line: 15, character: 1 },
                    },
                },
                ..Default::default()
            },
            Symbol {
                name: "providesTags".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "src/api.ts".to_string(),
                    position: Position { line: 20, character: 5 },
                },
                file_range: FileRange {
                    path: "src/api.ts".to_string(),
                    range: Range {
                        start: Position { line: 20, character: 1 },
                        end: Position { line: 25, character: 1 },
                    },
                },
                ..Default::default()
            },
            Symbol {
                name: "_internalHelper".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "src/api.ts".to_string(),
                    position: Position { line: 30, character: 5 },
                },
                file_range: FileRange {
                    path: "src/api.ts".to_string(),
                    range: Range {
                        start: Position { line: 30, character: 1 },
                        end: Position { line: 35, character: 1 },
                    },
                },
                ..Default::default()
            },
        ];

        let filtered = filter_sibling_exports(siblings, 10);

        assert_eq!(filtered.len(), 1, "must filter out internal builder symbols");
        assert_eq!(filtered[0].name, "useGetUserQuery", "must keep user exports");
    }

    #[test]
    fn test_filter_sibling_exports_respects_limit() {
        let siblings: Vec<Symbol> = (0..10)
            .map(|i| Symbol {
                name: format!("userExport{}", i),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "src/api.ts".to_string(),
                    position: Position { line: i * 10, character: 5 },
                },
                file_range: FileRange {
                    path: "src/api.ts".to_string(),
                    range: Range {
                        start: Position { line: i * 10, character: 1 },
                        end: Position { line: i * 10 + 5, character: 1 },
                    },
                },
                ..Default::default()
            })
            .collect();

        let filtered = filter_sibling_exports(siblings, 5);

        assert_eq!(filtered.len(), 5, "must respect siblings limit");
    }

    #[test]
    fn test_truncate_signature_short_string_unchanged() {
        let sig = "fn foo(x: i32) -> bool";
        let result = truncate_signature(sig, Some(50));
        assert_eq!(result, sig);
    }

    #[test]
    fn test_truncate_signature_truncates_at_generic_opener() {
        // Complex generic types should truncate at `<`
        let sig = "fn configure_store(options: StoreOptions<State, Middleware, Enhancers>) -> EnhancedStore<State>";
        let result = truncate_signature(sig, Some(50)); // Limit smaller than string length
        assert_eq!(result, "fn configure_store(options: StoreOptions...");
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_signature_normalizes_whitespace() {
        // Multiline signatures should be normalized to single line
        let sig = "const store: EnhancedStore<{\n    member: CombinedState<{\n        foo: Bar\n    }>\n}>";
        let result = truncate_signature(sig, Some(40)); // Limit smaller than normalized string
        // Should normalize whitespace and truncate at <
        assert_eq!(result, "const store: EnhancedStore...");
        assert!(!result.contains('\n'));
    }

    #[test]
    fn test_truncate_signature_default_length() {
        let sig = "a".repeat(250);
        let result = truncate_signature(&sig, None);
        // DEFAULT_MAX_SIGNATURE_LENGTH is now 100
        assert!(result.len() <= DEFAULT_MAX_SIGNATURE_LENGTH);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_signature_unicode_safe() {
        // Unicode chars that could be split unsafely
        let sig = "fn 测试函数<T>(参数: 类型) -> 返回值";
        let result = truncate_signature(sig, Some(20)); // Limit smaller than string
        // Should truncate at < and not panic
        assert_eq!(result, "fn 测试函数...");
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_signature_simple_generic_preserved() {
        // Short signatures with simple generics should be kept
        let sig = "Vec<String>";
        let result = truncate_signature(sig, Some(100));
        assert_eq!(result, "Vec<String>");
    }

    #[test]
    fn test_truncate_signature_fallback_byte_truncation() {
        // Long signatures without < should fall back to byte truncation
        let sig = "fn very_long_function_name_without_generics(arg1: Type1, arg2: Type2, arg3: Type3)";
        let result = truncate_signature(sig, Some(50));
        assert!(result.len() <= 50);
        assert!(result.ends_with("..."));
    }
}
