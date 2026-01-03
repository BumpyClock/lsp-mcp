// ABOUTME: Markdown formatting for codemap responses.
// ABOUTME: Formats overview, impact, and context query results for LLM consumption.

use crate::codemap::types::{CodemapResponse, Edge, EdgeKind, Node, QueryMode};

/// Format a codemap response as markdown
pub fn format_codemap_response(response: &CodemapResponse) -> String {
    match response.mode {
        QueryMode::Overview => format_overview(response),
        QueryMode::Impact => format_impact(response),
        QueryMode::Context => format_context(response),
    }
}

fn format_overview(response: &CodemapResponse) -> String {
    let mut output = String::new();

    // Header with counts
    output.push_str(&format!(
        "Codebase Map ({} files, {} symbols)\n\n",
        response.file_count, response.symbol_count
    ));

    // Modules section
    if !response.modules.is_empty() {
        output.push_str(&format!("Modules ({})\n", response.modules.len()));
        for module in &response.modules {
            output.push_str(&format!(
                "  {} — {} files, {} symbols\n",
                module.path, module.file_count, module.symbol_count
            ));
        }
        output.push('\n');
    }

    // Key symbols section
    if !response.symbols.is_empty() {
        output.push_str(&format!("Key Symbols (top {})\n", response.symbols.len()));
        for symbol in &response.symbols {
            output.push_str(&format!(
                "  {} ({}) — {}:{} — {} refs\n",
                symbol.name,
                format_symbol_kind(&symbol.kind),
                symbol.location.path,
                symbol.location.position.line,
                symbol.reference_count
            ));
        }
        output.push('\n');
    }

    // Truncation notice
    if response.truncated {
        output.push_str(&format!(
            "[Showing {} of {} — use limit/offset for more]\n",
            response.symbols.len(),
            response.symbol_count
        ));
    }

    output
}

fn format_impact(response: &CodemapResponse) -> String {
    let mut output = String::new();

    // Header
    let target = response.target.as_deref().unwrap_or("unknown");
    output.push_str(&format!("Impact Analysis for `{}`\n\n", target));

    // Group edges by type
    let mut imports_count = 0;
    let mut calls_count = 0;
    let mut defines_count = 0;

    for edge in &response.edges {
        match edge.edge_kind() {
            EdgeKind::Imports => imports_count += 1,
            EdgeKind::Calls => calls_count += 1,
            EdgeKind::Defines => defines_count += 1,
        }
    }

    // Direct dependents
    output.push_str(&format!(
        "Direct Dependents ({} edges)\n",
        response.edges.len()
    ));

    // Group by file for readability
    let mut by_file: std::collections::HashMap<String, Vec<&Edge>> = std::collections::HashMap::new();
    for edge in &response.edges {
        if let Some(node) = response.nodes.iter().find(|n| n.id() == edge.from_node_id()) {
            let path = match node {
                Node::File(f) => f.path.clone(),
                Node::Symbol(s) => s.location.path.clone(),
                Node::Module(m) => m.path.clone(),
            };
            by_file.entry(path).or_default().push(edge);
        }
    }

    for (path, edges) in by_file.iter().take(10) {
        output.push_str(&format!("  {}\n", path));
        for edge in edges.iter().take(3) {
            output.push_str(&format!("    {} edge\n", format_edge_kind(&edge.edge_kind())));
        }
        if edges.len() > 3 {
            output.push_str(&format!("    ... and {} more\n", edges.len() - 3));
        }
    }
    output.push('\n');

    // By edge type summary
    output.push_str("By Edge Type\n");
    if imports_count > 0 {
        output.push_str(&format!("  imports: {}\n", imports_count));
    }
    if calls_count > 0 {
        output.push_str(&format!("  calls: {}\n", calls_count));
    }
    if defines_count > 0 {
        output.push_str(&format!("  defines: {}\n", defines_count));
    }
    output.push('\n');

    if response.truncated {
        output.push_str("[Results truncated — use detail=full for complete listing]\n");
    }

    output
}

