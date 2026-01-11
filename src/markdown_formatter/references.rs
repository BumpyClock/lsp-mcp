// ABOUTME: Markdown formatter for references response types.
// ABOUTME: Converts ReferencesResponse and ReferencedSymbolsResponse to readable markdown.

use super::{escape_inline_code, ToMarkdown};
use crate::api_types::ReferencedSymbolsResponse;
use crate::service::types::response::{McpReferenceLocation, McpReferencesResponse, ReferenceType};

pub fn format_references_summary(
    response: &McpReferencesResponse,
    file_limit: Option<u32>,
) -> String {
    let mut output = String::new();
    let symbol_name = escape_inline_code(&response.selected_identifier.name);

    output.push_str(&format!(
        "References to `{}` ({} total)\n",
        symbol_name, response.total_count
    ));

    if let Some(selection) = &response.selection {
        let chosen_name = escape_inline_code(&selection.chosen.name);
        let chosen_kind = selection.chosen.kind.as_deref().unwrap_or("unknown");
        let chosen_pos = &selection.chosen.position;
        if let Some(module) = &selection.chosen.module {
            output.push_str(&format!(
                "Selected candidate: `{}` ({}) — {}:{}:{} (module: {})\n",
                chosen_name,
                chosen_kind,
                selection.chosen.path,
                chosen_pos.line,
                chosen_pos.character,
                escape_inline_code(module)
            ));
        } else {
            output.push_str(&format!(
                "Selected candidate: `{}` ({}) — {}:{}:{}\n",
                chosen_name,
                chosen_kind,
                selection.chosen.path,
                chosen_pos.line,
                chosen_pos.character
            ));
        }
    }

    let mut definitions: Vec<String> = Vec::new();
    let mut reexports: Vec<String> = Vec::new();
    for file_group in &response.by_file {
        for reference in &file_group.refs {
            let entry = format!(
                "{}:{}:{}",
                file_group.path, reference.position.line, reference.position.character
            );
            match reference.reference_type {
                ReferenceType::Definition => definitions.push(entry),
                ReferenceType::ReExport => reexports.push(entry),
                _ => {}
            }
        }
    }

    if !definitions.is_empty() {
        if definitions.len() == 1 {
            output.push_str(&format!("Definition: {}\n", definitions[0]));
        } else {
            output.push_str(&format!(
                "Definitions ({}): {}\n",
                definitions.len(),
                definitions.join(", ")
            ));
        }
    }

    if !reexports.is_empty() {
        if reexports.len() == 1 {
            output.push_str(&format!("Re-export: {}\n", reexports[0]));
        } else {
            output.push_str(&format!(
                "Re-exports ({}): {}\n",
                reexports.len(),
                reexports.join(", ")
            ));
        }
    }

    let by_type = &response.by_type;
    if response.total_count > 0 {
        output.push_str(&format!(
            "By type: def {}, import {}, re-export {}, call {}\n",
            by_type.definition, by_type.import, by_type.reexport, by_type.call
        ));
    }

    let mut file_counts: Vec<(String, u32)> = response
        .by_file
        .iter()
        .map(|group| (group.path.clone(), group.count))
        .collect();
    file_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let total_files = file_counts.len();
    let max_files = file_limit.unwrap_or(10) as usize;
    if max_files > 0 && !file_counts.is_empty() {
        let shown: Vec<String> = file_counts
            .iter()
            .take(max_files)
            .map(|(path, count)| format!("{} ({})", path, count))
            .collect();
        output.push_str(&format!("Used in: {}\n", shown.join(", ")));

        if total_files > max_files {
            output.push_str(&format!(
                "[Showing {} of {} files]\n",
                max_files, total_files
            ));
        }
    }

    output
}

