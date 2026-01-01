// ABOUTME: Markdown formatter for symbol response types.
// ABOUTME: Converts Symbol, WorkspaceSymbolResponse, and McpIdentifierResponse to readable markdown.

use crate::api_types::{Symbol, WorkspaceSymbolResponse};
use crate::service::types::response::{McpIdentifierResponse, McpSymbolsResponse};
use super::{escape_inline_code, ToMarkdown};

impl ToMarkdown for McpSymbolsResponse {
    fn to_markdown(&self) -> String {
        let mut output = format!("Symbols in {}\n\n", self.path);

        let symbol_count = count_symbols_recursive(&self.symbols);

        for symbol in &self.symbols {
            format_symbol_recursive(symbol, 0, &self.path, &mut output);
        }

        if self.truncated {
            output.push_str(&format!("\n[Showing {} of more, truncated]", self.symbols.len()));
        } else {
            output.push_str(&format!("\n[Total: {} symbols]", symbol_count));
        }

        output
    }
}

impl ToMarkdown for WorkspaceSymbolResponse {
    fn to_markdown(&self) -> String {
        let count = self.symbols.len();
        let result_word = if count == 1 { "result" } else { "results" };
        let mut output = format!("Symbols ({} {})\n\n", count, result_word);

        for symbol in &self.symbols {
            let is_external = symbol.location.path.contains("node_modules");
            let external_marker = if is_external { " [external]" } else { "" };

            let container_prefix = symbol.container_name.as_ref()
                .map(|c| format!("{}.", c))
                .unwrap_or_default();

            output.push_str(&format!(
                "  {}{} ({}) - {}:{}{}\n",
                container_prefix,
                symbol.name,
                symbol.kind,
                symbol.location.path,
                symbol.location.position.line,
                external_marker
            ));

            if let Some(ref sig) = symbol.signature {
                output.push_str(&format!("    `{}`\n", escape_inline_code(sig)));
            }

            if let Some(ref snippet) = symbol.snippet {
                format_snippet(&snippet.source_code, "  ", &mut output);
            }
        }

        if self.truncated {
            output.push_str(&format!("\n[Showing {} of more]", self.symbols.len()));
        }

        output
    }
}

impl ToMarkdown for McpIdentifierResponse {
    fn to_markdown(&self) -> String {
        let count = self.identifiers.len();
        let result_word = if count == 1 { "identifier" } else { "identifiers" };
        let mut output = format!("Identifiers ({} {})\n\n", count, result_word);

        for identifier in &self.identifiers {
            let kind = identifier.kind_or_default();
            let path = &identifier.file_range.path;
            let start_line = identifier.file_range.range.start.line;
            let end_line = identifier.file_range.range.end.line;

            if start_line == end_line {
                output.push_str(&format!(
                    "  {} ({}) - {}:{}\n",
                    identifier.name, kind, path, start_line
                ));
            } else {
                output.push_str(&format!(
                    "  {} ({}) - {}:{}-{}\n",
                    identifier.name, kind, path, start_line, end_line
                ));
            }
        }

        if self.truncated {
            output.push_str(&format!("\n[Showing {} of more]", self.identifiers.len()));
        }

        output
    }
}

fn count_symbols_recursive(symbols: &[Symbol]) -> usize {
    let mut count = symbols.len();
    for symbol in symbols {
        if let Some(ref children) = symbol.children {
            count += count_symbols_recursive(children);
        }
    }
    count
}

