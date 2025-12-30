use std::io::{Error, ErrorKind};
use tokio::process::Command;

const SYMBOL_CONFIG_PATH: &str = "/usr/src/ast_grep/symbol/config.yml";
const IDENTIFIER_CONFIG_PATH: &str = "/usr/src/ast_grep/identifier/config.yml";
const REFERENCE_CONFIG_PATH: &str = "/usr/src/ast_grep/reference/config.yml";

use super::types::{AstGrepMatch, AstGrepRange};

pub struct AstGrepClient;

impl AstGrepClient {
    pub async fn get_symbol_match_from_position(
        &self,
        file_name: &str,
        identifier_position: &lsp_types::Position,
    ) -> Result<AstGrepMatch, Box<dyn std::error::Error>> {
        // Get all symbols in the file
        let file_symbols = self.scan_file(SYMBOL_CONFIG_PATH, file_name).await?;
        // Select the best match by context containment or nearest fallback
        match select_symbol_match(file_symbols, identifier_position) {
            Some(matched_symbol) => Ok(matched_symbol),
            None => Err(Box::new(Error::new(
                ErrorKind::NotFound,
                "No symbol found for position",
            ))),
        }
    }

    pub async fn get_file_symbols(
        &self,
        file_name: &str,
    ) -> Result<Vec<AstGrepMatch>, Box<dyn std::error::Error>> {
        self.scan_file(SYMBOL_CONFIG_PATH, file_name).await
    }

    pub async fn get_file_identifiers(
        &self,
        file_name: &str,
    ) -> Result<Vec<AstGrepMatch>, Box<dyn std::error::Error>> {
        self.scan_file(IDENTIFIER_CONFIG_PATH, file_name).await
    }

    pub async fn get_symbol_and_references(
        &self,
        file_name: &str,
        position: &lsp_types::Position,
        full_scan: bool,
    ) -> Result<(AstGrepMatch, Vec<AstGrepMatch>), Box<dyn std::error::Error>> {
        let symbol_match = self
            .get_symbol_match_from_position(file_name, position)
            .await?;
        let references = self
            .get_references_contained_in_symbol_match(file_name, &symbol_match, full_scan)
            .await?;
        Ok((symbol_match, references))
    }

    pub async fn get_references_contained_in_symbol_match(
        &self,
        file_name: &str,
        symbol_match: &AstGrepMatch,
        full_scan: bool,
    ) -> Result<Vec<AstGrepMatch>, Box<dyn std::error::Error>> {
        // Get all references
        let matches = self.scan_file(REFERENCE_CONFIG_PATH, file_name).await?;

        // Filter matches to those within the symbol's range
        // And if not full_scan, exclude matches with rule_id "non-function"
        let contained_references = matches
            .into_iter()
            .filter(|m| {
                let contained = symbol_match.contains(m);
                let all_ref = m.rule_id == "all-references";

                // If we're doing a full scan, we want to use the more permissive "all-references"
                // rule, whereas if we're not doing a full scan, we just want to use the targeted
                // rules
                contained && ((full_scan && all_ref) || (!full_scan && !all_ref))
            })
            .collect();

        Ok(contained_references)
    }

    async fn scan_file(
        &self,
        config_path: &str,
        file_name: &str,
    ) -> Result<Vec<AstGrepMatch>, Box<dyn std::error::Error>> {
        let command_result = Command::new("ast-grep")
            .arg("scan")
            .arg("--config")
            .arg(config_path)
            .arg("--json")
            .arg(file_name)
            .output()
            .await?;

        if !command_result.status.success() {
            let error = String::from_utf8_lossy(&command_result.stderr);
            return Err(format!("sg command failed: {}", error).into());
        }

        let output = String::from_utf8(command_result.stdout)?;

        let mut symbols: Vec<AstGrepMatch> =
            serde_json::from_str(&output).map_err(|e| format!("Failed to parse JSON: {}", e))?;
        symbols = symbols.into_iter().collect();
        symbols.sort_by_key(|s| s.get_identifier_range().start.line);
        Ok(symbols)
    }
}

