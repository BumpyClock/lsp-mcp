// ABOUTME: Position and range types for text document locations.
// ABOUTME: Provides 1-indexed positions and ranges with LSP type conversions.

use serde::{Deserialize, Serialize};

/// A position within a text document, using 1-based indexing (matching editor display)
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Position {
    /// 1-indexed line number (first line is 1).
    pub line: u32,
    /// 1-indexed character/column within the line (first column is 1).
    pub character: u32,
}

/// A position within a specific file in the workspace
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct FilePosition {
    /// Path to the file, relative to the workspace root.
    /// Omitted when parent context provides the path (e.g., in McpSymbolsResponse).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// Position within the file
    pub position: Position,
}

/// A range within a specific file, defined by start and end positions
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct FileRange {
    /// The path to the file.
    /// Omitted when parent context provides the path (e.g., in McpSymbolsResponse).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// The range within the file
    pub range: Range,
}

impl FileRange {
    pub fn contains(&self, position: FilePosition) -> bool {
        let pos = &position.position;
        self.path == position.path
            && self.range.start.line <= pos.line
            && self.range.end.line >= pos.line
            && (self.range.start.line != pos.line || self.range.start.character <= pos.character)
            && (self.range.end.line != pos.line || self.range.end.character >= pos.character)
    }
}

impl From<FileRange> for lsp_types::Range {
    fn from(range: FileRange) -> Self {
        lsp_types::Range::new(
            lsp_types::Position::from(range.range.start),
            lsp_types::Position::from(range.range.end),
        )
    }
}

impl From<Position> for lsp_types::Position {
    fn from(position: Position) -> Self {
        lsp_types::Position {
            line: position.line.saturating_sub(1),
            character: position.character.saturating_sub(1),
        }
    }
}

impl From<lsp_types::Position> for Position {
    fn from(position: lsp_types::Position) -> Self {
        Position {
            line: position.line + 1,
            character: position.character + 1,
        }
    }
}

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Range {
    /// The start position of the range.
    pub start: Position,
    /// The end position of the range.
    pub end: Position,
}

impl From<lsp_types::Location> for FilePosition {
    fn from(location: lsp_types::Location) -> Self {
        use crate::utils::file_utils::uri_to_relative_path_string;
        FilePosition {
            path: uri_to_relative_path_string(&location.uri),
            position: Position {
                line: location.range.start.line + 1,
                character: location.range.start.character + 1,
            },
        }
    }
}

impl From<lsp_types::LocationLink> for FilePosition {
    fn from(link: lsp_types::LocationLink) -> Self {
        use crate::utils::file_utils::uri_to_relative_path_string;
        FilePosition {
            path: uri_to_relative_path_string(&link.target_uri),
            position: Position {
                line: link.target_range.start.line + 1,
                character: link.target_range.start.character + 1,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_multi_line_range() {
        let range = FileRange {
            path: "test.rs".to_string(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 12,
                    character: 10,
                },
            },
        };

        assert!(
            range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 11,
                    character: 0
                }
            }),
            "middle line should be contained"
        );
        assert!(
            range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 10,
                    character: 5
                }
            }),
            "start position should be contained"
        );
        assert!(
            range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 12,
                    character: 10
                }
            }),
            "end position should be contained"
        );
    }

    #[test]
    fn test_contains_multi_line_range_outside_positions() {
        let range = FileRange {
            path: "test.rs".to_string(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 12,
                    character: 10,
                },
            },
        };

        assert!(
            !range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 9,
                    character: 0
                }
            }),
            "line before start should not be contained"
        );
        assert!(
            !range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 13,
                    character: 0
                }
            }),
            "line after end should not be contained"
        );
        assert!(
            !range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 10,
                    character: 4
                }
            }),
            "position before start on first line should not be contained"
        );
        assert!(
            !range.contains(FilePosition {
                path: range.path.clone(),
                position: Position {
                    line: 12,
                    character: 11
                }
            }),
            "position after end on last line should not be contained"
        );
    }

    #[test]
    fn test_contains_single_line_range() {
        let single_line_range = FileRange {
            path: "test.rs".to_string(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 10,
                    character: 10,
                },
            },
        };

        assert!(
            single_line_range.contains(FilePosition {
                path: single_line_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 7
                }
            }),
            "position within single line range should be contained"
        );
        assert!(
            !single_line_range.contains(FilePosition {
                path: single_line_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 4
                }
            }),
            "position before single line range should not be contained"
        );
        assert!(
            !single_line_range.contains(FilePosition {
                path: single_line_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 11
                }
            }),
            "position after single line range should not be contained"
        );
    }

    #[test]
    fn test_contains_zero_width_range() {
        let zero_width_range = FileRange {
            path: "test.rs".to_string(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 10,
                    character: 5,
                },
            },
        };

        assert!(
            zero_width_range.contains(FilePosition {
                path: zero_width_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 5
                }
            }),
            "position at zero-width range should be contained"
        );
        assert!(
            !zero_width_range.contains(FilePosition {
                path: zero_width_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 4
                }
            }),
            "position before zero-width range should not be contained"
        );
        assert!(
            !zero_width_range.contains(FilePosition {
                path: zero_width_range.path.clone(),
                position: Position {
                    line: 10,
                    character: 6
                }
            }),
            "position after zero-width range should not be contained"
        );
    }
}
