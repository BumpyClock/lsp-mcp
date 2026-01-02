// ABOUTME: Signature extraction and processing utilities.
// ABOUTME: Handles LSP hover parsing, source-based extraction, and symbol filtering.

use crate::api_types::{Position, Symbol};
use crate::lsp::manager::Manager;
use futures::stream::{self, StreamExt};
use lsp_types::Position as LspPosition;

/// Default maximum length for signatures in responses
pub const DEFAULT_MAX_SIGNATURE_LENGTH: usize = 100;

/// Extracts signature and documentation from hover markdown text.
/// Uses pulldown-cmark for robust parsing - works with any language.
pub fn extract_signature_and_docs_from_markdown(text: &str) -> (Option<String>, Option<String>) {
    use super::hover_parser;

    let parsed = hover_parser::parse_hover_markdown(text);
    let signature = hover_parser::select_signature(&parsed);
    let docs = if parsed.text_content.is_empty() {
        None
    } else {
        Some(parsed.text_content)
    };

    (signature, docs)
}

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

/// Extracts signature and documentation from LSP hover contents.
/// Uses pulldown-cmark for robust markdown parsing.
pub(crate) fn extract_signature_and_docs(contents: &lsp_types::HoverContents) -> (Option<String>, Option<String>) {
    use super::hover_parser;
    use lsp_types::{HoverContents, MarkedString, MarkupContent};

    // Convert to unified markdown text
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

    // Parse using pulldown-cmark
    let parsed = hover_parser::parse_hover_markdown(&text);

    // Select signature deterministically
    let signature = hover_parser::select_signature(&parsed);

    // Extract docs from text content
    let docs = if parsed.text_content.is_empty() {
        None
    } else {
        Some(parsed.text_content)
    };

    (signature, docs)
}

/// Extracts signature from source code (fallback when LSP unavailable)
pub(crate) fn extract_signature_from_source(source: &str, symbol_name: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Skip comments and blank lines
        if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with("/*") || trimmed.is_empty() {
            i += 1;
            continue;
        }

        if trimmed.contains(symbol_name) {
            // Check for single-line arrow function
            let is_single_line_arrow = trimmed.contains("=>")
                && (trimmed.contains("const ")
                    || trimmed.contains("let ")
                    || trimmed.contains("var ")
                    || trimmed.contains("export const ")
                    || trimmed.contains("export let ")
                    || trimmed.contains("export var "));

            // Check for non-arrow signatures (fn, function, class, etc.)
            let is_standard_sig = trimmed.contains("fn ") || trimmed.contains("function ") ||
               trimmed.contains("class ") || trimmed.contains("def ") ||
               trimmed.contains("struct ") || trimmed.contains("enum ") ||
               trimmed.contains("interface ") || trimmed.contains("type ");

            if is_single_line_arrow || is_standard_sig {
                // Single-line signature: truncate at `{` or `;`
                let sig = if let Some(brace_pos) = trimmed.find('{') {
                    trimmed[..brace_pos].trim()
                } else if let Some(semi_pos) = trimmed.find(';') {
                    trimmed[..semi_pos].trim()
                } else {
                    trimmed
                };
                return Some(sig.to_string());
            }

            // Check for multi-line const/let/var arrow function declaration
            let is_multiline_arrow_start = !trimmed.contains("=>")
                && (trimmed.contains("const ")
                    || trimmed.contains("let ")
                    || trimmed.contains("var ")
                    || trimmed.contains("export const ")
                    || trimmed.contains("export let ")
                    || trimmed.contains("export var "))
                && (trimmed.contains('(') || trimmed.contains('='));

            if is_multiline_arrow_start {
                // Accumulate lines until we hit `=>`, `{`, or `;`
                let mut accumulated = trimmed.to_string();
                let max_lookahead = 8;

                for j in 1..=max_lookahead {
                    if i + j >= lines.len() {
                        break;
                    }
                    let next_line = lines[i + j].trim();
                    // Skip comments and blank lines in accumulation
                    if next_line.starts_with("//") || next_line.starts_with("#") || next_line.starts_with("/*") || next_line.is_empty() {
                        continue;
                    }
                    accumulated.push(' ');
                    accumulated.push_str(next_line);

                    // Check if we've found the arrow or terminator
                    if accumulated.contains("=>") || accumulated.contains('{') || accumulated.contains(';') {
                        break;
                    }
                }

                // Only return if we found `=>`
                if accumulated.contains("=>") {
                    // Truncate at `{` or `;` if present
                    let sig = if let Some(brace_pos) = accumulated.find('{') {
                        accumulated[..brace_pos].trim()
                    } else if let Some(semi_pos) = accumulated.find(';') {
                        accumulated[..semi_pos].trim()
                    } else {
                        accumulated.trim()
                    };
                    // Normalize whitespace
                    let normalized: String = sig.split_whitespace().collect::<Vec<_>>().join(" ");
                    return Some(normalized);
                }
            }
        }

        i += 1;
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

/// Detects if a symbol is exported based on its kind (best-effort heuristic)
pub(crate) fn detect_exported(kind: &str) -> Option<bool> {
    match kind {
        k if k.contains("export") => Some(true),
        k if k.contains("pub") => Some(true),
        k if k.starts_with("public-") => Some(true),
        _ => Some(false),
    }
}

/// Holds enrichment data computed for a symbol.
/// Used for parallel batch processing.
#[derive(Default)]
struct SymbolEnrichment {
    signature: Option<String>,
    jsdoc_summary: Option<String>,
    exported: Option<bool>,
    dependencies: Option<Vec<String>>,
    line_count: Option<u32>,
}

/// Computes enrichment data for a symbol without mutating it.
/// This allows parallel execution across multiple symbols.
async fn compute_symbol_enrichment(
    manager: &Manager,
    file_path: &str,
    symbol: &Symbol,
) -> SymbolEnrichment {
    let mut enrichment = SymbolEnrichment {
        line_count: Some(
            symbol.file_range.range.end.line
                .saturating_sub(symbol.file_range.range.start.line)
                .saturating_add(1)
        ),
        exported: detect_exported(&symbol.kind),
        ..Default::default()
    };

    let hover_position = lsp_types::Position {
        line: symbol.identifier_position.position.line.saturating_sub(1),
        character: symbol.identifier_position.position.character.saturating_sub(1),
    };

    // Try LSP hover first
    if let Ok(Some(hover)) = manager.hover(file_path, hover_position).await {
        let (sig, jsdoc) = extract_signature_and_docs(&hover.contents);
        enrichment.signature = sig;
        enrichment.jsdoc_summary = jsdoc;
    }

    // Fallback to source-based extraction if needed
    if enrichment.signature.is_none() || enrichment.jsdoc_summary.is_none() {
        if let Ok(source_code) = manager.read_source_code(
            file_path,
            Some(lsp_types::Range::new(
                lsp_types::Position {
                    line: symbol.file_range.range.start.line.saturating_sub(1),
                    character: 0,
                },
                lsp_types::Position {
                    line: symbol.file_range.range.end.line,
                    character: 0,
                },
            )),
        ).await {
            if enrichment.signature.is_none() {
                enrichment.signature = extract_signature_from_source(&source_code, &symbol.name);
            }
            if enrichment.jsdoc_summary.is_none() {
                enrichment.jsdoc_summary = extract_docs_from_source(&source_code);
            }
        }
    }

    // Truncate signature
    if let Some(ref sig) = enrichment.signature {
        enrichment.signature = Some(truncate_signature(sig, None));
    }

    // Get dependencies
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
            enrichment.dependencies = Some(unique_deps);
        }
    }

    enrichment
}

