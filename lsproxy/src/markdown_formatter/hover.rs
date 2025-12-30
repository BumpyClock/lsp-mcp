// ABOUTME: Markdown formatter for hover response types.
// ABOUTME: Converts HoverResponse to readable markdown preserving code blocks.

use super::ToMarkdown;
use crate::api_types::{HoverBatchItem, HoverContents, HoverResponse};

/// Wrapper for batch hover results that implements ToMarkdown.
pub struct HoverBatchResponse {
    pub results: Vec<HoverBatchItem>,
}

impl ToMarkdown for HoverBatchResponse {
    fn to_markdown(&self) -> String {
        if self.results.is_empty() {
            return "No hover results".to_string();
        }

        let formatted: Vec<String> = self
            .results
            .iter()
            .map(|item| match item {
                HoverBatchItem::Success(response) => response.to_markdown(),
                HoverBatchItem::Error { error } => format!("Error: {}", error),
            })
            .collect();

        formatted.join("\n\n")
    }
}

impl ToMarkdown for HoverResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        let content = self.extract_content();
        if content.is_empty() {
            output.push_str("No hover information available");
        } else {
            output.push_str(&content);
        }

        if let Some(ref def) = self.definition {
            output.push_str("\n\nDefinition: ");
            output.push_str(&def.path);
            output.push(':');
            output.push_str(&def.line.to_string());
            if def.external == Some(true) {
                output.push_str(" [external]");
            }
        }

        output
    }
}

impl HoverResponse {
    fn extract_content(&self) -> String {
        match &self.contents {
            None => String::new(),
            Some(HoverContents::Markup(s)) => {
                if s.trim().is_empty() {
                    String::new()
                } else {
                    s.clone()
                }
            }
            Some(HoverContents::Array(items)) => {
                if items.is_empty() {
                    String::new()
                } else {
                    items.join("\n\n")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::DefinitionLocation;

    fn create_hover_response_with_markup(content: &str) -> HoverResponse {
        HoverResponse {
            raw_response: None,
            contents: Some(HoverContents::Markup(content.to_string())),
            range: None,
            definition: None,
        }
    }

    fn create_hover_response_with_array(items: Vec<&str>) -> HoverResponse {
        HoverResponse {
            raw_response: None,
            contents: Some(HoverContents::Array(items.into_iter().map(String::from).collect())),
            range: None,
            definition: None,
        }
    }

    fn create_hover_response_with_definition(
        content: &str,
        path: &str,
        line: u32,
        external: Option<bool>,
    ) -> HoverResponse {
        HoverResponse {
            raw_response: None,
            contents: Some(HoverContents::Markup(content.to_string())),
            range: None,
            definition: Some(DefinitionLocation {
                path: path.to_string(),
                line,
                external,
            }),
        }
    }

    #[test]
    fn it_renders_no_information_when_contents_is_none() {
        let response = HoverResponse {
            raw_response: None,
            contents: None,
            range: None,
            definition: None,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("No hover information available"),
            "negative: must indicate no hover info when contents is None"
        );
    }

    #[test]
    fn it_preserves_markup_content_with_code_blocks() {
        let code_block = "```typescript\nfunction configureStore<S>(): EnhancedStore\n```";
        let response = create_hover_response_with_markup(code_block);

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("```typescript"),
            "negative: must preserve code block language annotation"
        );
        assert!(
            markdown.contains("configureStore"),
            "negative: must preserve function name in code block"
        );
    }

    #[test]
    fn it_joins_array_contents_with_separators() {
        let items = vec!["First item", "Second item", "Third item"];
        let response = create_hover_response_with_array(items);

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("First item"),
            "negative: must include first array item"
        );
        assert!(
            markdown.contains("Second item"),
            "negative: must include second array item"
        );
        assert!(
            markdown.contains("Third item"),
            "negative: must include third array item"
        );
    }

    #[test]
    fn it_includes_definition_location_with_path_and_line() {
        let response = create_hover_response_with_definition(
            "Some documentation",
            "src/lib.rs",
            42,
            None,
        );

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("src/lib.rs"),
            "negative: must include definition file path"
        );
        assert!(
            markdown.contains("42"),
            "negative: must include definition line number"
        );
    }

    #[test]
    fn it_marks_external_definitions_with_external_indicator() {
        let response = create_hover_response_with_definition(
            "Redux store configuration",
            "node_modules/@reduxjs/toolkit/dist/index.d.mts",
            1847,
            Some(true),
        );

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[external]"),
            "negative: must mark external definitions with [external] indicator"
        );
    }

    #[test]
    fn it_omits_external_marker_for_non_external_definitions() {
        let response = create_hover_response_with_definition(
            "Local function docs",
            "src/utils.rs",
            15,
            Some(false),
        );

        let markdown = response.to_markdown();

        assert!(
            !markdown.contains("[external]"),
            "negative: must not include [external] for non-external definitions"
        );
    }