fn select_symbol_match(
    file_symbols: Vec<AstGrepMatch>,
    position: &lsp_types::Position,
) -> Option<AstGrepMatch> {
    let mut containing = Vec::new();
    let mut nearest: Option<(AstGrepMatch, u64, u64)> = None;

    for symbol in file_symbols {
        let context_range = symbol.get_context_range();
        if range_contains_position(&context_range, position) {
            containing.push(symbol);
            continue;
        }

        let distance = range_distance(&context_range, position);
        let span_score = range_span_score(&context_range);
        match &nearest {
            None => nearest = Some((symbol, distance, span_score)),
            Some((_, best_distance, best_span)) => {
                if distance < *best_distance
                    || (distance == *best_distance && span_score < *best_span)
                {
                    nearest = Some((symbol, distance, span_score));
                }
            }
        }
    }

    if !containing.is_empty() {
        containing.sort_by_key(|symbol| range_span_score(&symbol.get_context_range()));
        return containing.into_iter().next();
    }

    nearest.map(|(symbol, _, _)| symbol)
}

fn range_contains_position(range: &AstGrepRange, position: &lsp_types::Position) -> bool {
    let start = &range.start;
    let end = &range.end;

    (start.line < position.line
        || (start.line == position.line && start.column <= position.character))
        && (end.line > position.line || (end.line == position.line && end.column >= position.character))
}

fn range_span_score(range: &AstGrepRange) -> u64 {
    let line_span = range.end.line.saturating_sub(range.start.line) as u64;
    let col_span = range.end.column.saturating_sub(range.start.column) as u64;
    line_span * 1_000_000 + col_span
}

