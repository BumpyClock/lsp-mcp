// ABOUTME: Registry for tree-sitter queries, loading .scm files via include_str!
// ABOUTME: Provides query lookup by language and query type (symbol/identifier/reference)

use crate::shared::languages::ProgrammingLanguage;
use std::collections::HashMap;
use std::sync::OnceLock;
use tree_sitter::Query;

/// Type of query to execute
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryType {
    Symbol,
    Identifier,
    Reference,
}

/// Registry holding compiled tree-sitter queries
pub struct QueryRegistry {
    queries: HashMap<(String, QueryType), Query>,
}

// Embed all query files at compile time
// Symbol queries
const RUST_SYMBOL: &str = include_str!("queries/symbol/rust.scm");
const TYPESCRIPT_SYMBOL: &str = include_str!("queries/symbol/typescript.scm");
const TSX_SYMBOL: &str = include_str!("queries/symbol/tsx.scm");
const JAVASCRIPT_SYMBOL: &str = include_str!("queries/symbol/javascript.scm");
const PYTHON_SYMBOL: &str = include_str!("queries/symbol/python.scm");
const GO_SYMBOL: &str = include_str!("queries/symbol/go.scm");
const JAVA_SYMBOL: &str = include_str!("queries/symbol/java.scm");
const CSHARP_SYMBOL: &str = include_str!("queries/symbol/csharp.scm");
const CPP_SYMBOL: &str = include_str!("queries/symbol/cpp.scm");
const PHP_SYMBOL: &str = include_str!("queries/symbol/php.scm");
const RUBY_SYMBOL: &str = include_str!("queries/symbol/ruby.scm");

// Identifier queries
const RUST_IDENTIFIER: &str = include_str!("queries/identifier/rust.scm");
const TYPESCRIPT_IDENTIFIER: &str = include_str!("queries/identifier/typescript.scm");
const TSX_IDENTIFIER: &str = include_str!("queries/identifier/tsx.scm");
const JAVASCRIPT_IDENTIFIER: &str = include_str!("queries/identifier/javascript.scm");
const PYTHON_IDENTIFIER: &str = include_str!("queries/identifier/python.scm");
const GO_IDENTIFIER: &str = include_str!("queries/identifier/go.scm");
const JAVA_IDENTIFIER: &str = include_str!("queries/identifier/java.scm");
const CSHARP_IDENTIFIER: &str = include_str!("queries/identifier/csharp.scm");
const CPP_IDENTIFIER: &str = include_str!("queries/identifier/cpp.scm");
const PHP_IDENTIFIER: &str = include_str!("queries/identifier/php.scm");
const RUBY_IDENTIFIER: &str = include_str!("queries/identifier/ruby.scm");

// Reference queries
const RUST_REFERENCE: &str = include_str!("queries/reference/rust.scm");
const TYPESCRIPT_REFERENCE: &str = include_str!("queries/reference/typescript.scm");
const TSX_REFERENCE: &str = include_str!("queries/reference/tsx.scm");
const JAVASCRIPT_REFERENCE: &str = include_str!("queries/reference/javascript.scm");
const PYTHON_REFERENCE: &str = include_str!("queries/reference/python.scm");
const GO_REFERENCE: &str = include_str!("queries/reference/go.scm");
const JAVA_REFERENCE: &str = include_str!("queries/reference/java.scm");
const CSHARP_REFERENCE: &str = include_str!("queries/reference/csharp.scm");
const CPP_REFERENCE: &str = include_str!("queries/reference/cpp.scm");
const PHP_REFERENCE: &str = include_str!("queries/reference/php.scm");
const RUBY_REFERENCE: &str = include_str!("queries/reference/ruby.scm");

static REGISTRY: OnceLock<QueryRegistry> = OnceLock::new();