impl ToMarkdown for McpReferencesResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "References to `{}` ({} total)\n",
            self.selected_identifier.name, self.total_count
        ));

        if let Some(selection) = &self.selection {
            let chosen_name = escape_inline_code(&selection.chosen.name);
            let chosen_kind = selection.chosen.kind.as_deref().unwrap_or("unknown");
            let chosen_path = &selection.chosen.path;
            let chosen_pos = &selection.chosen.position;

            if let Some(module) = &selection.chosen.module {
                output.push_str(&format!(
                    "Selected candidate: `{}` ({}) — {}:{}:{} (module: {})\n",
                    chosen_name,
                    chosen_kind,
                    chosen_path,
                    chosen_pos.line,
                    chosen_pos.character,
                    escape_inline_code(module)
                ));
            } else {
                output.push_str(&format!(
                    "Selected candidate: `{}` ({}) — {}:{}:{}\n",
                    chosen_name, chosen_kind, chosen_path, chosen_pos.line, chosen_pos.character
                ));
            }

            if !selection.others.is_empty() {
                output.push_str(&format!("Other candidates ({})\n", selection.others.len()));
                for (idx, other) in selection.others.iter().enumerate() {
                    let other_name = escape_inline_code(&other.name);
                    let other_kind = other.kind.as_deref().unwrap_or("unknown");
                    if let Some(module) = &other.module {
                        output.push_str(&format!(
                            "  {}) `{}` ({}) — {} (module: {})\n",
                            idx + 1,
                            other_name,
                            other_kind,
                            other.path,
                            escape_inline_code(module)
                        ));
                    } else {
                        output.push_str(&format!(
                            "  {}) `{}` ({}) — {}\n",
                            idx + 1,
                            other_name,
                            other_kind,
                            other.path
                        ));
                    }
                }
            }
        }

        for file_group in &self.by_file {
            let ref_word = if file_group.count == 1 { "ref" } else { "refs" };
            output.push_str(&format!(
                "\n{} ({} {})\n",
                file_group.path, file_group.count, ref_word
            ));

            let mut grouped_refs: Vec<Vec<&McpReferenceLocation>> =
                vec![vec![], vec![], vec![], vec![]];

            for reference in &file_group.refs {
                let index = match reference.reference_type {
                    ReferenceType::Definition => 0,
                    ReferenceType::Import => 1,
                    ReferenceType::ReExport => 2,
                    ReferenceType::Call => 3,
                };
                grouped_refs[index].push(reference);
            }

            for group in grouped_refs {
                for reference in group {
                    let line = reference.position.line;
                    let tag = reference_type_tag(reference.reference_type);
                    match &reference.snippet {
                        Some(ctx) => {
                            let context_start_line = ctx.range.range.start.line;
                            let offset = line.saturating_sub(context_start_line) as usize;

                            let target_line = ctx
                                .source_code
                                .lines()
                                .nth(offset)
                                .or_else(|| ctx.source_code.lines().next())
                                .unwrap_or("");

                            let escaped = escape_inline_code(target_line.trim());
                            output.push_str(&format!(
                                "  [{}] Line {}:{}: `{}`\n",
                                tag, line, reference.position.character, escaped
                            ));
                        }
                        None => {
                            output.push_str(&format!(
                                "  [{}] Line {}:{}\n",
                                tag, line, reference.position.character
                            ));
                        }
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

fn reference_type_tag(reference_type: ReferenceType) -> &'static str {
    match reference_type {
        ReferenceType::Definition => "def",
        ReferenceType::Import => "import",
        ReferenceType::Call => "call",
        ReferenceType::ReExport => "re-export",
    }
}

impl ToMarkdown for ReferencedSymbolsResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        let total_workspace = self.workspace_symbols.len();
        let total_external = self.external_symbols.len();
        let total_not_found = self.not_found.len();
        let grand_total = total_workspace + total_external + total_not_found;

        output.push_str(&format!("Referenced Symbols ({} total)\n", grand_total));

        if !self.workspace_symbols.is_empty() {
            output.push_str(&format!("\nWorkspace Symbols ({})\n", total_workspace));
            for ws in &self.workspace_symbols {
                let ref_name = escape_inline_code(&ws.reference.name);
                let kind = ws.reference.kind_or_default();
                output.push_str(&format!("\n`{}` ({})\n", ref_name, kind));
                output.push_str(&format!(
                    "  Reference: {}:{}:{}\n",
                    ws.reference.file_range.path,
                    ws.reference.file_range.range.start.line,
                    ws.reference.file_range.range.start.character
                ));
                if ws.definitions.is_empty() {
                    output.push_str("  Definitions: none\n");
                } else {
                    output.push_str(&format!("  Definitions ({}):\n", ws.definitions.len()));
                    for def in &ws.definitions {
                        let def_name = escape_inline_code(&def.name);
                        output.push_str(&format!(
                            "    `{}` ({}) at {}:{}:{}\n",
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

        if !self.external_symbols.is_empty() {
            output.push_str(&format!("\nExternal Symbols ({})\n", total_external));
            for ext in &self.external_symbols {
                let name = escape_inline_code(&ext.name);
                let kind = ext.kind_or_default();
                output.push_str(&format!(
                    "  `{}` ({}) at {}:{}:{}\n",
                    name,
                    kind,
                    ext.file_range.path,
                    ext.file_range.range.start.line,
                    ext.file_range.range.start.character
                ));
            }
        }

        if !self.not_found.is_empty() {
            output.push_str(&format!(
                "\nNot Found\n{} unresolved symbols (likely stdlib/builtins)\n",
                total_not_found
            ));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{CodeContext, FileRange, Identifier, Position, Range};
    use crate::service::types::response::{
        FileGroup, McpReferenceLocation, ReferenceCandidate, ReferenceType, ReferencesSelection,
        TypeCounts,
    };
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
        format!(
            "symbol_{}_unicode_\u{03B1}\u{03B2}",
            rand::rng().random_range(100..999)
        )
    }

    fn create_test_identifier(name: &str) -> Identifier {
        Identifier {
            name: name.to_string(),
            file_range: FileRange {
                path: "src/test.rs".to_string(),
                range: Range {
                    start: Position {
                        line: 1,
                        character: 1,
                    },
                    end: Position {
                        line: 1,
                        character: 10,
                    },
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
                end: Position {
                    line,
                    character: 15,
                },
            },
            snippet: Some(CodeContext {
                range: FileRange {
                    path: "src/test.rs".to_string(),
                    range: Range {
                        start: Position { line, character: 1 },
                        end: Position {
                            line,
                            character: 50,
                        },
                    },
                },
                source_code: source_code.to_string(),
            }),
            reference_type: ReferenceType::Call,
        }
    }

    fn create_reference_without_snippet(line: u32) -> McpReferenceLocation {
        McpReferenceLocation {
            path: None,
            position: Position { line, character: 5 },
            symbol_range: Range {
                start: Position { line, character: 5 },
                end: Position {
                    line,
                    character: 15,
                },
            },
            snippet: None,
            reference_type: ReferenceType::Call,
        }
    }

    fn create_reference_with_type(
        line: u32,
        reference_type: ReferenceType,
    ) -> McpReferenceLocation {
        McpReferenceLocation {
            path: None,
            position: Position { line, character: 5 },
            symbol_range: Range {
                start: Position { line, character: 5 },
                end: Position {
                    line,
                    character: 15,
                },
            },
            snippet: None,
            reference_type,
        }
    }

    #[test]
    fn it_includes_symbol_name_in_header() {
        let symbol_name = random_irregular_string();
        let total = random_count();

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier(&symbol_name),
            selection: None,
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: total,
            by_file: vec![],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("References to `{}`", symbol_name)),
            "negative: header must contain symbol name in backticks"
        );
        assert!(
            markdown.contains(&format!("({} total)", total)),
            "negative: header must contain total count"
        );
    }

    #[test]
    fn it_renders_candidate_selection_when_present() {
        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("Store"),
            selection: Some(ReferencesSelection {
                chosen: ReferenceCandidate {
                    name: "Store".to_string(),
                    kind: Some("class".to_string()),
                    module: Some("App".to_string()),
                    path: "src/app/store.ts".to_string(),
                    position: Position {
                        line: 12,
                        character: 3,
                    },
                },
                others: vec![
                    ReferenceCandidate {
                        name: "Store".to_string(),
                        kind: Some("type".to_string()),
                        module: None,
                        path: "src/types/store.ts".to_string(),
                        position: Position {
                            line: 5,
                            character: 1,
                        },
                    },
                    ReferenceCandidate {
                        name: "Store".to_string(),
                        kind: Some("class".to_string()),
                        module: Some("Legacy".to_string()),
                        path: "src/legacy/store.ts".to_string(),
                        position: Position {
                            line: 9,
                            character: 1,
                        },
                    },
                ],
            }),
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 0,
            by_file: vec![],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Selected candidate:"),
            "negative: selection header must be rendered"
        );
        assert!(
            markdown.contains("Other candidates"),
            "negative: alternates header must be rendered"
        );
        assert!(
            markdown.contains("src/app/store.ts"),
            "negative: chosen filepath must be rendered"
        );
    }

    #[test]
    fn it_includes_reference_type_tags() {
        let reference = McpReferenceLocation {
            path: None,
            position: Position {
                line: 10,
                character: 3,
            },
            symbol_range: Range {
                start: Position {
                    line: 10,
                    character: 3,
                },
                end: Position {
                    line: 10,
                    character: 12,
                },
            },
            snippet: None,
            reference_type: ReferenceType::Import,
        };

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("scoreMember"),
            selection: None,
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 1,
            by_file: vec![FileGroup {
                path: "src/example.ts".to_string(),
                count: 1,
                refs: vec![reference],
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[import]"),
            "negative: reference type tags must be rendered"
        );
    }

    #[test]
    fn it_groups_references_by_file_path() {
        let file_path = "src/components/button.tsx";
        let count = random_count();

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("handleClick"),
            selection: None,
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
            markdown.contains(&format!("{} ({} refs)", file_path, count)),
            "negative: must include file path with ref count"
        );
    }

    #[test]
    fn it_displays_line_number_with_snippet_as_inline_code() {
        let line = random_line();
        let snippet_code = "const result = processData(input);";

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("processData"),
            selection: None,
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
            markdown.contains(&format!("  [call] Line {}:5:", line)),
            "negative: must show line:column with indent"
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
            selection: None,
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
            markdown.contains(&format!("  [call] Line {}:5", line)),
            "negative: must show line:column with indent even without snippet"
        );
    }

    #[test]
    fn it_shows_truncation_indicator_when_truncated() {
        let showing = 10u32;
        let total = 25u32;

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("store"),
            selection: None,
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
            selection: None,
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
            selection: None,
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
            markdown.contains(&format!("{} ({} refs)", file1, count1)),
            "negative: must include first file header"
        );
        assert!(
            markdown.contains(&format!("{} ({} refs)", file2, count2)),
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
            selection: None,
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
            selection: None,
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 0,
            by_file: vec![],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("References to `unusedSymbol`"),
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
            selection: None,
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
            selection: None,
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
    fn it_groups_references_by_type_within_file() {
        let file_path = "src/services/auth.ts";

        let def_ref = McpReferenceLocation {
            path: None,
            position: Position {
                line: 5,
                character: 10,
            },
            symbol_range: Range {
                start: Position {
                    line: 5,
                    character: 10,
                },
                end: Position {
                    line: 5,
                    character: 20,
                },
            },
            snippet: Some(CodeContext {
                range: FileRange {
                    path: file_path.to_string(),
                    range: Range {
                        start: Position {
                            line: 5,
                            character: 0,
                        },
                        end: Position {
                            line: 5,
                            character: 50,
                        },
                    },
                },
                source_code: "export const authenticate = () => {}".to_string(),
            }),
            reference_type: ReferenceType::Definition,
        };

        let import_ref = McpReferenceLocation {
            path: None,
            position: Position {
                line: 10,
                character: 3,
            },
            symbol_range: Range {
                start: Position {
                    line: 10,
                    character: 3,
                },
                end: Position {
                    line: 10,
                    character: 13,
                },
            },
            snippet: Some(CodeContext {
                range: FileRange {
                    path: file_path.to_string(),
                    range: Range {
                        start: Position {
                            line: 10,
                            character: 0,
                        },
                        end: Position {
                            line: 10,
                            character: 50,
                        },
                    },
                },
                source_code: "import { authenticate } from './auth'".to_string(),
            }),
            reference_type: ReferenceType::Import,
        };

        let call_ref = McpReferenceLocation {
            path: None,
            position: Position {
                line: 25,
                character: 7,
            },
            symbol_range: Range {
                start: Position {
                    line: 25,
                    character: 7,
                },
                end: Position {
                    line: 25,
                    character: 17,
                },
            },
            snippet: Some(CodeContext {
                range: FileRange {
                    path: file_path.to_string(),
                    range: Range {
                        start: Position {
                            line: 25,
                            character: 0,
                        },
                        end: Position {
                            line: 25,
                            character: 50,
                        },
                    },
                },
                source_code: "const user = authenticate(credentials)".to_string(),
            }),
            reference_type: ReferenceType::Call,
        };

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("authenticate"),
            selection: None,
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 3,
            by_file: vec![FileGroup {
                path: file_path.to_string(),
                count: 3,
                refs: vec![call_ref, def_ref, import_ref],
            }],
            by_type: Default::default(),
        };

        let markdown = response.to_markdown();

        let def_pos = markdown
            .find("[def]")
            .expect("negative: must include [def] tag");
        let import_pos = markdown
            .find("[import]")
            .expect("negative: must include [import] tag");
        let call_pos = markdown
            .find("[call]")
            .expect("negative: must include [call] tag");

        assert!(
            def_pos < import_pos,
            "negative: definitions must appear before imports but def at {} and import at {}",
            def_pos,
            import_pos
        );
        assert!(
            import_pos < call_pos,
            "negative: imports must appear before calls but import at {} and call at {}",
            import_pos,
            call_pos
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
            reference_type: ReferenceType::Call,
        };

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("testSymbol"),
            selection: None,
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

    #[test]
    fn it_formats_summary_with_counts_and_files() {
        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: create_test_identifier("LspClient"),
            selection: None,
            limit: 50,
            offset: 0,
            truncated: false,
            total_count: 4,
            by_file: vec![
                FileGroup {
                    path: "src/alpha.rs".to_string(),
                    count: 3,
                    refs: vec![
                        create_reference_with_type(10, ReferenceType::Call),
                        create_reference_with_type(12, ReferenceType::Import),
                        create_reference_with_type(14, ReferenceType::Definition),
                    ],
                },
                FileGroup {
                    path: "src/beta.rs".to_string(),
                    count: 1,
                    refs: vec![create_reference_with_type(4, ReferenceType::ReExport)],
                },
            ],
            by_type: TypeCounts {
                definition: 1,
                import: 1,
                reexport: 1,
                call: 1,
            },
        };

        let summary = format_references_summary(&response, Some(5));

        assert!(
            summary.contains("References to `LspClient`"),
            "negative: summary must include header"
        );
        assert!(
            summary.contains("Definition: src/alpha.rs:14:5"),
            "negative: summary must include definition location"
        );
        assert!(
            summary.contains("Re-export: src/beta.rs:4:5"),
            "negative: summary must include re-export location"
        );
        assert!(
            summary.contains("Used in:"),
            "negative: summary must include file list"
        );
        assert!(
            summary.contains("By type:"),
            "negative: summary must include type counts"
        );
    }
}
