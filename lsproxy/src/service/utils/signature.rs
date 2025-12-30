// ABOUTME: Signature extraction and processing utilities.
// ABOUTME: Handles LSP hover parsing, source-based extraction, and symbol filtering.

use crate::api_types::{Position, Symbol};
use crate::lsp::manager::Manager;
use lsp_types::Position as LspPosition;

/// Default maximum length for signatures in responses
pub const DEFAULT_MAX_SIGNATURE_LENGTH: usize = 100;

/// Truncates a signature with semantic awareness:
/// 1. Normalizes whitespace (collapses newlines/spaces)
/// 2. Truncates at generic opener `<` for complex types
/// 3. Falls back to byte truncation with char-boundary safety
pub fn truncate_signature(sig: &str, max_length: Option<usize>) -> String {
    let limit = max_length.unwrap_or(DEFAULT_MAX_SIGNATURE_LENGTH);

    let normalized: String = sig.split_whitespace().collect::<Vec<_>>().join(" ");

    if normalized.len() <= limit {
        return normalized;
    }

    if let Some(angle_pos) = normalized.find('<') {
        if angle_pos > 0 && angle_pos < limit {
            return format!("{}...", &normalized[..angle_pos]);
        }
    }

    let truncate_at = limit.saturating_sub(3);
    let end = normalized
        .char_indices()
        .take_while(|(i, _)| *i < truncate_at)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(truncate_at);
    format!("{}...", &normalized[..end])
}

/// Extracts signature and documentation from LSP hover contents
pub(crate) fn extract_signature_and_docs(contents: &lsp_types::HoverContents) -> (Option<String>, Option<String>) {
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

    let lines: Vec<&str> = text.lines().collect();
    let mut candidate_signatures: Vec<String> = Vec::new();
    let mut example_signatures: Vec<String> = Vec::new();
    let mut docs = Vec::new();
    let mut in_code_block = false;
    let mut code_lines = Vec::new();
    let mut pending_example_block = false;
    let mut current_block_is_example = false;

    for line in lines {
        if line.starts_with("```") {
            if in_code_block {
                if !code_lines.is_empty() {
                    let block_content = code_lines.join("\n");
                    // rust-analyzer returns module name first, then signature
                    // Collect all blocks that look like signatures
                    if is_likely_signature(&block_content) {
                        if current_block_is_example {
                            example_signatures.push(block_content);
                        } else {
                            candidate_signatures.push(block_content);
                        }
                    }
                    code_lines.clear();
                }
                in_code_block = false;
                current_block_is_example = false;
            } else {
                in_code_block = true;
                current_block_is_example = pending_example_block;
                pending_example_block = false;
            }
        } else if in_code_block {
            code_lines.push(line);
        } else if !line.is_empty() && line != "---" {
            if is_example_tag_line(line) {
                pending_example_block = true;
            }
            docs.push(line);
        }
    }

    // Select the best signature from candidates:
    // 1. Prefer longer signatures (more complete type info)
    // 2. Among equal lengths, prefer those with more type information (contains ':' or '->')
    let select_signature = |candidates: Vec<String>| {
        candidates.into_iter().max_by(|a, b| {
            let score_a = a.len() + if a.contains(':') || a.contains("->") { 50 } else { 0 };
            let score_b = b.len() + if b.contains(':') || b.contains("->") { 50 } else { 0 };
            score_a.cmp(&score_b)
        })
    };

    let signature = if !candidate_signatures.is_empty() {
        select_signature(candidate_signatures)
    } else if !example_signatures.is_empty() {
        select_signature(example_signatures)
    } else {
        None
    };

    // Fallback: if no code blocks found, try to extract signature from first lines
    let signature = if signature.is_none() && !docs.is_empty() {
        let extracted = extract_signature_from_plain_text(&docs);
        if extracted.is_some() {
            docs = docs.into_iter()
                .filter(|line| !looks_like_signature(line))
                .collect();
        }
        extracted
    } else {
        signature
    };

    let jsdoc = if docs.is_empty() {
        None
    } else {
        Some(docs.join(" ").trim().to_string())
    };

    (signature, jsdoc)
}

