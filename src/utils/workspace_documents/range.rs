// ABOUTME: Range extraction utilities for extracting text ranges from document content
// ABOUTME: Handles line and character-based range slicing with bounds checking

use log::warn;
use lsp_types::Range;
use std::error::Error;

/// Byte offset of the `n`-th character (by character index) in `s`.
///
/// Maps a *character* offset to a byte offset so slicing always lands on a
/// UTF-8 char boundary. Offsets past the end clamp to `s.len()`, and an empty
/// input maps every offset to 0. This is the char-boundary-safe replacement
/// for using character offsets directly as byte indices (which panics on
/// multi-byte UTF-8 like `─`).
fn char_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte, _)| byte)
        .unwrap_or(s.len())
}

/// Extracts a range of text from document content
pub fn extract_range(content: &str, range: Range) -> Result<String, Box<dyn Error + Send + Sync>> {
    let lines: Vec<&str> = content.split('\n').collect();
    let total_lines = lines.len();

    if total_lines == 0 {
        return Ok(String::new());
    }

    let mut start_line = range.start.line as usize;
    let mut end_line = range.end.line as usize;

    if start_line >= total_lines {
        warn!(
            "Start line exceeds total lines: {} >= {}, clamping to last line",
            start_line, total_lines
        );
        start_line = total_lines.saturating_sub(1);
    }

    if end_line >= total_lines {
        warn!(
            "End line exceeds total lines: {} >= {}, adjusting to include up to and including the last line",
            end_line, total_lines
        );
        end_line = total_lines.saturating_sub(1);
    }

    if start_line > end_line {
        warn!("Invalid range: start_line > end_line");
        return Ok(String::new());
    }

    let extracted: Vec<String> = lines[start_line..=end_line]
        .iter()
        .enumerate()
        .map(|(i, &line)| {
            let trimmed_line = line.trim_end_matches('\r');
            let line_len = trimmed_line.chars().count();
            let result = match (i, start_line == end_line) {
                (0, true) => {
                    let start_char = range.start.character.min(line_len as u32) as usize;
                    let end_char = range.end.character.min(line_len as u32) as usize;
                    let start_byte = char_byte_index(trimmed_line, start_char);
                    let end_byte = char_byte_index(trimmed_line, end_char);
                    trimmed_line.get(start_byte..end_byte).unwrap_or("")
                }
                (0, false) => {
                    let start_char = range.start.character.min(line_len as u32) as usize;
                    let start_byte = char_byte_index(trimmed_line, start_char);
                    trimmed_line.get(start_byte..).unwrap_or("")
                }
                (n, _) if n == end_line - start_line => {
                    let end_char = range.end.character.min(line_len as u32) as usize;
                    let end_byte = char_byte_index(trimmed_line, end_char);
                    trimmed_line.get(..end_byte).unwrap_or("")
                }
                _ => trimmed_line,
            };
            result.to_string()
        })
        .collect();

    log::debug!("Extracted range lines: {:?}", extracted);
    Ok(extracted.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;
    use rand::Rng;

    fn random_line_count() -> usize {
        rand::rng().random_range(5..50)
    }

    #[test]
    fn it_clamps_start_line_when_exceeding_total_lines_instead_of_returning_empty() {
        let content = "line 0\nline 1\nline 2";
        let _total_lines = 3; // Documenting expected line count
        let out_of_bounds_start = 47usize;

        let range = Range {
            start: Position {
                line: out_of_bounds_start as u32,
                character: 0,
            },
            end: Position {
                line: out_of_bounds_start as u32,
                character: 10,
            },
        };

        let result = extract_range(content, range).unwrap();

        assert!(
            !result.is_empty(),
            "negative: start_line exceeding total_lines should clamp to last line, not return empty"
        );
        assert!(
            result.contains("line 2"),
            "negative: when start_line exceeds bounds it should extract last available line but got: {}",
            result
        );
    }

    #[test]
    fn it_preserves_trailing_empty_lines_from_context() {
        let line_count = random_line_count();
        let mut lines_vec = Vec::new();
        for i in 0..line_count {
            lines_vec.push(format!("content line {}", i));
        }
        lines_vec.push(String::new());
        lines_vec.push(String::new());

        let content = lines_vec.join("\n");

        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: (lines_vec.len() - 1) as u32,
                character: 0,
            },
        };

        let result = extract_range(&content, range).unwrap();
        let result_line_count = result.split('\n').count();
        let expected_line_count = lines_vec.len();

        assert_eq!(
            result_line_count, expected_line_count,
            "negative: trailing empty lines must be preserved but expected {} lines and got {}",
            expected_line_count, result_line_count
        );
    }

    #[test]
    fn it_handles_windows_line_endings_without_leaving_carriage_returns() {
        let content = "line 0\r\nline 1\r\nline 2\r\n";

        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 2,
                character: 10,
            },
        };

        let result = extract_range(content, range).unwrap();

        assert!(
            !result.contains('\r'),
            "negative: carriage returns must be stripped from extracted content but found in: {:?}",
            result
        );
    }

    /// The exact production trigger: a line of 36 ASCII chars plus one `─`.
    /// Its character count is 37, but byte 37 sits inside the 3-byte `─`
    /// (bytes 36..39) — the old code sliced `[..37]` and panicked with
    /// "end byte index 37 is not a char boundary; it is inside '─'".
    #[test]
    fn it_extracts_the_exact_production_trigger_line() {
        let content = format!("{}─", "x".repeat(36));
        let full = extract_range(
            &content,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 37,
                },
            },
        )
        .unwrap();
        assert_eq!(full, content);

        // Same line, trailing content: char count 38 still falls inside `─`.
        let with_tail = format!("{}─y", "x".repeat(36));
        let full_tail = extract_range(
            &with_tail,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 38,
                },
            },
        )
        .unwrap();
        assert_eq!(full_tail, with_tail);
    }

    /// Regression: a single-line range whose end character lands after a
    /// multi-byte UTF-8 char (`─`, 3 bytes) used to panic with "end byte index
    /// N is not a char boundary" because character offsets were used as byte
    /// indices.
    #[test]
    fn it_extracts_single_line_ranges_past_multibyte_chars_without_panicking() {
        // "── step 2: hello" — two box-drawing chars up front.
        let content = "── step 2: hello";
        let line_chars = content.chars().count() as u32;

        // Full-line range: end character == line length, past the multi-byte chars.
        let full = extract_range(
            content,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: line_chars,
                },
            },
        )
        .unwrap();
        assert_eq!(full, "── step 2: hello");

        // End character exactly after the second `─` (char index 2).
        let prefix = extract_range(
            content,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 2,
                },
            },
        )
        .unwrap();
        assert_eq!(prefix, "──");

        // Start inside the multi-byte region: char index 1 is the second `─`.
        let tail = extract_range(
            content,
            Range {
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            },
        )
        .unwrap();
        assert_eq!(tail, "─ st");
    }

    /// Regression: when the range spans several lines, the *first* line's
    /// character offset must also be mapped to a byte boundary.
    #[test]
    fn it_extracts_multiline_ranges_starting_inside_a_multibyte_char_line() {
        let content = "── first\nsecond line";
        let result = extract_range(
            content,
            Range {
                start: Position {
                    line: 0,
                    character: 2,
                },
                end: Position {
                    line: 1,
                    character: 6,
                },
            },
        )
        .unwrap();
        // First line from char 2 (` `), whole second line's first 6 chars.
        assert_eq!(result, " first\nsecond");
    }

    /// Regression: the *last* line of a multi-line range with a multi-byte
    /// char before the end character also used to panic.
    #[test]
    fn it_extracts_multiline_ranges_ending_after_a_multibyte_char_line() {
        let content = "plain start\n── end";
        let result = extract_range(
            content,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 6,
                },
            },
        )
        .unwrap();
        assert_eq!(result, "plain start\n── end");
    }

    /// Four-byte UTF-8 (emoji) must slice on char boundaries too.
    #[test]
    fn it_extracts_emoji_on_character_boundaries() {
        let content = "a😀b";
        let result = extract_range(
            content,
            Range {
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 2,
                },
            },
        )
        .unwrap();
        assert_eq!(result, "😀");
    }

    /// ASCII behavior is unchanged: character and byte offsets coincide.
    #[test]
    fn it_extracts_ascii_exactly_as_before() {
        let content = "hello world";
        let result = extract_range(
            content,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            },
        )
        .unwrap();
        assert_eq!(result, "hello");
    }

    /// Character offsets past the end of the line clamp to the whole line.
    #[test]
    fn it_clamps_character_offsets_past_the_line_end() {
        let content = "── short";
        let result = extract_range(
            content,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 999,
                },
            },
        )
        .unwrap();
        assert_eq!(result, "── short");
    }
}
