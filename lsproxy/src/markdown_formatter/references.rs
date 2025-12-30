// ABOUTME: Markdown formatter for references response types.
// ABOUTME: Converts ReferencesResponse and ReferencedSymbolsResponse to readable markdown.

use super::{escape_inline_code, ToMarkdown};
use crate::api_types::ReferencedSymbolsResponse;
use crate::service::types::response::McpReferencesResponse;

impl ToMarkdown for McpReferencesResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "## References to `{}` ({} total)\n",
            self.selected_identifier.name, self.total_count
        ));

        for file_group in &self.by_file {
            let ref_word = if file_group.count == 1 { "ref" } else { "refs" };
            output.push_str(&format!(
                "\n### {} ({} {})\n",
                file_group.path, file_group.count, ref_word
            ));

            for reference in &file_group.refs {
                let line = reference.position.line;
                match &reference.snippet {
                    Some(ctx) => {
                        // Calculate which line within the context corresponds to the reference
                        let context_start_line = ctx.range.range.start.line;
                        let offset = line.saturating_sub(context_start_line) as usize;

                        let target_line = ctx
                            .source_code
                            .lines()
                            .nth(offset)
                            .or_else(|| ctx.source_code.lines().next())
                            .unwrap_or("");

                        let escaped = escape_inline_code(target_line.trim());
                        output.push_str(&format!("- **Line {}**: `{}`\n", line, escaped));
                    }
                    None => {
                        output.push_str(&format!("- **Line {}**\n", line));
                    }
                }
            }
        }

        if self.truncated {
            let shown: u32 = self.by_file.iter().map(|g| g.refs.len() as u32).sum();
            output.push_str(&format!(
                "\n[Showing {} of {} - truncated]\n",
                shown, self.total_count
            ));
        }

        output
    }
}

