// ABOUTME: LSP client creation and startup logic for language server initialization
// ABOUTME: Provides factory method for creating language-specific LSP clients

use crate::api_types::SupportedLanguages;
use crate::lsp::client::LspClient;
use crate::lsp::registry::LanguageMetadata;
use notify_debouncer_mini::DebouncedEvent;
use std::error::Error;

/// Creates an LSP client for the given language using the registry factory
pub async fn create_lsp_client(
    lsp: SupportedLanguages,
    workspace_path: &str,
    events_rx: tokio::sync::broadcast::Receiver<DebouncedEvent>,
    binary: Option<&str>,
) -> Result<Box<dyn LspClient>, Box<dyn Error + Send + Sync>> {
    let metadata = LanguageMetadata::get(lsp)
        .ok_or_else(|| format!("No registry entry found for language {:?}", lsp))?;

    (metadata.factory)(workspace_path, events_rx, binary).await
}