/// Checks if a code block is a variable assignment with a value (usage example).
/// These typically come from @example JSDoc blocks and should not be signatures.
///
/// Matches: `const x = value`, `let y: Type = value`, `var z = value`
/// Does NOT match: `const x: Type` (no value, just type annotation)
fn is_variable_assignment(block: &str) -> bool {
    let trimmed = block.trim();

    let is_var_decl = trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ");

    if !is_var_decl {
        return false;
    }

    trimmed.contains('=')
}

/// Checks if a code block looks like an actual signature vs just a module name
fn is_likely_signature(block: &str) -> bool {
    let trimmed = block.trim();

    // Empty blocks are not signatures
    if trimmed.is_empty() {
        return false;
    }

    // Variable assignments are usage examples, not signatures
    if is_variable_assignment(trimmed) {
        return false;
    }

    // Reference/pointer types starting with &, *, or [ are valid signatures
    if trimmed.starts_with('&') || trimmed.starts_with('*') || trimmed.starts_with('[') {
        return true;
    }

    // Single word without spaces/parens/colons is likely just a module name
    // But allow if it contains type indicators like '<', ':', '->'
    if !trimmed.contains(' ') && !trimmed.contains('(') && !trimmed.contains('<')
        && !trimmed.contains(':') && !trimmed.contains("->") {
        return false;
    }

    // Look for signature patterns by keyword prefixes
    let has_keyword_prefix =
        // Rust signatures
        trimmed.starts_with("fn ") ||
        trimmed.starts_with("pub ") ||
        trimmed.starts_with("pub(") ||
        trimmed.starts_with("async ") ||
        trimmed.starts_with("const ") ||
        trimmed.starts_with("static ") ||
        trimmed.starts_with("type ") ||
        trimmed.starts_with("struct ") ||
        trimmed.starts_with("enum ") ||
        trimmed.starts_with("trait ") ||
        trimmed.starts_with("impl ") ||
        trimmed.starts_with("impl<") ||
        trimmed.starts_with("mod ") ||
        trimmed.starts_with("use ") ||
        trimmed.starts_with("macro") ||
        trimmed.starts_with("unsafe ") ||
        trimmed.starts_with("extern ") ||
        trimmed.starts_with("dyn ") ||
        // TypeScript/JavaScript
        trimmed.starts_with("function ") ||
        trimmed.starts_with("class ") ||
        trimmed.starts_with("interface ") ||
        trimmed.starts_with("export ") ||
        trimmed.starts_with("let ") ||
        trimmed.starts_with("var ") ||
        trimmed.starts_with("readonly ") ||
        trimmed.starts_with("abstract ") ||
        trimmed.starts_with("private ") ||
        trimmed.starts_with("protected ") ||
        trimmed.starts_with("public ") ||
        // Python
        trimmed.starts_with("def ") ||
        trimmed.starts_with("async def ") ||
        trimmed.starts_with("@") ||  // decorators often precede signatures
        // Go
        trimmed.starts_with("func ") ||
        // C/C++
        trimmed.starts_with("void ") ||
        trimmed.starts_with("int ") ||
        trimmed.starts_with("char ") ||
        trimmed.starts_with("auto ") ||
        trimmed.starts_with("template") ||
        trimmed.starts_with("virtual ") ||
        trimmed.starts_with("inline ");

    // Look for signature patterns by content (type annotations, etc.)
    let has_signature_content =
        // Type annotations (variable: Type, param: Type)
        trimmed.contains(": ") ||
        // Return type indicator
        trimmed.contains("->") ||
        // Generic bounds
        trimmed.contains("where ") ||
        // Function call parens (likely a function signature)
        trimmed.contains('(') ||
        // Generic type parameters
        (trimmed.contains('<') && trimmed.contains('>')) ||
        // Array/slice types
        trimmed.contains('[');

    has_keyword_prefix || has_signature_content
}

fn is_example_tag_line(line: &str) -> bool {
    let trimmed = line.trim();
    let trimmed = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .unwrap_or(trimmed);
    let trimmed = trimmed.trim_matches(|c: char| c == '*' || c == '_' || c == '`');
    let trimmed = trimmed.trim_end_matches(':');
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("@example") || lower.starts_with("example")
}

