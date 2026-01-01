// ABOUTME: Post-processing filters for tree-sitter query captures
// ABOUTME: Filters out unwanted matches that tree-sitter queries alone can't exclude

use tree_sitter::Node;

/// Check if a node is inside a definition context (for filtering references)
/// This filters out identifiers that are part of definitions, not references
pub fn is_inside_definition(node: Node, lang: &str) -> bool {
    let definition_parent_kinds = match lang {
        "typescript" | "tsx" | "javascript" => &[
            "function_declaration",
            "method_definition",
            "class_declaration",
            "interface_declaration",
            "type_alias_declaration",
            "variable_declarator",
            "import_specifier",
            "import_clause",
            "property_signature",
            "required_parameter",
            "optional_parameter",
        ][..],
        "python" => &[
            "function_definition",
            "class_definition",
            "parameters",
            "import_from_statement",
            "import_statement",
        ][..],
        "rust" => &[
            "function_item",
            "struct_item",
            "enum_item",
            "trait_item",
            "impl_item",
            "type_item",
            "let_declaration",
            "use_declaration",
        ][..],
        "csharp" => &[
            "method_declaration",
            "class_declaration",
            "interface_declaration",
            "property_declaration",
            "field_declaration",
            "variable_declaration",
            "parameter",
        ][..],
        "php" => &[
            "function_definition",
            "method_declaration",
            "class_declaration",
            "property_declaration",
            "formal_parameters",
        ][..],
        _ => &[][..],
    };

    is_inside_any_of(node, definition_parent_kinds)
}

/// Check if a node is inside an import statement
pub fn is_inside_import(node: Node, lang: &str) -> bool {
    let import_kinds = match lang {
        "typescript" | "tsx" | "javascript" => {
            &["import_statement", "import_clause", "import_specifier"][..]
        }
        "python" => &["import_statement", "import_from_statement"][..],
        "rust" => &["use_declaration"][..],
        "csharp" => &["using_directive"][..],
        "php" => &["use_declaration", "namespace_use_clause"][..],
        _ => &[][..],
    };

    is_inside_any_of(node, import_kinds)
}

/// Check if a node is inside a JSX element (for filtering HTML-like tags)
pub fn is_jsx_html_element(node: Node, source: &[u8]) -> bool {
    // Common HTML element names to filter out in JSX
    const HTML_ELEMENTS: &[&str] = &[
        "div", "span", "p", "a", "button", "input", "form", "label", "h1", "h2", "h3", "h4", "h5",
        "h6", "ul", "ol", "li", "table", "tr", "td", "th", "thead", "tbody", "tfoot", "img",
        "video", "audio", "canvas", "svg", "path", "header", "footer", "nav", "main", "section",
        "article", "aside", "textarea", "select", "option", "optgroup",
    ];

    let text = node.utf8_text(source).unwrap_or("");
    HTML_ELEMENTS.contains(&text.to_lowercase().as_str())
}

/// Helper: Check if a node has any ancestor with the given kinds
fn is_inside_any_of(node: Node, kinds: &[&str]) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if kinds.contains(&parent.kind()) {
            return true;
        }
        current = parent.parent();
    }
    false
}

/// Check if a node is the left-hand side of an assignment
pub fn is_assignment_target(node: Node, lang: &str) -> bool {
    if let Some(parent) = node.parent() {
        let is_assignment = match lang {
            "typescript" | "tsx" | "javascript" => {
                parent.kind() == "assignment_expression"
                    && parent.child_by_field_name("left").map(|n| n.id()) == Some(node.id())
            }
            "python" => {
                parent.kind() == "assignment"
                    && parent.child_by_field_name("left").map(|n| n.id()) == Some(node.id())
            }
            _ => false,
        };
        return is_assignment;
    }
    false
}

/// Check if a node is a property key (not a reference)
pub fn is_property_key(node: Node, lang: &str) -> bool {
    if let Some(parent) = node.parent() {
        match lang {
            "typescript" | "tsx" | "javascript" => {
                if parent.kind() == "pair" {
                    // Check if this is the key, not the value
                    if let Some(key) = parent.child_by_field_name("key") {
                        return key.id() == node.id();
                    }
                }
            }
            "python" => {
                if parent.kind() == "dictionary" || parent.kind() == "pair" {
                    if let Some(key) = parent.child_by_field_name("key") {
                        return key.id() == node.id();
                    }
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    // Tests would require parsing actual code, which would be done in integration tests
    // The functions are pure and take tree-sitter Nodes, so they need real parse trees
}