    #[test]
    fn it_handles_unicode_content_in_markup() {
        let content = "Docs with unicode: \u{1F600} emoji and \u{4E2D}\u{6587} characters";
        let response = create_hover_response_with_markup(content);

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("\u{1F600}"),
            "negative: must preserve emoji characters"
        );
        assert!(
            markdown.contains("\u{4E2D}\u{6587}"),
            "negative: must preserve CJK characters"
        );
    }

    #[test]
    fn it_handles_empty_markup_content() {
        let response = create_hover_response_with_markup("");

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("No hover information available"),
            "negative: empty markup should be treated as no information"
        );
    }

    #[test]
    fn it_handles_empty_array_contents() {
        let response = HoverResponse {
            raw_response: None,
            contents: Some(HoverContents::Array(vec![])),
            range: None,
            definition: None,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("No hover information available"),
            "negative: empty array should be treated as no information"
        );
    }

    #[test]
    fn it_includes_definition_section_header() {
        let response = create_hover_response_with_definition(
            "Some content",
            "src/main.rs",
            100,
            None,
        );

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Definition:"),
            "negative: must include Definition section label"
        );
    }

    #[test]
    fn it_preserves_multiline_documentation() {
        let multiline_docs = "Creates a Redux store with good defaults.\nAutomatically sets up the Redux DevTools Extension.\n\nSee documentation for more info.";
        let response = create_hover_response_with_markup(multiline_docs);

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Creates a Redux store"),
            "negative: must preserve first line of multiline docs"
        );
        assert!(
            markdown.contains("Redux DevTools Extension"),
            "negative: must preserve middle lines of multiline docs"
        );
        assert!(
            markdown.contains("See documentation"),
            "negative: must preserve last line of multiline docs"
        );
    }

    #[test]
    fn it_formats_definition_with_colon_line_separator() {
        let response = create_hover_response_with_definition(
            "Docs",
            "path/to/file.ts",
            256,
            None,
        );

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("path/to/file.ts:256"),
            "negative: must format definition as path:line"
        );
    }

    mod hover_batch_response_tests {
        use super::*;
        use crate::markdown_formatter::hover::HoverBatchResponse;

        fn create_success_item(content: &str) -> HoverBatchItem {
            HoverBatchItem::Success(HoverResponse {
                raw_response: None,
                contents: Some(HoverContents::Markup(content.to_string())),
                range: None,
                definition: None,
            })
        }

        fn create_error_item(message: &str) -> HoverBatchItem {
            HoverBatchItem::Error {
                error: message.to_string(),
            }
        }

        #[test]
        fn it_renders_empty_message_when_results_are_empty() {
            let batch = HoverBatchResponse { results: vec![] };

            let markdown = batch.to_markdown();

            assert!(
                markdown.contains("No hover results"),
                "negative: empty batch must indicate no results"
            );
        }

        #[test]
        fn it_renders_single_success_result_using_hover_response_format() {
            let batch = HoverBatchResponse {
                results: vec![create_success_item("Function documentation")],
            };

            let markdown = batch.to_markdown();

            assert!(
                markdown.contains("Function documentation"),
                "negative: single success must include hover content"
            );
        }

        #[test]
        fn it_renders_error_item_with_error_prefix() {
            let batch = HoverBatchResponse {
                results: vec![create_error_item("File not found")],
            };

            let markdown = batch.to_markdown();

            assert!(
                markdown.contains("Error:"),
                "negative: error item must have Error prefix"
            );
            assert!(
                !markdown.contains("**Error:**"),
                "negative: error prefix must not use markdown bold"
            );
            assert!(
                markdown.contains("File not found"),
                "negative: error item must include error message"
            );
        }

        #[test]
        fn it_separates_multiple_results_with_double_newline() {
            let batch = HoverBatchResponse {
                results: vec![
                    create_success_item("First result"),
                    create_success_item("Second result"),
                ],
            };

            let markdown = batch.to_markdown();

            assert!(
                !markdown.contains("---"),
                "negative: multiple results must not use horizontal rule separator"
            );
            assert!(
                markdown.contains("First result\n\nSecond result"),
                "negative: results must be separated by double newline"
            );
        }

        #[test]
        fn it_handles_mixed_success_and_error_items() {
            let batch = HoverBatchResponse {
                results: vec![
                    create_success_item("Success content"),
                    create_error_item("Error message"),
                    create_success_item("Another success"),
                ],
            };

            let markdown = batch.to_markdown();

            assert!(
                markdown.contains("Success content"),
                "negative: must include first success"
            );
            assert!(
                markdown.contains("Error:"),
                "negative: must include error indicator"
            );
            assert!(
                markdown.contains("Error message"),
                "negative: must include error message"
            );
            assert!(
                markdown.contains("Another success"),
                "negative: must include second success"
            );
        }

        #[test]
        fn it_uses_double_newline_separator_between_results() {
            let batch = HoverBatchResponse {
                results: vec![
                    create_success_item("First"),
                    create_success_item("Second"),
                ],
            };

            let markdown = batch.to_markdown();

            // Separator should be "\n\n" (no horizontal rule)
            assert!(
                markdown.contains("First\n\nSecond"),
                "negative: results must be separated by double newline"
            );
            assert!(
                !markdown.contains("---"),
                "negative: results must not use horizontal rule separator"
            );
        }
    }
}
