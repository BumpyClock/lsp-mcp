// ABOUTME: Language registry providing a single source of truth for language server metadata
// ABOUTME: Centralizes extensions and factory functions for all supported languages

use crate::api_types::SupportedLanguages;
use crate::lsp::client::LspClient;
use crate::lsp::languages::{
    CSharpClient, ClangdClient, GoplsClient, JdtlsClient, JediClient, PhpactorClient, RubyClient,
    RustAnalyzerClient, TypeScriptLanguageClient,
};
use crate::utils::workspace_documents::{
    CSHARP_EXTENSIONS, C_AND_CPP_EXTENSIONS, GOLANG_EXTENSIONS, JAVA_EXTENSIONS, PHP_EXTENSIONS,
    PYTHON_EXTENSIONS, RUBY_EXTENSIONS, RUST_EXTENSIONS, TYPESCRIPT_AND_JAVASCRIPT_EXTENSIONS,
};
use notify_debouncer_mini::DebouncedEvent;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::broadcast::Receiver;

/// Type alias for the async factory function that creates LSP clients
pub type LspClientFactory = fn(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
>;

fn create_python_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
> {
    let workspace_path = workspace_path.to_string();
    let binary = binary.map(|s| s.to_string());
    Box::pin(async move {
        JediClient::new(&workspace_path, events_rx, binary.as_deref())
            .await
            .map(|c| Box::new(c) as Box<dyn LspClient>)
    })
}

fn create_typescript_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
> {
    let workspace_path = workspace_path.to_string();
    let binary = binary.map(|s| s.to_string());
    Box::pin(async move {
        TypeScriptLanguageClient::new(&workspace_path, events_rx, binary.as_deref())
            .await
            .map(|c| Box::new(c) as Box<dyn LspClient>)
    })
}

fn create_rust_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
> {
    let workspace_path = workspace_path.to_string();
    let binary = binary.map(|s| s.to_string());
    Box::pin(async move {
        RustAnalyzerClient::new(&workspace_path, events_rx, binary.as_deref())
            .await
            .map(|c| Box::new(c) as Box<dyn LspClient>)
    })
}

fn create_cpp_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
> {
    let workspace_path = workspace_path.to_string();
    let binary = binary.map(|s| s.to_string());
    Box::pin(async move {
        ClangdClient::new(&workspace_path, events_rx, binary.as_deref())
            .await
            .map(|c| Box::new(c) as Box<dyn LspClient>)
    })
}

fn create_csharp_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
> {
    let workspace_path = workspace_path.to_string();
    let binary = binary.map(|s| s.to_string());
    Box::pin(async move {
        CSharpClient::new(&workspace_path, events_rx, binary.as_deref())
            .await
            .map(|c| Box::new(c) as Box<dyn LspClient>)
    })
}

fn create_java_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
> {
    let workspace_path = workspace_path.to_string();
    let binary = binary.map(|s| s.to_string());
    Box::pin(async move {
        JdtlsClient::new(&workspace_path, events_rx, binary.as_deref())
            .await
            .map(|c| Box::new(c) as Box<dyn LspClient>)
    })
}

fn create_golang_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
> {
    let workspace_path = workspace_path.to_string();
    let binary = binary.map(|s| s.to_string());
    Box::pin(async move {
        GoplsClient::new(&workspace_path, events_rx, binary.as_deref())
            .await
            .map(|c| Box::new(c) as Box<dyn LspClient>)
    })
}

fn create_php_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
> {
    let workspace_path = workspace_path.to_string();
    let binary = binary.map(|s| s.to_string());
    Box::pin(async move {
        PhpactorClient::new(&workspace_path, events_rx, binary.as_deref())
            .await
            .map(|c| Box::new(c) as Box<dyn LspClient>)
    })
}

fn create_ruby_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<
    Box<
        dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
> {
    let workspace_path = workspace_path.to_string();
    let binary = binary.map(|s| s.to_string());
    Box::pin(async move {
        RubyClient::new(&workspace_path, events_rx, binary.as_deref())
            .await
            .map(|c| Box::new(c) as Box<dyn LspClient>)
    })
}

/// Static metadata for a language server
pub struct LanguageMetadata {
    /// Enum variant identifier
    pub id: SupportedLanguages,
    /// Display name (e.g., "Python", "TypeScript/JavaScript")
    pub name: &'static str,
    /// File extensions (without dots)
    pub extensions: &'static [&'static str],
    /// Default language server binary name
    pub default_binary: &'static str,
    /// Factory function for creating LSP client instances
    pub factory: LspClientFactory,
}

/// The language registry - single source of truth for all language metadata
pub static LANGUAGE_REGISTRY: &[LanguageMetadata] = &[
    LanguageMetadata {
        id: SupportedLanguages::Python,
        name: "Python",
        extensions: PYTHON_EXTENSIONS,
        default_binary: JediClient::DEFAULT_BINARY,
        factory: create_python_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::TypeScriptJavaScript,
        name: "TypeScript/JavaScript",
        extensions: TYPESCRIPT_AND_JAVASCRIPT_EXTENSIONS,
        default_binary: TypeScriptLanguageClient::DEFAULT_BINARY,
        factory: create_typescript_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::Rust,
        name: "Rust",
        extensions: RUST_EXTENSIONS,
        default_binary: RustAnalyzerClient::DEFAULT_BINARY,
        factory: create_rust_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::CPP,
        name: "C/C++",
        extensions: C_AND_CPP_EXTENSIONS,
        default_binary: ClangdClient::DEFAULT_BINARY,
        factory: create_cpp_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::CSharp,
        name: "C#",
        extensions: CSHARP_EXTENSIONS,
        default_binary: CSharpClient::DEFAULT_BINARY,
        factory: create_csharp_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::Java,
        name: "Java",
        extensions: JAVA_EXTENSIONS,
        default_binary: JdtlsClient::DEFAULT_BINARY,
        factory: create_java_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::Golang,
        name: "Go",
        extensions: GOLANG_EXTENSIONS,
        default_binary: GoplsClient::DEFAULT_BINARY,
        factory: create_golang_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::PHP,
        name: "PHP",
        extensions: PHP_EXTENSIONS,
        default_binary: PhpactorClient::DEFAULT_BINARY,
        factory: create_php_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::Ruby,
        name: "Ruby",
        extensions: RUBY_EXTENSIONS,
        default_binary: RubyClient::DEFAULT_BINARY,
        factory: create_ruby_client,
    },
];

