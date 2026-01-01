// ABOUTME: Language server and language enum types.
// ABOUTME: Includes SupportedLanguages, LspStatus, and health/error response types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum_macros::{Display, EnumString};

/// Response returned when an API error occurs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Description of the error that occurred
    pub error: String,
}

/// Status of a language server
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LspStatus {
    /// Language server is available and ready
    Ready,
    /// Language server is starting up in the background
    Initializing,
    /// Language server is not available (not installed or failed to start)
    Unavailable,
}

/// Response returned by the health check endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Current status of the service ("ok" or error description)
    pub status: String,
    /// Version of the service
    pub version: String,
    /// Map of supported languages and their availability status
    pub languages: HashMap<SupportedLanguages, LspStatus>,
    /// Whether debug mode is enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_mode: Option<bool>,
    /// Session ID when debug mode is enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Log file path when debug mode is enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_file: Option<String>,
}

#[derive(Debug, EnumString, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[strum(serialize_all = "lowercase")]
pub enum SupportedLanguages {
    #[serde(rename = "python")]
    Python,
    /// TypeScript and JavaScript are handled by the same langserver
    #[serde(rename = "typescript_javascript")]
    #[strum(serialize = "typescript", serialize = "javascript", serialize = "typescriptjavascript")]
    TypeScriptJavaScript,
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "cpp")]
    CPP,
    #[serde(rename = "csharp")]
    CSharp,
    #[serde(rename = "java")]
    Java,
    #[serde(rename = "golang")]
    Golang,
    #[serde(rename = "php")]
    PHP,
    #[serde(rename = "ruby")]
    Ruby,
}
