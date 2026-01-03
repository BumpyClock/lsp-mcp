// ABOUTME: Markdown formatting module for converting LSP responses to human-readable output.
// ABOUTME: Provides ToMarkdown trait and utility functions for consistent markdown generation.

mod call_hierarchy;
mod definition;
mod diagnostics;
mod files;
mod health;
mod hover;
mod references;
mod symbols;

pub use files::SourceCodeResponse;
pub use hover::HoverBatchResponse;
pub use references::format_references_summary;

/// Trait for converting types to markdown representation.
///
/// Implementors produce human-readable markdown output optimized for
/// token efficiency and readability in LLM contexts.
pub trait ToMarkdown {
    fn to_markdown(&self) -> String;
}

/// Escape markdown special characters in text for use in inline code contexts.
///
/// Escapes backticks to prevent breaking inline code spans.
pub fn escape_inline_code(text: &str) -> String {
    text.replace('`', "\\`")
}

/// Truncate content with a line count indicator.
///
/// If the content exceeds `max_lines`, truncates and appends
/// `[truncated, N total lines]` indicator.
pub fn truncate_lines(content: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();

    if total <= max_lines {
        return content.to_string();
    }

    let truncated: String = lines[..max_lines].join("\n");
    format!("{}\n[truncated, {} total lines]", truncated, total)
}

/// Format a line:character position consistently.
///
/// Produces output like "42:10" for line 42, character 10.
pub fn format_position(line: u32, character: u32) -> String {
    format!("{}:{}", line, character)
}

/// Format a file path with position.
///
/// Produces output like "src/main.rs:42:10".
pub fn format_file_position(path: &str, line: u32, character: u32) -> String {
    format!("{}:{}:{}", path, line, character)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    fn random_line_count() -> usize {
        let mut rng = rand::rng();
        rng.random_range(5..50)
    }

    fn random_position() -> (u32, u32) {
        let mut rng = rand::rng();
        (rng.random_range(1..1000), rng.random_range(1..200))
    }

    fn generate_multiline_content(num_lines: usize) -> String {
        (0..num_lines)
            .map(|i| format!("line {} content", i))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn it_escapes_backticks_in_inline_code() {
        let input = "code with `backticks` inside";
        let result = escape_inline_code(input);
        let unescaped_backtick_count = result.matches('`').count() - result.matches("\\`").count();

        assert!(
            result.contains("\\`"),
            "negative: escaped string must contain backslash-backtick sequence"
        );
        assert_eq!(
            unescaped_backtick_count, 0,
            "negative: all backticks must be escaped"
        );
    }

    #[test]
    fn it_preserves_text_without_backticks() {
        let input = "plain text without special chars";
        let result = escape_inline_code(input);

        assert_eq!(
            result, input,
            "negative: text without backticks must remain unchanged"
        );
    }

    #[test]
    fn it_escapes_multiple_backticks() {
        let input = "`one` and `two` and `three`";
        let result = escape_inline_code(input);
        let backtick_count = result.matches("\\`").count();

        assert_eq!(
            backtick_count, 6,
            "negative: all six backticks must be escaped"
        );
    }

    #[test]
    fn it_truncates_content_exceeding_max_lines() {
        let num_lines = random_line_count();
        let max_lines = num_lines / 2;
        let content = generate_multiline_content(num_lines);

        let result = truncate_lines(&content, max_lines);

        assert!(
            result.contains("[truncated,"),
            "negative: truncated content must contain truncation indicator"
        );
        assert!(
            result.contains(&format!("{} total lines]", num_lines)),
            "negative: truncation indicator must show total line count"
        );
    }

    #[test]
    fn it_preserves_content_within_max_lines() {
        let num_lines = random_line_count();
        let max_lines = num_lines + 10;
        let content = generate_multiline_content(num_lines);

        let result = truncate_lines(&content, max_lines);

        assert_eq!(
            result, content,
            "negative: content within limit must remain unchanged"
        );
    }

    #[test]
    fn it_preserves_content_at_exact_max_lines() {
        let num_lines = random_line_count();
        let content = generate_multiline_content(num_lines);

        let result = truncate_lines(&content, num_lines);

        assert_eq!(
            result, content,
            "negative: content at exact limit must remain unchanged"
        );
    }

    #[test]
    fn it_formats_position_as_line_colon_character() {
        let (line, character) = random_position();

        let result = format_position(line, character);

        assert_eq!(
            result,
            format!("{}:{}", line, character),
            "negative: position must be formatted as line:character"
        );
    }

    #[test]
    fn it_formats_file_position_with_path() {
        let (line, character) = random_position();
        let path = "src/lib.rs";

        let result = format_file_position(path, line, character);

        assert!(
            result.starts_with(path),
            "negative: file position must start with path"
        );
        assert!(
            result.contains(&format!("{}:{}", line, character)),
            "negative: file position must contain line:character"
        );
    }

    #[test]
    fn it_handles_empty_content_in_truncate() {
        let result = truncate_lines("", 10);

        assert_eq!(result, "", "negative: empty content must remain empty");
    }

    #[test]
    fn it_handles_single_line_content() {
        let content = "single line";
        let result = truncate_lines(content, 1);

        assert_eq!(
            result, content,
            "negative: single line at limit must remain unchanged"
        );
    }

    #[test]
    fn it_handles_unicode_in_escape_inline_code() {
        let input = "code with `unicode` \u{1F600} and more `backticks`";
        let result = escape_inline_code(input);

        assert!(
            result.contains("\u{1F600}"),
            "negative: unicode characters must be preserved"
        );
        assert!(
            result.contains("\\`"),
            "negative: backticks must be escaped even with unicode"
        );
    }

    #[test]
    fn it_handles_zero_position_values() {
        let result = format_position(0, 0);

        assert_eq!(result, "0:0", "negative: zero positions must be formatted correctly");
    }
}
