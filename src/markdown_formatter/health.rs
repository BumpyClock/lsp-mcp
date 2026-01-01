// ABOUTME: Markdown formatter for health check response types.
// ABOUTME: Converts server health status to readable markdown with status indicators.

use super::ToMarkdown;
use crate::api_types::{HealthResponse, LspStatus, SupportedLanguages};

impl ToMarkdown for LspStatus {
    fn to_markdown(&self) -> String {
        match self {
            LspStatus::Ready => "ready".to_string(),
            LspStatus::Initializing => "initializing".to_string(),
            LspStatus::Unavailable => "unavailable".to_string(),
        }
    }
}

impl ToMarkdown for SupportedLanguages {
    fn to_markdown(&self) -> String {
        match self {
            SupportedLanguages::Python => "python".to_string(),
            SupportedLanguages::TypeScriptJavaScript => "typescript".to_string(),
            SupportedLanguages::Rust => "rust".to_string(),
            SupportedLanguages::CPP => "cpp".to_string(),
            SupportedLanguages::CSharp => "csharp".to_string(),
            SupportedLanguages::Java => "java".to_string(),
            SupportedLanguages::Golang => "golang".to_string(),
            SupportedLanguages::PHP => "php".to_string(),
            SupportedLanguages::Ruby => "ruby".to_string(),
        }
    }
}

