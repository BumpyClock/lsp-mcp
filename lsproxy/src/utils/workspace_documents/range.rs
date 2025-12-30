// ABOUTME: Range extraction utilities for extracting text ranges from document content
// ABOUTME: Handles line and character-based range slicing with bounds checking

use log::warn;
use lsp_types::Range;
use std::error::Error;

/// Extracts a range of text from document content
pub fn extract_range(content: &str, range: Range) -> Result<String, Box<dyn Error + Send + Sync>> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    if total_lines == 0 {
        return Ok(String::new());
    }

    let start_line = range.start.line as usize;
    let mut end_line = range.end.line as usize;

    if end_line >= total_lines {
        warn!(
            "End line exceeds total lines: {} >= {}. Adjusting to include up to and including the last line.",
            end_line, total_lines
        );
        end_line = total_lines.saturating_sub(1);
    }

    if start_line > end_line {
        warn!("Invalid range: start_line > end_line");
        return Ok(String::new());
    }

    let extracted: Vec<&str> = lines[start_line..=end_line]
        .iter()
        .enumerate()
        .map(|(i, &line)| {
            let line_len = line.chars().count();
            match (i, start_line == end_line) {
                (0, true) => {
                    let start_char = range.start.character.min(line_len as u32) as usize;
                    let end_char = range.end.character.min(line_len as u32) as usize;
                    line[..line_len].get(start_char..end_char).unwrap_or("")
                }
                (0, false) => {
                    let start_char = range.start.character.min(line_len as u32) as usize;
                    line[..line_len].get(start_char..).unwrap_or("")
                }
                (n, _) if n == end_line - start_line => {
                    let end_char = range.end.character.min(line_len as u32) as usize;
                    line[..line_len].get(..end_char).unwrap_or("")
                }
                _ => line,
            }
        })
        .collect();

    log::debug!("Extracted range lines: {:?}", extracted);
    Ok(extracted.join("\n"))
}
