// ABOUTME: MCP tool response formatting with direct JSON output.
// ABOUTME: Returns data directly on success, with optional metadata in verbose mode.

use crate::config::OutputMode;
use crate::markdown_formatter::ToMarkdown;
use crate::service::ServiceError;
use rmcp::model::{CallToolResult, Content};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Response metadata (only in verbose mode)
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ResponseMeta {
    pub tool: String,
    pub mode: String,
    pub indexing: String,
    pub line_indexing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<HashMap<String, usize>>,
}

/// Normalize LSP symbol kind strings to lower-kebab-case
pub fn normalize_kind(kind: &str) -> String {
    // Handle common LSP SymbolKind formats
    let mut result = String::new();
    let mut prev_was_lowercase = false;

    for (i, ch) in kind.chars().enumerate() {
        if i == 0 {
            // First character is always lowercase
            result.push(ch.to_ascii_lowercase());
            prev_was_lowercase = ch.is_lowercase();
        } else if ch.is_uppercase() {
            // Insert hyphen before uppercase if previous was lowercase
            if prev_was_lowercase {
                result.push('-');
            }
            result.push(ch.to_ascii_lowercase());
            prev_was_lowercase = false;
        } else {
            result.push(ch);
            prev_was_lowercase = ch.is_lowercase();
        }
    }

    result
}

/// Create a success response - returns data directly, with meta as sibling in verbose mode
pub fn success_response(
    tool_name: &str,
    data: Value,
    output_mode: OutputMode,
    counts: Option<HashMap<String, usize>>,
) -> String {
    match output_mode {
        OutputMode::Default => serde_json::to_string(&data).unwrap(),
        OutputMode::Verbose => {
            // Add meta as a sibling field to the data
            let mut obj = match data {
                Value::Object(map) => map,
                _ => {
                    // If data is not an object, wrap it
                    let mut map = Map::new();
                    map.insert("data".to_string(), data);
                    map
                }
            };

            let meta = ResponseMeta {
                tool: tool_name.to_string(),
                mode: "verbose".to_string(),
                indexing: "one-based".to_string(),
                line_indexing: "one-based".to_string(),
                counts,
            };
            obj.insert("meta".to_string(), serde_json::to_value(meta).unwrap());

            serde_json::to_string_pretty(&Value::Object(obj)).unwrap()
        }
    }
}

/// Format an error for MCP protocol-level error response
pub fn format_error(error: &ServiceError) -> String {
    let base_message = error.to_string();
    let suggestions = error.suggestions();

    if suggestions.is_empty() {
        return base_message;
    }

    let mut result = base_message;
    result.push_str("\n\nSuggestion:");
    for suggestion in &suggestions {
        result.push_str("\n  - ");
        result.push_str(suggestion);
    }

    result
}

/// Convert a ServiceError to a CallToolResult.
///
/// IdentifierSelection and CallHierarchy errors are returned as text messages
/// (not MCP errors) because they are informational - providing suggestions for
/// nearby identifiers or callables. All other errors are returned with is_error: true.
pub fn tool_result_from_error(error: ServiceError) -> CallToolResult {
    let message = format_error(&error);
    match error {
        ServiceError::IdentifierSelection(_) | ServiceError::CallHierarchy(_) => {
            CallToolResult::success(vec![Content::text(message)])
        }
        _ => CallToolResult::error(vec![Content::text(message)]),
    }
}

/// Create a success CallToolResult with text content.
pub fn tool_result_success(text: String) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text)])
}

/// Create an error CallToolResult with text content.
pub fn tool_result_error(message: String) -> CallToolResult {
    CallToolResult::error(vec![Content::text(message)])
}

/// Format a tool disabled message for MCP protocol-level error response
pub fn tool_disabled_message(tool_name: &str) -> String {
    format!(
        "Tool '{}' is disabled. Enable it in your .lsp-mcp.json config.",
        tool_name
    )
}

