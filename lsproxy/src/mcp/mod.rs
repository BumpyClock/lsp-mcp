// ABOUTME: MCP server module for exposing LSP-based code navigation tools.
// ABOUTME: Organizes tool handlers into submodules with public server exports.

mod call_hierarchy;
mod definitions;
mod diagnostics;
mod files;
pub mod filter;
mod hover;
mod references;
mod server;
mod symbols;

pub use filter::FilteredToolHandler;
pub use server::run_server;

use crate::config::{LspMcpConfig, OutputMode};
use crate::lsp::manager::Manager;
use crate::service::{create_service, LspService};
use mcpkit::prelude::*;
use std::sync::Arc;

/// LSP MCP Server that exposes code navigation tools for a workspace.
pub struct LspMcpServer {
    service: LspService,
    output_mode: OutputMode,
}

impl LspMcpServer {
    pub fn new(manager: Arc<Manager>, config: &LspMcpConfig) -> Self {
        LspMcpServer {
            service: create_service(manager),
            output_mode: config.output_mode(),
        }
    }
}

#[mcp_server(
    name = "lsp-mcp",
    version = "0.4.4",
    instructions = "All line and character positions use 1-based indexing (first line is 1, first character is 1). This matches what editors display to users."
)]
impl LspMcpServer {
    #[tool(name = "documentSymbol", description = "Symbols defined in a file (top-level only by default)")]
    async fn definitions_in_file(
        &self,
        path: String,
        include_locals: Option<bool>,
        limit: Option<u32>,
        offset: Option<u32>,
        context_lines: Option<u32>,
    ) -> ToolOutput {
        definitions::definitions_in_file(
            &self.service,
            self.output_mode,
            path,
            include_locals,
            limit,
            offset,
            context_lines,
        )
        .await
    }

    #[tool(name = "goToDefinition", description = "Definition of symbol at position. Returns signature, source code (first ~100 lines), and related symbols (max 5).")]
    async fn find_definition(
        &self,
        path: String,
        line: u32,
        character: u32,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        definitions::find_definition(
            &self.service,
            self.output_mode,
            path,
            line,
            character,
            limit,
            offset,
        )
        .await
    }

    #[tool(name = "findReferences", description = "References to symbol at position")]
    async fn find_references(
        &self,
        path: String,
        line: u32,
        character: u32,
        context_lines: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        references::find_references(
            &self.service,
            self.output_mode,
            path,
            line,
            character,
            context_lines,
            limit,
            offset,
        )
        .await
    }

    #[tool(name = "hover", description = "Hover info at position. Use include_definition to also get definition location. Use 'requests' for batch mode with array of {path, line, character}")]
    async fn hover(
        &self,
        path: Option<String>,
        line: Option<u32>,
        character: Option<u32>,
        include_definition: Option<bool>,
        requests: Option<String>,
    ) -> ToolOutput {
        hover::hover(
            &self.service,
            self.output_mode,
            path,
            line,
            character,
            include_definition,
            requests,
        )
        .await
    }

    #[tool(name = "workspaceSymbol", description = "Search symbols by name")]
    async fn workspace_symbol(
        &self,
        query: String,
        exact: Option<bool>,
        limit: Option<u32>,
        offset: Option<u32>,
        context_lines: Option<u32>,
    ) -> ToolOutput {
        symbols::workspace_symbol(
            &self.service,
            self.output_mode,
            query,
            exact,
            limit,
            offset,
            context_lines,
        )
        .await
    }

    #[tool(name = "goToImplementation", description = "Implementations of interface/trait")]
    async fn go_to_implementation(
        &self,
        path: String,
        line: u32,
        character: u32,
    ) -> ToolOutput {
        call_hierarchy::go_to_implementation(&self.service, self.output_mode, path, line, character)
            .await
    }

    #[tool(name = "callHierarchy", description = "Incoming or outgoing calls at position. External deps included by default. Set externals=false to exclude.")]
    async fn call_hierarchy(
        &self,
        path: String,
        line: u32,
        character: u32,
        direction: String,
        externals: Option<bool>,
        context_lines: Option<u32>,
    ) -> ToolOutput {
        call_hierarchy::call_hierarchy(
            &self.service,
            self.output_mode,
            path,
            line,
            character,
            direction,
            externals,
            context_lines,
        )
        .await
    }

    #[tool(name = "findReferencedSymbols", description = "Symbols referenced by definition. External deps included by default. Set externals=false to exclude.")]
    async fn find_referenced_symbols(
        &self,
        path: String,
        line: u32,
        character: u32,
        full_scan: Option<bool>,
        externals: Option<bool>,
    ) -> ToolOutput {
        references::find_referenced_symbols(
            &self.service,
            self.output_mode,
            path,
            line,
            character,
            full_scan,
            externals,
        )
        .await
    }

    #[tool(name = "findIdentifier", description = "Identifiers by name in file")]
    async fn find_identifier(
        &self,
        path: String,
        name: String,
        line: Option<u32>,
        character: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        symbols::find_identifier(
            &self.service,
            self.output_mode,
            path,
            name,
            line,
            character,
            limit,
            offset,
        )
        .await
    }

    #[tool(name = "listFiles", description = "List workspace files")]
    async fn list_files(&self, limit: Option<u32>, offset: Option<u32>) -> ToolOutput {
        files::list_files(&self.service, self.output_mode, limit, offset).await
    }

