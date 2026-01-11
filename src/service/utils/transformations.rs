// ABOUTME: LSP type transformation utilities for service layer.
// ABOUTME: Converts LSP types to domain types for API responses.

use crate::api_types::{
    CallHierarchyItemInfo, CodeContext, FilePosition, HoverContents, Position, Range, Symbol,
    WorkspaceSymbolInfo,
};
use crate::mcp_response::normalize_kind;
use crate::service::types::response::{McpDefinitionLocation, McpReferenceLocation};
use crate::service::utils::external::ExternalInfo;
use crate::utils::file_utils::uri_to_relative_path_string;
use lsp_types::{GotoDefinitionResponse, Location, Range as LspRange};

pub(crate) fn range_from_lsp(range: &LspRange) -> Range {
    Range {
        start: Position {
            line: range.start.line + 1,
            character: range.start.character + 1,
        },
        end: Position {
            line: range.end.line + 1,
            character: range.end.character + 1,
        },
    }
}

pub(crate) fn definition_locations(definitions: &GotoDefinitionResponse) -> Vec<FilePosition> {
    match definitions {
        GotoDefinitionResponse::Scalar(location) => vec![FilePosition {
            path: uri_to_relative_path_string(&location.uri),
            position: Position {
                line: location.range.start.line + 1,
                character: location.range.start.character + 1,
            },
        }],
        GotoDefinitionResponse::Array(locations) => locations
            .iter()
            .map(|location| FilePosition {
                path: uri_to_relative_path_string(&location.uri),
                position: Position {
                    line: location.range.start.line + 1,
                    character: location.range.start.character + 1,
                },
            })
            .collect(),
        GotoDefinitionResponse::Link(links) => links
            .iter()
            .map(|link| FilePosition {
                path: uri_to_relative_path_string(&link.target_uri),
                position: Position {
                    line: link.target_selection_range.start.line + 1,
                    character: link.target_selection_range.start.character + 1,
                },
            })
            .collect(),
    }
}

pub(crate) fn definition_locations_lsp(definitions: &GotoDefinitionResponse) -> Vec<Location> {
    match definitions {
        GotoDefinitionResponse::Scalar(location) => vec![location.clone()],
        GotoDefinitionResponse::Array(locations) => locations.clone(),
        GotoDefinitionResponse::Link(links) => links
            .iter()
            .map(|link| Location::new(link.target_uri.clone(), link.target_selection_range))
            .collect(),
    }
}

pub(crate) fn definition_item_from_location(
    location: &Location,
    symbol: Option<Symbol>,
    snippet: Option<CodeContext>,
    signature: Option<String>,
    doc: Option<String>,
    reference_count: Option<u32>,
) -> McpDefinitionLocation {
    let path = uri_to_relative_path_string(&location.uri);
    let position = Position {
        line: location.range.start.line + 1,
        character: location.range.start.character + 1,
    };
    let (definition_range, symbol_kind) = match &symbol {
        Some(symbol) => (symbol.file_range.range.clone(), Some(symbol.kind.clone())),
        None => (range_from_lsp(&location.range), None),
    };

    let external_info = ExternalInfo::from_path(&path);
    let (external, package) = match external_info {
        Some(info) => (Some(info.external), info.package),
        None => (None, None),
    };

    McpDefinitionLocation {
        path,
        position,
        definition_range,
        symbol_kind,
        snippet,
        signature,
        doc,
        external,
        package,
        reference_count,
    }
}

pub(crate) fn reference_item_from_location(
    location: &Location,
    snippet: Option<CodeContext>,
    reference_type: crate::service::types::response::ReferenceType,
) -> McpReferenceLocation {
    let path = uri_to_relative_path_string(&location.uri);
    let position = Position {
        line: location.range.start.line + 1,
        character: location.range.start.character + 1,
    };
    McpReferenceLocation {
        path: Some(path),
        position,
        symbol_range: range_from_lsp(&location.range),
        snippet,
        reference_type,
    }
}

pub(crate) fn workspace_symbol_info_from_lsp(
    sym: lsp_types::SymbolInformation,
    path: String,
) -> WorkspaceSymbolInfo {
    WorkspaceSymbolInfo {
        name: sym.name,
        kind: normalize_kind(&format!("{:?}", sym.kind)),
        location: FilePosition {
            path,
            position: Position {
                line: sym.location.range.start.line + 1,
                character: sym.location.range.start.character + 1,
            },
        },
        container_name: sym.container_name,
        match_kind: None,
        match_score: None,
        signature: None,
        snippet: None,
    }
}

