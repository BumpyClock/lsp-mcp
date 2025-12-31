// ABOUTME: Markdown formatter for definition response types.
// ABOUTME: Converts DefinitionResponse and related types to readable markdown.

use super::{escape_inline_code, format_file_position, truncate_lines, ToMarkdown};
use crate::api_types::RelatedSymbols;
use crate::service::types::response::McpDefinitionResponse;

const SOURCE_CODE_MAX_LINES: usize = 100;

impl ToMarkdown for McpDefinitionResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "Definition of `{}`\n\n",
            escape_inline_code(&self.selected_identifier.name)
        ));

        if self.definitions.is_empty() {
            output.push_str("No definitions found.\n");
            return output;
        }

        for (index, def) in self.definitions.iter().enumerate() {
            if self.definitions.len() > 1 {
                output.push_str(&format!("Definition {}\n\n", index + 1));
            }

            let external_tag = if def.external.unwrap_or(false) {
                " [external]"
            } else {
                ""
            };

            output.push_str(&format!(
                "Location: {}{}\n",
                format_file_position(&def.path, def.position.line, def.position.character),
                external_tag
            ));

            if let Some(ref kind) = def.symbol_kind {
                output.push_str(&format!("Kind: {}\n", kind));
            }

            if let Some(ref signature) = def.signature {
                output.push_str(&format!("Signature: `{}`\n", escape_inline_code(signature)));
            }

            if let Some(ref package) = def.package {
                output.push_str(&format!(
                    "Package: {}@{}\n",
                    package.name, package.version
                ));
            }

            if let Some(ref_count) = def.reference_count {
                output.push_str(&format!("References: {}\n", ref_count));
            }

            if let Some(ref doc) = def.doc {
                if !doc.is_empty() {
                    output.push_str("\nDocumentation\n");
                    output.push_str(doc);
                    output.push('\n');
                }
            }

            let source_to_render = if let Some(ref snippet) = def.snippet {
                if !snippet.source_code.is_empty() {
                    Some(snippet.source_code.as_str())
                } else {
                    None
                }
            } else {
                self.source_code_context
                    .as_ref()
                    .and_then(|contexts| contexts.get(index))
                    .map(|ctx| ctx.source_code.as_str())
            };

            if let Some(source) = source_to_render {
                let language = detect_language(&def.path);
                let truncated_source = truncate_lines(source, SOURCE_CODE_MAX_LINES);
                output.push_str("\nSource\n");
                output.push_str(&format!("```{}\n{}\n```\n", language, truncated_source));
            }
        }

        if let Some(ref related) = self.related {
            format_related_symbols(&mut output, related);
        }

        if self.truncated {
            output.push_str(&format!(
                "\nResults truncated (showing {} of more)\n",
                self.definitions.len()
            ));
        }

        output
    }
}

fn detect_language(path: &str) -> &'static str {
    if let Some(ext) = path.rsplit('.').next() {
        match ext.to_lowercase().as_str() {
            "rs" => "rust",
            "ts" | "tsx" => "typescript",
            "js" | "jsx" | "mjs" | "cjs" => "javascript",
            "py" => "python",
            "go" => "go",
            "java" => "java",
            "cs" => "csharp",
            "cpp" | "cc" | "cxx" | "hpp" | "h" => "cpp",
            "rb" => "ruby",
            "php" => "php",
            _ => "",
        }
    } else {
        ""
    }
}

