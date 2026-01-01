// ABOUTME: Error types for LSP manager operations
// ABOUTME: Provides structured errors for file not found, client issues, and internal errors

use crate::api_types::SupportedLanguages;
use std::fmt;

#[derive(Debug)]
pub enum LspManagerError {
    FileNotFound(String),
    LspClientNotFound(SupportedLanguages),
    LspClientInitializing(SupportedLanguages),
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
            LspManagerError::LspClientNotFound(lang) => {
                write!(f, "LSP client not found for {:?}", lang)
            }
            LspManagerError::LspClientInitializing(lang) => {
                write!(f, "The {:?} language server is still initializing, please try again shortly", lang)
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