fn format_symbol_recursive(symbol: &Symbol, depth: usize, root_path: &str, output: &mut String) {
    let indent = "  ".repeat(depth);
    let start_line = symbol.file_range.range.start.line;
    let end_line = symbol.file_range.range.end.line;
    let path = if symbol.file_range.path.is_empty() {
        root_path
    } else {
        &symbol.file_range.path
    };

    if start_line == end_line {
        output.push_str(&format!(
            "{}  {} ({}) - {}:{}\n",
            indent, symbol.name, symbol.kind, path, start_line
        ));
    } else {
        output.push_str(&format!(
            "{}  {} ({}) - {}:{}-{}\n",
            indent, symbol.name, symbol.kind, path, start_line, end_line
        ));
    }

    if let Some(ref sig) = symbol.signature {
        output.push_str(&format!("{}  `{}`\n", indent, escape_inline_code(sig)));
    }

    if let Some(ref summary) = symbol.jsdoc_summary {
        let line = summary.lines().next().unwrap_or("").trim();
        if !line.is_empty() {
            output.push_str(&format!("{}  {}\n", indent, line));
        }
    }

    if let Some(ref snippet) = symbol.snippet {
        format_snippet(&snippet.source_code, &indent, output);
    }

    if let Some(ref children) = symbol.children {
        for child in children {
            format_symbol_recursive(child, depth + 1, root_path, output);
        }
    }
}