impl ToMarkdown for ReferencedSymbolsResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        let total_workspace = self.workspace_symbols.len();
        let total_external = self.external_symbols.len();
        let total_not_found = self.not_found.len();
        let grand_total = total_workspace + total_external + total_not_found;

        output.push_str(&format!("## Referenced Symbols ({} total)\n", grand_total));

        // Workspace symbols with definitions
        if !self.workspace_symbols.is_empty() {
            output.push_str(&format!("\n### Workspace Symbols ({})\n", total_workspace));
            for ws in &self.workspace_symbols {
                let ref_name = escape_inline_code(&ws.reference.name);
                let kind = ws.reference.kind_or_default();
                output.push_str(&format!("\n#### `{}` ({})\n", ref_name, kind));
                output.push_str(&format!(
                    "- **Reference**: {}:{}:{}\n",
                    ws.reference.file_range.path,
                    ws.reference.file_range.range.start.line,
                    ws.reference.file_range.range.start.character
                ));
                if ws.definitions.is_empty() {
                    output.push_str("- **Definitions**: none\n");
                } else {
                    output.push_str(&format!("- **Definitions** ({}):\n", ws.definitions.len()));
                    for def in &ws.definitions {
                        let def_name = escape_inline_code(&def.name);
                        output.push_str(&format!(
                            "  - `{}` ({}) at {}:{}:{}\n",
                            def_name,
                            def.kind,
                            def.identifier_position.path,
                            def.identifier_position.position.line,
                            def.identifier_position.position.character
                        ));
                    }
                }
            }
        }

        // External symbols
        if !self.external_symbols.is_empty() {
            output.push_str(&format!("\n### External Symbols ({})\n", total_external));
            for ext in &self.external_symbols {
                let name = escape_inline_code(&ext.name);
                let kind = ext.kind_or_default();
                output.push_str(&format!(
                    "- `{}` ({}) at {}:{}:{}\n",
                    name,
                    kind,
                    ext.file_range.path,
                    ext.file_range.range.start.line,
                    ext.file_range.range.start.character
                ));
            }
        }

        // Not found symbols
        if !self.not_found.is_empty() {
            output.push_str(&format!("\n### Not Found ({})\n", total_not_found));
            for nf in &self.not_found {
                let name = escape_inline_code(&nf.name);
                let kind = nf.kind_or_default();
                output.push_str(&format!(
                    "- `{}` ({}) at {}:{}:{}\n",
                    name,
                    kind,
                    nf.file_range.path,
                    nf.file_range.range.start.line,
                    nf.file_range.range.start.character
                ));
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{CodeContext, FileRange, Identifier, Position, Range};
    use crate::service::types::response::{FileGroup, McpReferenceLocation};
    use rand::Rng;

    fn random_line() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..500)
    }

    fn random_count() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..20)
    }

    fn random_irregular_string() -> String {
        format!("symbol_{}_unicode_\u{03B1}\u{03B2}", rand::rng().random_range(100..999))
    }

    fn create_test_identifier(name: &str) -> Identifier {
        Identifier {
            name: name.to_string(),
            file_range: FileRange {
                path: "src/test.rs".to_string(),
                range: Range {
                    start: Position { line: 1, character: 1 },
                    end: Position { line: 1, character: 10 },
                },
            },
            kind: Some("function".to_string()),
        }
    }

    fn create_reference_with_snippet(line: u32, source_code: &str) -> McpReferenceLocation {
        McpReferenceLocation {
            path: None,
            position: Position { line, character: 5 },
            symbol_range: Range {
                start: Position { line, character: 5 },
                end: Position { line, character: 15 },
            },
            snippet: Some(CodeContext {
                range: FileRange {
                    path: "src/test.rs".to_string(),
                    range: Range {
                        start: Position { line, character: 1 },
                        end: Position { line, character: 50 },
                    },
                },
                source_code: source_code.to_string(),
            }),
        }
    }

    fn create_reference_without_snippet(line: u32) -> McpReferenceLocation {
        McpReferenceLocation {
            path: None,
            position: Position { line, character: 5 },
            symbol_range: Range {
                start: Position { line, character: 5 },
                end: Position { line, character: 15 },
            },
            snippet: None,
        }
    }

    #[test]
    fn it_includes_symbol_name_in_header() {
        let symbol_name = random_irregular_string();
        let total = random_count();

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier(&symbol_name),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: total,
            by_file: vec![],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("## References to `{}`", symbol_name)),
            "negative: header must contain symbol name in backticks"
        );
        assert!(
            markdown.contains(&format!("({} total)", total)),
            "negative: header must contain total count"
        );
    }

    #[test]
    fn it_groups_references_by_file_path() {
        let file_path = "src/components/button.tsx";
        let count = random_count();

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("handleClick"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: count,
            by_file: vec![FileGroup {
                path: file_path.to_string(),
                count,
                refs: vec![create_reference_with_snippet(10, "onClick={handleClick}")],
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("### {} ({} refs)", file_path, count)),
            "negative: must include file path as h3 header with ref count"
        );
    }

    #[test]
    fn it_displays_line_number_with_snippet_as_inline_code() {
        let line = random_line();
        let snippet_code = "const result = processData(input);";

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("processData"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 1,
            by_file: vec![FileGroup {
                path: "src/utils.ts".to_string(),
                count: 1,
                refs: vec![create_reference_with_snippet(line, snippet_code)],
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("- **Line {}**:", line)),
            "negative: must show line number in bold"
        );
        assert!(
            markdown.contains(&format!("`{}`", snippet_code)),
            "negative: must show snippet as inline code"
        );
    }

    #[test]
    fn it_displays_only_line_number_when_no_snippet() {
        let line = random_line();

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("testFn"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 1,
            by_file: vec![FileGroup {
                path: "src/test.rs".to_string(),
                count: 1,
                refs: vec![create_reference_without_snippet(line)],
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("- **Line {}**", line)),
            "negative: must show line number in bold even without snippet"
        );
    }

    #[test]
    fn it_shows_truncation_indicator_when_truncated() {
        let showing = 10u32;
        let total = 25u32;

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("store"),
            limit: showing,
            offset: 0,
            truncated: true,
            total_count: total,
            by_file: vec![FileGroup {
                path: "src/app/store.ts".to_string(),
                count: showing,
                refs: (0..showing)
                    .map(|i| create_reference_without_snippet(i + 1))
                    .collect(),
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[Showing"),
            "negative: must have truncation indicator starting with [Showing"
        );
        assert!(
            markdown.contains("truncated]"),
            "negative: must have truncation indicator ending with truncated]"
        );
        assert!(
            markdown.contains(&format!("{}", total)),
            "negative: truncation indicator must include total count"
        );
    }

    #[test]
    fn it_does_not_show_truncation_when_not_truncated() {
        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("myFunc"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 5,
            by_file: vec![FileGroup {
                path: "src/lib.rs".to_string(),
                count: 5,
                refs: (0..5)
                    .map(|i| create_reference_without_snippet(i + 1))
                    .collect(),
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            !markdown.contains("truncated"),
            "negative: must not show truncation indicator when not truncated"
        );
    }

    #[test]
    fn it_handles_multiple_files_with_references() {
        let file1 = "src/app/store.ts";
        let file2 = "src/main.tsx";
        let count1 = 4u32;
        let count2 = 2u32;

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("store"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: count1 + count2,
            by_file: vec![
                FileGroup {
                    path: file1.to_string(),
                    count: count1,
                    refs: vec![
                        create_reference_with_snippet(22, "export const store = configureStore({"),
                        create_reference_with_snippet(55, "setupListeners(store.dispatch);"),
                    ],
                },
                FileGroup {
                    path: file2.to_string(),
                    count: count2,
                    refs: vec![create_reference_with_snippet(
                        12,
                        "import { store } from './app/store.ts';",
                    )],
                },
            ],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("### {} ({} refs)", file1, count1)),
            "negative: must include first file header"
        );
        assert!(
            markdown.contains(&format!("### {} ({} refs)", file2, count2)),
            "negative: must include second file header"
        );
        assert!(
            markdown.find(file1).unwrap() < markdown.find(file2).unwrap(),
            "negative: files must appear in order of by_file vector"
        );
    }

    #[test]
    fn it_escapes_backticks_in_snippets() {
        let snippet_with_backticks = "const msg = `Hello ${name}`";

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("msg"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 1,
            by_file: vec![FileGroup {
                path: "src/greet.ts".to_string(),
                count: 1,
                refs: vec![create_reference_with_snippet(5, snippet_with_backticks)],
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();
        let escaped_snippet = escape_inline_code(snippet_with_backticks);

        assert!(
            markdown.contains(&escaped_snippet),
            "negative: backticks in snippets must be escaped"
        );
    }

    #[test]
    fn it_handles_empty_references_list() {
        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("unusedSymbol"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 0,
            by_file: vec![],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("## References to `unusedSymbol`"),
            "negative: must still show header even with no references"
        );
        assert!(
            markdown.contains("(0 total)"),
            "negative: must show 0 total for empty references"
        );
    }

    #[test]
    fn it_uses_singular_ref_for_single_reference_file() {
        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("singleUse"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 1,
            by_file: vec![FileGroup {
                path: "src/once.rs".to_string(),
                count: 1,
                refs: vec![create_reference_without_snippet(42)],
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("(1 ref)"),
            "negative: must use singular 'ref' for count of 1"
        );
    }

    #[test]
    fn it_trims_multiline_snippets_to_first_line() {
        let multiline_snippet = "fn process(\n    input: String,\n) -> Result";

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("process"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 1,
            by_file: vec![FileGroup {
                path: "src/lib.rs".to_string(),
                count: 1,
                refs: vec![create_reference_with_snippet(10, multiline_snippet)],
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            !markdown.contains('\n') || markdown.lines().all(|l| !l.contains("input: String")),
            "negative: multiline snippets should be trimmed or shown as first line only"
        );
    }

    #[test]
    fn it_displays_correct_line_from_context_window() {
        // Bug scenario: reference is at line 27, but context starts at line 25 (context_lines=2)
        // The formatter should display line 27's content, not line 25's
        let ref_line = 27u32;
        let context_start_line = 25u32;

        // Context contains lines 25-29 (5 lines total)
        let context_source = "line 25 content - WRONG\n\
                              line 26 content - WRONG\n\
                              line 27 content - CORRECT\n\
                              line 28 content\n\
                              line 29 content";

        let reference = McpReferenceLocation {
            path: None,
            position: Position {
                line: ref_line,
                character: 5,
            },
            symbol_range: Range {
                start: Position {
                    line: ref_line,
                    character: 5,
                },
                end: Position {
                    line: ref_line,
                    character: 15,
                },
            },
            snippet: Some(CodeContext {
                range: FileRange {
                    path: "src/test.rs".to_string(),
                    range: Range {
                        start: Position {
                            line: context_start_line,
                            character: 1,
                        },
                        end: Position {
                            line: context_start_line + 4,
                            character: 50,
                        },
                    },
                },
                source_code: context_source.to_string(),
            }),
        };

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("testSymbol"),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 1,
            by_file: vec![FileGroup {
                path: "src/test.rs".to_string(),
                count: 1,
                refs: vec![reference],
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        // Should display the CORRECT line (line 27), not the first line of context (line 25)
        assert!(
            markdown.contains("line 27 content - CORRECT"),
            "negative: must display the actual reference line content, not first line of context. Got: {}",
            markdown
        );
        assert!(
            !markdown.contains("WRONG"),
            "negative: must not display content from context lines before the reference. Got: {}",
            markdown
        );
    }
}
