// ABOUTME: Markdown formatter for file listing response types.
// ABOUTME: Converts file list results to readable markdown with grouping.

use super::ToMarkdown;
use crate::service::types::response::McpListFilesResponse;

impl ToMarkdown for McpListFilesResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "## Workspace Files ({} total)\n\n",
            self.files.len()
        ));

        for file in &self.files {
            output.push_str(&format!("- {}\n", file));
        }

        if self.truncated {
            let start = self.offset + 1;
            let end = self.offset + self.files.len() as u32;
            output.push_str(&format!("\n[Showing {}-{}]", start, end));
        }

        output
    }
}

/// Response wrapper for source code content with metadata.
///
/// Encapsulates source file content along with positioning information
/// for formatted markdown output with line numbers.
#[derive(Debug, Clone)]
pub struct SourceCodeResponse {
    pub path: String,
    pub content: String,
    pub start_line: u32,
    pub end_line: u32,
    pub total_lines: u32,
}

impl SourceCodeResponse {
    fn detect_language(&self) -> &'static str {
        let extension = self
            .path
            .rsplit('.')
            .next()
            .unwrap_or("");

        match extension {
            "ts" | "tsx" => "typescript",
            "js" | "jsx" => "javascript",
            "rs" => "rust",
            "py" => "python",
            "go" => "go",
            "java" => "java",
            "c" => "c",
            "cpp" | "cc" | "cxx" => "cpp",
            "h" | "hpp" => "cpp",
            "rb" => "ruby",
            "php" => "php",
            "swift" => "swift",
            "kt" | "kts" => "kotlin",
            "cs" => "csharp",
            "sh" | "bash" => "bash",
            "json" => "json",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "md" => "markdown",
            "html" | "htm" => "html",
            "css" => "css",
            "scss" | "sass" => "scss",
            "sql" => "sql",
            "xml" => "xml",
            _ => "",
        }
    }

    fn is_partial_read(&self) -> bool {
        self.start_line != 1 || self.end_line != self.total_lines
    }
}

