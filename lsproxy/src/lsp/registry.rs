// ABOUTME: Language registry providing a single source of truth for language server metadata
// ABOUTME: Centralizes file patterns, extensions, binary names, and factory functions for all supported languages

use crate::api_types::SupportedLanguages;
use crate::lsp::client::LspClient;
use crate::lsp::languages::{
    ClangdClient, CSharpClient, GoplsClient, JdtlsClient, JediClient, PhpactorClient, RubyClient,
    RustAnalyzerClient, TypeScriptLanguageClient,
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
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>>;

fn create_python_client(
    workspace_path: &str,
    events_rx: Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
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
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
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
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
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
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
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
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
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
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
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
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
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
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
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
) -> Pin<Box<dyn Future<Output = Result<Box<dyn LspClient>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
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
    /// Default binary name for the language server
    pub default_binary: &'static str,
    /// Files that indicate the presence of this language's project
    pub root_files: &'static [&'static str],
    /// Glob patterns for source files
    pub file_patterns: &'static [&'static str],
    /// File extensions (without dots)
    pub extensions: &'static [&'static str],
    /// Factory function for creating LSP client instances
    pub factory: LspClientFactory,
}

/// The language registry - single source of truth for all language metadata
pub static LANGUAGE_REGISTRY: &[LanguageMetadata] = &[
    LanguageMetadata {
        id: SupportedLanguages::Python,
        name: "Python",
        default_binary: "jedi-language-server",
        root_files: &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "Pipfile",
            "pyrightconfig.json",
        ],
        file_patterns: &["**/*.py", "**/*.pyx", "**/*.pyi"],
        extensions: &["py", "pyx", "pyi"],
        factory: create_python_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::TypeScriptJavaScript,
        name: "TypeScript/JavaScript",
        default_binary: "typescript-language-server",
        root_files: &["tsconfig.json", "jsconfig.json", "package.json"],
        file_patterns: &["**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
        extensions: &["ts", "tsx", "js", "jsx"],
        factory: create_typescript_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::Rust,
        name: "Rust",
        default_binary: "rust-analyzer",
        root_files: &["Cargo.toml"],
        file_patterns: &["**/*.rs"],
        extensions: &["rs"],
        factory: create_rust_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::CPP,
        name: "C/C++",
        default_binary: "clangd",
        root_files: &[
            "makefile",
            ".clangd",
            ".clang-tidy",
            ".clang-format",
            "compile_commands.json",
            "compile_flags.txt",
            "configure.ac",
            ".git",
        ],
        file_patterns: &[
            "**/*.cpp", "**/*.cc", "**/*.c", "**/*.cxx", "**/*.h", "**/*.hpp", "**/*.hxx", "**/*.hh",
        ],
        extensions: &["cpp", "cc", "c", "cxx", "h", "hpp", "hxx", "hh"],
        factory: create_cpp_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::CSharp,
        name: "C#",
        default_binary: "OmniSharp",
        root_files: &["*.sln", "*.csproj", "*.vcxproj"],
        file_patterns: &["**/*.cs"],
        extensions: &["cs"],
        factory: create_csharp_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::Java,
        name: "Java",
        default_binary: "jdtls",
        root_files: &["gradlew", ".git", "mvnw"],
        file_patterns: &["**/*.java"],
        extensions: &["java"],
        factory: create_java_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::Golang,
        name: "Go",
        default_binary: "gopls",
        root_files: &["go.mod", "go.work"],
        file_patterns: &["**/*.go", "**/*.gomod", "**/*.gowork", "**/*.gotmpl"],
        extensions: &["go"],
        factory: create_golang_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::PHP,
        name: "PHP",
        default_binary: "phpactor",
        root_files: &[
            "composer.json",
            "composer.lock",
            "phpunit.xml",
            "artisan",
            ".env",
            "index.php",
            "wp-config.php",
        ],
        file_patterns: &[
            "**/*.php",
            "**/*.phtml",
            "**/*.phps",
            "**/*.php5",
            "**/*.php7",
            "**/*.php8",
        ],
        extensions: &["php", "phtml", "phps", "php5", "php7", "php8"],
        factory: create_php_client,
    },
    LanguageMetadata {
        id: SupportedLanguages::Ruby,
        name: "Ruby",
        default_binary: "solargraph",
        root_files: &["Gemfile", "Rakefile", ".ruby-version", "config.ru", ".gemspec"],
        file_patterns: &["**/*.rb", "**/*.erb"],
        extensions: &["rb", "erb"],
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

    /// Get all registered language IDs
    pub fn all_ids() -> impl Iterator<Item = SupportedLanguages> {
        LANGUAGE_REGISTRY.iter().map(|m| m.id)
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
        let metadata = LanguageMetadata::from_extension("py")
            .expect("py extension must map to a language");
        assert_eq!(
            metadata.id,
            SupportedLanguages::Python,
            "py extension must map to Python"
        );
    }

    #[test]
    fn extension_lookup_returns_correct_language_for_typescript() {
        let metadata = LanguageMetadata::from_extension("ts")
            .expect("ts extension must map to a language");
        assert_eq!(
            metadata.id,
            SupportedLanguages::TypeScriptJavaScript,
            "ts extension must map to TypeScriptJavaScript"
        );
    }

    #[test]
    fn extension_lookup_returns_correct_language_for_rust() {
        let metadata = LanguageMetadata::from_extension("rs")
            .expect("rs extension must map to a language");
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
    fn all_ids_returns_all_nine_languages() {
        let ids: Vec<_> = LanguageMetadata::all_ids().collect();
        assert_eq!(ids.len(), 9, "registry must contain exactly 9 languages");
    }

    #[test]
    fn all_returns_iterator_over_all_metadata() {
        let count = LanguageMetadata::all().count();
        assert_eq!(count, 9, "all() must iterate over 9 language entries");
    }

    #[test]
    fn get_returns_correct_metadata_for_golang() {
        let metadata = LanguageMetadata::get(SupportedLanguages::Golang)
            .expect("Golang must be in registry");
        assert_eq!(metadata.name, "Go", "Golang display name must be 'Go'");
        assert_eq!(
            metadata.default_binary, "gopls",
            "Golang default binary must be 'gopls'"
        );
    }

    #[test]
    fn python_metadata_has_correct_root_files() {
        let metadata = LanguageMetadata::get(SupportedLanguages::Python)
            .expect("Python must be in registry");
        assert!(
            metadata.root_files.contains(&"pyproject.toml"),
            "Python root_files must contain pyproject.toml"
        );
        assert!(
            metadata.root_files.contains(&"requirements.txt"),
            "Python root_files must contain requirements.txt"
        );
    }

    #[test]
    fn cpp_metadata_has_correct_extensions() {
        let metadata = LanguageMetadata::get(SupportedLanguages::CPP)
            .expect("CPP must be in registry");
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
    fn typescript_javascript_metadata_has_correct_file_patterns() {
        let metadata = LanguageMetadata::get(SupportedLanguages::TypeScriptJavaScript)
            .expect("TypeScriptJavaScript must be in registry");
        assert!(
            metadata.file_patterns.contains(&"**/*.ts"),
            "TS/JS file_patterns must contain **/*.ts"
        );
        assert!(
            metadata.file_patterns.contains(&"**/*.jsx"),
            "TS/JS file_patterns must contain **/*.jsx"
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
        let metadata = LanguageMetadata::get(SupportedLanguages::Ruby)
            .expect("Ruby must be in registry");
        assert!(
            metadata.extensions.contains(&"erb"),
            "Ruby extensions must contain erb"
        );
    }
}
