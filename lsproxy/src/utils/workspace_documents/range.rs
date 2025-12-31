// ABOUTME: Range extraction utilities for extracting text ranges from document content
// ABOUTME: Handles line and character-based range slicing with bounds checking

use log::warn;
use lsp_types::Range;
use std::error::Error;

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
                    trimmed_line[..line_len].get(start_char..end_char).unwrap_or("")
                }
                (0, false) => {
                    let start_char = range.start.character.min(line_len as u32) as usize;
                    trimmed_line[..line_len].get(start_char..).unwrap_or("")
                }
                (n, _) if n == end_line - start_line => {
                    let end_char = range.end.character.min(line_len as u32) as usize;
                    trimmed_line[..line_len].get(..end_char).unwrap_or("")
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
        let total_lines = 3;
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
}
