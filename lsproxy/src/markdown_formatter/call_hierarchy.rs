// ABOUTME: Markdown formatter for call hierarchy response types.
// ABOUTME: Converts incoming/outgoing call results to readable markdown.

use super::{escape_inline_code, format_file_position, ToMarkdown};
use crate::api_types::{
    CallHierarchyDirection, CallHierarchyResponse, CallInfo, ImplementationResponse,
};

impl ToMarkdown for CallHierarchyResponse {
    fn to_markdown(&self) -> String {
        let call_count = self.calls.len();
        let (direction_label, entity_label) = match self.direction {
            CallHierarchyDirection::Incoming => ("Incoming Calls", "callers"),
            CallHierarchyDirection::Outgoing => ("Outgoing Calls from", "callees"),
        };

        let preposition = match self.direction {
            CallHierarchyDirection::Incoming => "to",
            CallHierarchyDirection::Outgoing => "",
        };

        let header = if preposition.is_empty() {
            format!(
                "## {} ({} {})\n",
                direction_label, call_count, entity_label
            )
        } else {
            format!(
                "## {} {} ({} {})\n",
                direction_label, preposition, call_count, entity_label
            )
        };

        if self.calls.is_empty() {
            return format!("{}No {} found", header, entity_label);
        }

        let calls_markdown: Vec<String> = self
            .calls
            .iter()
            .map(|call| format_call_info(call))
            .collect();

        format!("{}\n{}", header, calls_markdown.join("\n\n"))
    }
}

fn format_call_info(call: &CallInfo) -> String {
    let item = &call.item;
    let position = format_file_position(
        &item.location.path,
        item.location.position.line,
        item.location.position.character,
    );
    let external_tag = if item.external.unwrap_or(false) {
        " [external]"
    } else {
        ""
    };

    let header = match &item.detail {
        Some(detail) => format!("### {} ({}{})\n*{}*", item.name, position, external_tag, detail),
        None => format!("### {} ({}{})", item.name, position, external_tag),
    };

    if call.call_ranges.is_empty() {
        return header;
    }

    let ranges: Vec<String> = call
        .call_ranges
        .iter()
        .enumerate()
        .map(|(i, range)| {
            let line_col = format!("{}:{}", range.start.line, range.start.character);
            match call.call_snippets.as_ref().and_then(|s| s.get(i)) {
                Some(snippet) if !snippet.is_empty() => {
                    let escaped = escape_inline_code(snippet.trim());
                    format!("- **Line {}**: `{}`", line_col, escaped)
                }
                _ => format!("- **Line {}**: call site", line_col),
            }
        })
        .collect();

    format!("{}\n{}", header, ranges.join("\n"))
}

impl ToMarkdown for ImplementationResponse {
    fn to_markdown(&self) -> String {
        let count = self.implementations.len();
        let identifier_name = &self.selected_identifier.name;

        let header = format!(
            "## Implementations of `{}` ({} found)\n",
            identifier_name, count
        );

        if self.implementations.is_empty() {
            return format!("{}No implementations found", header);
        }

        let implementations_markdown: Vec<String> = self
            .implementations
            .iter()
            .map(|impl_pos| {
                format!(
                    "- {}:{}",
                    impl_pos.path, impl_pos.position.line
                )
            })
            .collect();

        format!("{}\n{}", header, implementations_markdown.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{
        CallHierarchyItemInfo, FilePosition, FileRange, Identifier, Position, Range,
    };
    use rand::Rng;

    fn random_line() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..500)
    }

