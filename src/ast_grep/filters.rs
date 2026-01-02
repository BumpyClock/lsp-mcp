// ABOUTME: Post-processing filters for tree-sitter query captures
// ABOUTME: Filters out unwanted matches that tree-sitter queries alone can't exclude

use tree_sitter::Node;

/// Common HTML element names to filter out in JSX
const HTML_ELEMENTS: &[&str] = &[
    "div", "span", "p", "a", "button", "input", "form", "label", "h1", "h2", "h3", "h4", "h5",
    "h6", "ul", "ol", "li", "table", "tr", "td", "th", "thead", "tbody", "tfoot", "img", "video",
    "audio", "canvas", "svg", "path", "header", "footer", "nav", "main", "section", "article",
    "aside", "textarea", "select", "option", "optgroup",
];

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
        "go" => &[
            "function_declaration",
            "method_declaration",
            "type_declaration",
            "type_spec",
            "var_declaration",
            "var_spec",
            "const_declaration",
            "const_spec",
            "short_var_declaration",
        ][..],
        "java" => &[
            "method_declaration",
            "class_declaration",
            "interface_declaration",
            "enum_declaration",
            "record_declaration",
            "constructor_declaration",
            "field_declaration",
            "local_variable_declaration",
            "formal_parameter",
        ][..],
        "ruby" => &[
            "method",
            "singleton_method",
            "class",
            "singleton_class",
            "module",
            "assignment",
        ][..],
        "cpp" => &[
            "function_definition",
            "class_specifier",
            "struct_specifier",
            "union_specifier",
            "enum_specifier",
            "declaration",
            "type_definition",
            "namespace_definition",
            "template_declaration",
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
        "go" => &["import_declaration", "import_spec"][..],
        "java" => &["import_declaration"][..],
        "ruby" => &[][..],
        "cpp" => &["preproc_include", "using_declaration"][..],
        _ => &[][..],
    };

    is_inside_any_of(node, import_kinds)
}