/// Applies enrichment data to a symbol.
fn apply_enrichment(symbol: &mut Symbol, enrichment: SymbolEnrichment) {
    symbol.line_count = enrichment.line_count;
    symbol.exported = enrichment.exported;
    if enrichment.signature.is_some() {
        symbol.signature = enrichment.signature;
    }
    if enrichment.jsdoc_summary.is_some() {
        symbol.jsdoc_summary = enrichment.jsdoc_summary;
    }
    if enrichment.dependencies.is_some() {
        symbol.dependencies = enrichment.dependencies;
    }
}

/// Index path to locate a symbol in a nested tree.
/// Each element is the index at that level of nesting.
type SymbolIndexPath = Vec<usize>;

/// Collects all symbols from a tree into a flat list with their index paths.
fn collect_symbol_paths(symbols: &[Symbol]) -> Vec<(SymbolIndexPath, Position)> {
    fn collect_recursive(
        symbols: &[Symbol],
        current_path: &[usize],
        result: &mut Vec<(SymbolIndexPath, Position)>,
    ) {
        for (i, symbol) in symbols.iter().enumerate() {
            let mut path = current_path.to_vec();
            path.push(i);
            result.push((path.clone(), symbol.identifier_position.position.clone()));

            if let Some(ref children) = symbol.children {
                collect_recursive(children, &path, result);
            }
        }
    }

    let mut result = Vec::new();
    collect_recursive(symbols, &[], &mut result);
    result
}

/// Gets a mutable reference to a symbol at the given index path.
fn get_symbol_mut<'a>(symbols: &'a mut [Symbol], path: &[usize]) -> Option<&'a mut Symbol> {
    if path.is_empty() {
        return None;
    }

    let mut current = symbols.get_mut(path[0])?;
    for &idx in &path[1..] {
        current = current.children.as_mut()?.get_mut(idx)?;
    }
    Some(current)
}