fn format_snippet(source_code: &str, indent: &str, output: &mut String) {
    if source_code.contains('\n') {
        output.push_str(&format!("{}```\n", indent));
        for line in source_code.lines() {
            output.push_str(&format!("{}{}\n", indent, line));
        }
        output.push_str(&format!("{}```\n", indent));
    } else if !source_code.is_empty() {
        output.push_str(&format!("{}  `{}`\n", indent, escape_inline_code(source_code)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{FilePosition, FileRange, Identifier, Position, Range, WorkspaceSymbolInfo};
    use rand::Rng;

    fn random_line() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..500)
    }

    fn create_symbol(name: &str, kind: &str, line: u32, signature: Option<String>) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: kind.to_string(),
            identifier_position: FilePosition {
                path: "src/test.ts".to_string(),
                position: Position { line, character: 0 },
            },
            file_range: FileRange {
                path: "src/test.ts".to_string(),
                range: Range {
                    start: Position { line, character: 0 },
                    end: Position { line: line + 5, character: 0 },
                },
            },
            signature,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
            children: None,
            snippet: None,
        }
    }

    fn create_workspace_symbol(
        name: &str,
        kind: &str,
        path: &str,
        line: u32,
        signature: Option<String>,
    ) -> WorkspaceSymbolInfo {
        WorkspaceSymbolInfo {
            name: name.to_string(),
            kind: kind.to_string(),
            location: FilePosition {
                path: path.to_string(),
                position: Position { line, character: 0 },
            },
            container_name: None,
            match_kind: None,
            match_score: None,
            signature,
            snippet: None,
        }
    }

    fn create_identifier(name: &str, kind: Option<&str>, path: &str, start_line: u32, end_line: u32) -> Identifier {
        Identifier {
            name: name.to_string(),
            kind: kind.map(|k| k.to_string()),
            file_range: FileRange {
                path: path.to_string(),
                range: Range {
                    start: Position { line: start_line, character: 0 },
                    end: Position { line: end_line, character: 0 },
                },
            },
        }
    }

    #[test]
    fn mcp_symbols_response_contains_file_path_in_header() {
        let line = random_line();
        let response = McpSymbolsResponse {
            path: "src/utils/helpers.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![create_symbol("testFunc", "function", line, None)],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Symbols in src/utils/helpers.ts"),
            "negative: markdown header must contain file path"
        );
    }

    #[test]
    fn mcp_symbols_response_formats_symbol_with_name_kind_and_line() {
        let line = random_line();
        let response = McpSymbolsResponse {
            path: "src/app.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![create_symbol("myFunction", "function", line, None)],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("myFunction"),
            "negative: symbol name must be present"
        );
        assert!(
            markdown.contains("(function)"),
            "negative: symbol kind must be in parentheses"
        );
        assert!(
            markdown.contains(&format!("src/test.ts:{}-{}", line, line + 5)),
            "negative: symbol range must be shown"
        );
    }

    #[test]
    fn mcp_symbols_response_includes_signature_when_present() {
        let line = random_line();
        let signature = "export async function myFunction(arg: string): Promise<Result>".to_string();
        let response = McpSymbolsResponse {
            path: "src/app.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![create_symbol("myFunction", "function", line, Some(signature.clone()))],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("`{}`", signature)),
            "negative: signature must be formatted as inline code"
        );
    }

    #[test]
    fn mcp_symbols_response_shows_total_count() {
        let response = McpSymbolsResponse {
            path: "src/app.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![
                create_symbol("func1", "function", 10, None),
                create_symbol("func2", "function", 20, None),
                create_symbol("Const1", "constant", 30, None),
            ],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[Total: 3 symbols]"),
            "negative: total count must be shown at bottom"
        );
    }

    #[test]
    fn mcp_symbols_response_handles_nested_children_with_indentation() {
        let parent = Symbol {
            name: "MyClass".to_string(),
            kind: "class".to_string(),
            identifier_position: FilePosition {
                path: "src/app.ts".to_string(),
                position: Position { line: 10, character: 0 },
            },
            file_range: FileRange {
                path: "src/app.ts".to_string(),
                range: Range {
                    start: Position { line: 10, character: 0 },
                    end: Position { line: 30, character: 0 },
                },
            },
            signature: None,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
            children: Some(vec![
                create_symbol("methodA", "method", 15, None),
                create_symbol("methodB", "method", 20, None),
            ]),
            snippet: None,
        };

        let response = McpSymbolsResponse {
            path: "src/app.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![parent],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("MyClass"),
            "negative: parent symbol must be present"
        );
        assert!(
            markdown.contains("    methodA"),
            "negative: child symbols must be indented"
        );
        assert!(
            markdown.contains("    methodB"),
            "negative: second child symbol must be indented"
        );
    }

    #[test]
    fn mcp_symbols_response_shows_truncation_indicator() {
        let response = McpSymbolsResponse {
            path: "src/app.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![create_symbol("func1", "function", 10, None)],
            limit: 10,
            offset: 0,
            truncated: true,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[truncated]") || markdown.contains("[Showing"),
            "negative: truncation indicator must be present when truncated"
        );
    }

    #[test]
    fn workspace_symbol_response_contains_query_placeholder_in_header() {
        let response = WorkspaceSymbolResponse {
            raw_response: None,
            symbols: vec![create_workspace_symbol("store", "variable", "src/app/store.ts", 22, None)],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Symbols"),
            "negative: header must contain Symbols"
        );
        assert!(
            markdown.contains("1 result"),
            "negative: result count must be in header"
        );
    }

    #[test]
    fn workspace_symbol_response_formats_symbol_with_path_and_line() {
        let line = random_line();
        let response = WorkspaceSymbolResponse {
            raw_response: None,
            symbols: vec![create_workspace_symbol("myStore", "variable", "src/stores/main.ts", line, None)],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("myStore"),
            "negative: symbol name must be present"
        );
        assert!(
            markdown.contains("(variable)"),
            "negative: symbol kind must be in parentheses"
        );
        assert!(
            markdown.contains("src/stores/main.ts"),
            "negative: file path must be present"
        );
        assert!(
            markdown.contains(&format!(":{}", line)),
            "negative: line number must follow path with colon"
        );
    }

    #[test]
    fn workspace_symbol_response_marks_external_symbols() {
        let response = WorkspaceSymbolResponse {
            raw_response: None,
            symbols: vec![
                create_workspace_symbol("configureStore", "function", "node_modules/@reduxjs/toolkit/dist/index.d.mts", 1847, None),
            ],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[external]"),
            "negative: node_modules symbols must be marked as external"
        );
    }

    #[test]
    fn workspace_symbol_response_includes_signature_when_present() {
        let signature = "export const store: Store<RootState>".to_string();
        let response = WorkspaceSymbolResponse {
            raw_response: None,
            symbols: vec![create_workspace_symbol("store", "variable", "src/app/store.ts", 22, Some(signature.clone()))],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains(&format!("`{}`", signature)),
            "negative: signature must be formatted as inline code"
        );
    }

    #[test]
    fn workspace_symbol_response_shows_pagination_info_when_truncated() {
        let response = WorkspaceSymbolResponse {
            raw_response: None,
            symbols: vec![
                create_workspace_symbol("store1", "variable", "src/app/store1.ts", 10, None),
                create_workspace_symbol("store2", "variable", "src/app/store2.ts", 20, None),
            ],
            limit: 2,
            offset: 0,
            truncated: true,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[Showing 2"),
            "negative: pagination info must show count when truncated"
        );
    }

    #[test]
    fn workspace_symbol_response_handles_empty_symbols_list() {
        let response = WorkspaceSymbolResponse {
            raw_response: None,
            symbols: vec![],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("0 result") || markdown.contains("No symbols"),
            "negative: empty result must indicate no symbols found"
        );
    }

    #[test]
    fn mcp_symbols_response_handles_empty_symbols_list() {
        let response = McpSymbolsResponse {
            path: "src/empty.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("src/empty.ts"),
            "negative: file path must still be in header"
        );
        assert!(
            markdown.contains("[Total: 0 symbols]") || markdown.contains("No symbols"),
            "negative: empty result must indicate zero symbols"
        );
    }

    #[test]
    fn mcp_symbols_response_handles_unicode_in_symbol_names() {
        let response = McpSymbolsResponse {
            path: "src/unicode.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![create_symbol("proces\u{0327}ador", "function", 10, None)],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("proces\u{0327}ador"),
            "negative: unicode characters must be preserved in symbol names"
        );
    }

    #[test]
    fn workspace_symbol_response_includes_container_name_when_present() {
        let mut symbol = create_workspace_symbol("methodA", "method", "src/app.ts", 15, None);
        symbol.container_name = Some("MyClass".to_string());

        let response = WorkspaceSymbolResponse {
            raw_response: None,
            symbols: vec![symbol],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("MyClass"),
            "negative: container name must be shown when present"
        );
    }

    #[test]
    fn mcp_symbols_response_handles_deeply_nested_children() {
        let grandchild = Symbol {
            name: "nestedMethod".to_string(),
            kind: "method".to_string(),
            identifier_position: FilePosition {
                path: "src/app.ts".to_string(),
                position: Position { line: 25, character: 0 },
            },
            file_range: FileRange {
                path: "src/app.ts".to_string(),
                range: Range {
                    start: Position { line: 25, character: 0 },
                    end: Position { line: 28, character: 0 },
                },
            },
            signature: None,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
            children: None,
            snippet: None,
        };

        let child = Symbol {
            name: "InnerClass".to_string(),
            kind: "class".to_string(),
            identifier_position: FilePosition {
                path: "src/app.ts".to_string(),
                position: Position { line: 20, character: 0 },
            },
            file_range: FileRange {
                path: "src/app.ts".to_string(),
                range: Range {
                    start: Position { line: 20, character: 0 },
                    end: Position { line: 30, character: 0 },
                },
            },
            signature: None,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
            children: Some(vec![grandchild]),
            snippet: None,
        };

        let parent = Symbol {
            name: "OuterClass".to_string(),
            kind: "class".to_string(),
            identifier_position: FilePosition {
                path: "src/app.ts".to_string(),
                position: Position { line: 10, character: 0 },
            },
            file_range: FileRange {
                path: "src/app.ts".to_string(),
                range: Range {
                    start: Position { line: 10, character: 0 },
                    end: Position { line: 40, character: 0 },
                },
            },
            signature: None,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
            children: Some(vec![child]),
            snippet: None,
        };

        let response = McpSymbolsResponse {
            path: "src/app.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![parent],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("OuterClass"),
            "negative: parent class must be present"
        );
        assert!(
            markdown.contains("    InnerClass"),
            "negative: child class must be indented once"
        );
        assert!(
            markdown.contains("      nestedMethod"),
            "negative: grandchild must be indented twice"
        );
    }

    #[test]
    fn mcp_identifier_response_formats_single_identifier() {
        let line = random_line();
        let response = McpIdentifierResponse {
            identifiers: vec![create_identifier("myVar", Some("variable"), "src/app.ts", line, line)],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Identifiers (1 identifier)"),
            "negative: header must show singular form for 1 identifier"
        );
        assert!(
            markdown.contains("myVar"),
            "negative: identifier name must be present"
        );
        assert!(
            markdown.contains("(variable)"),
            "negative: kind must be in parentheses"
        );
    }

    #[test]
    fn mcp_identifier_response_formats_multiple_identifiers() {
        let response = McpIdentifierResponse {
            identifiers: vec![
                create_identifier("var1", Some("variable"), "src/app.ts", 10, 10),
                create_identifier("var2", Some("function"), "src/app.ts", 20, 25),
            ],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Identifiers (2 identifiers)"),
            "negative: header must show plural form for multiple identifiers"
        );
    }

    #[test]
    fn mcp_identifier_response_shows_range_for_multiline() {
        let response = McpIdentifierResponse {
            identifiers: vec![create_identifier("myFunc", Some("function"), "src/app.ts", 10, 20)],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("src/app.ts:10-20"),
            "negative: multiline identifiers must show range"
        );
    }

    #[test]
    fn mcp_identifier_response_uses_default_kind_when_none() {
        let response = McpIdentifierResponse {
            identifiers: vec![create_identifier("unknownThing", None, "src/app.ts", 10, 10)],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("(identifier)"),
            "negative: missing kind must default to 'identifier'"
        );
    }

    #[test]
    fn mcp_identifier_response_shows_truncation() {
        let response = McpIdentifierResponse {
            identifiers: vec![create_identifier("var1", Some("variable"), "src/app.ts", 10, 10)],
            limit: 1,
            offset: 0,
            truncated: true,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[Showing 1 of more]"),
            "negative: truncation indicator must be present"
        );
    }

    #[test]
    fn format_snippet_renders_multiline_as_fenced_code_block() {
        let mut output = String::new();
        let source = "fn foo() {\n    bar()\n}";
        format_snippet(source, "", &mut output);

        assert!(
            output.contains("```"),
            "negative: multiline snippet must be fenced with triple backticks"
        );
        assert!(
            output.contains("fn foo()"),
            "negative: snippet must include source code"
        );
        assert!(
            output.contains("    bar()"),
            "negative: snippet must preserve indentation"
        );
    }

    #[test]
    fn format_snippet_renders_single_line_as_inline_code() {
        let mut output = String::new();
        let source = "let x = 42;";
        format_snippet(source, "", &mut output);

        assert!(
            output.contains("`let x = 42;`"),
            "negative: single-line snippet must use inline code"
        );
        assert!(
            !output.contains("```"),
            "negative: single-line snippet must not use fenced block"
        );
    }

    #[test]
    fn format_snippet_skips_empty_source() {
        let mut output = String::new();
        format_snippet("", "", &mut output);

        assert!(
            output.is_empty(),
            "negative: empty snippet must produce no output"
        );
    }

    #[test]
    fn mcp_symbols_response_renders_snippet_when_present() {
        use crate::api_types::CodeContext;

        let snippet = CodeContext {
            range: FileRange {
                path: "src/test.ts".to_string(),
                range: Range {
                    start: Position { line: 9, character: 1 },
                    end: Position { line: 11, character: 1 },
                },
            },
            source_code: "function foo\tbar() {\n  return 42;\n}".to_string(),
        };

        let mut symbol = create_symbol("foo\tbar", "function", 10, None);
        symbol.snippet = Some(snippet);

        let response = McpSymbolsResponse {
            path: "src/test.ts".to_string(),
            mtime: "2025-01-01T00:00:00Z".to_string(),
            symbols: vec![symbol],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("```"),
            "negative: multiline snippet must use fenced code block"
        );
        assert!(
            markdown.contains("function foo\tbar()"),
            "negative: snippet source must be rendered"
        );
    }

    #[test]
    fn workspace_symbol_response_renders_snippet_when_present() {
        use crate::api_types::CodeContext;

        let snippet = CodeContext {
            range: FileRange {
                path: "src/lib.rs".to_string(),
                range: Range {
                    start: Position { line: 9, character: 1 },
                    end: Position { line: 11, character: 1 },
                },
            },
            source_code: "pub fn example\tvalue() -> i32".to_string(),
        };

        let mut symbol = create_workspace_symbol("example\tvalue", "function", "src/lib.rs", 10, None);
        symbol.snippet = Some(snippet);

        let response = WorkspaceSymbolResponse {
            raw_response: None,
            symbols: vec![symbol],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("`pub fn example\tvalue() -> i32`"),
            "negative: single-line snippet must render as inline code"
        );
    }
}