    #[tool(name = "readSourceCode", description = "Read source code from file")]
    async fn read_source_code(
        &self,
        path: String,
        start_line: Option<u32>,
        start_character: Option<u32>,
        end_line: Option<u32>,
        end_character: Option<u32>,
    ) -> ToolOutput {
        files::read_source_code(
            &self.service,
            self.output_mode,
            path,
            start_line,
            start_character,
            end_line,
            end_character,
        )
        .await
    }

    #[tool(name = "health", description = "Service status")]
    async fn health(&self) -> ToolOutput {
        diagnostics::health(&self.service, self.output_mode).await
    }

    #[tool(name = "getDiagnostics", description = "Diagnostics for file or workspace")]
    async fn get_diagnostics(&self, file_path: Option<String>) -> ToolOutput {
        diagnostics::get_diagnostics(&self.service, self.output_mode, file_path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputConfig;
    use crate::lsp::manager::Manager;
    use tempfile::TempDir;

    async fn create_test_server() -> (LspMcpServer, TempDir) {
        create_test_server_with_mode(OutputMode::Default).await
    }

    async fn create_test_server_with_mode(output_mode: OutputMode) -> (LspMcpServer, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workspace_root = temp_dir.path();

        let test_file = workspace_root.join("test.rs");
        std::fs::write(&test_file, "fn main() {}").expect("Failed to write test file");

        let manager = Manager::new(workspace_root.to_str().unwrap())
            .await
            .expect("Failed to create manager");

        let config = LspMcpConfig {
            output: Some(OutputConfig { mode: output_mode }),
            ..Default::default()
        };
        let server = LspMcpServer::new(Arc::new(manager), &config);
        (server, temp_dir)
    }

    fn extract_text_content(tool_output: &ToolOutput) -> String {
        use mcpkit::types::Content;

        let content = match tool_output {
            ToolOutput::Success(result) => &result.content,
            ToolOutput::RecoverableError { message, .. } => return message.clone(),
        };

        for item in content {
            if let Content::Text(text_content) = item {
                return text_content.text.clone();
            }
        }

        String::new()
    }

    fn is_error_output(tool_output: &ToolOutput) -> bool {
        matches!(tool_output, ToolOutput::RecoverableError { .. })
    }

    #[tokio::test]
    async fn test_find_identifier_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let output = server
            .find_identifier(
                "test.rs".to_string(),
                "main".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;

        if is_error_output(&output) {
            let error_msg = extract_text_content(&output);
            // Error messages should be plain text, not JSON
            assert!(!error_msg.contains("\"ok\""));
            assert!(!error_msg.starts_with('{'));
        } else {
            let text = extract_text_content(&output);
            // Markdown output expectations
            assert!(!text.contains("\"ok\""));
            assert!(text.contains("Identifiers ("), "Expected markdown header");
            assert!(text.contains("main ("), "negative: identifier name missing");
            assert!(!text.contains("\"meta\""));
        }
    }

    #[tokio::test]
    async fn test_list_files_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let output = server.list_files(None, None).await;
        let text = extract_text_content(&output);

        // Markdown output expectations
        assert!(!text.contains("\"ok\""));
        assert!(text.contains("Workspace Files"), "Expected markdown header");
        // Note: Files may not be indexed in test environment, so we just check format
        assert!(text.contains("total)"), "Expected total count in header");
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_list_files_verbose_returns_markdown() {
        // Verbose mode now returns markdown, same as default mode
        // (format_response always produces markdown)
        let (server, _temp) = create_test_server_with_mode(OutputMode::Verbose).await;
        let output = server.list_files(None, None).await;
        let text = extract_text_content(&output);

        // Should be markdown, not JSON
        assert!(
            text.contains("Workspace Files"),
            "Expected markdown header even in verbose mode"
        );
        // Note: Files may not be indexed in test environment, so we just check format
        assert!(text.contains("total)"), "Expected total count in header");
        // Markdown output has newlines
        assert!(text.contains('\n'));
    }

    #[tokio::test]
    async fn test_read_source_code_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let output = server
            .read_source_code("test.rs".to_string(), None, None, None, None)
            .await;

        if is_error_output(&output) {
            let error_msg = extract_text_content(&output);
            // Error messages should be plain text, not JSON
            assert!(!error_msg.contains("\"ok\""));
            assert!(!error_msg.starts_with('{'));
        } else {
            let text = extract_text_content(&output);
            // Markdown output expectations
            assert!(!text.contains("\"ok\""));
            assert!(text.contains("Source: test.rs"), "Expected markdown source header");
            assert!(text.contains("```rust"), "Expected rust code fence");
            assert!(text.contains("fn main()"), "Expected source content");
            assert!(!text.contains("\"meta\""));
        }
    }

    #[tokio::test]
    async fn test_health_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let output = server.health().await;
        let text = extract_text_content(&output);

        // Markdown output expectations
        assert!(!text.contains("\"ok\":true"));
        assert!(text.contains("LSP-MCP Health"), "Expected markdown health header");
        assert!(text.contains("Status:"), "Expected markdown status field");
        assert!(text.contains("Version:"), "Expected markdown version field");
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_get_diagnostics_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let output = server.get_diagnostics(None).await;
        let text = extract_text_content(&output);

        // Markdown output expectations
        assert!(!text.contains("\"ok\""));
        assert!(text.contains("Diagnostics ("), "Expected markdown diagnostics header");
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_error_uses_mcp_protocol_error() {
        let (server, _temp) = create_test_server().await;
        let output = server
            .read_source_code("nonexistent.rs".to_string(), None, None, None, None)
            .await;

        assert!(is_error_output(&output));
        let error_message = extract_text_content(&output);
        assert!(!error_message.starts_with('{'));
    }
}
