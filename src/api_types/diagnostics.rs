// ABOUTME: Diagnostic types for LSP error and warning reporting.
// ABOUTME: Includes severity levels, diagnostic details, and aggregated response types.

use super::{Position, Range};
use serde::{Deserialize, Serialize};

/// Diagnostic severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl From<lsp_types::DiagnosticSeverity> for DiagnosticSeverity {
    fn from(severity: lsp_types::DiagnosticSeverity) -> Self {
        match severity {
            lsp_types::DiagnosticSeverity::ERROR => DiagnosticSeverity::Error,
            lsp_types::DiagnosticSeverity::WARNING => DiagnosticSeverity::Warning,
            lsp_types::DiagnosticSeverity::INFORMATION => DiagnosticSeverity::Information,
            lsp_types::DiagnosticSeverity::HINT => DiagnosticSeverity::Hint,
            _ => DiagnosticSeverity::Error,
        }
    }
}

/// Aggregated counts of diagnostics by severity level
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeverityCounts {
    /// Number of error-level diagnostics
    pub error: u32,
    /// Number of warning-level diagnostics
    pub warning: u32,
    /// Number of informational diagnostics
    pub info: u32,
    /// Number of hint-level diagnostics
    pub hint: u32,
}

/// A diagnostic message (error, warning, etc.) for a specific location
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The range where the diagnostic applies
    pub range: Range,
    /// The severity of the diagnostic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<DiagnosticSeverity>,
    /// The diagnostic's code (e.g., error number)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The source of the diagnostic (e.g., "typescript", "eslint")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The diagnostic message
    pub message: String,
    /// Whether a quick-fix code action is available for this diagnostic
    pub has_quick_fix: bool,
}

impl From<lsp_types::Diagnostic> for Diagnostic {
    fn from(diag: lsp_types::Diagnostic) -> Self {
        Self {
            range: Range {
                start: Position {
                    line: diag.range.start.line + 1,
                    character: diag.range.start.character + 1,
                },
                end: Position {
                    line: diag.range.end.line + 1,
                    character: diag.range.end.character + 1,
                },
            },
            severity: diag.severity.map(DiagnosticSeverity::from),
            code: diag.code.map(|c| match c {
                lsp_types::NumberOrString::Number(n) => n.to_string(),
                lsp_types::NumberOrString::String(s) => s,
            }),
            source: diag.source,
            message: diag.message,
            has_quick_fix: false,
        }
    }
}

/// Diagnostics for a single file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDiagnostics {
    /// Path to the file, relative to workspace root
    pub path: String,
    /// The diagnostics for this file
    pub diagnostics: Vec<Diagnostic>,
}

