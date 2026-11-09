// ABOUTME: Error types for LSP manager operations
// ABOUTME: Provides structured errors for file not found, client issues, and internal errors

use crate::api_types::SupportedLanguages;
use crate::lsp::registry::LanguageMetadata;
use std::fmt;

#[derive(Debug)]
pub enum LspManagerError {
    FileNotFound(String),
    LspClientNotFound(SupportedLanguages),
    LspClientInitializing(SupportedLanguages),
    LspStartupFailed {
        language: SupportedLanguages,
        reason: String,
    },
    InternalError(String),
    UnsupportedFileType(String),
    NotImplemented(String),
}

impl fmt::Display for LspManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LspManagerError::FileNotFound(path) => {
                write!(f, "File '{}' not found in workspace", path)
            }
            LspManagerError::LspClientNotFound(lang)
            | LspManagerError::LspStartupFailed { language: lang, .. } => {
                let metadata = LanguageMetadata::get(*lang).expect("language must be registered");
                write!(
                    f,
                    "The {} language server is unavailable. Its default executable is '{}'.",
                    metadata.name, metadata.default_binary
                )?;
                if let LspManagerError::LspStartupFailed { reason, .. } = self {
                    write!(f, " {reason}.")?;
                }
                write!(f, " If it is not installed, install it and ensure it is on PATH, or configure its path in binaries. If it is already installed, check the startup logs for the cause.")
            }
            LspManagerError::LspClientInitializing(lang) => {
                let metadata = LanguageMetadata::get(*lang).expect("language must be registered");
                write!(
                    f,
                    "The {} language server ('{}') is still initializing. Please try again shortly.",
                    metadata.name, metadata.default_binary
                )
            }
            LspManagerError::InternalError(msg) => write!(f, "Internal error: {}", msg),
            LspManagerError::UnsupportedFileType(path) => {
                write!(f, "Unsupported file type: {}", path)
            }
            LspManagerError::NotImplemented(msg) => {
                write!(f, "Not implemented: {}", msg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_errors_name_default_server_and_conditional_remedies() {
        for (language, name, executable) in [
            (SupportedLanguages::Rust, "Rust", "rust-analyzer"),
            (SupportedLanguages::Python, "Python", "jedi-language-server"),
            (
                SupportedLanguages::TypeScriptJavaScript,
                "TypeScript/JavaScript",
                "typescript-language-server",
            ),
        ] {
            let message = LspManagerError::LspClientNotFound(language).to_string();
            assert!(message.contains(name), "{message}");
            assert!(message.contains(executable), "{message}");
            assert!(message.contains("If it is not installed"), "{message}");
            assert!(message.contains("PATH"), "{message}");
            assert!(message.contains("If it is already installed"), "{message}");
            assert!(message.contains("startup logs"), "{message}");
        }
    }

    #[test]
    fn initializing_error_names_server_and_requests_retry() {
        let message = LspManagerError::LspClientInitializing(SupportedLanguages::Rust).to_string();
        assert!(message.contains("Rust"));
        assert!(message.contains("rust-analyzer"));
        assert!(message.contains("initializing"));
        assert!(message.contains("try again"));
        assert!(!message.contains("install"));
    }
}

impl std::error::Error for LspManagerError {}

impl LspManagerError {
    /// Check if the error indicates tree-sitter/ast-grep functionality is unavailable.
    /// Used to enable graceful fallback to LSP-only operations.
    pub fn is_ast_grep_missing(&self) -> bool {
        matches!(
            self,
            LspManagerError::InternalError(message)
                if message.contains("ast-grep binary not found") ||
                   message.contains("tree-sitter parser not found") ||
                   message.contains("query not found for language") ||
                   message.contains("Unsupported language for extension")
        )
    }
}