/// Check if a node is inside a JSX element (for filtering HTML-like tags)
pub fn is_jsx_html_element(node: Node, source: &[u8]) -> bool {
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
            "rust" => {
                parent.kind() == "assignment_expression"
                    && parent.child_by_field_name("left").map(|n| n.id()) == Some(node.id())
            }
            "csharp" => {
                parent.kind() == "assignment_expression"
                    && parent.child_by_field_name("left").map(|n| n.id()) == Some(node.id())
            }
            "php" => {
                parent.kind() == "assignment_expression"
                    && parent.child_by_field_name("left").map(|n| n.id()) == Some(node.id())
            }
            "go" => {
                parent.kind() == "assignment_statement"
                    && parent.child_by_field_name("left").map(|n| n.id()) == Some(node.id())
            }
            "java" => {
                parent.kind() == "assignment_expression"
                    && parent.child_by_field_name("left").map(|n| n.id()) == Some(node.id())
            }
            "ruby" => {
                parent.kind() == "assignment"
                    && parent.child_by_field_name("left").map(|n| n.id()) == Some(node.id())
            }
            "cpp" => {
                parent.kind() == "assignment_expression"
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
    use super::*;
    use tree_sitter::Parser;

    fn parse_rust(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn parse_tsx(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn parse_go(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn parse_java(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn parse_ruby(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn parse_cpp(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn find_node_by_text<'a>(node: Node<'a>, text: &str, source: &[u8]) -> Option<Node<'a>> {
        if node.utf8_text(source).ok() == Some(text) {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_node_by_text(child, text, source) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn is_inside_definition_detects_rust_function_definition() {
        let source = b"fn foo() { bar(); }";
        let tree = parse_rust(std::str::from_utf8(source).unwrap());
        let foo_node = find_node_by_text(tree.root_node(), "foo", source).unwrap();
        assert!(
            is_inside_definition(foo_node, "rust"),
            "foo should be inside function_item definition"
        );
    }

    #[test]
    fn is_inside_definition_detects_struct_definition() {
        let source = b"struct Foo { bar: i32 }";
        let tree = parse_rust(std::str::from_utf8(source).unwrap());
        let foo_node = find_node_by_text(tree.root_node(), "Foo", source).unwrap();
        assert!(
            is_inside_definition(foo_node, "rust"),
            "Foo should be inside struct_item definition"
        );
    }

    #[test]
    fn is_inside_import_detects_rust_use_declaration() {
        let source = b"use std::io::Read;";
        let tree = parse_rust(std::str::from_utf8(source).unwrap());
        let read_node = find_node_by_text(tree.root_node(), "Read", source).unwrap();
        assert!(
            is_inside_import(read_node, "rust"),
            "Read should be inside use_declaration"
        );
    }

    #[test]
    fn is_jsx_html_element_detects_common_html_tags() {
        let source = b"<div>content</div>";
        let tree = parse_tsx(std::str::from_utf8(source).unwrap());
        let div_node = find_node_by_text(tree.root_node(), "div", source).unwrap();
        assert!(
            is_jsx_html_element(div_node, source),
            "div should be detected as HTML element"
        );
    }

    #[test]
    fn is_jsx_html_element_returns_false_for_custom_components() {
        let source = b"<MyComponent />";
        let tree = parse_tsx(std::str::from_utf8(source).unwrap());
        let component_node = find_node_by_text(tree.root_node(), "MyComponent", source).unwrap();
        assert!(
            !is_jsx_html_element(component_node, source),
            "MyComponent should not be detected as HTML element"
        );
    }

    #[test]
    fn is_jsx_html_element_is_case_insensitive() {
        let source = b"<DIV>content</DIV>";
        let tree = parse_tsx(std::str::from_utf8(source).unwrap());
        let div_node = find_node_by_text(tree.root_node(), "DIV", source).unwrap();
        assert!(
            is_jsx_html_element(div_node, source),
            "DIV (uppercase) should be detected as HTML element"
        );
    }

    #[test]
    fn is_inside_definition_detects_go_function() {
        let source = b"package main\nfunc foo() {}";
        let tree = parse_go(std::str::from_utf8(source).unwrap());
        let foo_node = find_node_by_text(tree.root_node(), "foo", source).unwrap();
        assert!(
            is_inside_definition(foo_node, "go"),
            "foo should be inside function_declaration"
        );
    }

    #[test]
    fn is_inside_import_detects_go_import() {
        let source = b"package main\nimport \"fmt\"";
        let tree = parse_go(std::str::from_utf8(source).unwrap());
        let fmt_node = find_node_by_text(tree.root_node(), "\"fmt\"", source).unwrap();
        assert!(
            is_inside_import(fmt_node, "go"),
            "fmt should be inside import_declaration"
        );
    }

    #[test]
    fn is_inside_definition_detects_java_method() {
        let source = b"class Foo { void bar() {} }";
        let tree = parse_java(std::str::from_utf8(source).unwrap());
        let bar_node = find_node_by_text(tree.root_node(), "bar", source).unwrap();
        assert!(
            is_inside_definition(bar_node, "java"),
            "bar should be inside method_declaration"
        );
    }

    #[test]
    fn is_inside_import_detects_java_import() {
        let source = b"import java.util.List;";
        let tree = parse_java(std::str::from_utf8(source).unwrap());
        let list_node = find_node_by_text(tree.root_node(), "List", source).unwrap();
        assert!(
            is_inside_import(list_node, "java"),
            "List should be inside import_declaration"
        );
    }

    #[test]
    fn is_inside_definition_detects_ruby_method() {
        let source = b"def foo; end";
        let tree = parse_ruby(std::str::from_utf8(source).unwrap());
        let foo_node = find_node_by_text(tree.root_node(), "foo", source).unwrap();
        assert!(
            is_inside_definition(foo_node, "ruby"),
            "foo should be inside method definition"
        );
    }

    #[test]
    fn is_inside_definition_detects_ruby_class() {
        let source = b"class Foo; end";
        let tree = parse_ruby(std::str::from_utf8(source).unwrap());
        let foo_node = find_node_by_text(tree.root_node(), "Foo", source).unwrap();
        assert!(
            is_inside_definition(foo_node, "ruby"),
            "Foo should be inside class definition"
        );
    }

    #[test]
    fn is_inside_definition_detects_cpp_function() {
        let source = b"void foo() {}";
        let tree = parse_cpp(std::str::from_utf8(source).unwrap());
        let foo_node = find_node_by_text(tree.root_node(), "foo", source).unwrap();
        assert!(
            is_inside_definition(foo_node, "cpp"),
            "foo should be inside function_definition"
        );
    }

    #[test]
    fn is_inside_definition_detects_cpp_class() {
        let source = b"class Foo {};";
        let tree = parse_cpp(std::str::from_utf8(source).unwrap());
        let foo_node = find_node_by_text(tree.root_node(), "Foo", source).unwrap();
        assert!(
            is_inside_definition(foo_node, "cpp"),
            "Foo should be inside class_specifier"
        );
    }
}