/// Checks if a line looks like a code signature
fn looks_like_signature(line: &str) -> bool {
    let trimmed = line.trim();
    // Rust signatures
    trimmed.starts_with("fn ") ||
    trimmed.starts_with("pub fn ") ||
    trimmed.starts_with("pub(") ||
    trimmed.starts_with("async fn ") ||
    trimmed.starts_with("pub async fn ") ||
    trimmed.starts_with("const ") ||
    trimmed.starts_with("pub const ") ||
    trimmed.starts_with("static ") ||
    trimmed.starts_with("pub static ") ||
    trimmed.starts_with("type ") ||
    trimmed.starts_with("pub type ") ||
    trimmed.starts_with("struct ") ||
    trimmed.starts_with("pub struct ") ||
    trimmed.starts_with("enum ") ||
    trimmed.starts_with("pub enum ") ||
    trimmed.starts_with("trait ") ||
    trimmed.starts_with("pub trait ") ||
    trimmed.starts_with("impl ") ||
    trimmed.starts_with("impl<") ||
    trimmed.starts_with("mod ") ||
    trimmed.starts_with("pub mod ") ||
    // TypeScript/JavaScript signatures
    trimmed.starts_with("function ") ||
    trimmed.starts_with("class ") ||
    trimmed.starts_with("interface ") ||
    trimmed.starts_with("export ") ||
    // Python signatures
    trimmed.starts_with("def ") ||
    trimmed.starts_with("async def ") ||
    trimmed.starts_with("class ") ||
    // Go signatures
    trimmed.starts_with("func ") ||
    trimmed.starts_with("func (")
}

/// Extracts signature from plain text (no code fences)
fn extract_signature_from_plain_text(lines: &[&str]) -> Option<String> {
    let mut sig_lines = Vec::new();

    for line in lines {
        if looks_like_signature(line) {
            sig_lines.push(*line);
            // For multi-line signatures, continue collecting until we hit a closing brace/paren
            // or empty line
        } else if !sig_lines.is_empty() {
            // Check if this continues a signature (e.g., generic bounds, return type on next line)
            let trimmed = line.trim();
            if trimmed.starts_with("->") ||
               trimmed.starts_with("where") ||
               trimmed.starts_with("<") ||
               (trimmed.len() > 0 && !trimmed.ends_with(".") && !trimmed.starts_with("//")) {
                sig_lines.push(*line);
            } else {
                break;
            }
        }
    }

    if sig_lines.is_empty() {
        None
    } else {
        Some(sig_lines.join("\n"))
    }
}

