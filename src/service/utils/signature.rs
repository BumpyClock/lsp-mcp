// ABOUTME: Signature extraction and processing utilities.
// ABOUTME: Handles LSP hover parsing, source-based extraction, and symbol filtering.

use crate::api_types::Symbol;

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
pub(crate) fn extract_signature_and_docs(
    contents: &lsp_types::HoverContents,
) -> (Option<String>, Option<String>) {
    use super::hover_parser;
    use lsp_types::{HoverContents, MarkedString, MarkupContent};

    // Convert to unified markdown text
    let text = match contents {
        HoverContents::Scalar(MarkedString::String(s)) => s.clone(),
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => {
            format!("```{}\n{}\n```", ls.language, ls.value)
        }
        HoverContents::Markup(MarkupContent { value, .. }) => value.clone(),
        HoverContents::Array(arr) => arr
            .iter()
            .map(|m| match m {
                MarkedString::String(s) => s.clone(),
                MarkedString::LanguageString(ls) => {
                    format!("```{}\n{}\n```", ls.language, ls.value)
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
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

/// Extracts the identifier name from hover contents.
/// Takes the first identifier-like word from the signature.
pub(crate) fn extract_identifier_name_from_hover(contents: &lsp_types::HoverContents) -> String {
    use lsp_types::{HoverContents, MarkedString, MarkupContent};

    let text = match contents {
        HoverContents::Scalar(MarkedString::String(s)) => s.clone(),
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => ls.value.clone(),
        HoverContents::Markup(MarkupContent { value, .. }) => value.clone(),
        HoverContents::Array(arr) => arr
            .first()
            .map(|m| match m {
                MarkedString::String(s) => s.clone(),
                MarkedString::LanguageString(ls) => ls.value.clone(),
            })
            .unwrap_or_default(),
    };

    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .find(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
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
pub fn extract_active_signature(
    sig_help: &lsp_types::SignatureHelp,
) -> Option<ActiveSignatureInfo> {
    if sig_help.signatures.is_empty() {
        return None;
    }

    let active_idx = sig_help.active_signature.unwrap_or(0) as usize;
    let signature = sig_help
        .signatures
        .get(active_idx)
        .or_else(|| sig_help.signatures.first())?;

    let active_param = signature.active_parameter.or(sig_help.active_parameter);

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
        assert!(
            is_internal_builder_symbol("_baseEndpointQuery"),
            "underscore prefix indicates internal"
        );
        assert!(
            is_internal_builder_symbol("providesTags"),
            "RTK builder function"
        );
        assert!(
            is_internal_builder_symbol("invalidatesTags"),
            "RTK builder function"
        );
        assert!(
            is_internal_builder_symbol("query"),
            "generic builder method"
        );
        assert!(
            is_internal_builder_symbol("mutation"),
            "generic builder method"
        );
        assert!(
            is_internal_builder_symbol("endpoints"),
            "RTK builder config"
        );

        assert!(
            !is_internal_builder_symbol("useGetUserQuery"),
            "user hook export"
        );
        assert!(
            !is_internal_builder_symbol("UserService"),
            "user service export"
        );
        assert!(
            !is_internal_builder_symbol("getUserById"),
            "user function export"
        );
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
        assert!(
            sig.starts_with("pub async fn"),
            "Should get actual signature, not module name. Got: {}",
            sig
        );
        assert!(
            sig.contains("initialize_manager"),
            "Should contain function name"
        );

        assert!(doc.is_some(), "Should extract docs");
        assert!(
            doc.unwrap().contains("Initialize"),
            "Docs should contain description"
        );
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
        assert!(
            sig.is_none(),
            "Single word module name should not be signature"
        );
    }

    #[test]
    fn test_extract_signature_prefers_longer_signature() {
        use lsp_types::{HoverContents, MarkupContent, MarkupKind};

        // When multiple code blocks match, prefer the longer/more complete one
        let hover = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```rust\nFoo\n```\n\n```rust\npub struct Foo {\n    field: String,\n}\n```"
                .to_string(),
        });

        let (sig, _) = extract_signature_and_docs(&hover);

        assert!(sig.is_some(), "Should extract signature");
        let sig = sig.unwrap();
        assert!(
            sig.contains("field"),
            "Should prefer longer signature with field info. Got: {}",
            sig
        );
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
        assert!(
            sig.contains("fn example_func"),
            "Should select first definition. Got: {}",
            sig
        );
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
        use lsp_types::{
            ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation,
        };

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
