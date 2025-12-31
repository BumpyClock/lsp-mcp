// ABOUTME: Markdown formatter for diagnostics response types.
// ABOUTME: Converts diagnostic results to readable markdown with severity indicators.

use super::ToMarkdown;
use crate::api_types::{Diagnostic, DiagnosticSeverity, DiagnosticsResponse, SeverityCounts};

impl ToMarkdown for DiagnosticsResponse {
    fn to_markdown(&self) -> String {
        let mut output = format!("Diagnostics ({} total)\n\n", self.total_count);

        if self.total_count == 0 {
            output.push_str("No diagnostics found.\n");
            return output;
        }

        output.push_str(&format!("Summary: {}\n\n", format_severity_summary(&self.by_severity)));

        for file in &self.files {
            let issue_word = if file.diagnostics.len() == 1 { "issue" } else { "issues" };
            output.push_str(&format!(
                "{} ({} {})\n",
                file.path,
                file.diagnostics.len(),
                issue_word
            ));

            for diag in &file.diagnostics {
                output.push_str(&format_diagnostic(diag));
            }
            output.push('\n');
        }

        output
    }
}

fn format_severity_summary(counts: &SeverityCounts) -> String {
    let mut parts = Vec::new();

    if counts.error > 0 {
        let word = if counts.error == 1 { "error" } else { "errors" };
        parts.push(format!("{} {}", counts.error, word));
    }

    if counts.warning > 0 {
        let word = if counts.warning == 1 { "warning" } else { "warnings" };
        parts.push(format!("{} {}", counts.warning, word));
    }

    if counts.info > 0 {
        let word = if counts.info == 1 { "info" } else { "infos" };
        parts.push(format!("{} {}", counts.info, word));
    }

    if counts.hint > 0 {
        let word = if counts.hint == 1 { "hint" } else { "hints" };
        parts.push(format!("{} {}", counts.hint, word));
    }

    if parts.is_empty() {
        return "none".to_string();
    }

    parts.join(", ")
}