impl LanguageMetadata {
    /// Get metadata for a specific language
    pub fn get(lang: SupportedLanguages) -> Option<&'static LanguageMetadata> {
        LANGUAGE_REGISTRY.iter().find(|m| m.id == lang)
    }

    /// Iterate over all registered languages
    pub fn all() -> impl Iterator<Item = &'static LanguageMetadata> {
        LANGUAGE_REGISTRY.iter()
    }

    /// Find language by file extension
    pub fn from_extension(ext: &str) -> Option<&'static LanguageMetadata> {
        LANGUAGE_REGISTRY
            .iter()
            .find(|m| m.extensions.contains(&ext))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_all_supported_languages() {
        let languages = [
            SupportedLanguages::Python,
            SupportedLanguages::TypeScriptJavaScript,
            SupportedLanguages::Rust,
            SupportedLanguages::CPP,
            SupportedLanguages::CSharp,
            SupportedLanguages::Java,
            SupportedLanguages::Golang,
            SupportedLanguages::PHP,
            SupportedLanguages::Ruby,
        ];

        for lang in languages {
            assert!(
                LanguageMetadata::get(lang).is_some(),
                "language {:?} must be present in registry",
                lang
            );
        }
    }

    #[test]
    fn extension_lookup_returns_correct_language_for_python() {
        let metadata =
            LanguageMetadata::from_extension("py").expect("py extension must map to a language");
        assert_eq!(
            metadata.id,
            SupportedLanguages::Python,
            "py extension must map to Python"
        );
    }

    #[test]
    fn extension_lookup_returns_correct_language_for_typescript() {
        let metadata =
            LanguageMetadata::from_extension("ts").expect("ts extension must map to a language");
        assert_eq!(
            metadata.id,
            SupportedLanguages::TypeScriptJavaScript,
            "ts extension must map to TypeScriptJavaScript"
        );
    }

    #[test]
    fn extension_lookup_returns_correct_language_for_rust() {
        let metadata =
            LanguageMetadata::from_extension("rs").expect("rs extension must map to a language");
        assert_eq!(
            metadata.id,
            SupportedLanguages::Rust,
            "rs extension must map to Rust"
        );
    }

    #[test]
    fn extension_lookup_returns_none_for_unknown_extension() {
        assert!(
            LanguageMetadata::from_extension("unknown_ext_xyz").is_none(),
            "unknown extension must return None"
        );
    }

    #[test]
    fn all_returns_iterator_over_all_metadata() {
        let count = LanguageMetadata::all().count();
        assert_eq!(count, 9, "all() must iterate over 9 language entries");
    }

    #[test]
    fn get_returns_correct_metadata_for_golang() {
        let metadata =
            LanguageMetadata::get(SupportedLanguages::Golang).expect("Golang must be in registry");
        assert_eq!(metadata.name, "Go", "Golang display name must be 'Go'");
    }

    #[test]
    fn cpp_metadata_has_correct_extensions() {
        let metadata =
            LanguageMetadata::get(SupportedLanguages::CPP).expect("CPP must be in registry");
        let expected_extensions = ["cpp", "cc", "c", "cxx", "h", "hpp", "hxx", "hh"];
        for ext in expected_extensions {
            assert!(
                metadata.extensions.contains(&ext),
                "CPP extensions must contain '{}'",
                ext
            );
        }
    }

    #[test]
    fn typescript_javascript_metadata_has_correct_extensions() {
        let metadata = LanguageMetadata::get(SupportedLanguages::TypeScriptJavaScript)
            .expect("TypeScriptJavaScript must be in registry");
        assert!(
            metadata.extensions.contains(&"ts"),
            "TS/JS extensions must contain ts"
        );
        assert!(
            metadata.extensions.contains(&"jsx"),
            "TS/JS extensions must contain jsx"
        );
    }

    #[test]
    fn extension_lookup_works_for_all_php_variants() {
        let php_extensions = ["php", "phtml", "phps", "php5", "php7", "php8"];
        for ext in php_extensions {
            let metadata = LanguageMetadata::from_extension(ext)
                .unwrap_or_else(|| panic!("{} extension must map to a language", ext));
            assert_eq!(
                metadata.id,
                SupportedLanguages::PHP,
                "{} extension must map to PHP",
                ext
            );
        }
    }

    #[test]
    fn ruby_metadata_has_erb_extension() {
        let metadata =
            LanguageMetadata::get(SupportedLanguages::Ruby).expect("Ruby must be in registry");
        assert!(
            metadata.extensions.contains(&"erb"),
            "Ruby extensions must contain erb"
        );
    }
}