/// Gets a reference to a symbol at the given index path.
fn get_symbol<'a>(symbols: &'a [Symbol], path: &[usize]) -> Option<&'a Symbol> {
    if path.is_empty() {
        return None;
    }

    let mut current = symbols.get(path[0])?;
    for &idx in &path[1..] {
        current = current.children.as_ref()?.get(idx)?;
    }
    Some(current)
}

/// Default concurrency limit for batch enrichment.
pub const DEFAULT_ENRICHMENT_CONCURRENCY: usize = 8;

/// Enriches multiple symbols in parallel with bounded concurrency.
///
/// This function:
/// 1. Flattens the symbol tree into a list of index paths
/// 2. Computes enrichment for each symbol concurrently (limited to `concurrency_limit`)
/// 3. Applies results back to the symbols
///
/// Using bounded concurrency prevents overwhelming the LSP server while still
/// achieving significant speedup over sequential processing.
pub(crate) async fn batch_enrich_symbols(
    manager: &Manager,
    file_path: &str,
    symbols: &mut [Symbol],
    concurrency_limit: usize,
) {
    let symbol_paths = collect_symbol_paths(symbols);

    if symbol_paths.is_empty() {
        return;
    }

    // Collect (path, symbol_clone) pairs for processing
    // We clone symbols because we can't hold references across await points
    let symbol_data: Vec<(SymbolIndexPath, Symbol)> = symbol_paths
        .iter()
        .filter_map(|(path, _)| {
            get_symbol(symbols, path).map(|s| (path.clone(), s.clone()))
        })
        .collect();

    // Process symbols with bounded concurrency
    let enrichments: Vec<(SymbolIndexPath, SymbolEnrichment)> = stream::iter(symbol_data)
        .map(|(path, symbol)| async move {
            let enrichment = compute_symbol_enrichment(manager, file_path, &symbol).await;
            (path, enrichment)
        })
        .buffer_unordered(concurrency_limit)
        .collect()
        .await;

    // Apply enrichments back to symbols
    for (path, enrichment) in enrichments {
        if let Some(symbol) = get_symbol_mut(symbols, &path) {
            apply_enrichment(symbol, enrichment);
        }
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

/// Result of extracting active signature from SignatureHelp
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveSignatureInfo {
    pub label: String,
    pub active_parameter: Option<u32>,
}

/// Extracts the active signature label and parameter from LSP SignatureHelp
///
/// Returns the active signature's label (truncated) and the active parameter index.
/// If no signatures are available, returns None.
pub fn extract_active_signature(sig_help: &lsp_types::SignatureHelp) -> Option<ActiveSignatureInfo> {
    if sig_help.signatures.is_empty() {
        return None;
    }

    let active_idx = sig_help.active_signature.unwrap_or(0) as usize;
    let signature = sig_help.signatures.get(active_idx).or_else(|| sig_help.signatures.first())?;

    let active_param = signature
        .active_parameter
        .or(sig_help.active_parameter);

    Some(ActiveSignatureInfo {
        label: truncate_signature(&signature.label, None),
        active_parameter: active_param,
    })
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
    fn test_extract_signature_from_source_handles_export_const_arrow() {
        let source = "export const filterNavigationByPermissions = <T>(items: T[], hasPermission: boolean): T[] => {";
        let sig = extract_signature_from_source(source, "filterNavigationByPermissions");
        assert!(sig.is_some(), "arrow function signature must be detected");
        assert!(
            sig.unwrap().starts_with("export const filterNavigationByPermissions"),
            "signature must include export const declaration"
        );
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
    fn test_extract_signature_uses_first_definition() {
        use lsp_types::{HoverContents, MarkupContent, MarkupKind};

        // Deterministic selection: first definition-like block wins
        let hover = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```rust\nfn example_func\n```\n\n```rust\nfn example() -> i32\n```".to_string(),
        });

        let (sig, _) = extract_signature_and_docs(&hover);

        assert!(sig.is_some(), "Should extract signature");
        let sig = sig.unwrap();
        // First definition-like block is selected
        assert!(sig.contains("fn example_func"), "Should select first definition. Got: {}", sig);
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

    mod active_signature_tests {
        use super::*;
        use lsp_types::{ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation};

        fn create_signature_info(label: &str, active_param: Option<u32>) -> SignatureInformation {
            SignatureInformation {
                label: label.to_string(),
                documentation: None,
                parameters: Some(vec![
                    ParameterInformation {
                        label: ParameterLabel::Simple("arg1".to_string()),
                        documentation: None,
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("arg2".to_string()),
                        documentation: None,
                    },
                ]),
                active_parameter: active_param,
            }
        }

        #[test]
        fn it_returns_none_when_signatures_array_is_empty() {
            let sig_help = SignatureHelp {
                signatures: vec![],
                active_signature: None,
                active_parameter: None,
            };

            let result = extract_active_signature(&sig_help);

            assert!(
                result.is_none(),
                "negative: must return None for empty signatures"
            );
        }

        #[test]
        fn it_extracts_first_signature_when_active_signature_is_none() {
            let sig_help = SignatureHelp {
                signatures: vec![
                    create_signature_info("fn\tfirst(\u{00A0}x: i32)", None),
                    create_signature_info("fn second(y: String)", None),
                ],
                active_signature: None,
                active_parameter: Some(1),
            };

            let result = extract_active_signature(&sig_help);

            assert!(result.is_some(), "negative: must extract signature");
            let info = result.unwrap();
            assert!(
                info.label.contains("first"),
                "negative: must use first signature when active_signature is None, got: {}",
                info.label
            );
            assert_eq!(
                info.active_parameter,
                Some(1),
                "negative: must use SignatureHelp.active_parameter"
            );
        }

        #[test]
        fn it_extracts_signature_at_active_signature_index() {
            let sig_help = SignatureHelp {
                signatures: vec![
                    create_signature_info("fn zeroth()", None),
                    create_signature_info("fn\tone(\u{4E2D}\u{6587}_param: \u{1F600})", Some(0)),
                    create_signature_info("fn second()", None),
                ],
                active_signature: Some(1),
                active_parameter: None,
            };

            let result = extract_active_signature(&sig_help);

            assert!(result.is_some(), "negative: must extract signature");
            let info = result.unwrap();
            assert!(
                info.label.contains("one"),
                "negative: must use signature at active_signature index, got: {}",
                info.label
            );
            assert_eq!(
                info.active_parameter,
                Some(0),
                "negative: must prefer SignatureInformation.active_parameter"
            );
        }

        #[test]
        fn it_falls_back_to_first_signature_when_index_out_of_bounds() {
            let sig_help = SignatureHelp {
                signatures: vec![create_signature_info("fn only_one()", None)],
                active_signature: Some(99),
                active_parameter: Some(0),
            };

            let result = extract_active_signature(&sig_help);

            assert!(result.is_some(), "negative: must extract signature");
            let info = result.unwrap();
            assert!(
                info.label.contains("only_one"),
                "negative: must fallback to first signature when index OOB, got: {}",
                info.label
            );
        }

        #[test]
        fn it_truncates_long_signature_labels() {
            let long_label = format!(
                "fn very_long_function_name<T: SomeTrait + AnotherTrait>(arg1: {}, arg2: {})",
                "VeryLongTypeName".repeat(10),
                "AnotherLongType".repeat(10)
            );
            let sig_help = SignatureHelp {
                signatures: vec![create_signature_info(&long_label, None)],
                active_signature: Some(0),
                active_parameter: None,
            };

            let result = extract_active_signature(&sig_help);

            assert!(result.is_some(), "negative: must extract signature");
            let info = result.unwrap();
            assert!(
                info.label.len() <= DEFAULT_MAX_SIGNATURE_LENGTH,
                "negative: signature must be truncated to max length, got len={}",
                info.label.len()
            );
        }

        #[test]
        fn it_prefers_signature_active_parameter_over_help_active_parameter() {
            let sig_help = SignatureHelp {
                signatures: vec![create_signature_info("fn test()", Some(42))],
                active_signature: Some(0),
                active_parameter: Some(7),
            };

            let result = extract_active_signature(&sig_help);

            assert!(result.is_some(), "negative: must extract signature");
            let info = result.unwrap();
            assert_eq!(
                info.active_parameter,
                Some(42),
                "negative: must prefer SignatureInformation.active_parameter over SignatureHelp.active_parameter"
            );
        }

        #[test]
        fn it_uses_help_active_parameter_when_signature_has_none() {
            let sig_help = SignatureHelp {
                signatures: vec![create_signature_info("fn test()", None)],
                active_signature: Some(0),
                active_parameter: Some(3),
            };

            let result = extract_active_signature(&sig_help);

            assert!(result.is_some(), "negative: must extract signature");
            let info = result.unwrap();
            assert_eq!(
                info.active_parameter,
                Some(3),
                "negative: must fallback to SignatureHelp.active_parameter"
            );
        }

        #[test]
        fn it_handles_unicode_in_signature_label() {
            let sig_help = SignatureHelp {
                signatures: vec![create_signature_info(
                    "fn \u{4E2D}\u{6587}\u{51FD}\u{6570}(\u{53C2}\u{6570}: \u{5B57}\u{7B26}\u{4E32}) -> \u{7ED3}\u{679C}",
                    None,
                )],
                active_signature: Some(0),
                active_parameter: None,
            };

            let result = extract_active_signature(&sig_help);

            assert!(result.is_some(), "negative: must extract unicode signature");
            let info = result.unwrap();
            assert!(
                info.label.contains('\u{4E2D}'),
                "negative: must preserve CJK characters in signature"
            );
        }
    }
}
