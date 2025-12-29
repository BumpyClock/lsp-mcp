// ABOUTME: MCP tool response formatting with uniform JSON envelopes.
// ABOUTME: Provides success/error response wrappers with optional metadata for verbose mode.

use crate::config::OutputMode;
use crate::lsp::manager::LspManagerError;
use crate::service::{PositionError, ServiceError};
use crate::api_types::{get_mount_dir, SupportedLanguages};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Standardized success response envelope
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SuccessResponse {
    pub ok: bool,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

/// Standardized error response envelope
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ErrorResponse {
    pub ok: bool,
    pub error: ErrorInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

/// Error information structure
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported_languages: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
}

/// Response metadata (only in verbose mode)
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ResponseMeta {
    pub tool: String,
    pub mode: String,
    pub indexing: String,
    pub line_indexing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<HashMap<String, usize>>,
}

fn workspace_root_string() -> String {
    get_mount_dir().to_string_lossy().into_owned()
}

fn supported_language_names() -> Vec<String> {
    [
        SupportedLanguages::Python,
        SupportedLanguages::TypeScriptJavaScript,
        SupportedLanguages::Rust,
        SupportedLanguages::CPP,
        SupportedLanguages::CSharp,
        SupportedLanguages::Java,
        SupportedLanguages::Golang,
        SupportedLanguages::PHP,
        SupportedLanguages::Ruby,
    ]
    .iter()
    .map(|lang| lang.to_string())
    .collect()
}

/// Normalize LSP symbol kind strings to lower-kebab-case
pub fn normalize_kind(kind: &str) -> String {
    // Handle common LSP SymbolKind formats
    let mut result = String::new();
    let mut prev_was_lowercase = false;

    for (i, ch) in kind.chars().enumerate() {
        if i == 0 {
            // First character is always lowercase
            result.push(ch.to_ascii_lowercase());
            prev_was_lowercase = ch.is_lowercase();
        } else if ch.is_uppercase() {
            // Insert hyphen before uppercase if previous was lowercase
            if prev_was_lowercase {
                result.push('-');
            }
            result.push(ch.to_ascii_lowercase());
            prev_was_lowercase = false;
        } else {
            result.push(ch);
            prev_was_lowercase = ch.is_lowercase();
        }
    }

    result
}

/// Map ServiceError to error code
pub fn error_to_code(error: &ServiceError) -> &'static str {
    match error {
        ServiceError::Lsp(lsp_err) => match lsp_err {
            LspManagerError::FileNotFound(_) => "file_not_found",
            LspManagerError::LspClientNotFound(_) => "lsp_client_not_found",
            LspManagerError::LspClientInitializing(_) => "lsp_client_initializing",
            LspManagerError::UnsupportedFileType(_) => "unsupported_file_type",
            LspManagerError::NotImplemented(_) => "not_implemented",
            LspManagerError::InternalError(_) => "internal_error",
        },
        ServiceError::IdentifierSelection(PositionError::IdentifierNotFound { .. }) => {
            "identifier_not_found"
        }
        ServiceError::Serialization(_) => "serialization_error",
    }
}

/// Create a success response
pub fn success_response(
    tool_name: &str,
    data: Value,
    output_mode: OutputMode,
    counts: Option<HashMap<String, usize>>,
) -> String {
    let meta = if matches!(output_mode, OutputMode::Verbose) {
        Some(ResponseMeta {
            tool: tool_name.to_string(),
            mode: "verbose".to_string(),
            indexing: "zero-based".to_string(),
            line_indexing: "zero-based".to_string(),
            counts,
        })
    } else {
        None
    };

    let response = SuccessResponse {
        ok: true,
        data,
        meta,
    };

    match output_mode {
        OutputMode::Default => serde_json::to_string(&response).unwrap(),
        OutputMode::Verbose => serde_json::to_string_pretty(&response).unwrap(),
    }
}