fn format_context(response: &CodemapResponse) -> String {
    let mut output = String::new();

    // Header
    let target = response.target.as_deref().unwrap_or("unknown");
    output.push_str(&format!("Context for `{}`\n\n", target));

    // Find the target symbol in nodes
    if let Some(target_node) = response.nodes.first() {
        match target_node {
            Node::Symbol(s) => {
                output.push_str("Symbol Info\n");
                output.push_str(&format!("  Kind: {}\n", format_symbol_kind(&s.kind)));
                output.push_str(&format!(
                    "  Location: {}:{}\n",
                    s.location.path, s.location.position.line
                ));
                if let Some(sig) = &s.signature {
                    output.push_str(&format!("  Signature: {}\n", sig));
                }
                if let Some(container) = &s.container_name {
                    output.push_str(&format!("  Container: {}\n", container));
                }
                output.push('\n');
            }
            Node::File(f) => {
                output.push_str("File Info\n");
                output.push_str(&format!("  Path: {}\n", f.path));
                output.push_str(&format!("  Language: {}\n", f.language));
                output.push_str(&format!("  Lines: {}\n", f.line_count));
                output.push('\n');
            }
            Node::Module(m) => {
                output.push_str("Module Info\n");
                output.push_str(&format!("  Name: {}\n", m.name));
                output.push_str(&format!("  Path: {}\n", m.path));
                output.push('\n');
            }
        }
    }

    // Incoming edges
    let incoming: Vec<_> = response
        .edges
        .iter()
        .filter(|e| {
            response
                .nodes
                .first()
                .map(|n| e.to_node_id() == n.id())
                .unwrap_or(false)
        })
        .collect();

    if !incoming.is_empty() {
        output.push_str(&format!("Incoming ({})\n", incoming.len()));
        for edge in incoming.iter().take(10) {
            let from_name = response
                .nodes
                .iter()
                .find(|n| n.id() == edge.from_node_id())
                .map(|n| get_node_name(n))
                .unwrap_or_else(|| "unknown".to_string());
            output.push_str(&format!(
                "  {} — {}\n",
                from_name,
                format_edge_kind(&edge.edge_kind())
            ));
        }
        output.push('\n');
    }

    // Outgoing edges
    let outgoing: Vec<_> = response
        .edges
        .iter()
        .filter(|e| {
            response
                .nodes
                .first()
                .map(|n| e.from_node_id() == n.id())
                .unwrap_or(false)
        })
        .collect();

    if !outgoing.is_empty() {
        output.push_str(&format!("Outgoing ({})\n", outgoing.len()));
        for edge in outgoing.iter().take(10) {
            let to_name = response
                .nodes
                .iter()
                .find(|n| n.id() == edge.to_node_id())
                .map(|n| get_node_name(n))
                .unwrap_or_else(|| "unknown".to_string());
            output.push_str(&format!(
                "  {} — {}\n",
                to_name,
                format_edge_kind(&edge.edge_kind())
            ));
        }
        output.push('\n');
    }

    if response.truncated {
        output.push_str("[Results truncated — use detail=full for complete listing]\n");
    }

    output
}

fn format_symbol_kind(kind: &crate::codemap::types::SymbolKind) -> &'static str {
    use crate::codemap::types::SymbolKind;
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Class => "class",
        SymbolKind::Interface => "interface",
        SymbolKind::Trait => "trait",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::EnumVariant => "enum variant",
        SymbolKind::Type => "type",
        SymbolKind::TypeAlias => "type alias",
        SymbolKind::Field => "field",
        SymbolKind::Property => "property",
        SymbolKind::Variable => "variable",
        SymbolKind::Constant => "constant",
        SymbolKind::Module => "module",
        SymbolKind::Namespace => "namespace",
        SymbolKind::Unknown => "unknown",
    }
}

fn format_edge_kind(kind: &EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Defines => "defines",
        EdgeKind::Imports => "imports",
        EdgeKind::Calls => "calls",
    }
}

