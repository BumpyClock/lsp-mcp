// ABOUTME: Diagnostics operations (get_diagnostics).
// ABOUTME: Handles fetching errors, warnings, and hints from LSP servers.

use crate::api_types::{Diagnostic, DiagnosticSeverity, DiagnosticsResponse, FileDiagnostics, SeverityCounts};
use crate::lsp::manager::Manager;
use std::sync::Arc;

use crate::service::types::errors::ServiceError;

/// Gets diagnostics (errors, warnings, hints) for a file or the entire workspace.
///
/// If `file_path` is provided (relative to workspace root), returns diagnostics for that file only.
/// If None, returns all diagnostics from all language clients.
pub(crate) async fn get_diagnostics_impl(
    manager: &Arc<Manager>,
    file_path: Option<&str>,
) -> Result<DiagnosticsResponse, ServiceError> {
    let raw_diagnostics = manager.get_diagnostics(file_path).await?;

    let mut files: Vec<FileDiagnostics> = Vec::new();
    let mut by_severity = SeverityCounts::default();

    for (path, lsp_diagnostics) in raw_diagnostics {
        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        for lsp_diag in lsp_diagnostics {
            let lsp_range = lsp_diag.range;
            let lsp_diag_clone = lsp_diag.clone();
            let mut diag = Diagnostic::from(lsp_diag);

            match diag.severity {
                Some(DiagnosticSeverity::Error) => by_severity.error += 1,
                Some(DiagnosticSeverity::Warning) => by_severity.warning += 1,
                Some(DiagnosticSeverity::Information) => by_severity.info += 1,
                Some(DiagnosticSeverity::Hint) => by_severity.hint += 1,
                None => {}
            }

            if let Ok(Some(actions)) = manager
                .code_action(&path, lsp_range, vec![lsp_diag_clone])
                .await
            {
                diag.has_quick_fix = actions.iter().any(|action| {
                    match action {
                        lsp_types::CodeActionOrCommand::CodeAction(ca) => ca
                            .kind
                            .as_ref()
                            .is_some_and(|k| k.as_str().starts_with("quickfix")),
                        lsp_types::CodeActionOrCommand::Command(_) => false,
                    }
                });
            }

            diagnostics.push(diag);
        }

        files.push(FileDiagnostics { path, diagnostics });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    let total_count: usize = files.iter().map(|f| f.diagnostics.len()).sum();

    Ok(DiagnosticsResponse {
        total_count,
        by_severity,
        files,
    })
}