impl ToMarkdown for SourceCodeResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("## Source: {}\n\n", self.path));

        let language = self.detect_language();
        output.push_str(&format!("```{}\n", language));

        for (idx, line) in self.content.lines().enumerate() {
            let line_num = self.start_line + idx as u32;
            output.push_str(&format!("{}: {}\n", line_num, line));
        }

        output.push_str("```");

        if self.is_partial_read() {
            output.push_str(&format!(
                "\n\n[Lines {}-{} of {}]",
                self.start_line, self.end_line, self.total_lines
            ));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    fn random_file_count() -> usize {
        let mut rng = rand::rng();
        rng.random_range(3..20)
    }

    fn generate_file_paths(count: usize) -> Vec<String> {
        (0..count)
            .map(|i| format!("src/module_{}/file_{}.ts", i % 3, i))
            .collect()
    }

    #[allow(dead_code)]
    fn random_line_count() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(10..500)
    }

    fn generate_source_content(num_lines: usize) -> String {
        (0..num_lines)
            .map(|i| format!("const line{} = {};", i, i * 2))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // McpListFilesResponse tests

    #[test]
    fn it_formats_file_list_with_header_showing_total_count() {
        let file_count = random_file_count();
        let files = generate_file_paths(file_count);
        let response = McpListFilesResponse {
            files: files.clone(),
            limit: file_count as u32,
            offset: 0,
            truncated: false,
        };

        let result = response.to_markdown();

        assert!(
            result.contains(&format!("## Workspace Files ({} total)", file_count)),
            "negative: markdown must include header with total file count"
        );
    }

    #[test]
    fn it_lists_all_files_as_markdown_items() {
        let file_count = random_file_count();
        let files = generate_file_paths(file_count);
        let response = McpListFilesResponse {
            files: files.clone(),
            limit: file_count as u32,
            offset: 0,
            truncated: false,
        };

        let result = response.to_markdown();

        for file in &files {
            assert!(
                result.contains(&format!("- {}", file)),
                "negative: each file must appear as a markdown list item"
            );
        }
    }

    #[test]
    fn it_shows_pagination_info_when_truncated() {
        let file_count = random_file_count();
        let files = generate_file_paths(file_count);
        let _total_files = file_count * 10;
        let response = McpListFilesResponse {
            files: files.clone(),
            limit: file_count as u32,
            offset: 0,
            truncated: true,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("[Showing"),
            "negative: truncated response must show pagination indicator"
        );
    }

    #[test]
    fn it_omits_pagination_info_when_not_truncated() {
        let file_count = random_file_count();
        let files = generate_file_paths(file_count);
        let response = McpListFilesResponse {
            files: files.clone(),
            limit: file_count as u32,
            offset: 0,
            truncated: false,
        };

        let result = response.to_markdown();

        assert!(
            !result.contains("[Showing"),
            "negative: non-truncated response must not show pagination indicator"
        );
    }

    #[test]
    fn it_handles_empty_file_list() {
        let response = McpListFilesResponse {
            files: vec![],
            limit: 100,
            offset: 0,
            truncated: false,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("## Workspace Files (0 total)"),
            "negative: empty file list must show zero total"
        );
    }

    #[test]
    fn it_includes_offset_in_pagination_info() {
        let file_count = random_file_count();
        let files = generate_file_paths(file_count);
        let offset = 50u32;
        let response = McpListFilesResponse {
            files: files.clone(),
            limit: file_count as u32,
            offset,
            truncated: true,
        };

        let result = response.to_markdown();

        assert!(
            result.contains(&format!("{}", offset + 1)) || result.contains(&format!("{}", offset)),
            "negative: pagination must reference offset position"
        );
    }

    // SourceCodeResponse tests

    #[test]
    fn it_formats_source_code_with_path_header() {
        let path = "src/utils/helpers.ts";
        let content = generate_source_content(10);
        let response = SourceCodeResponse {
            path: path.to_string(),
            content,
            start_line: 1,
            end_line: 10,
            total_lines: 10,
        };

        let result = response.to_markdown();

        assert!(
            result.contains(&format!("## Source: {}", path)),
            "negative: source code markdown must include path header"
        );
    }

    #[test]
    fn it_adds_line_numbers_to_source_content() {
        let content = "const a = 1;\nconst b = 2;\nconst c = 3;";
        let response = SourceCodeResponse {
            path: "test.ts".to_string(),
            content: content.to_string(),
            start_line: 1,
            end_line: 3,
            total_lines: 3,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("1: const a = 1;"),
            "negative: first line must have line number prefix"
        );
        assert!(
            result.contains("2: const b = 2;"),
            "negative: second line must have line number prefix"
        );
        assert!(
            result.contains("3: const c = 3;"),
            "negative: third line must have line number prefix"
        );
    }

    #[test]
    fn it_shows_line_range_for_partial_reads() {
        let content = generate_source_content(10);
        let total_lines = 147u32;
        let response = SourceCodeResponse {
            path: "src/large_file.ts".to_string(),
            content,
            start_line: 1,
            end_line: 10,
            total_lines,
        };

        let result = response.to_markdown();

        assert!(
            result.contains(&format!("[Lines 1-10 of {}]", total_lines)),
            "negative: partial read must show line range indicator"
        );
    }

    #[test]
    fn it_omits_line_range_when_showing_complete_file() {
        let num_lines = 5usize;
        let content = generate_source_content(num_lines);
        let response = SourceCodeResponse {
            path: "src/small_file.ts".to_string(),
            content,
            start_line: 1,
            end_line: num_lines as u32,
            total_lines: num_lines as u32,
        };

        let result = response.to_markdown();

        assert!(
            !result.contains("[Lines"),
            "negative: complete file must not show line range indicator"
        );
    }

    #[test]
    fn it_uses_correct_line_numbers_for_offset_reads() {
        let content = "const offset_line = 50;";
        let start_line = 50u32;
        let response = SourceCodeResponse {
            path: "src/file.ts".to_string(),
            content: content.to_string(),
            start_line,
            end_line: 50,
            total_lines: 100,
        };

        let result = response.to_markdown();

        assert!(
            result.contains(&format!("{}: const offset_line = 50;", start_line)),
            "negative: line numbers must start from start_line offset"
        );
    }

    #[test]
    fn it_wraps_source_in_fenced_code_block() {
        let content = "const x = 1;";
        let response = SourceCodeResponse {
            path: "test.ts".to_string(),
            content: content.to_string(),
            start_line: 1,
            end_line: 1,
            total_lines: 1,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("```"),
            "negative: source code must be wrapped in fenced code block"
        );
    }

    #[test]
    fn it_detects_language_from_file_extension() {
        let content = "const x = 1;";
        let response = SourceCodeResponse {
            path: "test.ts".to_string(),
            content: content.to_string(),
            start_line: 1,
            end_line: 1,
            total_lines: 1,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("```typescript"),
            "negative: typescript files must use typescript code fence"
        );
    }

    #[test]
    fn it_handles_rust_file_extension() {
        let content = "fn main() {}";
        let response = SourceCodeResponse {
            path: "test.rs".to_string(),
            content: content.to_string(),
            start_line: 1,
            end_line: 1,
            total_lines: 1,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("```rust"),
            "negative: rust files must use rust code fence"
        );
    }

    #[test]
    fn it_handles_python_file_extension() {
        let content = "def main(): pass";
        let response = SourceCodeResponse {
            path: "test.py".to_string(),
            content: content.to_string(),
            start_line: 1,
            end_line: 1,
            total_lines: 1,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("```python"),
            "negative: python files must use python code fence"
        );
    }

    #[test]
    fn it_handles_empty_source_content() {
        let response = SourceCodeResponse {
            path: "empty.ts".to_string(),
            content: String::new(),
            start_line: 1,
            end_line: 0,
            total_lines: 0,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("## Source: empty.ts"),
            "negative: empty file must still show header"
        );
    }

    #[test]
    fn it_handles_unicode_in_source_content() {
        let content = "const emoji = '\u{1F600}';\nconst chinese = '\u{4E2D}\u{6587}';";
        let response = SourceCodeResponse {
            path: "unicode.ts".to_string(),
            content: content.to_string(),
            start_line: 1,
            end_line: 2,
            total_lines: 2,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("\u{1F600}"),
            "negative: unicode emoji must be preserved"
        );
        assert!(
            result.contains("\u{4E2D}\u{6587}"),
            "negative: unicode chinese characters must be preserved"
        );
    }

    #[test]
    fn it_handles_paths_with_special_characters() {
        let path = "src/my-module/file_name.component.ts";
        let content = "export const x = 1;";
        let response = SourceCodeResponse {
            path: path.to_string(),
            content: content.to_string(),
            start_line: 1,
            end_line: 1,
            total_lines: 1,
        };

        let result = response.to_markdown();

        assert!(
            result.contains(&format!("## Source: {}", path)),
            "negative: paths with special characters must be preserved"
        );
    }
}