/// Response containing diagnostics for one or more files
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticsResponse {
    /// Total number of diagnostics across all files
    pub total_count: usize,
    /// Counts of diagnostics aggregated by severity level
    pub by_severity: SeverityCounts,
    /// Diagnostics grouped by file
    pub files: Vec<FileDiagnostics>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_severity_from_lsp() {
        assert_eq!(
            DiagnosticSeverity::from(lsp_types::DiagnosticSeverity::ERROR),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticSeverity::from(lsp_types::DiagnosticSeverity::WARNING),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            DiagnosticSeverity::from(lsp_types::DiagnosticSeverity::INFORMATION),
            DiagnosticSeverity::Information
        );
        assert_eq!(
            DiagnosticSeverity::from(lsp_types::DiagnosticSeverity::HINT),
            DiagnosticSeverity::Hint
        );
    }

    #[test]
    fn test_diagnostic_from_lsp_full() {
        let lsp_diag = lsp_types::Diagnostic {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: 10,
                    character: 5,
                },
                end: lsp_types::Position {
                    line: 10,
                    character: 15,
                },
            },
            severity: Some(lsp_types::DiagnosticSeverity::ERROR),
            code: Some(lsp_types::NumberOrString::String("E0001".to_string())),
            source: Some("rustc".to_string()),
            message: "mismatched types".to_string(),
            ..Default::default()
        };

        let diag = Diagnostic::from(lsp_diag);

        assert_eq!(diag.range.start.line, 11);
        assert_eq!(diag.range.start.character, 6);
        assert_eq!(diag.range.end.line, 11);
        assert_eq!(diag.range.end.character, 16);
        assert_eq!(diag.severity, Some(DiagnosticSeverity::Error));
        assert_eq!(diag.code, Some("E0001".to_string()));
        assert_eq!(diag.source, Some("rustc".to_string()));
        assert_eq!(diag.message, "mismatched types");
    }

    #[test]
    fn test_diagnostic_from_lsp_with_numeric_code() {
        let lsp_diag = lsp_types::Diagnostic {
            range: lsp_types::Range::default(),
            severity: Some(lsp_types::DiagnosticSeverity::WARNING),
            code: Some(lsp_types::NumberOrString::Number(42)),
            source: None,
            message: "test warning".to_string(),
            ..Default::default()
        };

        let diag = Diagnostic::from(lsp_diag);

        assert_eq!(diag.code, Some("42".to_string()));
        assert_eq!(diag.severity, Some(DiagnosticSeverity::Warning));
    }

    #[test]
    fn test_diagnostic_from_lsp_minimal() {
        let lsp_diag = lsp_types::Diagnostic {
            range: lsp_types::Range::default(),
            severity: None,
            code: None,
            source: None,
            message: "simple error".to_string(),
            ..Default::default()
        };

        let diag = Diagnostic::from(lsp_diag);

        assert_eq!(diag.severity, None);
        assert_eq!(diag.code, None);
        assert_eq!(diag.source, None);
        assert_eq!(diag.message, "simple error");
    }

    #[test]
    fn test_diagnostics_response_structure() {
        let response = DiagnosticsResponse {
            total_count: 2,
            by_severity: SeverityCounts {
                error: 1,
                warning: 1,
                info: 0,
                hint: 0,
            },
            files: vec![
                FileDiagnostics {
                    path: "src/main.rs".to_string(),
                    diagnostics: vec![Diagnostic {
                        range: Range {
                            start: Position {
                                line: 1,
                                character: 0,
                            },
                            end: Position {
                                line: 1,
                                character: 10,
                            },
                        },
                        severity: Some(DiagnosticSeverity::Error),
                        code: Some("E0001".to_string()),
                        source: Some("rustc".to_string()),
                        message: "error 1".to_string(),
                        has_quick_fix: false,
                    }],
                },
                FileDiagnostics {
                    path: "src/lib.rs".to_string(),
                    diagnostics: vec![Diagnostic {
                        range: Range {
                            start: Position {
                                line: 5,
                                character: 0,
                            },
                            end: Position {
                                line: 5,
                                character: 20,
                            },
                        },
                        severity: Some(DiagnosticSeverity::Warning),
                        code: None,
                        source: None,
                        message: "warning 1".to_string(),
                        has_quick_fix: true,
                    }],
                },
            ],
        };

        assert_eq!(response.total_count, 2);
        assert_eq!(response.by_severity.error, 1);
        assert_eq!(response.by_severity.warning, 1);
        assert_eq!(response.files.len(), 2);
        assert_eq!(response.files[0].path, "src/main.rs");
        assert_eq!(response.files[0].diagnostics.len(), 1);
        assert_eq!(response.files[1].path, "src/lib.rs");
        assert_eq!(response.files[1].diagnostics.len(), 1);
    }

    #[test]
    fn test_severity_counts_default_has_all_zeros() {
        let counts = SeverityCounts::default();

        assert_eq!(counts.error, 0, "default error count must be zero");
        assert_eq!(counts.warning, 0, "default warning count must be zero");
        assert_eq!(counts.info, 0, "default info count must be zero");
        assert_eq!(counts.hint, 0, "default hint count must be zero");
    }

    #[test]
    fn test_severity_counts_serialization_roundtrip() {
        let counts = SeverityCounts {
            error: 3,
            warning: 5,
            info: 2,
            hint: 1,
        };

        let json = serde_json::to_string(&counts).expect("failed to serialize severity counts");
        let deserialized: SeverityCounts =
            serde_json::from_str(&json).expect("failed to deserialize severity counts");

        assert_eq!(
            counts, deserialized,
            "severity counts must survive serialization roundtrip"
        );
    }

    #[test]
    fn test_diagnostics_response_includes_by_severity() {
        let response = DiagnosticsResponse {
            total_count: 4,
            by_severity: SeverityCounts {
                error: 2,
                warning: 1,
                info: 1,
                hint: 0,
            },
            files: vec![],
        };

        let json =
            serde_json::to_value(&response).expect("failed to serialize diagnostics response");

        assert!(
            json.get("by_severity").is_some(),
            "by_severity field must be present in serialized output"
        );
        assert_eq!(json["by_severity"]["error"], 2, "error count must match");
        assert_eq!(
            json["by_severity"]["warning"], 1,
            "warning count must match"
        );
    }

    #[test]
    fn test_diagnostic_has_quick_fix_field_serializes() {
        let diag_with_fix = Diagnostic {
            range: Range {
                start: Position {
                    line: 1,
                    character: 1,
                },
                end: Position {
                    line: 1,
                    character: 10,
                },
            },
            severity: Some(DiagnosticSeverity::Error),
            code: Some("E0001".to_string()),
            source: Some("rustc".to_string()),
            message: "unused variable".to_string(),
            has_quick_fix: true,
        };

        let diag_without_fix = Diagnostic {
            range: Range {
                start: Position {
                    line: 2,
                    character: 1,
                },
                end: Position {
                    line: 2,
                    character: 15,
                },
            },
            severity: Some(DiagnosticSeverity::Warning),
            code: None,
            source: None,
            message: "some warning".to_string(),
            has_quick_fix: false,
        };

        let json_with = serde_json::to_value(&diag_with_fix).expect("failed to serialize");
        let json_without = serde_json::to_value(&diag_without_fix).expect("failed to serialize");

        assert_eq!(
            json_with["has_quick_fix"], true,
            "has_quick_fix must be true when set"
        );
        assert_eq!(
            json_without["has_quick_fix"], false,
            "has_quick_fix must be false when not set"
        );
    }

    #[test]
    fn test_diagnostic_from_lsp_sets_has_quick_fix_false() {
        let lsp_diag = lsp_types::Diagnostic {
            range: lsp_types::Range::default(),
            severity: Some(lsp_types::DiagnosticSeverity::ERROR),
            code: None,
            source: None,
            message: "test error".to_string(),
            ..Default::default()
        };

        let diag = Diagnostic::from(lsp_diag);

        assert_eq!(
            diag.has_quick_fix, false,
            "diagnostic from LSP must have has_quick_fix set to false by default"
        );
    }
}