    fn random_character() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..100)
    }

    fn create_call_hierarchy_item(name: &str, path: &str, line: u32) -> CallHierarchyItemInfo {
        CallHierarchyItemInfo {
            name: name.to_string(),
            kind: "function".to_string(),
            location: FilePosition {
                path: path.to_string(),
                position: Position {
                    line,
                    character: random_character(),
                },
            },
            range: Range {
                start: Position { line, character: 1 },
                end: Position {
                    line: line + 10,
                    character: 1,
                },
            },
            detail: None,
            external: None,
        }
    }

    fn create_call_info(name: &str, path: &str, call_lines: &[u32]) -> CallInfo {
        let line = random_line();
        CallInfo {
            item: create_call_hierarchy_item(name, path, line),
            call_ranges: call_lines
                .iter()
                .map(|&l| Range {
                    start: Position {
                        line: l,
                        character: random_character(),
                    },
                    end: Position {
                        line: l,
                        character: random_character() + 10,
                    },
                })
                .collect(),
            call_snippets: None,
        }
    }

    #[test]
    fn incoming_calls_response_produces_header_with_caller_count() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Incoming,
            raw_response: None,
            calls: vec![
                create_call_info("caller_one", "src/a.rs", &[random_line()]),
                create_call_info("caller_two", "src/b.rs", &[random_line()]),
            ],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("## Incoming Calls to (2 callers)"),
            "header must indicate incoming calls with count: got {}",
            markdown
        );
    }

    #[test]
    fn outgoing_calls_response_produces_header_with_callee_count() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Outgoing,
            raw_response: None,
            calls: vec![
                create_call_info("callee_one", "src/lib.rs", &[random_line()]),
                create_call_info("callee_two", "src/utils.rs", &[random_line()]),
                create_call_info("callee_three", "src/helpers.rs", &[random_line()]),
            ],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("## Outgoing Calls from (3 callees)"),
            "header must indicate outgoing calls with count: got {}",
            markdown
        );
    }

    #[test]
    fn empty_incoming_calls_response_shows_no_callers_message() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Incoming,
            raw_response: None,
            calls: vec![],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("No callers found"),
            "empty incoming calls must show no callers message: got {}",
            markdown
        );
    }

    #[test]
    fn empty_outgoing_calls_response_shows_no_callees_message() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Outgoing,
            raw_response: None,
            calls: vec![],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("No callees found"),
            "empty outgoing calls must show no callees message: got {}",
            markdown
        );
    }

    #[test]
    fn call_info_includes_function_name_and_file_position() {
        let line = random_line();
        let character = random_character();
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Incoming,
            raw_response: None,
            calls: vec![CallInfo {
                item: CallHierarchyItemInfo {
                    name: "handleRequest".to_string(),
                    kind: "function".to_string(),
                    location: FilePosition {
                        path: "src/handlers/request.ts".to_string(),
                        position: Position { line, character },
                    },
                    range: Range {
                        start: Position { line, character: 1 },
                        end: Position {
                            line: line + 20,
                            character: 1,
                        },
                    },
                    detail: None,
                    external: None,
                },
                call_ranges: vec![],
                call_snippets: None,
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("### handleRequest"),
            "call info must include function name: got {}",
            markdown
        );
        assert!(
            markdown.contains("src/handlers/request.ts"),
            "call info must include file path: got {}",
            markdown
        );
    }

    #[test]
    fn call_info_with_detail_includes_signature() {
        let line = random_line();
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Outgoing,
            raw_response: None,
            calls: vec![CallInfo {
                item: CallHierarchyItemInfo {
                    name: "processData".to_string(),
                    kind: "method".to_string(),
                    location: FilePosition {
                        path: "src/processor.rs".to_string(),
                        position: Position {
                            line,
                            character: random_character(),
                        },
                    },
                    range: Range {
                        start: Position { line, character: 1 },
                        end: Position {
                            line: line + 15,
                            character: 1,
                        },
                    },
                    detail: Some("fn processData(&self, data: &[u8]) -> Result<()>".to_string()),
                    external: None,
                },
                call_ranges: vec![],
                call_snippets: None,
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("fn processData(&self, data: &[u8]) -> Result<()>"),
            "call info with detail must include signature: got {}",
            markdown
        );
    }

    #[test]
    fn call_info_external_tag_is_included() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Incoming,
            raw_response: None,
            calls: vec![CallInfo {
                item: CallHierarchyItemInfo {
                    name: "external_call".to_string(),
                    kind: "function".to_string(),
                    location: FilePosition {
                        path: "/usr/lib/external.rs".to_string(),
                        position: Position {
                            line: 1,
                            character: 1,
                        },
                    },
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 2, character: 1 },
                    },
                    detail: None,
                    external: Some(true),
                },
                call_ranges: vec![],
                call_snippets: None,
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[external]"),
            "external calls must be tagged: got {}",
            markdown
        );
    }

    #[test]
    fn call_ranges_are_listed_as_line_numbers() {
        let call_line_1 = random_line();
        let call_line_2 = call_line_1 + 10;
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Incoming,
            raw_response: None,
            calls: vec![CallInfo {
                item: create_call_hierarchy_item("multiCaller", "src/multi.rs", random_line()),
                call_ranges: vec![
                    Range {
                        start: Position {
                            line: call_line_1,
                            character: 5,
                        },
                        end: Position {
                            line: call_line_1,
                            character: 20,
                        },
                    },
                    Range {
                        start: Position {
                            line: call_line_2,
                            character: 10,
                        },
                        end: Position {
                            line: call_line_2,
                            character: 25,
                        },
                    },
                ],
                call_snippets: None,
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("Line {}:5", call_line_1)),
            "must include first call line:col number: got {}",
            markdown
        );
        assert!(
            markdown.contains(&format!("Line {}:10", call_line_2)),
            "must include second call line:col number: got {}",
            markdown
        );
    }

    #[test]
    fn implementation_response_produces_header_with_count() {
        let response = ImplementationResponse {
            raw_response: None,
            implementations: vec![
                FilePosition {
                    path: "src/impl1.rs".to_string(),
                    position: Position {
                        line: random_line(),
                        character: random_character(),
                    },
                },
                FilePosition {
                    path: "src/impl2.rs".to_string(),
                    position: Position {
                        line: random_line(),
                        character: random_character(),
                    },
                },
            ],
            selected_identifier: Identifier {
                name: "Serializable".to_string(),
                file_range: FileRange {
                    path: "src/traits.rs".to_string(),
                    range: Range {
                        start: Position { line: 10, character: 1 },
                        end: Position {
                            line: 10,
                            character: 12,
                        },
                    },
                },
                kind: Some("trait".to_string()),
            },
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("## Implementations of `Serializable` (2 found)"),
            "header must include identifier name and count: got {}",
            markdown
        );
    }

    #[test]
    fn empty_implementation_response_shows_no_implementations_message() {
        let response = ImplementationResponse {
            raw_response: None,
            implementations: vec![],
            selected_identifier: Identifier {
                name: "NonExistent".to_string(),
                file_range: FileRange {
                    path: "src/missing.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 11 },
                    },
                },
                kind: None,
            },
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("No implementations found"),
            "empty implementations must show appropriate message: got {}",
            markdown
        );
    }

    #[test]
    fn implementation_response_lists_each_implementation_with_path_and_line() {
        let line1 = random_line();
        let line2 = random_line();
        let response = ImplementationResponse {
            raw_response: None,
            implementations: vec![
                FilePosition {
                    path: "src/json_serializer.rs".to_string(),
                    position: Position {
                        line: line1,
                        character: random_character(),
                    },
                },
                FilePosition {
                    path: "src/xml_serializer.rs".to_string(),
                    position: Position {
                        line: line2,
                        character: random_character(),
                    },
                },
            ],
            selected_identifier: Identifier {
                name: "Serializer".to_string(),
                file_range: FileRange {
                    path: "src/lib.rs".to_string(),
                    range: Range {
                        start: Position { line: 5, character: 1 },
                        end: Position { line: 5, character: 11 },
                    },
                },
                kind: Some("trait".to_string()),
            },
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("src/json_serializer.rs:{}", line1)),
            "must include first implementation path and line: got {}",
            markdown
        );
        assert!(
            markdown.contains(&format!("src/xml_serializer.rs:{}", line2)),
            "must include second implementation path and line: got {}",
            markdown
        );
    }

    #[test]
    fn implementation_response_handles_unicode_identifier_names() {
        let response = ImplementationResponse {
            raw_response: None,
            implementations: vec![FilePosition {
                path: "src/unicode.rs".to_string(),
                position: Position {
                    line: random_line(),
                    character: random_character(),
                },
            }],
            selected_identifier: Identifier {
                name: "Caf\u{00e9}Handler".to_string(),
                file_range: FileRange {
                    path: "src/handlers.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 13 },
                    },
                },
                kind: Some("trait".to_string()),
            },
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Caf\u{00e9}Handler"),
            "must preserve unicode characters in identifier name: got {}",
            markdown
        );
    }

    #[test]
    fn call_hierarchy_response_handles_unicode_function_names() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Incoming,
            raw_response: None,
            calls: vec![CallInfo {
                item: CallHierarchyItemInfo {
                    name: "calcul\u{00e9}Total".to_string(),
                    kind: "function".to_string(),
                    location: FilePosition {
                        path: "src/calculs.rs".to_string(),
                        position: Position {
                            line: random_line(),
                            character: random_character(),
                        },
                    },
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 10, character: 1 },
                    },
                    detail: None,
                    external: None,
                },
                call_ranges: vec![],
                call_snippets: None,
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("calcul\u{00e9}Total"),
            "must preserve unicode characters in function name: got {}",
            markdown
        );
    }

    #[test]
    fn multiple_calls_are_separated_by_blank_lines() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Outgoing,
            raw_response: None,
            calls: vec![
                create_call_info("firstCallee", "src/first.rs", &[]),
                create_call_info("secondCallee", "src/second.rs", &[]),
            ],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("### firstCallee"),
            "must include first callee: got {}",
            markdown
        );
        assert!(
            markdown.contains("### secondCallee"),
            "must include second callee: got {}",
            markdown
        );
        let first_pos = markdown.find("### firstCallee").unwrap();
        let second_pos = markdown.find("### secondCallee").unwrap();
        let between = &markdown[first_pos..second_pos];
        assert!(
            between.contains("\n\n"),
            "calls must be separated by blank lines: got between section: {}",
            between
        );
    }
}