fn range_distance(range: &AstGrepRange, position: &lsp_types::Position) -> u64 {
    let start = &range.start;
    let end = &range.end;

    if position.line < start.line {
        let line = (start.line - position.line) as u64;
        let col = if position.line == start.line {
            start.column.saturating_sub(position.character) as u64
        } else {
            start.column as u64
        };
        return line * 1_000_000 + col;
    }

    if position.line > end.line {
        let line = (position.line - end.line) as u64;
        let col = if position.line == end.line {
            position.character.saturating_sub(end.column) as u64
        } else {
            position.character as u64
        };
        return line * 1_000_000 + col;
    }

    if position.character < start.column {
        return (start.column - position.character) as u64;
    }

    if position.character > end.column {
        return (position.character - end.column) as u64;
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_grep::types::{
        AstGrepPosition, ByteOffset, CharCount, MetaVariable, MetaVariables, MultiVariables,
        SingleVariable,
    };

    fn make_range(start_line: u32, start_col: u32, end_line: u32, end_col: u32) -> AstGrepRange {
        AstGrepRange {
            byte_offset: ByteOffset { start: 0, end: 0 },
            start: AstGrepPosition {
                line: start_line,
                column: start_col,
            },
            end: AstGrepPosition {
                line: end_line,
                column: end_col,
            },
        }
    }

    fn make_match(
        name: &str,
        name_range: (u32, u32, u32, u32),
        context_range: Option<(u32, u32, u32, u32)>,
    ) -> AstGrepMatch {
        let name_range = make_range(
            name_range.0,
            name_range.1,
            name_range.2,
            name_range.3,
        );
        let context_range = context_range.map(|range| {
            make_range(range.0, range.1, range.2, range.3)
        });
        let context_var = context_range.clone().map(|range| MetaVariable {
            text: "context".to_string(),
            range,
        });

        AstGrepMatch {
            text: name.to_string(),
            range: context_range.clone().unwrap_or_else(|| name_range.clone()),
            file: "test.rs".to_string(),
            lines: String::new(),
            char_count: CharCount {
                leading: 0,
                trailing: 0,
            },
            language: "rust".to_string(),
            meta_variables: MetaVariables {
                single: SingleVariable {
                    name: MetaVariable {
                        text: name.to_string(),
                        range: name_range,
                    },
                    context: context_var,
                },
                multi: MultiVariables { secondary: None },
            },
            rule_id: "function".to_string(),
            labels: None,
        }
    }

    #[test]
    fn select_symbol_match_prefers_smallest_container() {
        let outer = make_match("outer", (1, 0, 1, 5), Some((1, 0, 10, 0)));
        let inner = make_match("inner", (3, 0, 3, 5), Some((3, 0, 4, 0)));
        let position = lsp_types::Position {
            line: 3,
            character: 2,
        };

        let selected = select_symbol_match(vec![outer, inner], &position)
            .expect("expected match");

        assert_eq!(selected.meta_variables.single.name.text, "inner");
    }

    #[test]
    fn select_symbol_match_uses_context_range() {
        let symbol = make_match("symbol", (1, 0, 1, 5), Some((1, 0, 5, 0)));
        let position = lsp_types::Position {
            line: 3,
            character: 4,
        };

        let selected = select_symbol_match(vec![symbol], &position)
            .expect("expected match");

        assert_eq!(selected.meta_variables.single.name.text, "symbol");
    }

    #[tokio::test]
    async fn test_references() -> Result<(), Box<dyn std::error::Error>> {
        let client = AstGrepClient {};

        let path = "/mnt/lsproxy_root/sample_project/python/graph.py";
        let position = lsp_types::Position {
            line: 12,
            character: 6,
        };

        let symbol_match = client
            .get_symbol_match_from_position(path, &position)
            .await?;
        let references = client
            .get_references_contained_in_symbol_match(path, &symbol_match, false)
            .await?;
        let match_positions: Vec<lsp_types::Position> =
            references.iter().map(lsp_types::Position::from).collect();
        let expected = vec![
            lsp_types::Position {
                line: 15,
                character: 23,
            },
            lsp_types::Position {
                line: 22,
                character: 5,
            },
            lsp_types::Position {
                line: 35,
                character: 15,
            },
            lsp_types::Position {
                line: 35,
                character: 34,
            },
            lsp_types::Position {
                line: 39,
                character: 28,
            },
            lsp_types::Position {
                line: 40,
                character: 29,
            },
            lsp_types::Position {
                line: 63,
                character: 18,
            },
            lsp_types::Position {
                line: 65,
                character: 15,
            },
            lsp_types::Position {
                line: 67,
                character: 5,
            },
            lsp_types::Position {
                line: 71,
                character: 13,
            },
            lsp_types::Position {
                line: 72,
                character: 13,
            },
            lsp_types::Position {
                line: 73,
                character: 46,
            },
            lsp_types::Position {
                line: 75,
                character: 5,
            },
            lsp_types::Position {
                line: 86,
                character: 20,
            },
            lsp_types::Position {
                line: 87,
                character: 18,
            },
        ];
        assert_eq!(match_positions, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_contained_references() -> Result<(), Box<dyn std::error::Error>> {
        let client = AstGrepClient {};

        let path = "/mnt/lsproxy_root/sample_project/python/main.py";
        let position = lsp_types::Position {
            line: 14,
            character: 4,
        };

        let symbol_match = client
            .get_symbol_match_from_position(path, &position)
            .await?;
        let references = client
            .get_references_contained_in_symbol_match(path, &symbol_match, false)
            .await
            .unwrap();
        let match_positions: Vec<lsp_types::Position> = references
            .iter()
            .map(|ast_match: &AstGrepMatch| lsp_types::Position {
                line: ast_match.get_identifier_range().start.line,
                character: ast_match.get_identifier_range().start.column,
            })
            .collect();
        let expected = vec![
            lsp_types::Position {
                line: 15,
                character: 12,
            },
            lsp_types::Position {
                line: 16,
                character: 19,
            },
            lsp_types::Position {
                line: 17,
                character: 4,
            },
            lsp_types::Position {
                line: 18,
                character: 4,
            },
            lsp_types::Position {
                line: 19,
                character: 4,
            },
        ];
        assert_eq!(match_positions, expected);
        Ok(())
    }
}