pub(crate) fn call_hierarchy_item_to_info(
    item: &lsp_types::CallHierarchyItem,
) -> CallHierarchyItemInfo {
    CallHierarchyItemInfo {
        name: item.name.clone(),
        kind: normalize_kind(&format!("{:?}", item.kind)),
        location: FilePosition {
            path: uri_to_relative_path_string(&item.uri),
            position: Position {
                line: item.selection_range.start.line + 1,
                character: item.selection_range.start.character + 1,
            },
        },
        range: Range {
            start: Position {
                line: item.range.start.line + 1,
                character: item.range.start.character + 1,
            },
            end: Position {
                line: item.range.end.line + 1,
                character: item.range.end.character + 1,
            },
        },
        detail: item.detail.clone(),
        external: None,
    }
}

pub(crate) fn extract_hover_contents(contents: &lsp_types::HoverContents) -> HoverContents {
    match contents {
        lsp_types::HoverContents::Scalar(marked) => {
            HoverContents::Markup(extract_marked_string(marked))
        }
        lsp_types::HoverContents::Array(arr) => {
            HoverContents::Array(arr.iter().map(extract_marked_string).collect())
        }
        lsp_types::HoverContents::Markup(markup) => HoverContents::Markup(markup.value.clone()),
    }
}

pub(crate) fn extract_marked_string(marked: &lsp_types::MarkedString) -> String {
    match marked {
        lsp_types::MarkedString::String(s) => s.clone(),
        lsp_types::MarkedString::LanguageString(ls) => {
            format!("```{}\n{}\n```", ls.language, ls.value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{
        CallHierarchyItem, Location, LocationLink, Position as LspPosition, Range as LspRange,
        SymbolInformation, SymbolKind, Url,
    };

    #[allow(deprecated)]
    #[test]
    fn test_workspace_symbol_info_kind_normalized() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let range = LspRange {
            start: LspPosition {
                line: 1,
                character: 0,
            },
            end: LspPosition {
                line: 1,
                character: 4,
            },
        };
        let sym = SymbolInformation {
            name: "Example".to_string(),
            kind: SymbolKind::ENUM_MEMBER,
            tags: None,
            deprecated: None,
            location: Location { uri, range },
            container_name: None,
        };

        let info = workspace_symbol_info_from_lsp(sym, "src/main.rs".to_string());

        assert_eq!(info.kind, "enum-member");
        assert_eq!(info.location.path, "src/main.rs");
    }

    #[test]
    fn test_call_hierarchy_kind_normalized() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let range = LspRange {
            start: LspPosition {
                line: 2,
                character: 1,
            },
            end: LspPosition {
                line: 2,
                character: 6,
            },
        };
        let item = CallHierarchyItem {
            name: "Thing".to_string(),
            kind: SymbolKind::TYPE_PARAMETER,
            tags: None,
            detail: None,
            uri,
            range: range.clone(),
            selection_range: range,
            data: None,
        };

        let info = call_hierarchy_item_to_info(&item);

        assert_eq!(info.kind, "type-parameter");
    }

    #[test]
    fn test_definition_locations_uses_selection_range_for_links() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let target_range = LspRange {
            start: LspPosition {
                line: 10,
                character: 1,
            },
            end: LspPosition {
                line: 12,
                character: 1,
            },
        };
        let selection_range = LspRange {
            start: LspPosition {
                line: 20,
                character: 2,
            },
            end: LspPosition {
                line: 20,
                character: 8,
            },
        };
        let link = LocationLink {
            origin_selection_range: None,
            target_uri: uri.clone(),
            target_range,
            target_selection_range: selection_range,
        };

        let locations = definition_locations(&GotoDefinitionResponse::Link(vec![link]));

        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].position.line, 21);
        assert_eq!(locations[0].position.character, 3);
    }

    #[test]
    fn test_definition_locations_lsp_uses_selection_range_for_links() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let target_range = LspRange {
            start: LspPosition {
                line: 3,
                character: 1,
            },
            end: LspPosition {
                line: 4,
                character: 1,
            },
        };
        let selection_range = LspRange {
            start: LspPosition {
                line: 8,
                character: 4,
            },
            end: LspPosition {
                line: 8,
                character: 9,
            },
        };
        let link = LocationLink {
            origin_selection_range: None,
            target_uri: uri.clone(),
            target_range,
            target_selection_range: selection_range,
        };

        let locations = definition_locations_lsp(&GotoDefinitionResponse::Link(vec![link]));

        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.line, 8);
        assert_eq!(locations[0].range.start.character, 4);
    }
}
