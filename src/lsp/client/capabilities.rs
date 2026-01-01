// ABOUTME: LSP client capabilities configuration for language server negotiation
// ABOUTME: Provides standard client capabilities for document symbols, diagnostics, and code actions

use lsp_types::{
    ClientCapabilities, CodeActionClientCapabilities, CodeActionKindLiteralSupport,
    CodeActionLiteralSupport, DiagnosticTag, DocumentSymbolClientCapabilities, MarkupKind,
    PublishDiagnosticsClientCapabilities, SignatureHelpClientCapabilities,
    SelectionRangeClientCapabilities, SignatureInformationSettings, TagSupport,
    TextDocumentClientCapabilities,
};

/// Creates default client capabilities for LSP initialization
pub fn create_default_capabilities() -> ClientCapabilities {
    let mut capabilities = ClientCapabilities::default();
    capabilities.text_document = Some(TextDocumentClientCapabilities {
        document_symbol: Some(DocumentSymbolClientCapabilities {
            dynamic_registration: Some(false),
            hierarchical_document_symbol_support: Some(true),
            ..Default::default()
        }),
        publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
            related_information: Some(true),
            tag_support: Some(TagSupport {
                value_set: vec![DiagnosticTag::UNNECESSARY, DiagnosticTag::DEPRECATED],
            }),
            code_description_support: Some(true),
            data_support: Some(false),
            version_support: Some(true),
        }),
        code_action: Some(CodeActionClientCapabilities {
            dynamic_registration: Some(false),
            code_action_literal_support: Some(CodeActionLiteralSupport {
                code_action_kind: CodeActionKindLiteralSupport {
                    value_set: vec![
                        lsp_types::CodeActionKind::QUICKFIX.as_str().to_string(),
                        lsp_types::CodeActionKind::REFACTOR.as_str().to_string(),
                        lsp_types::CodeActionKind::SOURCE.as_str().to_string(),
                    ],
                },
            }),
            is_preferred_support: Some(true),
            disabled_support: Some(true),
            data_support: Some(true),
            resolve_support: None,
            honors_change_annotations: None,
        }),
        signature_help: Some(SignatureHelpClientCapabilities {
            dynamic_registration: Some(false),
            signature_information: Some(SignatureInformationSettings {
                documentation_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                active_parameter_support: Some(true),
                ..Default::default()
            }),
            context_support: Some(true),
        }),
        selection_range: Some(SelectionRangeClientCapabilities {
            dynamic_registration: Some(false),
        }),
        ..Default::default()
    });

    capabilities.experimental = Some(serde_json::json!({
        "serverStatusNotification": true
    }));
    capabilities
}