/// Format a response as markdown using the ToMarkdown trait.
///
/// Currently always produces markdown output. The `OutputMode` parameter is
/// reserved for future use (e.g., to support JSON output mode).
pub fn format_response<T: ToMarkdown>(response: &T, _output_mode: OutputMode) -> String {
    response.to_markdown()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::manager::LspManagerError;
    use rand::{distr::Alphanumeric, Rng};
    use serde_json::json;
    use std::thread;

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

    #[test]
    fn test_normalize_kind_simple_lowercase() {
        assert_eq!(normalize_kind("function"), "function");
        assert_eq!(normalize_kind("class"), "class");
    }

    #[test]
    fn test_normalize_kind_camel_case() {
        assert_eq!(normalize_kind("Function"), "function");
        assert_eq!(normalize_kind("EnumMember"), "enum-member");
        assert_eq!(normalize_kind("TypeParameter"), "type-parameter");
    }

    #[test]
    fn test_normalize_kind_single_uppercase() {
        assert_eq!(normalize_kind("Struct"), "struct");
        assert_eq!(normalize_kind("Module"), "module");
    }

    #[test]
    fn test_success_response_compact() {
        let data = json!({"symbols": [{"name": "foo"}]});
        let result = success_response("test_tool", data.clone(), OutputMode::Default, None);

        // Should be compact JSON
        assert!(!result.contains('\n'));

        // Parse and verify - data returned directly, no wrapper
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, data);

        // Should NOT have "ok" or "data" wrapper
        assert!(!result.contains("\"ok\""));
        assert!(parsed.get("symbols").is_some());
    }

    #[test]
    fn test_success_response_verbose() {
        let data = json!({"symbols": [{"name": "foo"}]});
        let counts = {
            let mut m = HashMap::new();
            m.insert("symbols".to_string(), 1);
            m
        };
        let result = success_response(
            "test_tool",
            data.clone(),
            OutputMode::Verbose,
            Some(counts.clone()),
        );

        // Should be pretty printed
        assert!(result.contains('\n'));

        // Parse and verify - data with meta as sibling
        let parsed: Value = serde_json::from_str(&result).unwrap();

        // Should NOT have "ok" wrapper
        assert!(parsed.get("ok").is_none());

        // Should have original data fields
        assert!(parsed.get("symbols").is_some());

        // Should have meta as sibling
        let meta = parsed.get("meta").expect("meta should be present");
        assert_eq!(meta.get("tool").unwrap().as_str().unwrap(), "test_tool");
        assert_eq!(meta.get("mode").unwrap().as_str().unwrap(), "verbose");
        assert_eq!(meta.get("indexing").unwrap().as_str().unwrap(), "one-based");
        assert_eq!(
            meta.get("counts")
                .unwrap()
                .get("symbols")
                .unwrap()
                .as_u64()
                .unwrap(),
            1
        );
    }

    #[test]
    fn test_format_error() {
        let error = ServiceError::Lsp(LspManagerError::FileNotFound("test.rs".to_string()));
        let result = format_error(&error);

        // Should be a simple error message string
        assert!(result.contains("test.rs"));
        // Should NOT be JSON
        assert!(!result.starts_with('{'));
    }

    #[test]
    fn test_tool_disabled_message() {
        let result = tool_disabled_message("disabled_tool");

        assert!(result.contains("disabled_tool"));
        assert!(result.contains("disabled"));
        assert!(result.contains(".lsp-mcp.json"));
        // Should NOT be JSON
        assert!(!result.starts_with('{'));
    }

    #[test]
    fn it_includes_line_indexing_in_verbose_metadata() {
        let tool_name = random_irregular_string();
        let value = random_irregular_string();
        let data = json!({"symbols": [{"name": value}]});
        let response = retry_with(|| {
            let tool_name = tool_name.clone();
            let data = data.clone();
            let handle = thread::spawn(move || {
                success_response(&tool_name, data, OutputMode::Verbose, None)
            });
            handle.join().ok()
        });
        let parsed: Value =
            serde_json::from_str(&response).expect("negative: response did not parse");
        let meta = parsed
            .get("meta")
            .expect("negative: meta missing from response");
        assert_eq!(
            meta.get("line_indexing").unwrap().as_str().unwrap(),
            "one-based",
            "negative: line indexing missing or incorrect"
        );
    }

    #[test]
    fn test_success_response_non_object_data() {
        // Test with array data (non-object)
        let data = json!(["item1", "item2"]);
        let result = success_response("test_tool", data.clone(), OutputMode::Default, None);

        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, data);
    }

    #[test]
    fn test_success_response_non_object_verbose() {
        // Test with array data in verbose mode - should wrap in object with "data" key
        let data = json!(["item1", "item2"]);
        let result = success_response("test_tool", data.clone(), OutputMode::Verbose, None);

        let parsed: Value = serde_json::from_str(&result).unwrap();
        // Should have "data" key containing the array
        assert_eq!(parsed.get("data").unwrap(), &data);
        // Should have meta
        assert!(parsed.get("meta").is_some());
    }

    #[test]
    fn it_formats_error_with_suggestions_when_available() {
        use crate::api_types::{FileRange, Identifier, Position, Range};
        use crate::service::PositionError;

        let closest = vec![Identifier {
            name: "nearby_symbol".to_string(),
            file_range: FileRange {
                path: "test.rs".to_string(),
                range: Range {
                    start: Position {
                        line: 5,
                        character: 1,
                    },
                    end: Position {
                        line: 5,
                        character: 14,
                    },
                },
            },
            kind: Some("function".to_string()),
        }];

        let error =
            ServiceError::IdentifierSelection(PositionError::IdentifierNotFound { closest });
        let result = format_error(&error);

        assert!(
            result.contains("Suggestion"),
            "negative: error format should include suggestions section"
        );
        assert!(
            result.contains("documentSymbol"),
            "negative: error format should include documentSymbol suggestion"
        );
    }

    #[test]
    fn it_formats_call_hierarchy_error_with_nearby_callables() {
        use crate::api_types::{FilePosition, FileRange, Position, Range, Symbol};
        use crate::service::CallHierarchyError;

        let nearby = vec![Symbol {
            name: "nearby_function".to_string(),
            kind: "function".to_string(),
            identifier_position: FilePosition {
                path: "test.rs".to_string(),
                position: Position {
                    line: 10,
                    character: 4,
                },
            },
            file_range: FileRange {
                path: "test.rs".to_string(),
                range: Range {
                    start: Position {
                        line: 10,
                        character: 1,
                    },
                    end: Position {
                        line: 15,
                        character: 1,
                    },
                },
            },
            ..Default::default()
        }];

        let error = ServiceError::CallHierarchy(CallHierarchyError::NoItemAtPosition {
            nearby_callables: nearby,
        });
        let result = format_error(&error);

        assert!(
            result.contains("Suggestion"),
            "negative: call hierarchy error should include suggestions"
        );
        assert!(
            result.contains("nearby_function"),
            "negative: call hierarchy error should include nearby callable names"
        );
    }

    #[test]
    fn it_formats_error_without_suggestions_for_simple_errors() {
        let error = ServiceError::Serialization("test error".to_string());
        let result = format_error(&error);

        assert!(
            !result.contains("Suggestion"),
            "negative: simple errors should not include suggestions section"
        );
    }

    // Test struct for format_response tests
    struct TestMarkdownResponse {
        content: String,
    }

    impl crate::markdown_formatter::ToMarkdown for TestMarkdownResponse {
        fn to_markdown(&self) -> String {
            format!("# Test Response\n\n{}", self.content)
        }
    }

    #[test]
    fn it_formats_response_using_to_markdown_trait() {
        let response = TestMarkdownResponse {
            content: "Hello, World!".to_string(),
        };

        let result = format_response(&response, OutputMode::Default);

        assert!(
            result.contains("# Test Response"),
            "negative: format_response must call to_markdown on the response"
        );
        assert!(
            result.contains("Hello, World!"),
            "negative: format_response must include the response content"
        );
    }

    #[test]
    fn it_ignores_output_mode_and_returns_markdown() {
        let response = TestMarkdownResponse {
            content: "test content".to_string(),
        };

        let default_result = format_response(&response, OutputMode::Default);
        let verbose_result = format_response(&response, OutputMode::Verbose);

        assert_eq!(
            default_result, verbose_result,
            "negative: format_response should return same markdown regardless of OutputMode"
        );
    }

    fn is_error_result(result: &CallToolResult) -> bool {
        result.is_error == Some(true)
    }

    #[test]
    fn tool_result_from_error_returns_success_for_identifier_selection() {
        use crate::api_types::{FileRange, Identifier, Position, Range};
        use crate::service::PositionError;

        let identifier = Identifier {
            name: "Button".to_string(),
            file_range: FileRange {
                path: "src/test.rs".to_string(),
                range: Range {
                    start: Position {
                        line: 1,
                        character: 1,
                    },
                    end: Position {
                        line: 1,
                        character: 6,
                    },
                },
            },
            kind: None,
        };
        let error = ServiceError::IdentifierSelection(PositionError::IdentifierNotFound {
            closest: vec![identifier],
        });

        let result = tool_result_from_error(error);
        assert!(
            !is_error_result(&result),
            "negative: identifier selection should not be an error result"
        );
    }

    #[test]
    fn tool_result_from_error_returns_error_for_other_errors() {
        let error = ServiceError::Serialization("test error".to_string());

        let result = tool_result_from_error(error);
        assert!(
            is_error_result(&result),
            "negative: non-identifier-selection errors should be error result"
        );
    }

    #[test]
    fn tool_result_from_error_returns_success_for_call_hierarchy() {
        use crate::api_types::{FilePosition, FileRange, Position, Range, Symbol};
        use crate::service::CallHierarchyError;

        let nearby = vec![Symbol {
            name: "nearby_function".to_string(),
            kind: "function".to_string(),
            identifier_position: FilePosition {
                path: "test.rs".to_string(),
                position: Position {
                    line: 10,
                    character: 4,
                },
            },
            file_range: FileRange {
                path: "test.rs".to_string(),
                range: Range {
                    start: Position {
                        line: 10,
                        character: 1,
                    },
                    end: Position {
                        line: 15,
                        character: 1,
                    },
                },
            },
            ..Default::default()
        }];

        let error = ServiceError::CallHierarchy(CallHierarchyError::NoItemAtPosition {
            nearby_callables: nearby,
        });

        let result = tool_result_from_error(error);
        assert!(
            !is_error_result(&result),
            "negative: call hierarchy errors should not be MCP errors"
        );
    }
}