impl QueryRegistry {
    /// Get the global query registry instance
    pub fn global() -> &'static QueryRegistry {
        REGISTRY.get_or_init(|| QueryRegistry::new().expect("Failed to initialize query registry"))
    }

    /// Create a new query registry, compiling all queries
    fn new() -> Result<Self, String> {
        let mut queries = HashMap::new();

        // Helper to compile and insert a query
        let mut add_query =
            |lang: ProgrammingLanguage, qtype: QueryType, source: &str| -> Result<(), String> {
                let ts_lang = lang.tree_sitter_language();
                let query = Query::new(&ts_lang, source).map_err(|e| {
                    format!(
                        "Failed to compile {:?} {:?} query: {}",
                        lang.name(),
                        qtype,
                        e
                    )
                })?;
                queries.insert((lang.query_language().to_string(), qtype), query);
                Ok(())
            };

        // Symbol queries
        add_query(ProgrammingLanguage::Rust, QueryType::Symbol, RUST_SYMBOL)?;
        add_query(
            ProgrammingLanguage::TypeScript,
            QueryType::Symbol,
            TYPESCRIPT_SYMBOL,
        )?;
        add_query(ProgrammingLanguage::Tsx, QueryType::Symbol, TSX_SYMBOL)?;
        add_query(
            ProgrammingLanguage::JavaScript,
            QueryType::Symbol,
            JAVASCRIPT_SYMBOL,
        )?;
        add_query(
            ProgrammingLanguage::Python,
            QueryType::Symbol,
            PYTHON_SYMBOL,
        )?;
        add_query(ProgrammingLanguage::Go, QueryType::Symbol, GO_SYMBOL)?;
        add_query(ProgrammingLanguage::Java, QueryType::Symbol, JAVA_SYMBOL)?;
        add_query(
            ProgrammingLanguage::CSharp,
            QueryType::Symbol,
            CSHARP_SYMBOL,
        )?;
        add_query(ProgrammingLanguage::Cpp, QueryType::Symbol, CPP_SYMBOL)?;
        add_query(ProgrammingLanguage::Php, QueryType::Symbol, PHP_SYMBOL)?;
        add_query(ProgrammingLanguage::Ruby, QueryType::Symbol, RUBY_SYMBOL)?;

        // Identifier queries
        add_query(
            ProgrammingLanguage::Rust,
            QueryType::Identifier,
            RUST_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::TypeScript,
            QueryType::Identifier,
            TYPESCRIPT_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::Tsx,
            QueryType::Identifier,
            TSX_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::JavaScript,
            QueryType::Identifier,
            JAVASCRIPT_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::Python,
            QueryType::Identifier,
            PYTHON_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::Go,
            QueryType::Identifier,
            GO_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::Java,
            QueryType::Identifier,
            JAVA_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::CSharp,
            QueryType::Identifier,
            CSHARP_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::Cpp,
            QueryType::Identifier,
            CPP_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::Php,
            QueryType::Identifier,
            PHP_IDENTIFIER,
        )?;
        add_query(
            ProgrammingLanguage::Ruby,
            QueryType::Identifier,
            RUBY_IDENTIFIER,
        )?;

        // Reference queries
        add_query(
            ProgrammingLanguage::Rust,
            QueryType::Reference,
            RUST_REFERENCE,
        )?;
        add_query(
            ProgrammingLanguage::TypeScript,
            QueryType::Reference,
            TYPESCRIPT_REFERENCE,
        )?;
        add_query(
            ProgrammingLanguage::Tsx,
            QueryType::Reference,
            TSX_REFERENCE,
        )?;
        add_query(
            ProgrammingLanguage::JavaScript,
            QueryType::Reference,
            JAVASCRIPT_REFERENCE,
        )?;
        add_query(
            ProgrammingLanguage::Python,
            QueryType::Reference,
            PYTHON_REFERENCE,
        )?;
        add_query(ProgrammingLanguage::Go, QueryType::Reference, GO_REFERENCE)?;
        add_query(
            ProgrammingLanguage::Java,
            QueryType::Reference,
            JAVA_REFERENCE,
        )?;
        add_query(
            ProgrammingLanguage::CSharp,
            QueryType::Reference,
            CSHARP_REFERENCE,
        )?;
        add_query(
            ProgrammingLanguage::Cpp,
            QueryType::Reference,
            CPP_REFERENCE,
        )?;
        add_query(
            ProgrammingLanguage::Php,
            QueryType::Reference,
            PHP_REFERENCE,
        )?;
        add_query(
            ProgrammingLanguage::Ruby,
            QueryType::Reference,
            RUBY_REFERENCE,
        )?;

        Ok(Self { queries })
    }

    /// Get a query for a language and query type
    pub fn get_query(&self, query_language: &str, query_type: QueryType) -> Option<&Query> {
        self.queries.get(&(query_language.to_string(), query_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test-only helper to check if a query exists
    impl QueryRegistry {
        fn has_query(&self, query_language: &str, query_type: QueryType) -> bool {
            self.get_query(query_language, query_type).is_some()
        }
    }

    #[test]
    fn test_global_registry_initializes() {
        let registry = QueryRegistry::global();
        assert!(registry.has_query("rust", QueryType::Symbol));
        assert!(registry.has_query("rust", QueryType::Identifier));
    }

    #[test]
    fn test_symbol_queries_available() {
        let registry = QueryRegistry::global();
        for lang in &[
            "rust",
            "typescript",
            "tsx",
            "javascript",
            "python",
            "go",
            "java",
            "csharp",
            "cpp",
            "php",
            "ruby",
        ] {
            assert!(
                registry.has_query(lang, QueryType::Symbol),
                "Missing symbol query for {}",
                lang
            );
        }
    }

    #[test]
    fn test_identifier_queries_available() {
        let registry = QueryRegistry::global();
        for lang in &[
            "rust",
            "typescript",
            "tsx",
            "javascript",
            "python",
            "go",
            "java",
            "csharp",
            "cpp",
            "php",
            "ruby",
        ] {
            assert!(
                registry.has_query(lang, QueryType::Identifier),
                "Missing identifier query for {}",
                lang
            );
        }
    }

    #[test]
    fn test_reference_queries_available() {
        let registry = QueryRegistry::global();
        for lang in &[
            "rust",
            "typescript",
            "tsx",
            "javascript",
            "python",
            "go",
            "java",
            "csharp",
            "cpp",
            "php",
            "ruby",
        ] {
            assert!(
                registry.has_query(lang, QueryType::Reference),
                "Missing reference query for {}",
                lang
            );
        }
    }

    #[test]
    fn test_get_query_returns_compiled_query() {
        let registry = QueryRegistry::global();
        let query = registry.get_query("rust", QueryType::Symbol);
        assert!(query.is_some());
        let query = query.unwrap();
        // Check that the query has capture names (meaning it compiled successfully)
        assert!(!query.capture_names().is_empty());
    }
}