fn format_diagnostic(diag: &Diagnostic) -> String {
    let severity_str = match diag.severity {
        Some(DiagnosticSeverity::Error) => "Error",
        Some(DiagnosticSeverity::Warning) => "Warning",
        Some(DiagnosticSeverity::Information) => "Info",
        Some(DiagnosticSeverity::Hint) => "Hint",
        None => "Unknown",
    };

    let position = format!("Line {}:{}", diag.range.start.line, diag.range.start.character);

    let code_or_source = diag.code.as_ref()
        .or(diag.source.as_ref())
        .map(|s| format!(" [{}]", s))
        .unwrap_or_default();

    let quick_fix = if diag.has_quick_fix { " [quick-fix]" } else { "" };

    format!(
        "  {} {} - `{}`{}{}\n",
        severity_str,
        position,
        diag.message,
        code_or_source,
        quick_fix
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{FileDiagnostics, Position, Range};
    use rand::Rng;

    fn random_line() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..1000)
    }

    fn random_char() -> u32 {
        let mut rng = rand::rng();
        rng.random_range(1..200)
    }

    fn make_diagnostic(
        line: u32,
        char: u32,
        severity: Option<DiagnosticSeverity>,
        code: Option<&str>,
        source: Option<&str>,
        message: &str,
        has_quick_fix: bool,
    ) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position {
                    line,
                    character: char,
                },
                end: Position {
                    line,
                    character: char + 10,
                },
            },
            severity,
            code: code.map(String::from),
            source: source.map(String::from),
            message: message.to_string(),
            has_quick_fix,
        }
    }

    #[test]
    fn it_formats_empty_diagnostics_gracefully() {
        let response = DiagnosticsResponse {
            total_count: 0,
            by_severity: SeverityCounts::default(),
            files: vec![],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Diagnostics (0 total)"),
            "negative: empty diagnostics must show zero count in header"
        );
        assert!(
            markdown.contains("No diagnostics"),
            "negative: empty diagnostics must indicate no issues found"
        );
    }

    #[test]
    fn it_shows_total_count_in_header() {
        let line = random_line();
        let response = DiagnosticsResponse {
            total_count: 5,
            by_severity: SeverityCounts {
                error: 2,
                warning: 2,
                info: 1,
                hint: 0,
            },
            files: vec![FileDiagnostics {
                path: "src/main.rs".to_string(),
                diagnostics: vec![
                    make_diagnostic(line, 1, Some(DiagnosticSeverity::Error), None, None, "e1", false),
                    make_diagnostic(line + 1, 1, Some(DiagnosticSeverity::Error), None, None, "e2", false),
                    make_diagnostic(line + 2, 1, Some(DiagnosticSeverity::Warning), None, None, "w1", false),
                    make_diagnostic(line + 3, 1, Some(DiagnosticSeverity::Warning), None, None, "w2", false),
                    make_diagnostic(line + 4, 1, Some(DiagnosticSeverity::Information), None, None, "i1", false),
                ],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Diagnostics (5 total)"),
            "negative: header must contain total count"
        );
    }

    #[test]
    fn it_shows_severity_summary_with_all_types() {
        let response = DiagnosticsResponse {
            total_count: 12,
            by_severity: SeverityCounts {
                error: 3,
                warning: 7,
                info: 0,
                hint: 2,
            },
            files: vec![],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("3 errors"),
            "negative: summary must include error count"
        );
        assert!(
            markdown.contains("7 warnings"),
            "negative: summary must include warning count"
        );
        assert!(
            markdown.contains("2 hints"),
            "negative: summary must include hint count"
        );
    }

    #[test]
    fn it_omits_zero_count_severities_from_summary() {
        let response = DiagnosticsResponse {
            total_count: 5,
            by_severity: SeverityCounts {
                error: 3,
                warning: 2,
                info: 0,
                hint: 0,
            },
            files: vec![],
        };

        let markdown = response.to_markdown();

        assert!(
            !markdown.contains("0 info"),
            "negative: summary must not include zero info count"
        );
        assert!(
            !markdown.contains("0 hints"),
            "negative: summary must not include zero hint count"
        );
    }

    #[test]
    fn it_groups_diagnostics_by_file_with_issue_count() {
        let line = random_line();
        let response = DiagnosticsResponse {
            total_count: 8,
            by_severity: SeverityCounts {
                error: 3,
                warning: 5,
                info: 0,
                hint: 0,
            },
            files: vec![
                FileDiagnostics {
                    path: "src/main.ts".to_string(),
                    diagnostics: vec![
                        make_diagnostic(line, 1, Some(DiagnosticSeverity::Error), None, None, "e1", false),
                        make_diagnostic(line + 1, 1, Some(DiagnosticSeverity::Error), None, None, "e2", false),
                        make_diagnostic(line + 2, 1, Some(DiagnosticSeverity::Warning), None, None, "w1", false),
                        make_diagnostic(line + 3, 1, Some(DiagnosticSeverity::Warning), None, None, "w2", false),
                        make_diagnostic(line + 4, 1, Some(DiagnosticSeverity::Warning), None, None, "w3", false),
                    ],
                },
                FileDiagnostics {
                    path: "src/utils.ts".to_string(),
                    diagnostics: vec![
                        make_diagnostic(line, 1, Some(DiagnosticSeverity::Error), None, None, "e3", false),
                        make_diagnostic(line + 1, 1, Some(DiagnosticSeverity::Warning), None, None, "w4", false),
                        make_diagnostic(line + 2, 1, Some(DiagnosticSeverity::Warning), None, None, "w5", false),
                    ],
                },
            ],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("src/main.ts (5 issues)"),
            "negative: first file section must have correct issue count"
        );
        assert!(
            markdown.contains("src/utils.ts (3 issues)"),
            "negative: second file section must have correct issue count"
        );
    }

    #[test]
    fn it_formats_diagnostic_with_severity_line_and_message() {
        let line = random_line();
        let char = random_char();
        let response = DiagnosticsResponse {
            total_count: 1,
            by_severity: SeverityCounts {
                error: 1,
                warning: 0,
                info: 0,
                hint: 0,
            },
            files: vec![FileDiagnostics {
                path: "src/main.ts".to_string(),
                diagnostics: vec![make_diagnostic(
                    line,
                    char,
                    Some(DiagnosticSeverity::Error),
                    None,
                    None,
                    "Type 'string' is not assignable to type 'number'",
                    false,
                )],
            }],
        };

        let markdown = response.to_markdown();
        let expected_pos = format!("Line {}:{}", line, char);

        assert!(
            markdown.contains("  Error"),
            "negative: diagnostic must show severity with 2-space indent"
        );
        assert!(
            markdown.contains(&expected_pos),
            "negative: diagnostic must show line and character position"
        );
        assert!(
            markdown.contains("Type 'string' is not assignable to type 'number'"),
            "negative: diagnostic must show message"
        );
    }

    #[test]
    fn it_includes_code_in_brackets_when_present() {
        let response = DiagnosticsResponse {
            total_count: 1,
            by_severity: SeverityCounts {
                error: 1,
                warning: 0,
                info: 0,
                hint: 0,
            },
            files: vec![FileDiagnostics {
                path: "src/main.ts".to_string(),
                diagnostics: vec![make_diagnostic(
                    45,
                    12,
                    Some(DiagnosticSeverity::Error),
                    Some("ts(2322)"),
                    None,
                    "Type mismatch",
                    false,
                )],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[ts(2322)]"),
            "negative: diagnostic with code must show code in brackets"
        );
    }

    #[test]
    fn it_includes_source_in_brackets_when_code_absent_but_source_present() {
        let response = DiagnosticsResponse {
            total_count: 1,
            by_severity: SeverityCounts {
                error: 0,
                warning: 1,
                info: 0,
                hint: 0,
            },
            files: vec![FileDiagnostics {
                path: "src/main.ts".to_string(),
                diagnostics: vec![make_diagnostic(
                    23,
                    1,
                    Some(DiagnosticSeverity::Warning),
                    None,
                    Some("eslint"),
                    "Unused variable",
                    false,
                )],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[eslint]"),
            "negative: diagnostic with source but no code must show source in brackets"
        );
    }

    #[test]
    fn it_adds_quick_fix_indicator_when_available() {
        let response = DiagnosticsResponse {
            total_count: 1,
            by_severity: SeverityCounts {
                error: 0,
                warning: 0,
                info: 0,
                hint: 1,
            },
            files: vec![FileDiagnostics {
                path: "src/main.ts".to_string(),
                diagnostics: vec![make_diagnostic(
                    12,
                    1,
                    Some(DiagnosticSeverity::Hint),
                    None,
                    None,
                    "Convert to arrow function",
                    true,
                )],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[quick-fix]"),
            "negative: diagnostic with quick fix must show indicator"
        );
    }

    #[test]
    fn it_shows_both_code_and_quick_fix_when_both_present() {
        let response = DiagnosticsResponse {
            total_count: 1,
            by_severity: SeverityCounts {
                error: 0,
                warning: 1,
                info: 0,
                hint: 0,
            },
            files: vec![FileDiagnostics {
                path: "src/main.ts".to_string(),
                diagnostics: vec![make_diagnostic(
                    50,
                    10,
                    Some(DiagnosticSeverity::Warning),
                    Some("no-unused-vars"),
                    Some("eslint"),
                    "Declared but never used",
                    true,
                )],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("[no-unused-vars]"),
            "negative: diagnostic with code must show code"
        );
        assert!(
            markdown.contains("[quick-fix]"),
            "negative: diagnostic with quick fix must show indicator"
        );
    }

    #[test]
    fn it_handles_diagnostic_without_severity() {
        let response = DiagnosticsResponse {
            total_count: 1,
            by_severity: SeverityCounts::default(),
            files: vec![FileDiagnostics {
                path: "src/main.rs".to_string(),
                diagnostics: vec![make_diagnostic(
                    10,
                    5,
                    None,
                    None,
                    None,
                    "Unknown diagnostic",
                    false,
                )],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Unknown diagnostic"),
            "negative: diagnostic without severity must still show message"
        );
        assert!(
            markdown.contains("Line 10:5"),
            "negative: diagnostic without severity must still show position"
        );
    }

    #[test]
    fn it_uses_singular_issue_for_single_diagnostic_per_file() {
        let response = DiagnosticsResponse {
            total_count: 1,
            by_severity: SeverityCounts {
                error: 1,
                warning: 0,
                info: 0,
                hint: 0,
            },
            files: vec![FileDiagnostics {
                path: "src/single.rs".to_string(),
                diagnostics: vec![make_diagnostic(
                    1,
                    1,
                    Some(DiagnosticSeverity::Error),
                    None,
                    None,
                    "single error",
                    false,
                )],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("(1 issue)"),
            "negative: single issue must use singular form"
        );
    }

    #[test]
    fn it_uses_singular_forms_in_summary_for_count_of_one() {
        let response = DiagnosticsResponse {
            total_count: 3,
            by_severity: SeverityCounts {
                error: 1,
                warning: 1,
                info: 0,
                hint: 1,
            },
            files: vec![],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("1 error") && !markdown.contains("1 errors"),
            "negative: single error must use singular form"
        );
        assert!(
            markdown.contains("1 warning") && !markdown.contains("1 warnings"),
            "negative: single warning must use singular form"
        );
        assert!(
            markdown.contains("1 hint") && !markdown.contains("1 hints"),
            "negative: single hint must use singular form"
        );
    }

    #[test]
    fn it_formats_all_severity_types_correctly() {
        let response = DiagnosticsResponse {
            total_count: 4,
            by_severity: SeverityCounts {
                error: 1,
                warning: 1,
                info: 1,
                hint: 1,
            },
            files: vec![FileDiagnostics {
                path: "src/all.rs".to_string(),
                diagnostics: vec![
                    make_diagnostic(1, 1, Some(DiagnosticSeverity::Error), None, None, "err", false),
                    make_diagnostic(2, 1, Some(DiagnosticSeverity::Warning), None, None, "warn", false),
                    make_diagnostic(3, 1, Some(DiagnosticSeverity::Information), None, None, "info", false),
                    make_diagnostic(4, 1, Some(DiagnosticSeverity::Hint), None, None, "hint", false),
                ],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("  Error Line"),
            "negative: error severity must be rendered with 2-space indent"
        );
        assert!(
            markdown.contains("  Warning Line"),
            "negative: warning severity must be rendered with 2-space indent"
        );
        assert!(
            markdown.contains("  Info Line"),
            "negative: information severity must be rendered as Info with 2-space indent"
        );
        assert!(
            markdown.contains("  Hint Line"),
            "negative: hint severity must be rendered with 2-space indent"
        );
    }

    #[test]
    fn it_uses_indented_format_for_diagnostics() {
        let response = DiagnosticsResponse {
            total_count: 2,
            by_severity: SeverityCounts {
                error: 2,
                warning: 0,
                info: 0,
                hint: 0,
            },
            files: vec![FileDiagnostics {
                path: "src/main.rs".to_string(),
                diagnostics: vec![
                    make_diagnostic(10, 1, Some(DiagnosticSeverity::Error), None, None, "first", false),
                    make_diagnostic(20, 1, Some(DiagnosticSeverity::Error), None, None, "second", false),
                ],
            }],
        };

        let markdown = response.to_markdown();
        let indent_count = markdown.matches("\n  Error").count();

        assert!(
            indent_count >= 2,
            "negative: each diagnostic must be formatted with 2-space indentation"
        );
    }

    #[test]
    fn it_handles_message_with_backticks() {
        let response = DiagnosticsResponse {
            total_count: 1,
            by_severity: SeverityCounts {
                error: 1,
                warning: 0,
                info: 0,
                hint: 0,
            },
            files: vec![FileDiagnostics {
                path: "src/main.rs".to_string(),
                diagnostics: vec![make_diagnostic(
                    10,
                    1,
                    Some(DiagnosticSeverity::Error),
                    None,
                    None,
                    "Cannot find name `unknownVar`",
                    false,
                )],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("Cannot find name `unknownVar`"),
            "negative: message with backticks must be preserved"
        );
    }

    #[test]
    fn it_handles_unicode_in_path_and_message() {
        let response = DiagnosticsResponse {
            total_count: 1,
            by_severity: SeverityCounts {
                error: 1,
                warning: 0,
                info: 0,
                hint: 0,
            },
            files: vec![FileDiagnostics {
                path: "src/日本語.rs".to_string(),
                diagnostics: vec![make_diagnostic(
                    10,
                    1,
                    Some(DiagnosticSeverity::Error),
                    None,
                    None,
                    "変数が見つかりません",
                    false,
                )],
            }],
        };

        let markdown = response.to_markdown();

        assert!(
            markdown.contains("src/日本語.rs"),
            "negative: unicode path must be preserved"
        );
        assert!(
            markdown.contains("変数が見つかりません"),
            "negative: unicode message must be preserved"
        );
    }
}
