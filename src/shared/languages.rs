// ABOUTME: Unified language mapping for tree-sitter parsers.
// ABOUTME: Provides extension-to-language mapping used by ast_grep and semantic_search.

use tree_sitter::Language;

/// Supported programming languages for tree-sitter parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProgrammingLanguage {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Python,
    C,
    Cpp,
    CSharp,
    Java,
    Go,
    Php,
    Ruby,
    Markdown,
}

impl ProgrammingLanguage {
    /// Get the canonical name for this language (used for rule_id prefixes, etc.)
    pub fn name(&self) -> &'static str {
        match self {
            ProgrammingLanguage::Rust => "rust",
            ProgrammingLanguage::TypeScript => "typescript",
            ProgrammingLanguage::Tsx => "tsx",
            ProgrammingLanguage::JavaScript => "javascript",
            ProgrammingLanguage::Jsx => "jsx",
            ProgrammingLanguage::Python => "python",
            ProgrammingLanguage::C => "c",
            ProgrammingLanguage::Cpp => "cpp",
            ProgrammingLanguage::CSharp => "csharp",
            ProgrammingLanguage::Java => "java",
            ProgrammingLanguage::Go => "go",
            ProgrammingLanguage::Php => "php",
            ProgrammingLanguage::Ruby => "ruby",
            ProgrammingLanguage::Markdown => "markdown",
        }
    }

    /// Get the tree-sitter Language for this programming language.
    pub fn tree_sitter_language(&self) -> Language {
        match self {
            ProgrammingLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
            // TypeScript (no JSX) uses LANGUAGE_TYPESCRIPT
            ProgrammingLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            // TSX and JSX use LANGUAGE_TSX (JSX-aware grammar)
            ProgrammingLanguage::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            // JavaScript uses LANGUAGE_TSX as it can parse plain JS
            // (TSX is a superset of JavaScript)
            ProgrammingLanguage::JavaScript => tree_sitter_typescript::LANGUAGE_TSX.into(),
            ProgrammingLanguage::Jsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            ProgrammingLanguage::Python => tree_sitter_python::LANGUAGE.into(),
            ProgrammingLanguage::C => tree_sitter_c::LANGUAGE.into(),
            ProgrammingLanguage::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            ProgrammingLanguage::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            ProgrammingLanguage::Java => tree_sitter_java::LANGUAGE.into(),
            ProgrammingLanguage::Go => tree_sitter_go::LANGUAGE.into(),
            ProgrammingLanguage::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            ProgrammingLanguage::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            ProgrammingLanguage::Markdown => tree_sitter_md::LANGUAGE.into(),
        }
    }

    /// Get the query language name for this programming language.
    /// Used to select the appropriate .scm query files.
    /// Multiple extensions may share the same query language (e.g., tsx and jsx share tsx queries).
    pub fn query_language(&self) -> &'static str {
        match self {
            ProgrammingLanguage::Rust => "rust",
            ProgrammingLanguage::TypeScript => "typescript",
            // TSX and JSX share the same queries
            ProgrammingLanguage::Tsx | ProgrammingLanguage::Jsx => "tsx",
            // JavaScript uses tsx queries (tsx can parse JS)
            ProgrammingLanguage::JavaScript => "javascript",
            ProgrammingLanguage::Python => "python",
            ProgrammingLanguage::C => "c",
            ProgrammingLanguage::Cpp => "cpp",
            ProgrammingLanguage::CSharp => "csharp",
            ProgrammingLanguage::Java => "java",
            ProgrammingLanguage::Go => "go",
            ProgrammingLanguage::Php => "php",
            ProgrammingLanguage::Ruby => "ruby",
            ProgrammingLanguage::Markdown => "markdown",
        }
    }
}

/// Check if a file extension is supported for tree-sitter parsing.
pub fn is_supported(extension: &str) -> bool {
    from_extension(extension).is_some()
}

/// Get the ProgrammingLanguage for a file extension.
pub fn from_extension(extension: &str) -> Option<ProgrammingLanguage> {
    match extension.to_lowercase().as_str() {
        "rs" => Some(ProgrammingLanguage::Rust),
        "ts" => Some(ProgrammingLanguage::TypeScript),
        "tsx" => Some(ProgrammingLanguage::Tsx),
        "js" => Some(ProgrammingLanguage::JavaScript),
        "jsx" => Some(ProgrammingLanguage::Jsx),
        "py" => Some(ProgrammingLanguage::Python),
        "c" => Some(ProgrammingLanguage::C),
        "cpp" | "hpp" | "cc" | "cxx" | "hxx" | "h" => Some(ProgrammingLanguage::Cpp),
        "cs" => Some(ProgrammingLanguage::CSharp),
        "java" => Some(ProgrammingLanguage::Java),
        "go" => Some(ProgrammingLanguage::Go),
        "php" => Some(ProgrammingLanguage::Php),
        "rb" => Some(ProgrammingLanguage::Ruby),
        "md" | "markdown" => Some(ProgrammingLanguage::Markdown),
        _ => None,
    }
}

/// Get the tree-sitter Language for a file extension.
pub fn get_language(extension: &str) -> Option<Language> {
    from_extension(extension).map(|lang| lang.tree_sitter_language())
}

/// Get the canonical language name for a file extension.
pub fn language_name(extension: &str) -> &'static str {
    from_extension(extension)
        .map(|lang| lang.name())
        .unwrap_or("text")
}

/// Get the query language name for a file extension.
/// This determines which .scm query files to load.
pub fn query_language_name(extension: &str) -> Option<&'static str> {
    from_extension(extension).map(|lang| lang.query_language())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tsx_uses_tsx_grammar() {
        let lang = from_extension("tsx").unwrap();
        assert_eq!(lang, ProgrammingLanguage::Tsx);
        // TSX should use LANGUAGE_TSX (JSX-aware)
        assert_eq!(lang.query_language(), "tsx");
    }

    #[test]
    fn test_ts_uses_typescript_grammar() {
        let lang = from_extension("ts").unwrap();
        assert_eq!(lang, ProgrammingLanguage::TypeScript);
        assert_eq!(lang.query_language(), "typescript");
    }

    #[test]
    fn test_jsx_uses_tsx_queries() {
        let lang = from_extension("jsx").unwrap();
        assert_eq!(lang, ProgrammingLanguage::Jsx);
        // JSX shares queries with TSX
        assert_eq!(lang.query_language(), "tsx");
    }

    #[test]
    fn test_js_uses_javascript_queries() {
        let lang = from_extension("js").unwrap();
        assert_eq!(lang, ProgrammingLanguage::JavaScript);
        assert_eq!(lang.query_language(), "javascript");
    }

    #[test]
    fn test_cpp_extensions() {
        for ext in &["cpp", "hpp", "cc", "cxx", "hxx", "h"] {
            let lang = from_extension(ext).unwrap();
            assert_eq!(lang, ProgrammingLanguage::Cpp);
        }
    }

    #[test]
    fn test_unsupported_extension() {
        assert!(from_extension("xyz").is_none());
        assert!(!is_supported("xyz"));
        assert_eq!(language_name("xyz"), "text");
    }
}
