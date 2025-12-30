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
    let mut signature = None;
    let mut docs = Vec::new();
    let mut in_code_block = false;
    let mut code_lines = Vec::new();

    for line in lines {
        if line.starts_with("```") {
            if in_code_block {
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
}