/// Extracts signature from source code (fallback when LSP unavailable)
pub(crate) fn extract_signature_from_source(source: &str, symbol_name: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with("/*") || trimmed.is_empty() {
            continue;
        }
        if trimmed.contains(symbol_name) {
            if trimmed.contains("fn ") || trimmed.contains("function ") ||
               trimmed.contains("class ") || trimmed.contains("def ") ||
               trimmed.contains("struct ") || trimmed.contains("enum ") ||
               trimmed.contains("interface ") || trimmed.contains("type ") {
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
pub(crate) fn extract_docs_from_source(source: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut docs = Vec::new();

    for line in &lines {
        let trimmed = line.trim();

        if let Some(doc) = trimmed.strip_prefix("///") {
            docs.push(doc.trim());
        }
        else if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let content = trimmed.trim_start_matches("\"\"\"").trim_start_matches("'''")
                                .trim_end_matches("\"\"\"").trim_end_matches("'''");
            if !content.is_empty() {
                docs.push(content);
            }
        }
        else if let Some(doc) = trimmed.strip_prefix("/**") {
            let content = doc.trim_end_matches("*/").trim();
            if !content.is_empty() {
                docs.push(content);
            }
        }
        else if let Some(doc) = trimmed.strip_prefix("*") {
            let content = doc.trim();
            if !content.is_empty() && !content.starts_with("*/") {
                docs.push(content);
            }
        }
        else if let Some(doc) = trimmed.strip_prefix("//") {
            docs.push(doc.trim());
        }
        else if let Some(doc) = trimmed.strip_prefix("#") {
            if !doc.starts_with("!") {
                docs.push(doc.trim());
            }
        }
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

/// Extracts the identifier name from hover contents.
/// Takes the first identifier-like word from the signature.
pub(crate) fn extract_identifier_name_from_hover(contents: &lsp_types::HoverContents) -> String {
    use lsp_types::{HoverContents, MarkedString, MarkupContent};

    let text = match contents {
        HoverContents::Scalar(MarkedString::String(s)) => s.clone(),
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => ls.value.clone(),
        HoverContents::Markup(MarkupContent { value, .. }) => value.clone(),
        HoverContents::Array(arr) => {
            arr.first().map(|m| match m {
                MarkedString::String(s) => s.clone(),
                MarkedString::LanguageString(ls) => ls.value.clone(),
            }).unwrap_or_default()
        }
    };

    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .find(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

/// Fetches signatures for multiple positions concurrently.
/// Returns a Vec of Option<String> in the same order as input.
pub(crate) async fn batch_hover_for_signatures(
    manager: &Manager,
    positions: Vec<(&str, Position)>,
) -> Vec<Option<String>> {
    use futures::future::join_all;

    let futures = positions.into_iter().map(|(path, pos)| {
        let path = path.to_string();
        async move {
            let lsp_pos = LspPosition {
                line: pos.line.saturating_sub(1),
                character: pos.character.saturating_sub(1),
            };
            match manager.hover(&path, lsp_pos).await {
                Ok(Some(hover)) => {
                    let (sig, _) = extract_signature_and_docs(&hover.contents);
                    sig.map(|s| truncate_signature(&s, None))
                }
                _ => None,
            }
        }
    });

    join_all(futures).await
}

/// Enriches a symbol with LSP hover data and source-based heuristics
pub(crate) async fn enrich_symbol(manager: &Manager, file_path: &str, symbol: &mut Symbol) {
    symbol.line_count = Some(
        symbol.file_range.range.end.line
            .saturating_sub(symbol.file_range.range.start.line)
            .saturating_add(1)
    );

    let hover_position = lsp_types::Position {
        line: symbol.identifier_position.position.line.saturating_sub(1),
        character: symbol.identifier_position.position.character.saturating_sub(1),
    };

    if let Ok(Some(hover)) = manager.hover(file_path, hover_position).await {
        let (sig, jsdoc) = extract_signature_and_docs(&hover.contents);
        if sig.is_some() {
            symbol.signature = sig;
        }
        if jsdoc.is_some() {
            symbol.jsdoc_summary = jsdoc;
        }
    }

    if symbol.signature.is_none() || symbol.jsdoc_summary.is_none() {
        if let Ok(source_code) = manager.read_source_code(
            file_path,
            Some(lsp_types::Range::new(
                lsp_types::Position {
                    line: symbol.file_range.range.start.line.saturating_sub(1),
                    character: 0,
                },
                lsp_types::Position {
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

    if let Some(ref sig) = symbol.signature {
        symbol.signature = Some(truncate_signature(sig, None));
    }

    symbol.exported = detect_exported(&symbol.kind);

    let position = lsp_types::Position {
        line: symbol.identifier_position.position.line.saturating_sub(1),
        character: symbol.identifier_position.position.character.saturating_sub(1),
    };
    if let Ok(referenced) = manager.find_referenced_symbols(file_path, position, false).await {
        let deps: Vec<String> = referenced
            .into_iter()
            .filter_map(|(ast_match, def_response)| {
                let locations = match def_response {
                    lsp_types::GotoDefinitionResponse::Scalar(loc) => vec![loc],
                    lsp_types::GotoDefinitionResponse::Array(locs) => locs,
                    lsp_types::GotoDefinitionResponse::Link(links) => {
                        links.into_iter()
                            .map(|l| lsp_types::Location {
                                uri: l.target_uri,
                                range: l.target_selection_range,
                            })
                            .collect()
                    }
                };

                let has_external_def = locations.iter().any(|loc| {
                    let def_path = loc.uri.path();
                    def_path != file_path && !def_path.ends_with(".d.ts")
                });

                if has_external_def {
                    Some(ast_match.meta_variables.single.name.text)
                } else {
                    None
                }
            })
            .collect();
        if !deps.is_empty() {
            let mut unique_deps: Vec<String> = deps.into_iter().collect::<std::collections::HashSet<_>>().into_iter().collect();
            unique_deps.sort();
            symbol.dependencies = Some(unique_deps);
        }
    }
}

/// Detects if a symbol is exported based on its kind (best-effort heuristic)
pub(crate) fn detect_exported(kind: &str) -> Option<bool> {
    match kind {
        k if k.contains("export") => Some(true),
        k if k.contains("pub") => Some(true),
        k if k.starts_with("public-") => Some(true),
        _ => Some(false),
    }
}

/// Checks if a symbol name is an internal builder symbol that should be filtered from siblings.
/// This includes RTK Query builder functions, underscore-prefixed internals, etc.
pub fn is_internal_builder_symbol(name: &str) -> bool {
    if name.starts_with('_') {
        return true;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_internal_builder_symbol() {
        assert!(is_internal_builder_symbol("_baseEndpointQuery"), "underscore prefix indicates internal");
        assert!(is_internal_builder_symbol("providesTags"), "RTK builder function");
        assert!(is_internal_builder_symbol("invalidatesTags"), "RTK builder function");
        assert!(is_internal_builder_symbol("query"), "generic builder method");
        assert!(is_internal_builder_symbol("mutation"), "generic builder method");
        assert!(is_internal_builder_symbol("endpoints"), "RTK builder config");

        assert!(!is_internal_builder_symbol("useGetUserQuery"), "user hook export");
        assert!(!is_internal_builder_symbol("UserService"), "user service export");
        assert!(!is_internal_builder_symbol("getUserById"), "user function export");
    }

    #[test]
    fn test_truncate_signature_short_string_unchanged() {
        let sig = "fn example() -> String";
        let result = truncate_signature(sig, Some(50));
        assert_eq!(result, sig);
    }

    #[test]
    fn test_truncate_signature_truncates_at_generic_opener() {
        let sig = "EnhancedStore<{ users: UserState; posts: PostState; comments: CommentState }>";
        let result = truncate_signature(sig, Some(50));
        assert_eq!(result, "EnhancedStore...");
    }

    #[test]
    fn test_truncate_signature_normalizes_whitespace() {
        let sig = "fn example(\n    arg1: String,\n    arg2: i32\n) -> Result<String, Error>";
        let result = truncate_signature(sig, Some(40));
        assert!(result.starts_with("fn example("));
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_signature_default_length() {
        let sig = "a".repeat(150);
        let result = truncate_signature(&sig, None);
        assert!(result.len() <= DEFAULT_MAX_SIGNATURE_LENGTH);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_signature_unicode_safe() {
        let sig = "fn 例子(参数: 字符串) -> 结果";
        let result = truncate_signature(sig, Some(20));
        assert!(result.ends_with("..."));
        assert!(result.is_char_boundary(result.len() - 3));
    }

    #[test]
    fn test_truncate_signature_simple_generic_preserved() {
        let sig = "Option<String>";
        let result = truncate_signature(sig, Some(100));
        assert_eq!(result, sig);
    }

    #[test]
    fn test_truncate_signature_fallback_byte_truncation() {
        let sig = "some_very_long_function_name_without_generics_that_exceeds_limit";
        let result = truncate_signature(sig, Some(50));
        assert!(result.len() <= 53);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_extract_signature_rust_analyzer_format() {
        use lsp_types::{HoverContents, MarkupContent, MarkupKind};

        // rust-analyzer returns module name first, then actual signature
        let hover = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "\n```rust\nlsproxy\n```\n\n```rust\npub async fn initialize_manager(path: &Path) -> Result<Manager>\n```\n\n---\n\nInitialize a workspace manager".to_string(),
        });

        let (sig, doc) = extract_signature_and_docs(&hover);

        assert!(sig.is_some(), "Should extract signature");
        let sig = sig.unwrap();
        assert!(sig.starts_with("pub async fn"), "Should get actual signature, not module name. Got: {}", sig);
        assert!(sig.contains("initialize_manager"), "Should contain function name");

        assert!(doc.is_some(), "Should extract docs");
        assert!(doc.unwrap().contains("Initialize"), "Docs should contain description");
    }

    #[test]
    fn test_extract_signature_skips_module_name() {
        use lsp_types::{HoverContents, MarkupContent, MarkupKind};

        // Just module name should not be treated as signature
        let hover = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```rust\nsome_module\n```".to_string(),
        });

        let (sig, _) = extract_signature_and_docs(&hover);
        assert!(sig.is_none(), "Single word module name should not be signature");
    }

    #[test]
    fn test_is_variable_assignment() {
        assert!(
            is_variable_assignment("const x = 5"),
            "const with value is assignment"
        );
        assert!(
            is_variable_assignment("const options: SearchOptions = { query: \"test\" }"),
            "const with type and value is assignment"
        );
        assert!(
            is_variable_assignment("let result = calculate()"),
            "let with value is assignment"
        );
        assert!(
            is_variable_assignment("var config: Config = {}"),
            "var with type and value is assignment"
        );

        assert!(
            !is_variable_assignment("const FOO: number"),
            "const with type only is not assignment"
        );
        assert!(
            !is_variable_assignment("type Foo = string"),
            "type alias is not variable assignment"
        );
        assert!(
            !is_variable_assignment("interface SearchOptions"),
            "interface is not variable assignment"
        );
        assert!(
            !is_variable_assignment("function calculate(): number"),
            "function is not variable assignment"
        );
    }

    #[test]
    fn test_is_likely_signature() {
        assert!(is_likely_signature("pub fn example() -> String"));
        assert!(is_likely_signature("fn example()"));
        assert!(is_likely_signature("async fn example()"));
        assert!(is_likely_signature("pub struct Foo<T>"));
        assert!(is_likely_signature("impl<T> Foo<T>"));

        assert!(!is_likely_signature("lsproxy"));
        assert!(!is_likely_signature("some_module"));
        assert!(!is_likely_signature("MyType"));
    }

    #[test]
    fn test_is_likely_signature_rejects_variable_assignments() {
        assert!(
            !is_likely_signature("const x = 5"),
            "const assignment should be rejected"
        );
        assert!(
            !is_likely_signature("let result = calculate()"),
            "let assignment should be rejected"
        );
        assert!(
            !is_likely_signature("var config: Config = {}"),
            "var assignment should be rejected"
        );

        assert!(
            is_likely_signature("const FOO: number"),
            "const with type annotation only should be valid"
        );
    }

    #[test]
    fn test_is_likely_signature_type_annotations() {
        // Type annotations should be recognized as signatures
        assert!(is_likely_signature("manager: &Arc<Manager>"), "type annotation with reference");
        assert!(is_likely_signature("x: i32"), "simple type annotation");
        assert!(is_likely_signature("result: Result<T, E>"), "generic type annotation");
    }

    #[test]
    fn test_is_likely_signature_reference_types() {
        // Reference and pointer types
        assert!(is_likely_signature("&str"), "string slice reference");
        assert!(is_likely_signature("&mut Vec<T>"), "mutable reference");
        assert!(is_likely_signature("*const u8"), "const pointer");
        assert!(is_likely_signature("*mut T"), "mutable pointer");
    }

    #[test]
    fn test_is_likely_signature_rust_special_cases() {
        // Rust-specific patterns
        assert!(is_likely_signature("unsafe fn dangerous()"), "unsafe function");
        assert!(is_likely_signature("extern \"C\" fn callback()"), "extern function");
        assert!(is_likely_signature("dyn Trait"), "trait object");
        assert!(is_likely_signature("pub(crate) fn internal()"), "visibility modifier");
    }

    #[test]
    fn test_is_likely_signature_return_types() {
        // Return type indicators
        assert!(is_likely_signature("-> Result<(), Error>"), "return type only");
        assert!(is_likely_signature("fn foo() -> impl Iterator<Item = i32>"), "impl trait return");
    }

    #[test]
    fn test_is_likely_signature_generic_bounds() {
        // Generic bounds
        assert!(is_likely_signature("where T: Clone + Send"), "where clause");
        assert!(is_likely_signature("T: Iterator<Item = u32>"), "inline bound");
    }

    #[test]
    fn test_extract_signature_prefers_longer_signature() {
        use lsp_types::{HoverContents, MarkupContent, MarkupKind};

        // When multiple code blocks match, prefer the longer/more complete one
        let hover = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```rust\nFoo\n```\n\n```rust\npub struct Foo {\n    field: String,\n}\n```".to_string(),
        });

        let (sig, _) = extract_signature_and_docs(&hover);

        assert!(sig.is_some(), "Should extract signature");
        let sig = sig.unwrap();
        assert!(sig.contains("field"), "Should prefer longer signature with field info. Got: {}", sig);
    }

    #[test]
    fn test_extract_signature_prefers_type_info() {
        use lsp_types::{HoverContents, MarkupContent, MarkupKind};

        // When signatures are similar length, prefer one with type annotations
        let hover = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```rust\nfn example_func\n```\n\n```rust\nfn example() -> i32\n```".to_string(),
        });

        let (sig, _) = extract_signature_and_docs(&hover);

        assert!(sig.is_some(), "Should extract signature");
        let sig = sig.unwrap();
        assert!(sig.contains("->"), "Should prefer signature with return type. Got: {}", sig);
    }

    #[test]
    fn test_extract_signature_ignores_example_block() {
        use lsp_types::{HoverContents, MarkupContent, MarkupKind};

        let hover = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```typescript\nexport interface NavItem {\n  id?: string;\n}\n```\n\nNavigation item interface.\n\n@example\n```typescript\n// RECOMMENDED: Provide stable IDs for reliable persistence\nconst navItem: NavItem = {\n  id: \"dashboard-overview\",\n  label: \"Dashboard\",\n  path: \"/dashboard\",\n};\n```\n"
                .to_string(),
        });

        let (sig, _) = extract_signature_and_docs(&hover);

        assert!(sig.is_some(), "Should extract signature");
        let sig = sig.unwrap();
        assert!(
            sig.contains("interface NavItem"),
            "Should prefer definition signature. Got: {}",
            sig
        );
        assert!(
            !sig.contains("const navItem"),
            "Should not return @example block. Got: {}",
            sig
        );
    }

    #[test]
    fn test_extract_signature_ignores_italic_example_block() {
        use lsp_types::{HoverContents, MarkupContent, MarkupKind};

        let hover = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "\n```typescript\ninterface NavItem\n```\nNavigation item interface for all navigation components.\n\n*@example*  \n```typescript\nconst navItem: NavItem = {\n  id: 'dashboard-overview',\n  label: 'Dashboard',\n  path: '/dashboard',\n};\n```"
                .to_string(),
        });

        let (sig, _) = extract_signature_and_docs(&hover);

        assert!(sig.is_some(), "Should extract signature");
        let sig = sig.unwrap();
        assert!(
            sig.contains("interface NavItem"),
            "Should prefer definition signature. Got: {}",
            sig
        );
        assert!(
            !sig.contains("const navItem"),
            "Should not return example block. Got: {}",
            sig
        );
    }

    #[test]
    fn test_extract_signature_array_prefers_display_string() {
        use lsp_types::{HoverContents, LanguageString, MarkedString};

        let hover = HoverContents::Array(vec![
            MarkedString::LanguageString(LanguageString {
                language: "typescript".to_string(),
                value: "interface NavSection".to_string(),
            }),
            MarkedString::String(
                "Navigation section containing grouped navigation items.".to_string(),
            ),
            MarkedString::LanguageString(LanguageString {
                language: "typescript".to_string(),
                value: "const section: NavSection = { id: \"user-management\", title: \"User Management\", items: [] };"
                    .to_string(),
            }),
        ]);

        let (sig, _) = extract_signature_and_docs(&hover);

        assert!(sig.is_some(), "Should extract signature");
        let sig = sig.unwrap();
        assert!(
            sig.contains("interface NavSection"),
            "Should prefer displayString. Got: {}",
            sig
        );
    }

    #[test]
    fn test_extract_signature_markup_prefers_definition_over_example_block() {
        use lsp_types::{HoverContents, MarkupContent, MarkupKind};

        let hover = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```typescript\ninterface NavSection {\n  id?: string;\n  title?: string;\n  items: NavItem[];\n}\n```\n\nNavigation section containing grouped navigation items.\n\n```typescript\nconst section: NavSection = {\n  id: 'user-management',\n  title: 'User Management',\n  items: [...]\n};\n```\n"
                .to_string(),
        });

        let (sig, _) = extract_signature_and_docs(&hover);

        assert!(sig.is_some(), "Should extract signature");
        let sig = sig.unwrap();
        assert!(
            sig.contains("interface NavSection"),
            "Should prefer definition signature. Got: {}",
            sig
        );
        assert!(
            !sig.contains("const section"),
            "Should not return example block. Got: {}",
            sig
        );
    }
}