fn format_related_symbols(output: &mut String, related: &RelatedSymbols) {
    if !related.sibling_exports.is_empty() {
        output.push_str(&format!(
            "\nSibling Exports ({})\n",
            related.sibling_exports.len()
        ));

        for sibling in &related.sibling_exports {
            let kind_part = if sibling.kind.is_empty() {
                String::new()
            } else {
                format!(" ({})", sibling.kind)
            };

            output.push_str(&format!(
                "  `{}`{} - line {}\n",
                escape_inline_code(&sibling.name),
                kind_part,
                sibling.identifier_position.position.line
            ));
        }
    }

    if !related.implements.is_empty() {
        output.push_str(&format!(
            "\nImplements ({})\n",
            related.implements.len()
        ));

        for item in &related.implements {
            output.push_str(&format!("  `{}`\n", escape_inline_code(&item.name)));
        }
    }

    if !related.extends.is_empty() {
        output.push_str(&format!("\nExtends ({})\n", related.extends.len()));

        for item in &related.extends {
            output.push_str(&format!("  `{}`\n", escape_inline_code(&item.name)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{CodeContext, FilePosition, FileRange, Identifier, Position, Range, Symbol};
    use crate::service::types::response::McpDefinitionLocation;
    use crate::service::utils::external::PackageInfo;
    use rand::Rng;

    fn random_line() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..1000)
    }

    fn random_character() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..200)
    }

    fn create_position(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn create_range(start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> Range {
        Range {
            start: Position {
                line: start_line,
                character: start_char,
            },
            end: Position {
                line: end_line,
                character: end_char,
            },
        }
    }

    fn create_file_range(path: &str, start_line: u32, end_line: u32) -> FileRange {
        FileRange {
            path: path.to_string(),
            range: create_range(start_line, 1, end_line, 1),
        }
    }

    fn create_identifier(name: &str, path: &str, line: u32) -> Identifier {
        Identifier {
            name: name.to_string(),
            file_range: create_file_range(path, line, line),
            kind: Some("function".to_string()),
        }
    }

    fn create_minimal_definition(path: &str, line: u32, character: u32) -> McpDefinitionLocation {
        McpDefinitionLocation {
            path: path.to_string(),
            position: create_position(line, character),
            definition_range: create_range(line, 1, line + 5, 1),
            symbol_kind: None,
            snippet: None,
            signature: None,
            doc: None,
            external: None,
            package: None,
            reference_count: None,
        }
    }

    fn create_full_definition(
        path: &str,
        line: u32,
        character: u32,
        kind: &str,
        signature: &str,
        doc: &str,
        source: &str,
    ) -> McpDefinitionLocation {
        McpDefinitionLocation {
            path: path.to_string(),
            position: create_position(line, character),
            definition_range: create_range(line, 1, line + 5, 1),
            symbol_kind: Some(kind.to_string()),
            snippet: Some(CodeContext {
                range: create_file_range(path, line, line + 5),
                source_code: source.to_string(),
            }),
            signature: Some(signature.to_string()),
            doc: Some(doc.to_string()),
            external: None,
            package: None,
            reference_count: None,
        }
    }

    fn create_response_with_definitions(
        name: &str,
        definitions: Vec<McpDefinitionLocation>,
    ) -> McpDefinitionResponse {
        McpDefinitionResponse {
            raw_response: None,
            definitions,
            source_code_context: None,
            selected_identifier: create_identifier(name, "src/test.ts", 10),
            related: None,
            limit: 100,
            offset: 0,
            truncated: false,
        }
    }

    fn create_symbol(name: &str, kind: &str, line: u32) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: kind.to_string(),
            identifier_position: FilePosition {
                path: "src/module.ts".to_string(),
                position: create_position(line, 1),
            },
            file_range: create_file_range("src/module.ts", line, line + 5),
            signature: None,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
            children: None,
            snippet: None,
        }
    }

    #[test]
    fn it_renders_header_with_identifier_name() {
        let line = random_line();
        let character = random_character();
        let response = create_response_with_definitions(
            "myFunction",
            vec![create_minimal_definition("src/utils.ts", line, character)],
        );

        let result = response.to_markdown();

        assert!(
            result.contains("Definition of `myFunction`"),
            "negative: header must contain escaped identifier name"
        );
    }

    #[test]
    fn it_escapes_backticks_in_identifier_name() {
        let response = create_response_with_definitions(
            "function`with`backticks",
            vec![create_minimal_definition("src/test.ts", 10, 5)],
        );

        let result = response.to_markdown();

        assert!(
            result.contains("\\`"),
            "negative: backticks in identifier name must be escaped"
        );
    }

    #[test]
    fn it_shows_no_definitions_found_when_empty() {
        let response = create_response_with_definitions("unknownSymbol", vec![]);

        let result = response.to_markdown();

        assert!(
            result.contains("No definitions found"),
            "negative: empty definitions must show 'No definitions found'"
        );
    }

    #[test]
    fn it_renders_location_with_path_and_position() {
        let line = random_line();
        let character = random_character();
        let response = create_response_with_definitions(
            "testFunc",
            vec![create_minimal_definition("src/helpers.ts", line, character)],
        );

        let result = response.to_markdown();

        let expected_location = format!("Location: src/helpers.ts:{}:{}", line, character);
        assert!(
            result.contains(&expected_location),
            "negative: output must contain location with path:line:character"
        );
    }

    #[test]
    fn it_renders_symbol_kind_when_present() {
        let mut def = create_minimal_definition("src/test.ts", 42, 5);
        def.symbol_kind = Some("function".to_string());
        let response = create_response_with_definitions("testFunc", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("Kind: function"),
            "negative: output must contain symbol kind"
        );
    }

    #[test]
    fn it_omits_kind_when_none() {
        let response = create_response_with_definitions(
            "testFunc",
            vec![create_minimal_definition("src/test.ts", 42, 5)],
        );

        let result = response.to_markdown();

        assert!(
            !result.contains("Kind:"),
            "negative: output must not contain Kind label when none"
        );
    }

    #[test]
    fn it_renders_signature_when_present() {
        let mut def = create_minimal_definition("src/test.ts", 42, 5);
        def.signature = Some("(arg: string) => Promise<Result>".to_string());
        let response = create_response_with_definitions("testFunc", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("Signature: `(arg: string) => Promise<Result>`"),
            "negative: output must contain escaped signature"
        );
    }

    #[test]
    fn it_escapes_backticks_in_signature() {
        let mut def = create_minimal_definition("src/test.ts", 42, 5);
        def.signature = Some("fn<T: `trait`>(arg: T)".to_string());
        let response = create_response_with_definitions("testFunc", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("\\`"),
            "negative: backticks in signature must be escaped"
        );
    }

    #[test]
    fn it_renders_documentation_when_present() {
        let def = create_full_definition(
            "src/test.ts",
            42,
            5,
            "function",
            "()",
            "Helper function that processes data.",
            "function test() {}",
        );
        let response = create_response_with_definitions("testFunc", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("Documentation"),
            "negative: output must contain Documentation section"
        );
        assert!(
            result.contains("Helper function that processes data."),
            "negative: output must contain documentation text"
        );
    }

    #[test]
    fn it_omits_documentation_section_when_empty() {
        let mut def = create_minimal_definition("src/test.ts", 42, 5);
        def.doc = Some(String::new());
        let response = create_response_with_definitions("testFunc", vec![def]);

        let result = response.to_markdown();

        assert!(
            !result.contains("Documentation"),
            "negative: output must not contain Documentation section when empty"
        );
    }

    #[test]
    fn it_renders_source_code_with_language_fence() {
        let def = create_full_definition(
            "src/test.ts",
            42,
            5,
            "function",
            "()",
            "",
            "export function test(): void {\n  console.log('hello');\n}",
        );
        let response = create_response_with_definitions("testFunc", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("Source"),
            "negative: output must contain Source section"
        );
        assert!(
            result.contains("```typescript"),
            "negative: output must use typescript code fence for .ts files"
        );
    }

    #[test]
    fn it_detects_rust_language_from_extension() {
        let def = create_full_definition(
            "src/lib.rs",
            10,
            1,
            "function",
            "()",
            "",
            "fn test() {}",
        );
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("```rust"),
            "negative: output must use rust code fence for .rs files"
        );
    }

    #[test]
    fn it_detects_python_language_from_extension() {
        let def = create_full_definition(
            "src/module.py",
            10,
            1,
            "function",
            "()",
            "",
            "def test(): pass",
        );
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("```python"),
            "negative: output must use python code fence for .py files"
        );
    }

    #[test]
    fn it_truncates_source_code_exceeding_100_lines() {
        let long_source = (0..150)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let mut def = create_minimal_definition("src/test.ts", 10, 1);
        def.snippet = Some(CodeContext {
            range: create_file_range("src/test.ts", 10, 160),
            source_code: long_source,
        });
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("[truncated, 150 total lines]"),
            "negative: output must show truncation indicator for long source"
        );
    }

    #[test]
    fn it_marks_external_definitions_with_tag() {
        let mut def = create_minimal_definition("node_modules/lodash/index.js", 10, 5);
        def.external = Some(true);
        let response = create_response_with_definitions("map", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("[external]"),
            "negative: external definitions must be marked with [external] tag"
        );
    }

    #[test]
    fn it_omits_external_tag_for_internal_definitions() {
        let mut def = create_minimal_definition("src/utils.ts", 10, 5);
        def.external = Some(false);
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            !result.contains("[external]"),
            "negative: internal definitions must not have [external] tag"
        );
    }

    #[test]
    fn it_renders_package_info_when_present() {
        let mut def = create_minimal_definition("node_modules/lodash/index.js", 10, 5);
        def.external = Some(true);
        def.package = Some(PackageInfo {
            name: "lodash".to_string(),
            version: "4.17.21".to_string(),
        });
        let response = create_response_with_definitions("map", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("Package: lodash@4.17.21"),
            "negative: output must contain package name and version"
        );
    }

    #[test]
    fn it_renders_reference_count_when_present() {
        let mut def = create_minimal_definition("src/utils.ts", 10, 5);
        def.reference_count = Some(42);
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("References: 42"),
            "negative: output must contain reference count"
        );
    }

    #[test]
    fn it_renders_multiple_definitions_with_headers() {
        let def1 = create_minimal_definition("src/utils.ts", 10, 5);
        let def2 = create_minimal_definition("src/helpers.ts", 20, 3);
        let response = create_response_with_definitions("test", vec![def1, def2]);

        let result = response.to_markdown();

        assert!(
            result.contains("Definition 1"),
            "negative: multiple definitions must have numbered headers"
        );
        assert!(
            result.contains("Definition 2"),
            "negative: multiple definitions must have numbered headers"
        );
    }

    #[test]
    fn it_omits_definition_headers_for_single_definition() {
        let response = create_response_with_definitions(
            "test",
            vec![create_minimal_definition("src/utils.ts", 10, 5)],
        );

        let result = response.to_markdown();

        assert!(
            !result.contains("Definition 1"),
            "negative: single definition must not have numbered header"
        );
    }

    #[test]
    fn it_renders_sibling_exports_when_present() {
        let mut response = create_response_with_definitions(
            "test",
            vec![create_minimal_definition("src/utils.ts", 10, 5)],
        );
        response.related = Some(RelatedSymbols {
            sibling_exports: vec![
                create_symbol("helperA", "function", 20),
                create_symbol("helperB", "constant", 30),
            ],
            implements: vec![],
            extends: vec![],
            used_by_types: vec![],
        });

        let result = response.to_markdown();

        assert!(
            result.contains("Sibling Exports (2)"),
            "negative: output must contain Sibling Exports section with count"
        );
        assert!(
            result.contains("  `helperA` (function) - line 20"),
            "negative: output must list sibling export with kind and line"
        );
        assert!(
            result.contains("  `helperB` (constant) - line 30"),
            "negative: output must list all sibling exports"
        );
    }

    #[test]
    fn it_renders_implements_when_present() {
        let mut response = create_response_with_definitions(
            "test",
            vec![create_minimal_definition("src/utils.ts", 10, 5)],
        );
        response.related = Some(RelatedSymbols {
            sibling_exports: vec![],
            implements: vec![create_symbol("IService", "interface", 5)],
            extends: vec![],
            used_by_types: vec![],
        });

        let result = response.to_markdown();

        assert!(
            result.contains("Implements (1)"),
            "negative: output must contain Implements section"
        );
        assert!(
            result.contains("  `IService`"),
            "negative: output must list implemented interface"
        );
    }

    #[test]
    fn it_renders_extends_when_present() {
        let mut response = create_response_with_definitions(
            "test",
            vec![create_minimal_definition("src/utils.ts", 10, 5)],
        );
        response.related = Some(RelatedSymbols {
            sibling_exports: vec![],
            implements: vec![],
            extends: vec![create_symbol("BaseClass", "class", 5)],
            used_by_types: vec![],
        });

        let result = response.to_markdown();

        assert!(
            result.contains("Extends (1)"),
            "negative: output must contain Extends section"
        );
        assert!(
            result.contains("  `BaseClass`"),
            "negative: output must list extended class"
        );
    }

    #[test]
    fn it_omits_related_sections_when_empty() {
        let mut response = create_response_with_definitions(
            "test",
            vec![create_minimal_definition("src/utils.ts", 10, 5)],
        );
        response.related = Some(RelatedSymbols::default());

        let result = response.to_markdown();

        assert!(
            !result.contains("Sibling Exports"),
            "negative: empty sibling exports must not be rendered"
        );
        assert!(
            !result.contains("Implements"),
            "negative: empty implements must not be rendered"
        );
        assert!(
            !result.contains("Extends"),
            "negative: empty extends must not be rendered"
        );
    }

    #[test]
    fn it_shows_truncation_indicator_when_truncated() {
        let mut response = create_response_with_definitions(
            "test",
            vec![create_minimal_definition("src/utils.ts", 10, 5)],
        );
        response.truncated = true;

        let result = response.to_markdown();

        assert!(
            result.contains("Results truncated"),
            "negative: truncated response must show truncation indicator"
        );
    }

    #[test]
    fn it_handles_unicode_in_identifier_name() {
        let response = create_response_with_definitions(
            "функция_тест",
            vec![create_minimal_definition("src/utils.ts", 10, 5)],
        );

        let result = response.to_markdown();

        assert!(
            result.contains("функция_тест"),
            "negative: unicode identifier names must be preserved"
        );
    }

    #[test]
    fn it_handles_unicode_in_documentation() {
        let def = create_full_definition(
            "src/test.ts",
            10,
            5,
            "function",
            "()",
            "Функция для обработки данных 🚀",
            "function test() {}",
        );
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("Функция для обработки данных 🚀"),
            "negative: unicode in documentation must be preserved"
        );
    }

    #[test]
    fn it_detects_javascript_from_js_extension() {
        let def = create_full_definition(
            "src/index.js",
            10,
            1,
            "function",
            "()",
            "",
            "function test() {}",
        );
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("```javascript"),
            "negative: output must use javascript code fence for .js files"
        );
    }

    #[test]
    fn it_detects_go_from_go_extension() {
        let def = create_full_definition(
            "main.go",
            10,
            1,
            "function",
            "()",
            "",
            "func test() {}",
        );
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("```go"),
            "negative: output must use go code fence for .go files"
        );
    }

    #[test]
    fn it_uses_empty_fence_for_unknown_extensions() {
        let def = create_full_definition(
            "src/config.xyz",
            10,
            1,
            "variable",
            "()",
            "",
            "some content",
        );
        let response = create_response_with_definitions("config", vec![def]);

        let result = response.to_markdown();

        assert!(
            result.contains("```\n"),
            "negative: unknown extensions must use empty code fence"
        );
    }

    #[test]
    fn it_omits_source_section_when_snippet_is_empty() {
        let mut def = create_minimal_definition("src/test.ts", 10, 5);
        def.snippet = Some(CodeContext {
            range: create_file_range("src/test.ts", 10, 15),
            source_code: String::new(),
        });
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            !result.contains("Source"),
            "negative: empty snippet must not render Source section"
        );
    }

    #[test]
    fn it_handles_sibling_export_without_kind() {
        let mut response = create_response_with_definitions(
            "test",
            vec![create_minimal_definition("src/utils.ts", 10, 5)],
        );
        let mut sibling = create_symbol("helper", "", 20);
        sibling.kind = String::new();
        response.related = Some(RelatedSymbols {
            sibling_exports: vec![sibling],
            implements: vec![],
            extends: vec![],
            used_by_types: vec![],
        });

        let result = response.to_markdown();

        assert!(
            result.contains("  `helper` - line 20"),
            "negative: sibling without kind must omit kind parentheses"
        );
        assert!(
            !result.contains("()"),
            "negative: empty kind must not show empty parentheses"
        );
    }

    #[test]
    fn it_renders_source_from_source_code_context_when_snippet_is_none() {
        let mut def = create_minimal_definition("src/test.ts", 10, 5);
        def.snippet = None;

        let mut response = create_response_with_definitions("testFunc", vec![def]);
        response.source_code_context = Some(vec![CodeContext {
            range: create_file_range("src/test.ts", 10, 15),
            source_code: "export function testFunc(): void {\n  console.log('from context');\n}".to_string(),
        }]);

        let result = response.to_markdown();

        assert!(
            result.contains("Source"),
            "negative: source_code_context must render Source section when snippet is None"
        );
        assert!(
            result.contains("from context"),
            "negative: source_code_context content must be rendered"
        );
        assert!(
            result.contains("```typescript"),
            "negative: output must use typescript code fence"
        );
    }

    #[test]
    fn it_truncates_source_code_context_exceeding_100_lines() {
        let long_source = (0..150)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let mut def = create_minimal_definition("src/test.go", 10, 1);
        def.snippet = None;

        let mut response = create_response_with_definitions("test", vec![def]);
        response.source_code_context = Some(vec![CodeContext {
            range: create_file_range("src/test.go", 10, 160),
            source_code: long_source,
        }]);

        let result = response.to_markdown();

        assert!(
            result.contains("[truncated, 150 total lines]"),
            "negative: source_code_context must show truncation indicator for long source"
        );
    }

    #[test]
    fn it_prefers_snippet_over_source_code_context_when_both_present() {
        let def = create_full_definition(
            "src/test.rs",
            42,
            5,
            "function",
            "()",
            "",
            "fn from_snippet() {}",
        );

        let mut response = create_response_with_definitions("testFunc", vec![def]);
        response.source_code_context = Some(vec![CodeContext {
            range: create_file_range("src/test.rs", 42, 47),
            source_code: "fn from_context() {}".to_string(),
        }]);

        let result = response.to_markdown();

        assert!(
            result.contains("from_snippet"),
            "negative: snippet must be preferred over source_code_context"
        );
        assert!(
            !result.contains("from_context"),
            "negative: source_code_context must not be used when snippet exists"
        );
    }

    #[test]
    fn it_omits_source_section_when_neither_snippet_nor_context() {
        let def = create_minimal_definition("src/test.ts", 10, 5);
        let response = create_response_with_definitions("test", vec![def]);

        let result = response.to_markdown();

        assert!(
            !result.contains("Source"),
            "negative: Source section must not appear when no snippet or context"
        );
    }

    #[test]
    fn it_handles_multiple_definitions_with_source_code_context() {
        let mut def1 = create_minimal_definition("src/utils.ts", 10, 5);
        def1.snippet = None;
        let mut def2 = create_minimal_definition("src/helpers.ts", 20, 3);
        def2.snippet = None;

        let mut response = create_response_with_definitions("test", vec![def1, def2]);
        response.source_code_context = Some(vec![
            CodeContext {
                range: create_file_range("src/utils.ts", 10, 15),
                source_code: "function test() { /* first */ }".to_string(),
            },
            CodeContext {
                range: create_file_range("src/helpers.ts", 20, 25),
                source_code: "function test() { /* second */ }".to_string(),
            },
        ]);

        let result = response.to_markdown();

        assert!(
            result.contains("first"),
            "negative: first definition source_code_context must be rendered"
        );
        assert!(
            result.contains("second"),
            "negative: second definition source_code_context must be rendered"
        );
    }

    #[test]
    fn it_handles_unicode_in_source_code_context() {
        let mut def = create_minimal_definition("src/test.py", 10, 5);
        def.snippet = None;

        let mut response = create_response_with_definitions("тест", vec![def]);
        response.source_code_context = Some(vec![CodeContext {
            range: create_file_range("src/test.py", 10, 15),
            source_code: "def тест():\n  print('Привет мир 🚀')".to_string(),
        }]);

        let result = response.to_markdown();

        assert!(
            result.contains("Привет мир 🚀"),
            "negative: unicode in source_code_context must be preserved"
        );
    }
}
