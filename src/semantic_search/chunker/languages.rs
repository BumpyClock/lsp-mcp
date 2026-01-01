// ABOUTME: Language-specific tree-sitter configuration for semantic chunking.
// ABOUTME: Maps file extensions to grammars and semantic node kinds.

use super::types::ChunkBoundary;
use crate::shared::languages;
use tree_sitter::Language;

/// Check if a file extension is supported for semantic chunking.
pub fn is_supported(extension: &str) -> bool {
    languages::is_supported(extension)
}

/// Get the tree-sitter language for a file extension.
pub fn get_language(extension: &str) -> Option<Language> {
    languages::get_language(extension)
}

/// Node kinds that represent semantic boundaries for each language.
pub struct NodeKinds {
    pub function_kinds: &'static [&'static str],
    pub type_kinds: &'static [&'static str],
    pub impl_kinds: &'static [&'static str],
    pub constant_kinds: &'static [&'static str],
}

impl NodeKinds {
    /// Classify a node kind into a chunk boundary type.
    pub fn classify(&self, kind: &str) -> Option<ChunkBoundary> {
        if self.function_kinds.contains(&kind) {
            Some(ChunkBoundary::Function)
        } else if self.type_kinds.contains(&kind) {
            Some(ChunkBoundary::Type)
        } else if self.impl_kinds.contains(&kind) {
            Some(ChunkBoundary::Implementation)
        } else if self.constant_kinds.contains(&kind) {
            Some(ChunkBoundary::Constant)
        } else {
            None
        }
    }
}

/// Get the semantic node kinds for a language.
pub fn get_node_kinds(extension: &str) -> NodeKinds {
    match extension.to_lowercase().as_str() {
        "rs" => NodeKinds {
            function_kinds: &["function_item", "impl_item"],
            type_kinds: &["struct_item", "enum_item", "trait_item", "type_item"],
            impl_kinds: &["impl_item"],
            constant_kinds: &["const_item", "static_item"],
        },
        "ts" | "tsx" | "js" | "jsx" => NodeKinds {
            function_kinds: &[
                "function_declaration",
                "method_definition",
                "arrow_function",
                "function_expression",
            ],
            type_kinds: &[
                "class_declaration",
                "interface_declaration",
                "type_alias_declaration",
                "enum_declaration",
            ],
            impl_kinds: &["class_body"],
            constant_kinds: &["lexical_declaration", "variable_declaration"],
        },
        "py" => NodeKinds {
            function_kinds: &["function_definition", "decorated_definition"],
            type_kinds: &["class_definition"],
            impl_kinds: &[],
            constant_kinds: &["assignment", "augmented_assignment"],
        },
        "go" => NodeKinds {
            function_kinds: &["function_declaration", "method_declaration"],
            type_kinds: &["type_declaration", "type_spec"],
            impl_kinds: &[],
            constant_kinds: &["const_declaration", "var_declaration"],
        },
        "java" => NodeKinds {
            function_kinds: &["method_declaration", "constructor_declaration"],
            type_kinds: &[
                "class_declaration",
                "interface_declaration",
                "enum_declaration",
            ],
            impl_kinds: &["class_body"],
            constant_kinds: &["field_declaration"],
        },
        "c" | "h" => NodeKinds {
            function_kinds: &["function_definition"],
            type_kinds: &["struct_specifier", "enum_specifier", "union_specifier"],
            impl_kinds: &[],
            constant_kinds: &["declaration"],
        },
        "cpp" | "hpp" | "cc" | "cxx" => NodeKinds {
            function_kinds: &["function_definition", "template_declaration"],
            type_kinds: &["class_specifier", "struct_specifier", "enum_specifier"],
            impl_kinds: &["class_specifier"],
            constant_kinds: &["declaration"],
        },
        "cs" => NodeKinds {
            function_kinds: &["method_declaration", "constructor_declaration"],
            type_kinds: &[
                "class_declaration",
                "struct_declaration",
                "interface_declaration",
                "enum_declaration",
            ],
            impl_kinds: &["declaration_list"],
            constant_kinds: &["field_declaration", "property_declaration"],
        },
        "php" => NodeKinds {
            function_kinds: &["function_definition", "method_declaration"],
            type_kinds: &["class_declaration", "interface_declaration", "trait_declaration"],
            impl_kinds: &["declaration_list"],
            constant_kinds: &["property_declaration", "const_declaration"],
        },
        "rb" => NodeKinds {
            function_kinds: &["method", "singleton_method"],
            type_kinds: &["class", "module"],
            impl_kinds: &["class", "module"],
            constant_kinds: &["assignment"],
        },
        "md" => NodeKinds {
            function_kinds: &[],
            type_kinds: &["section", "atx_heading"],
            impl_kinds: &[],
            constant_kinds: &[],
        },
        _ => NodeKinds {
            function_kinds: &[],
            type_kinds: &[],
            impl_kinds: &[],
            constant_kinds: &[],
        },
    }
}

/// Get a display name for a language extension.
pub fn language_name(extension: &str) -> &'static str {
    languages::language_name(extension)
}