fn get_node_name(node: &Node) -> String {
    match node {
        Node::Symbol(s) => s.name.clone(),
        Node::File(f) => f.path.clone(),
        Node::Module(m) => m.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{FilePosition, Position};
    use crate::codemap::types::{
        CallsEdge, CodemapResponse, Edge, EdgeId, EdgeMetadata, FileNode, ImportsEdge, ModuleNode,
        ModuleSummary, Node, NodeId, QueryMode, SymbolKind, SymbolNode, SymbolSummary,
    };
    use rand::Rng;

    fn random_line() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..1000)
    }

    fn random_count() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..100)
    }

    fn create_overview_response(
        file_count: u32,
        symbol_count: u32,
        modules: Vec<ModuleSummary>,
        symbols: Vec<SymbolSummary>,
        truncated: bool,
    ) -> CodemapResponse {
        CodemapResponse {
            mode: QueryMode::Overview,
            file_count,
            symbol_count,
            target: None,
            modules,
            symbols,
            nodes: Vec::new(),
            edges: Vec::new(),
            limit: 50,
            offset: 0,
            truncated,
        }
    }

    fn create_symbol_summary(name: &str, kind: SymbolKind, path: &str, line: u32, ref_count: u32) -> SymbolSummary {
        SymbolSummary {
            name: name.to_string(),
            kind,
            location: FilePosition {
                path: path.to_string(),
                position: Position { line, character: 0 },
            },
            reference_count: ref_count,
        }
    }

    fn create_module_summary(path: &str, file_count: u32, symbol_count: u32) -> ModuleSummary {
        ModuleSummary {
            path: path.to_string(),
            file_count,
            symbol_count,
        }
    }

    #[test]
    fn format_overview_includes_codebase_counts_in_header() {
        let file_count = random_count();
        let symbol_count = random_count();
        let response = create_overview_response(file_count, symbol_count, vec![], vec![], false);

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains(&format!("{} files", file_count)),
            "negative: header must include file count"
        );
        assert!(
            markdown.contains(&format!("{} symbols", symbol_count)),
            "negative: header must include symbol count"
        );
    }

    #[test]
    fn format_overview_lists_modules_with_counts() {
        let module = create_module_summary("src/core", 5, 25);
        let response = create_overview_response(10, 50, vec![module], vec![], false);

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("Modules (1)"),
            "negative: modules section must show count"
        );
        assert!(
            markdown.contains("src/core"),
            "negative: module path must be shown"
        );
        assert!(
            markdown.contains("5 files"),
            "negative: module file count must be shown"
        );
        assert!(
            markdown.contains("25 symbols"),
            "negative: module symbol count must be shown"
        );
    }

    #[test]
    fn format_overview_lists_key_symbols_with_reference_counts() {
        let line = random_line();
        let symbol = create_symbol_summary("main", SymbolKind::Function, "src/main.rs", line, 42);
        let response = create_overview_response(10, 50, vec![], vec![symbol], false);

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("Key Symbols"),
            "negative: symbols section header must be present"
        );
        assert!(
            markdown.contains("main"),
            "negative: symbol name must be shown"
        );
        assert!(
            markdown.contains("(function)"),
            "negative: symbol kind must be in parentheses"
        );
        assert!(
            markdown.contains(&format!("src/main.rs:{}", line)),
            "negative: symbol location must be shown"
        );
        assert!(
            markdown.contains("42 refs"),
            "negative: reference count must be shown"
        );
    }

    #[test]
    fn format_overview_shows_truncation_notice() {
        let symbols = vec![
            create_symbol_summary("func1", SymbolKind::Function, "src/a.rs", 10, 5),
            create_symbol_summary("func2", SymbolKind::Function, "src/b.rs", 20, 3),
        ];
        let response = create_overview_response(10, 100, vec![], symbols, true);

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("[Showing 2 of 100"),
            "negative: truncation notice must show actual vs total"
        );
    }

    #[test]
    fn format_impact_includes_target_in_header() {
        let response = CodemapResponse {
            mode: QueryMode::Impact,
            file_count: 0,
            symbol_count: 0,
            target: Some("MyClass::method".to_string()),
            modules: vec![],
            symbols: vec![],
            nodes: vec![],
            edges: vec![],
            limit: 50,
            offset: 0,
            truncated: false,
        };

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("Impact Analysis for `MyClass::method`"),
            "negative: header must include target symbol"
        );
    }

    #[test]
    fn format_impact_groups_edges_by_type() {
        let file_id = NodeId::for_file("src/a.rs");
        let symbol_id = NodeId::for_symbol("src/a.rs", 10, 0);
        let caller_id = NodeId::for_symbol("src/b.rs", 20, 0);

        let edges = vec![
            Edge::Imports(ImportsEdge {
                id: EdgeId::new(&file_id, &symbol_id, EdgeKind::Imports),
                from_file_id: file_id.clone(),
                to_target_id: symbol_id.clone(),
                import_path: "src/a".to_string(),
                metadata: EdgeMetadata::default(),
            }),
            Edge::Calls(CallsEdge {
                id: EdgeId::new(&caller_id, &symbol_id, EdgeKind::Calls),
                caller_id: caller_id.clone(),
                callee_id: symbol_id.clone(),
                call_sites: vec![],
                metadata: EdgeMetadata::default(),
            }),
        ];

        let response = CodemapResponse {
            mode: QueryMode::Impact,
            file_count: 0,
            symbol_count: 0,
            target: Some("target".to_string()),
            modules: vec![],
            symbols: vec![],
            nodes: vec![],
            edges,
            limit: 50,
            offset: 0,
            truncated: false,
        };

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("By Edge Type"),
            "negative: edge type summary section must be present"
        );
        assert!(
            markdown.contains("imports: 1"),
            "negative: imports count must be shown"
        );
        assert!(
            markdown.contains("calls: 1"),
            "negative: calls count must be shown"
        );
    }

    #[test]
    fn format_context_shows_symbol_information() {
        let symbol_node = SymbolNode {
            id: NodeId::for_symbol("src/lib.rs", 42, 4),
            name: "process_data".to_string(),
            kind: SymbolKind::Function,
            location: FilePosition {
                path: "src/lib.rs".to_string(),
                position: Position { line: 42, character: 4 },
            },
            end_position: None,
            signature: Some("fn process_data(input: &str) -> Result<String>".to_string()),
            container_name: Some("DataProcessor".to_string()),
            file_version: 1,
            indexed_at: 0,
            is_public_api: true,
        };

        let response = CodemapResponse {
            mode: QueryMode::Context,
            file_count: 0,
            symbol_count: 0,
            target: Some("process_data".to_string()),
            modules: vec![],
            symbols: vec![],
            nodes: vec![Node::Symbol(symbol_node)],
            edges: vec![],
            limit: 50,
            offset: 0,
            truncated: false,
        };

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("Context for `process_data`"),
            "negative: header must include target"
        );
        assert!(
            markdown.contains("Symbol Info"),
            "negative: symbol info section must be present"
        );
        assert!(
            markdown.contains("Kind: function"),
            "negative: symbol kind must be shown"
        );
        assert!(
            markdown.contains("Location: src/lib.rs:42"),
            "negative: symbol location must be shown"
        );
        assert!(
            markdown.contains("Signature: fn process_data"),
            "negative: signature must be shown when present"
        );
        assert!(
            markdown.contains("Container: DataProcessor"),
            "negative: container must be shown when present"
        );
    }

    #[test]
    fn format_context_shows_incoming_and_outgoing_edges() {
        let target_id = NodeId::for_symbol("src/lib.rs", 10, 0);
        let caller_id = NodeId::for_symbol("src/main.rs", 20, 0);
        let callee_id = NodeId::for_symbol("src/util.rs", 30, 0);

        let target_node = SymbolNode {
            id: target_id.clone(),
            name: "target_func".to_string(),
            kind: SymbolKind::Function,
            location: FilePosition {
                path: "src/lib.rs".to_string(),
                position: Position { line: 10, character: 0 },
            },
            end_position: None,
            signature: None,
            container_name: None,
            file_version: 1,
            indexed_at: 0,
            is_public_api: true,
        };

        let caller_node = SymbolNode {
            id: caller_id.clone(),
            name: "caller_func".to_string(),
            kind: SymbolKind::Function,
            location: FilePosition {
                path: "src/main.rs".to_string(),
                position: Position { line: 20, character: 0 },
            },
            end_position: None,
            signature: None,
            container_name: None,
            file_version: 1,
            indexed_at: 0,
            is_public_api: true,
        };

        let callee_node = SymbolNode {
            id: callee_id.clone(),
            name: "helper_func".to_string(),
            kind: SymbolKind::Function,
            location: FilePosition {
                path: "src/util.rs".to_string(),
                position: Position { line: 30, character: 0 },
            },
            end_position: None,
            signature: None,
            container_name: None,
            file_version: 1,
            indexed_at: 0,
            is_public_api: true,
        };

        let edges = vec![
            Edge::Calls(CallsEdge {
                id: EdgeId::new(&caller_id, &target_id, EdgeKind::Calls),
                caller_id: caller_id.clone(),
                callee_id: target_id.clone(),
                call_sites: vec![],
                metadata: EdgeMetadata::default(),
            }),
            Edge::Calls(CallsEdge {
                id: EdgeId::new(&target_id, &callee_id, EdgeKind::Calls),
                caller_id: target_id.clone(),
                callee_id: callee_id.clone(),
                call_sites: vec![],
                metadata: EdgeMetadata::default(),
            }),
        ];

        let response = CodemapResponse {
            mode: QueryMode::Context,
            file_count: 0,
            symbol_count: 0,
            target: Some("target_func".to_string()),
            modules: vec![],
            symbols: vec![],
            nodes: vec![
                Node::Symbol(target_node),
                Node::Symbol(caller_node),
                Node::Symbol(callee_node),
            ],
            edges,
            limit: 50,
            offset: 0,
            truncated: false,
        };

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("Incoming (1)"),
            "negative: incoming section must show count"
        );
        assert!(
            markdown.contains("caller_func"),
            "negative: caller must be shown in incoming"
        );
        assert!(
            markdown.contains("Outgoing (1)"),
            "negative: outgoing section must show count"
        );
        assert!(
            markdown.contains("helper_func"),
            "negative: callee must be shown in outgoing"
        );
    }

    #[test]
    fn format_symbol_kind_returns_readable_names() {
        assert_eq!(format_symbol_kind(&SymbolKind::Function), "function");
        assert_eq!(format_symbol_kind(&SymbolKind::Method), "method");
        assert_eq!(format_symbol_kind(&SymbolKind::Class), "class");
        assert_eq!(format_symbol_kind(&SymbolKind::EnumVariant), "enum variant");
        assert_eq!(format_symbol_kind(&SymbolKind::TypeAlias), "type alias");
    }

    #[test]
    fn format_edge_kind_returns_readable_names() {
        assert_eq!(format_edge_kind(&EdgeKind::Defines), "defines");
        assert_eq!(format_edge_kind(&EdgeKind::Imports), "imports");
        assert_eq!(format_edge_kind(&EdgeKind::Calls), "calls");
    }

    #[test]
    fn get_node_name_extracts_names_from_all_node_types() {
        let symbol_node = Node::Symbol(SymbolNode {
            id: NodeId::for_symbol("src/a.rs", 1, 0),
            name: "my_func".to_string(),
            kind: SymbolKind::Function,
            location: FilePosition {
                path: "src/a.rs".to_string(),
                position: Position { line: 1, character: 0 },
            },
            end_position: None,
            signature: None,
            container_name: None,
            file_version: 1,
            indexed_at: 0,
            is_public_api: true,
        });

        let file_node = Node::File(FileNode {
            id: NodeId::for_file("src/b.rs"),
            path: "src/b.rs".to_string(),
            language: "rust".to_string(),
            content_hash: "abc123".to_string(),
            mtime: 0,
            line_count: 100,
            is_external: false,
        });

        let module_node = Node::Module(ModuleNode {
            id: NodeId::for_module("core", "src/core"),
            name: "core".to_string(),
            path: "src/core".to_string(),
            entry_file: None,
            is_external: false,
        });

        assert_eq!(get_node_name(&symbol_node), "my_func");
        assert_eq!(get_node_name(&file_node), "src/b.rs");
        assert_eq!(get_node_name(&module_node), "core");
    }

    #[test]
    fn format_overview_handles_empty_response() {
        let response = create_overview_response(0, 0, vec![], vec![], false);

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("Codebase Map (0 files, 0 symbols)"),
            "negative: empty response must show zero counts"
        );
    }

    #[test]
    fn format_context_shows_file_info_when_target_is_file() {
        let file_node = FileNode {
            id: NodeId::for_file("src/main.rs"),
            path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            content_hash: "hash123".to_string(),
            mtime: 1234567890,
            line_count: 150,
            is_external: false,
        };

        let response = CodemapResponse {
            mode: QueryMode::Context,
            file_count: 0,
            symbol_count: 0,
            target: Some("src/main.rs".to_string()),
            modules: vec![],
            symbols: vec![],
            nodes: vec![Node::File(file_node)],
            edges: vec![],
            limit: 50,
            offset: 0,
            truncated: false,
        };

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("File Info"),
            "negative: file info section must be present"
        );
        assert!(
            markdown.contains("Path: src/main.rs"),
            "negative: file path must be shown"
        );
        assert!(
            markdown.contains("Language: rust"),
            "negative: language must be shown"
        );
        assert!(
            markdown.contains("Lines: 150"),
            "negative: line count must be shown"
        );
    }

    #[test]
    fn format_context_shows_module_info_when_target_is_module() {
        let module_node = ModuleNode {
            id: NodeId::for_module("utils", "src/utils"),
            name: "utils".to_string(),
            path: "src/utils".to_string(),
            entry_file: Some("src/utils/mod.rs".to_string()),
            is_external: false,
        };

        let response = CodemapResponse {
            mode: QueryMode::Context,
            file_count: 0,
            symbol_count: 0,
            target: Some("utils".to_string()),
            modules: vec![],
            symbols: vec![],
            nodes: vec![Node::Module(module_node)],
            edges: vec![],
            limit: 50,
            offset: 0,
            truncated: false,
        };

        let markdown = format_codemap_response(&response);

        assert!(
            markdown.contains("Module Info"),
            "negative: module info section must be present"
        );
        assert!(
            markdown.contains("Name: utils"),
            "negative: module name must be shown"
        );
        assert!(
            markdown.contains("Path: src/utils"),
            "negative: module path must be shown"
        );
    }
}