impl ToMarkdown for HealthResponse {
    fn to_markdown(&self) -> String {
        let mut output = String::new();

        if self.debug_mode == Some(true) {
            output.push_str("## Debug Mode Active\n\n");
            if let Some(log_file) = &self.log_file {
                output.push_str(&format!("Log file: {}\n\n", log_file));
            }
            output.push_str("**Important**: If tool responses seem incomplete or low quality, inspect the log file\n");
            output.push_str("to identify discrepancies between what the LSP returned and what was formatted.\n");
            output.push_str("Report any issues to the user.\n\n");
            output.push_str("---\n\n");
        }

        output.push_str("LSP-MCP Health\n\n");
        output.push_str(&format!("Status: {}\n", self.status));
        output.push_str(&format!("Version: {}\n", self.version));

        if let Some(session_id) = &self.session_id {
            output.push_str(&format!("Session ID: {}\n", session_id));
        }
        if self.debug_mode != Some(true) {
            if let Some(log_file) = &self.log_file {
                output.push_str(&format!("Log file: {}\n", log_file));
            }
        }

        if !self.languages.is_empty() {
            output.push_str("\nLanguages\n");

            let mut sorted_languages: Vec<_> = self.languages.iter().collect();
            sorted_languages.sort_by_key(|(lang, _)| lang.to_markdown());

            for (language, status) in sorted_languages {
                output.push_str(&format!(
                    "  {} - {}\n",
                    language.to_markdown(),
                    status.to_markdown()
                ));
            }
        }

        if let Some(semantic) = &self.semantic_search {
            output.push_str("\nSemantic Search\n");
            output.push_str(&format!("  enabled: {}\n", semantic.enabled));
            if let Some(state) = &semantic.state {
                output.push_str(&format!("  state: {}\n", state));
            }
            if let Some(provider) = &semantic.embedder_provider {
                output.push_str(&format!("  embedder: {}\n", provider));
            }
            if let Some(model) = &semantic.embedder_model {
                output.push_str(&format!("  model: {}\n", model));
            }
            if let Some(dimension) = semantic.embedder_dimension {
                output.push_str(&format!("  dimension: {}\n", dimension));
            }
            if let Some(stored_dimension) = semantic.stored_dimension {
                output.push_str(&format!("  stored dimension: {}\n", stored_dimension));
            }
            if let Some(mismatch) = semantic.dimension_mismatch {
                output.push_str(&format!("  dimension mismatch: {}\n", mismatch));
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_health_response(
        status: &str,
        version: &str,
        languages: Vec<(SupportedLanguages, LspStatus)>,
    ) -> HealthResponse {
        HealthResponse {
            status: status.to_string(),
            version: version.to_string(),
            languages: languages.into_iter().collect(),
            debug_mode: None,
            session_id: None,
            log_file: None,
            semantic_search: None,
        }
    }

    #[test]
    fn lsp_status_ready_renders_as_lowercase_ready() {
        let status = LspStatus::Ready;

        let result = status.to_markdown();

        assert_eq!(
            result, "ready",
            "negative: Ready status must render as lowercase 'ready'"
        );
    }

    #[test]
    fn lsp_status_initializing_renders_as_lowercase_initializing() {
        let status = LspStatus::Initializing;

        let result = status.to_markdown();

        assert_eq!(
            result, "initializing",
            "negative: Initializing status must render as lowercase 'initializing'"
        );
    }

    #[test]
    fn lsp_status_unavailable_renders_as_lowercase_unavailable() {
        let status = LspStatus::Unavailable;

        let result = status.to_markdown();

        assert_eq!(
            result, "unavailable",
            "negative: Unavailable status must render as lowercase 'unavailable'"
        );
    }

    #[test]
    fn supported_language_python_renders_as_python() {
        let lang = SupportedLanguages::Python;

        let result = lang.to_markdown();

        assert_eq!(
            result, "python",
            "negative: Python must render as 'python'"
        );
    }

    #[test]
    fn supported_language_typescript_javascript_renders_as_typescript() {
        let lang = SupportedLanguages::TypeScriptJavaScript;

        let result = lang.to_markdown();

        assert_eq!(
            result, "typescript",
            "negative: TypeScriptJavaScript must render as 'typescript'"
        );
    }

    #[test]
    fn supported_language_rust_renders_as_rust() {
        let lang = SupportedLanguages::Rust;

        let result = lang.to_markdown();

        assert_eq!(
            result, "rust",
            "negative: Rust must render as 'rust'"
        );
    }

    #[test]
    fn supported_language_cpp_renders_as_cpp() {
        let lang = SupportedLanguages::CPP;

        let result = lang.to_markdown();

        assert_eq!(
            result, "cpp",
            "negative: CPP must render as 'cpp'"
        );
    }

    #[test]
    fn supported_language_csharp_renders_as_csharp() {
        let lang = SupportedLanguages::CSharp;

        let result = lang.to_markdown();

        assert_eq!(
            result, "csharp",
            "negative: CSharp must render as 'csharp'"
        );
    }

    #[test]
    fn supported_language_java_renders_as_java() {
        let lang = SupportedLanguages::Java;

        let result = lang.to_markdown();

        assert_eq!(
            result, "java",
            "negative: Java must render as 'java'"
        );
    }

    #[test]
    fn supported_language_golang_renders_as_golang() {
        let lang = SupportedLanguages::Golang;

        let result = lang.to_markdown();

        assert_eq!(
            result, "golang",
            "negative: Golang must render as 'golang'"
        );
    }

    #[test]
    fn supported_language_php_renders_as_php() {
        let lang = SupportedLanguages::PHP;

        let result = lang.to_markdown();

        assert_eq!(
            result, "php",
            "negative: PHP must render as 'php'"
        );
    }

    #[test]
    fn supported_language_ruby_renders_as_ruby() {
        let lang = SupportedLanguages::Ruby;

        let result = lang.to_markdown();

        assert_eq!(
            result, "ruby",
            "negative: Ruby must render as 'ruby'"
        );
    }

    #[test]
    fn health_response_contains_header() {
        let response = create_health_response("ok", "1.0.0", vec![]);

        let result = response.to_markdown();

        assert!(
            result.contains("LSP-MCP Health"),
            "negative: health response must contain header"
        );
        assert!(
            !result.contains("##"),
            "negative: health response must not contain markdown headers"
        );
    }

    #[test]
    fn health_response_contains_status() {
        let response = create_health_response("ok", "1.0.0", vec![]);

        let result = response.to_markdown();

        assert!(
            result.contains("Status: ok"),
            "negative: health response must contain status"
        );
        assert!(
            !result.contains("**Status:**"),
            "negative: health response must not contain bold status"
        );
    }

    #[test]
    fn health_response_contains_version() {
        let response = create_health_response("ok", "0.4.4", vec![]);

        let result = response.to_markdown();

        assert!(
            result.contains("Version: 0.4.4"),
            "negative: health response must contain version"
        );
        assert!(
            !result.contains("**Version:**"),
            "negative: health response must not contain bold version"
        );
    }

    #[test]
    fn health_response_without_languages_omits_languages_section() {
        let response = create_health_response("ok", "1.0.0", vec![]);

        let result = response.to_markdown();

        assert!(
            !result.contains("Languages"),
            "negative: health response without languages must not contain Languages section"
        );
    }

    #[test]
    fn health_response_with_languages_contains_languages_section() {
        let response = create_health_response(
            "ok",
            "1.0.0",
            vec![(SupportedLanguages::Rust, LspStatus::Ready)],
        );

        let result = response.to_markdown();

        assert!(
            result.contains("Languages"),
            "negative: health response with languages must contain Languages section"
        );
        assert!(
            !result.contains("###"),
            "negative: health response must not contain markdown headers"
        );
    }

    #[test]
    fn health_response_renders_language_with_status() {
        let response = create_health_response(
            "ok",
            "1.0.0",
            vec![(SupportedLanguages::Rust, LspStatus::Ready)],
        );

        let result = response.to_markdown();

        assert!(
            result.contains("  rust - ready"),
            "negative: health response must render language with status"
        );
        assert!(
            !result.contains("- **"),
            "negative: health response must not contain bullet with bold"
        );
    }

    #[test]
    fn health_response_renders_initializing_language() {
        let response = create_health_response(
            "ok",
            "1.0.0",
            vec![(SupportedLanguages::Python, LspStatus::Initializing)],
        );

        let result = response.to_markdown();

        assert!(
            result.contains("  python - initializing"),
            "negative: health response must render initializing language"
        );
    }

    #[test]
    fn health_response_renders_unavailable_language() {
        let response = create_health_response(
            "ok",
            "1.0.0",
            vec![(SupportedLanguages::Golang, LspStatus::Unavailable)],
        );

        let result = response.to_markdown();

        assert!(
            result.contains("  golang - unavailable"),
            "negative: health response must render unavailable language"
        );
    }

    #[test]
    fn health_response_sorts_languages_alphabetically() {
        let response = create_health_response(
            "ok",
            "1.0.0",
            vec![
                (SupportedLanguages::Rust, LspStatus::Ready),
                (SupportedLanguages::Golang, LspStatus::Ready),
                (SupportedLanguages::Python, LspStatus::Ready),
            ],
        );

        let result = response.to_markdown();

        let golang_pos = result.find("golang").expect("golang not found");
        let python_pos = result.find("python").expect("python not found");
        let rust_pos = result.find("rust").expect("rust not found");

        assert!(
            golang_pos < python_pos,
            "negative: golang must appear before python"
        );
        assert!(
            python_pos < rust_pos,
            "negative: python must appear before rust"
        );
    }

    #[test]
    fn health_response_renders_multiple_languages_with_mixed_status() {
        let response = create_health_response(
            "ok",
            "0.4.4",
            vec![
                (SupportedLanguages::TypeScriptJavaScript, LspStatus::Ready),
                (SupportedLanguages::Rust, LspStatus::Ready),
                (SupportedLanguages::Python, LspStatus::Initializing),
                (SupportedLanguages::Golang, LspStatus::Unavailable),
            ],
        );

        let result = response.to_markdown();

        assert!(
            result.contains("  typescript - ready"),
            "negative: health response must render typescript ready"
        );
        assert!(
            result.contains("  rust - ready"),
            "negative: health response must render rust ready"
        );
        assert!(
            result.contains("  python - initializing"),
            "negative: health response must render python initializing"
        );
        assert!(
            result.contains("  golang - unavailable"),
            "negative: health response must render golang unavailable"
        );
    }

    #[test]
    fn health_response_handles_unicode_in_version() {
        let response = create_health_response("ok", "1.0.0-α", vec![]);

        let result = response.to_markdown();

        assert!(
            result.contains("Version: 1.0.0-α"),
            "negative: health response must preserve unicode in version"
        );
    }

    #[test]
    fn health_response_handles_error_status() {
        let response = create_health_response("error: connection failed", "1.0.0", vec![]);

        let result = response.to_markdown();

        assert!(
            result.contains("Status: error: connection failed"),
            "negative: health response must preserve error status message"
        );
    }

    #[test]
    fn health_response_includes_session_id_when_present() {
        let response = HealthResponse {
            status: "ok".to_string(),
            version: "1.0.0".to_string(),
            languages: Default::default(),
            debug_mode: None,
            session_id: Some("abc123-def456".to_string()),
            log_file: None,
            semantic_search: None,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("Session ID: abc123-def456"),
            "negative: health response must contain session ID"
        );
    }

    #[test]
    fn health_response_includes_log_file_when_present() {
        let response = HealthResponse {
            status: "ok".to_string(),
            version: "1.0.0".to_string(),
            languages: Default::default(),
            debug_mode: None,
            session_id: None,
            log_file: Some(".lsp-mcp/logs/sessions/abc.log".to_string()),
            semantic_search: None,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("Log file: .lsp-mcp/logs/sessions/abc.log"),
            "negative: health response must contain log file path"
        );
    }

    #[test]
    fn health_response_omits_session_info_when_not_present() {
        let response = create_health_response("ok", "1.0.0", vec![]);

        let result = response.to_markdown();

        assert!(
            !result.contains("Session ID:"),
            "negative: health response must not contain Session ID when not set"
        );
        assert!(
            !result.contains("Log file:"),
            "negative: health response must not contain Log file when not set"
        );
    }

    #[test]
    fn health_response_includes_debug_guidance_when_debug_mode_enabled() {
        let response = HealthResponse {
            status: "ok".to_string(),
            version: "1.0.0".to_string(),
            languages: Default::default(),
            debug_mode: Some(true),
            session_id: Some("test-session".to_string()),
            log_file: Some("/path/to/log.log".to_string()),
            semantic_search: None,
        };

        let result = response.to_markdown();

        assert!(
            result.contains("## Debug Mode Active"),
            "negative: debug mode response must contain debug mode header"
        );
        assert!(
            result.contains("Log file: /path/to/log.log"),
            "negative: debug mode response must contain log file path"
        );
        assert!(
            result.contains("inspect the log file"),
            "negative: debug mode response must contain log inspection guidance"
        );
        assert!(
            result.contains("Report any issues to the user"),
            "negative: debug mode response must instruct to report issues"
        );
    }

    #[test]
    fn health_response_debug_section_appears_before_health_header() {
        let response = HealthResponse {
            status: "ok".to_string(),
            version: "1.0.0".to_string(),
            languages: Default::default(),
            debug_mode: Some(true),
            session_id: None,
            log_file: Some("/path/to/log.log".to_string()),
            semantic_search: None,
        };

        let result = response.to_markdown();

        let debug_pos = result.find("## Debug Mode Active").expect("debug header not found");
        let health_pos = result.find("LSP-MCP Health").expect("health header not found");

        assert!(
            debug_pos < health_pos,
            "negative: debug section must appear before health header"
        );
    }

    #[test]
    fn health_response_omits_debug_section_when_debug_mode_not_enabled() {
        let response = create_health_response("ok", "1.0.0", vec![]);

        let result = response.to_markdown();

        assert!(
            !result.contains("## Debug Mode Active"),
            "negative: non-debug response must not contain debug mode header"
        );
        assert!(
            !result.contains("inspect the log file"),
            "negative: non-debug response must not contain log inspection guidance"
        );
    }
}
