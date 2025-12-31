use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    api_types::{FilePosition, FileRange, Identifier, Position, Range, Symbol},
    utils::file_utils::absolute_path_to_relative_path_string,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AstGrepMatch {
    pub text: String,
    pub range: AstGrepRange,
    pub file: String,
    pub lines: String,
    pub char_count: CharCount,
    pub language: String,
    pub meta_variables: MetaVariables,
    pub rule_id: String,
    pub labels: Option<Vec<Label>>,
}

impl AstGrepMatch {
    pub fn get_source_code(&self) -> String {
        if let Some(context) = &self.meta_variables.single.context {
            context.text.clone()
        } else {
            self.text.clone()
        }
    }

    pub fn get_context_range(&self) -> AstGrepRange {
        if let Some(context) = &self.meta_variables.single.context {
            context.range.clone()
        } else {
            self.range.clone()
        }
    }

    pub fn get_identifier_range(&self) -> AstGrepRange {
        self.meta_variables.single.name.range.clone()
    }

    pub fn contains(&self, other: &AstGrepMatch) -> bool {
        self.file == other.file
            && self.get_context_range().start.line <= other.get_context_range().start.line
            && self.get_context_range().end.line >= other.get_context_range().end.line
            && (self.get_context_range().start.line != other.get_context_range().start.line
                || self.get_context_range().start.column <= other.get_context_range().start.column)
            && (self.get_context_range().end.line != other.get_context_range().end.line
                || self.get_context_range().end.column >= other.get_context_range().end.column)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AstGrepRange {
    pub byte_offset: ByteOffset,
    pub start: AstGrepPosition,
    pub end: AstGrepPosition,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ByteOffset {
    pub start: usize,
    pub end: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AstGrepPosition {
    pub line: u32,
    pub column: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CharCount {
    pub leading: usize,
    pub trailing: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MetaVariables {
    pub single: SingleVariable,
    pub multi: MultiVariables,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SingleVariable {
    #[serde(rename = "NAME")]
    pub name: MetaVariable,
    #[serde(rename = "CONTEXT")]
    pub context: Option<MetaVariable>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiVariables {
    pub secondary: Option<Vec<MetaVariable>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MetaVariable {
    pub text: String,
    pub range: AstGrepRange,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub text: String,
    pub range: AstGrepRange,
}

impl From<&AstGrepMatch> for lsp_types::Position {
    fn from(ast_match: &AstGrepMatch) -> Self {
        Self {
            line: ast_match.range.start.line,
            character: ast_match.range.start.column,
        }
    }
}

impl From<AstGrepMatch> for Symbol {
    fn from(ast_match: AstGrepMatch) -> Self {
        assert!(ast_match.rule_id != "all-identifiers");
        let path = absolute_path_to_relative_path_string(&PathBuf::from(ast_match.file.clone()));
        let match_range = ast_match.get_context_range();
        Symbol {
            name: ast_match.meta_variables.single.name.text.clone(),
            kind: ast_match.rule_id.clone(),
            identifier_position: FilePosition {
                path: path.clone(),
                position: Position {
                    line: ast_match.range.start.line + 1,
                    character: ast_match.range.start.column + 1,
                },
            },
            file_range: FileRange {
                path: path.clone(),
                range: Range {
                    start: Position {
                        line: match_range.start.line + 1,
                        character: 1, // Returning the whole line for consistency
                    },
                    end: Position {
                        line: match_range.end.line + 1,
                        character: match_range.end.column + 1,
                    },
                },
            },
            signature: None,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
            children: None,
            snippet: None,
        }
    }
}

impl From<AstGrepMatch> for Identifier {
    fn from(ast_match: AstGrepMatch) -> Self {
        let path = absolute_path_to_relative_path_string(&PathBuf::from(ast_match.file.clone()));
        let match_range = ast_match.get_context_range();
        let kind = match ast_match.rule_id.as_str() {
            "all-identifiers" => None,
            "component-render" => Some("jsx-element".to_string()),
            _ => Some(ast_match.rule_id),
        };

        Identifier {
            name: ast_match.meta_variables.single.name.text.clone(),
            kind,
            file_range: FileRange {
                path: path.clone(),
                range: Range {
                    start: Position {
                        line: match_range.start.line + 1,
                        character: match_range.start.column + 1,
                    },
                    end: Position {
                        line: match_range.end.line + 1,
                        character: match_range.end.column + 1,
                    },
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_from_ast_match_maps_component_render_to_jsx_element() {
        let ast_match = AstGrepMatch {
            text: "<Button />".to_string(),
            range: AstGrepRange {
                byte_offset: ByteOffset { start: 0, end: 10 },
                start: AstGrepPosition { line: 5, column: 3 },
                end: AstGrepPosition { line: 5, column: 13 },
            },
            file: "/tmp/App.tsx".to_string(),
            lines: "<Button />".to_string(),
            char_count: CharCount { leading: 0, trailing: 0 },
            language: "tsx".to_string(),
            meta_variables: MetaVariables {
                single: SingleVariable {
                    name: MetaVariable {
                        text: "Button".to_string(),
                        range: AstGrepRange {
                            byte_offset: ByteOffset { start: 1, end: 7 },
                            start: AstGrepPosition { line: 5, column: 4 },
                            end: AstGrepPosition { line: 5, column: 10 },
                        },
                    },
                    context: None,
                },
                multi: MultiVariables { secondary: None },
            },
            rule_id: "component-render".to_string(),
            labels: None,
        };

        let identifier: Identifier = ast_match.into();

        assert_eq!(
            identifier.kind,
            Some("jsx-element".to_string()),
            "negative: component-render must map to jsx-element"
        );
        assert_eq!(
            identifier.name, "Button",
            "negative: identifier name must be preserved"
        );
    }

    #[test]
    fn identifier_from_ast_match_preserves_other_rule_ids() {
        let ast_match = AstGrepMatch {
            text: "function example".to_string(),
            range: AstGrepRange {
                byte_offset: ByteOffset { start: 0, end: 16 },
                start: AstGrepPosition { line: 2, column: 0 },
                end: AstGrepPosition { line: 2, column: 16 },
            },
            file: "/tmp/utils.ts".to_string(),
            lines: "function example".to_string(),
            char_count: CharCount { leading: 0, trailing: 0 },
            language: "typescript".to_string(),
            meta_variables: MetaVariables {
                single: SingleVariable {
                    name: MetaVariable {
                        text: "example".to_string(),
                        range: AstGrepRange {
                            byte_offset: ByteOffset { start: 9, end: 16 },
                            start: AstGrepPosition { line: 2, column: 9 },
                            end: AstGrepPosition { line: 2, column: 16 },
                        },
                    },
                    context: None,
                },
                multi: MultiVariables { secondary: None },
            },
            rule_id: "exported-function".to_string(),
            labels: None,
        };

        let identifier: Identifier = ast_match.into();

        assert_eq!(
            identifier.kind,
            Some("exported-function".to_string()),
            "negative: other rule IDs must be preserved unchanged"
        );
    }

    #[test]
    fn identifier_from_ast_match_handles_all_identifiers_special_case() {
        let ast_match = AstGrepMatch {
            text: "someVar".to_string(),
            range: AstGrepRange {
                byte_offset: ByteOffset { start: 0, end: 7 },
                start: AstGrepPosition { line: 10, column: 5 },
                end: AstGrepPosition { line: 10, column: 12 },
            },
            file: "/tmp/vars.ts".to_string(),
            lines: "someVar".to_string(),
            char_count: CharCount { leading: 0, trailing: 0 },
            language: "typescript".to_string(),
            meta_variables: MetaVariables {
                single: SingleVariable {
                    name: MetaVariable {
                        text: "someVar".to_string(),
                        range: AstGrepRange {
                            byte_offset: ByteOffset { start: 0, end: 7 },
                            start: AstGrepPosition { line: 10, column: 5 },
                            end: AstGrepPosition { line: 10, column: 12 },
                        },
                    },
                    context: None,
                },
                multi: MultiVariables { secondary: None },
            },
            rule_id: "all-identifiers".to_string(),
            labels: None,
        };

        let identifier: Identifier = ast_match.into();

        assert!(
            identifier.kind.is_none(),
            "negative: all-identifiers must have None kind"
        );
    }
}