/// Create an error response
pub fn error_response(
    tool_name: &str,
    error: &ServiceError,
    output_mode: OutputMode,
) -> String {
    let error_info = ErrorInfo {
        code: error_to_code(error).to_string(),
        message: error.to_string(),
        supported_languages: Some(supported_language_names()),
        workspace_root: Some(workspace_root_string()),
    };

    let meta = if matches!(output_mode, OutputMode::Verbose) {
        Some(ResponseMeta {
            tool: tool_name.to_string(),
            mode: "verbose".to_string(),
            indexing: "zero-based".to_string(),
            line_indexing: "zero-based".to_string(),
            counts: None,
        })
    } else {
        None
    };

    let response = ErrorResponse {
        ok: false,
        error: error_info,
        meta,
    };

    match output_mode {
        OutputMode::Default => serde_json::to_string(&response).unwrap(),
        OutputMode::Verbose => serde_json::to_string_pretty(&response).unwrap(),
    }
}

/// Create a tool disabled error response
pub fn tool_disabled_error(tool_name: &str, output_mode: OutputMode) -> String {
    let error_info = ErrorInfo {
        code: "tool_disabled".to_string(),
        message: format!(
            "Tool '{}' is disabled. Enable it in your .lsp-mcp.json config.",
            tool_name
        ),
        supported_languages: Some(supported_language_names()),
        workspace_root: Some(workspace_root_string()),
    };

    let meta = if matches!(output_mode, OutputMode::Verbose) {
        Some(ResponseMeta {
            tool: tool_name.to_string(),
            mode: "verbose".to_string(),
            indexing: "zero-based".to_string(),
            line_indexing: "zero-based".to_string(),
            counts: None,
        })
    } else {
        None
    };

    let response = ErrorResponse {
        ok: false,
        error: error_info,
        meta,
    };

    match output_mode {
        OutputMode::Default => serde_json::to_string(&response).unwrap(),
        OutputMode::Verbose => serde_json::to_string_pretty(&response).unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{distributions::Alphanumeric, Rng};
    use serde_json::json;
    use std::thread;

    fn random_irregular_string() -> String {
        let mut rng = rand::thread_rng();
        let len: usize = rng.gen_range(6..20);
        let mut value: String = rng
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect();
        value.push('_');
        value.push('\t');
        value
    }

    fn retry_with<T, F>(mut op: F) -> T
    where
        F: FnMut() -> Option<T>,
    {
        let mut rng = rand::thread_rng();
        let attempts: usize = rng.gen_range(2..5);
        for _ in 0..attempts {
            let result = op();
            if result.is_some() {
                return result.unwrap();
            }
        }
        let message = random_irregular_string();
        panic!("{}", message);
    }

    #[test]
    fn test_normalize_kind_simple_lowercase() {
        assert_eq!(normalize_kind("function"), "function");
        assert_eq!(normalize_kind("class"), "class");
    }

    #[test]
    fn test_normalize_kind_camel_case() {
        assert_eq!(normalize_kind("Function"), "function");
        assert_eq!(normalize_kind("EnumMember"), "enum-member");
        assert_eq!(normalize_kind("TypeParameter"), "type-parameter");
    }

    #[test]
    fn test_normalize_kind_single_uppercase() {
        assert_eq!(normalize_kind("Struct"), "struct");
        assert_eq!(normalize_kind("Module"), "module");
    }

    #[test]
    fn test_success_response_compact() {
        let data = json!({"symbols": [{"name": "foo"}]});
        let result = success_response("test_tool", data.clone(), OutputMode::Default, None);

        // Should be compact JSON
        assert!(!result.contains('\n'));

        // Parse and verify structure
        let parsed: SuccessResponse = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.ok, true);
        assert_eq!(parsed.data, data);
        assert!(parsed.meta.is_none());
    }

    #[test]
    fn test_success_response_verbose() {
        let data = json!({"symbols": [{"name": "foo"}]});
        let counts = {
            let mut m = HashMap::new();
            m.insert("symbols".to_string(), 1);
            m
        };
        let result = success_response(
            "test_tool",
            data.clone(),
            OutputMode::Verbose,
            Some(counts.clone()),
        );

        // Should be pretty printed
        assert!(result.contains('\n'));

        // Parse and verify structure
        let parsed: SuccessResponse = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.ok, true);
        assert_eq!(parsed.data, data);
        assert!(parsed.meta.is_some());

        let meta = parsed.meta.unwrap();
        assert_eq!(meta.tool, "test_tool");
        assert_eq!(meta.mode, "verbose");
        assert_eq!(meta.indexing, "zero-based");
        assert_eq!(meta.counts, Some(counts));
    }

    #[test]
    fn test_error_response_compact() {
        let error = ServiceError::Lsp(LspManagerError::FileNotFound(
            "test.rs".to_string(),
        ));
        let result = error_response("test_tool", &error, OutputMode::Default);

        // Should be compact JSON
        assert!(!result.contains('\n'));

        // Parse and verify structure
        let parsed: ErrorResponse = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.ok, false);
        assert_eq!(parsed.error.code, "file_not_found");
        assert!(parsed.error.message.contains("test.rs"));
        assert!(parsed.meta.is_none());
    }

    #[test]
    fn test_error_response_verbose() {
        let error = ServiceError::Lsp(LspManagerError::FileNotFound(
            "test.rs".to_string(),
        ));
        let result = error_response("test_tool", &error, OutputMode::Verbose);

        // Should be pretty printed
        assert!(result.contains('\n'));

        // Parse and verify structure
        let parsed: ErrorResponse = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.ok, false);
        assert_eq!(parsed.error.code, "file_not_found");
        assert!(parsed.meta.is_some());

        let meta = parsed.meta.unwrap();
        assert_eq!(meta.tool, "test_tool");
        assert_eq!(meta.mode, "verbose");
    }

    #[test]
    fn test_error_code_mapping() {
        assert_eq!(
            error_to_code(&ServiceError::Lsp(LspManagerError::FileNotFound(
                "".to_string()
            ))),
            "file_not_found"
        );
        assert_eq!(
            error_to_code(&ServiceError::Lsp(LspManagerError::LspClientNotFound(
                crate::api_types::SupportedLanguages::Rust
            ))),
            "lsp_client_not_found"
        );
        assert_eq!(
            error_to_code(&ServiceError::Lsp(
                LspManagerError::LspClientInitializing(crate::api_types::SupportedLanguages::Rust)
            )),
            "lsp_client_initializing"
        );
        assert_eq!(
            error_to_code(&ServiceError::IdentifierSelection(
                PositionError::IdentifierNotFound { closest: vec![] }
            )),
            "identifier_not_found"
        );
        assert_eq!(
            error_to_code(&ServiceError::Serialization("".to_string())),
            "serialization_error"
        );
    }

    #[test]
    fn test_tool_disabled_error() {
        let result = tool_disabled_error("disabled_tool", OutputMode::Default);

        let parsed: ErrorResponse = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.ok, false);
        assert_eq!(parsed.error.code, "tool_disabled");
        assert!(parsed.error.message.contains("disabled_tool"));
        assert!(parsed.meta.is_none());
    }

    #[test]
    fn it_includes_line_indexing_in_verbose_metadata() {
        let tool_name = random_irregular_string();
        let value = random_irregular_string();
        let data = json!({"symbols": [{"name": value}]});
        let response = retry_with(|| {
            let tool_name = tool_name.clone();
            let data = data.clone();
            let handle = thread::spawn(move || success_response(&tool_name, data, OutputMode::Verbose, None));
            handle.join().ok()
        });
        let parsed: SuccessResponse =
            serde_json::from_str(&response).expect("negative: response did not parse");
        let meta = parsed.meta.expect("negative: meta missing from response");
        assert_eq!(
            meta.line_indexing,
            "zero-based",
            "negative: line indexing missing or incorrect"
        );
    }

    #[test]
    fn it_includes_error_context_fields_in_payload() {
        let tool_name = random_irregular_string();
        let missing_path = random_irregular_string();
        let response = retry_with(|| {
            let tool_name = tool_name.clone();
            let missing_path = missing_path.clone();
            let handle = thread::spawn(move || {
                let error = ServiceError::Lsp(LspManagerError::FileNotFound(missing_path));
                error_response(&tool_name, &error, OutputMode::Default)
            });
            handle.join().ok()
        });
        let parsed: ErrorResponse =
            serde_json::from_str(&response).expect("negative: response did not parse");
        assert!(
            parsed.error.supported_languages.is_some(),
            "negative: supported languages missing"
        );
        assert!(
            parsed.error.workspace_root.is_some(),
            "negative: workspace root missing"
        );
    }
}
