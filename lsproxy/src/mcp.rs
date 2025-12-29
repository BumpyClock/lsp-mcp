// ABOUTME: MCP server tools and handler for exposing LSP-based code navigation.
// ABOUTME: Provides stdio MCP tool definitions and request handling for a workspace manager.
use crate::api_types::{HealthResponse, Position, Range};
use crate::config::{LspMcpConfig, OutputMode};
use crate::lsp::manager::Manager;
use crate::mcp_response::{error_response, success_response, tool_disabled_error};
use crate::service::{create_service, LspService, ServiceError};
use log::info;
use mcpkit::prelude::*;
use mcpkit::transport::stdio::StdioTransport;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
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
    version = "0.4.4"
)]
impl LspMcpServer {
    #[tool(description = "Returns symbols defined in a file relative to the workspace root")]
    async fn definitions_in_file(
        &self,
        file_path: String,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        match self
            .service
            .definitions_in_file(&file_path, limit, offset)
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("definitions_in_file", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("symbols".to_string(), response.symbols.len());
                let response = success_response(
                    "definitions_in_file",
                    data,
                    self.output_mode,
                    Some(counts),
                );
                ToolOutput::text(response)
            }
            Err(e) => {
                let response = error_response("definitions_in_file", &e, self.output_mode);
                ToolOutput::text(response)
            }
        }
    }

    #[tool(description = "Finds the definition of a symbol at a given file position")]
    async fn find_definition(
        &self,
        path: String,
        line: u32,
        character: u32,
        include_source_code: Option<bool>,
        include_raw_response: Option<bool>,
        context_lines: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .find_definition(
                &path,
                pos,
                include_source_code.unwrap_or(false),
                include_raw_response.unwrap_or(false),
                context_lines,
                limit,
                offset,
            )
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("find_definition", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("definitions".to_string(), response.definitions.len());
                let resp = success_response("find_definition", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("find_definition", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Finds references to a symbol at a given file position")]
    async fn find_references(
        &self,
        path: String,
        line: u32,
        character: u32,
        include_raw_response: Option<bool>,
        include_code_context_lines: Option<u32>,
        context_lines: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        let pos = Position { line, character };
        let effective_context_lines = context_lines.or(include_code_context_lines);
        match self
            .service
            .find_references(
                &path,
                pos,
                include_raw_response.unwrap_or(false),
                effective_context_lines,
                limit,
                offset,
            )
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("find_references", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("references".to_string(), response.references.len());
                let resp = success_response("find_references", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("find_references", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Get hover information (documentation, type info) for a symbol at a position")]
    async fn hover(
        &self,
        path: String,
        line: u32,
        character: u32,
        include_raw_response: Option<bool>,
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .hover(&path, pos, include_raw_response.unwrap_or(false))
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("hover", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                // No meaningful counts for hover
                let resp = success_response("hover", data, self.output_mode, None);
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("hover", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Search for symbols by name across the entire workspace")]
    async fn workspace_symbol(
        &self,
        query: String,
        include_raw_response: Option<bool>,
        exact: Option<bool>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        match self
            .service
            .workspace_symbol(
                &query,
                include_raw_response.unwrap_or(false),
                exact.unwrap_or(false),
                limit,
                offset,
            )
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("workspace_symbol", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("symbols".to_string(), response.symbols.len());
                let resp = success_response("workspace_symbol", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("workspace_symbol", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Find implementations of an interface, trait, or abstract method")]
    async fn go_to_implementation(
        &self,
        path: String,
        line: u32,
        character: u32,
        include_raw_response: Option<bool>,
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .find_implementation(&path, pos, include_raw_response.unwrap_or(false))
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("go_to_implementation", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("implementations".to_string(), response.implementations.len());
                let resp = success_response("go_to_implementation", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("go_to_implementation", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Get call hierarchy item at a position (functions/methods)")]
    async fn prepare_call_hierarchy(
        &self,
        path: String,
        line: u32,
        character: u32,
        include_raw_response: Option<bool>,
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .prepare_call_hierarchy(&path, pos, include_raw_response.unwrap_or(false))
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("prepare_call_hierarchy", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("items".to_string(), response.items.len());
                let resp = success_response("prepare_call_hierarchy", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("prepare_call_hierarchy", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Find all functions/methods that call the function at a position")]
    async fn incoming_calls(
        &self,
        path: String,
        line: u32,
        character: u32,
        include_raw_response: Option<bool>,
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .incoming_calls(&path, pos, include_raw_response.unwrap_or(false))
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("incoming_calls", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("calls".to_string(), response.calls.len());
                let resp = success_response("incoming_calls", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("incoming_calls", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Find all functions/methods called by the function at a position")]
    async fn outgoing_calls(
        &self,
        path: String,
        line: u32,
        character: u32,
        include_raw_response: Option<bool>,
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .outgoing_calls(&path, pos, include_raw_response.unwrap_or(false))
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("outgoing_calls", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("calls".to_string(), response.calls.len());
                let resp = success_response("outgoing_calls", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("outgoing_calls", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Finds symbols referenced by a symbol definition at a given position")]
    async fn find_referenced_symbols(
        &self,
        path: String,
        line: u32,
        character: u32,
        full_scan: Option<bool>,
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .find_referenced_symbols(&path, pos, full_scan.unwrap_or(false))
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("find_referenced_symbols", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("workspace_symbols".to_string(), response.workspace_symbols.len());
                counts.insert("external_symbols".to_string(), response.external_symbols.len());
                counts.insert("not_found".to_string(), response.not_found.len());
                let resp = success_response("find_referenced_symbols", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("find_referenced_symbols", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Finds identifiers by name in a file with an optional position")]
    async fn find_identifier(
        &self,
        path: String,
        name: String,
        line: Option<u32>,
        character: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        let position = match (line, character) {
            (Some(l), Some(c)) => Some(Position {
                line: l,
                character: c,
            }),
            _ => None,
        };
        match self
            .service
            .find_identifier(&path, &name, position, limit, offset)
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("find_identifier", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("identifiers".to_string(), response.identifiers.len());
                let resp = success_response("find_identifier", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("find_identifier", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Lists files in the workspace")]
    async fn list_files(&self, limit: Option<u32>, offset: Option<u32>) -> ToolOutput {
        match self.service.list_files(limit, offset).await {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("list_files", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("files".to_string(), response.files.len());
                let resp = success_response("list_files", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("list_files", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Reads source code from a file with an optional range")]
    async fn read_source_code(
        &self,
        path: String,
        start_line: Option<u32>,
        start_character: Option<u32>,
        end_line: Option<u32>,
        end_character: Option<u32>,
    ) -> ToolOutput {
        let range = match (start_line, start_character, end_line, end_character) {
            (Some(sl), Some(sc), Some(el), Some(ec)) => Some(Range {
                start: Position {
                    line: sl,
                    character: sc,
                },
                end: Position {
                    line: el,
                    character: ec,
                },
            }),
            _ => None,
        };
        match self.service.read_source_code(&path, range).await {
            Ok(source_code) => {
                let data = json!({"source": source_code});
                let mut counts = HashMap::new();
                counts.insert("chars".to_string(), source_code.len());
                let resp = success_response("read_source_code", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("read_source_code", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Returns service status and supported language availability")]
    async fn health(&self) -> ToolOutput {
        let response = HealthResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            languages: self.service.health().await,
        };
        match serde_json::to_value(&response) {
            Ok(data) => {
                // No meaningful counts for health
                let resp = success_response("health", data, self.output_mode, None);
                ToolOutput::text(resp)
            }
            Err(e) => {
                let err = ServiceError::Serialization(e.to_string());
                let resp = error_response("health", &err, self.output_mode);
                ToolOutput::text(resp)
            }
        }
    }

    #[tool(description = "Returns diagnostics (errors, warnings, hints) for a file or the entire workspace. Diagnostics are pushed by language servers and cached. Pass a file path for single file diagnostics, or omit for all workspace diagnostics.")]
    async fn get_diagnostics(&self, file_path: Option<String>) -> ToolOutput {
        match self.service.get_diagnostics(file_path.as_deref()).await {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        let resp = error_response("get_diagnostics", &err, self.output_mode);
                        return ToolOutput::text(resp);
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("diagnostics".to_string(), response.total_count);
                counts.insert("files".to_string(), response.files.len());
                let resp = success_response("get_diagnostics", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => {
                let resp = error_response("get_diagnostics", &e, self.output_mode);
                ToolOutput::text(resp)
            }
        }
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

        // Create a simple test file
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
        // ToolOutput from mcpkit - extract text content
        use mcpkit::types::Content;

        let content = match tool_output {
            ToolOutput::Success(result) => &result.content,
            ToolOutput::RecoverableError { .. } => return String::new(),
        };

        for item in content {
            if let Content::Text(text_content) = item {
                return text_content.text.clone();
            }
        }

        String::new()
    }

    // Note: These tests verify the structure after implementation
    // They will initially fail until we implement the response envelope wrapping

    #[tokio::test]
    async fn test_find_identifier_envelope_compact() {
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
        let text = extract_text_content(&output);

        // Should contain JSON envelope structure with "ok" field
        assert!(text.contains("\"ok\""));
        // Should contain either "data" (success) or "error" (failure)
        assert!(text.contains("\"data\"") || text.contains("\"error\""));
        // Should not contain meta in default mode
        assert!(!text.contains("\"meta\""));
        // Should be compact (no newlines)
        assert!(!text.contains('\n'));
    }

    #[tokio::test]
    async fn test_list_files_envelope_compact() {
        let (server, _temp) = create_test_server().await;
        let output = server.list_files(None, None).await;
        let text = extract_text_content(&output);

        // Should contain JSON envelope structure
        assert!(text.contains("\"ok\""));
        assert!(text.contains("\"data\""));
        assert!(text.contains("\"files\""));
        // Should not contain meta in default mode
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_list_files_envelope_verbose() {
        let (server, _temp) = create_test_server_with_mode(OutputMode::Verbose).await;
        let output = server.list_files(None, None).await;
        let text = extract_text_content(&output);

        let parsed: serde_json::Value =
            serde_json::from_str(&text).expect("Expected JSON response");

        assert!(text.contains("\"ok\""));
        assert!(text.contains("\"data\""));
        assert!(text.contains("\"meta\""));
        assert_eq!(
            parsed
                .get("meta")
                .and_then(|meta| meta.get("mode"))
                .and_then(|mode| mode.as_str()),
            Some("verbose")
        );
        assert!(text.contains('\n'));
    }

    #[tokio::test]
    async fn test_read_source_code_envelope_compact() {
        let (server, _temp) = create_test_server().await;
        let output = server
            .read_source_code("test.rs".to_string(), None, None, None, None)
            .await;
        let text = extract_text_content(&output);

        // Should contain JSON envelope structure with "ok" field
        assert!(text.contains("\"ok\""));
        // Should contain either "data" (success) or "error" (failure)
        assert!(text.contains("\"data\"") || text.contains("\"error\""));
        // Should not contain meta in default mode
        assert!(!text.contains("\"meta\""));
        // Should be compact (no newlines)
        assert!(!text.contains('\n'));
    }

    #[tokio::test]
    async fn test_health_envelope_compact() {
        let (server, _temp) = create_test_server().await;
        let output = server.health().await;
        let text = extract_text_content(&output);

        // Should contain JSON envelope structure
        assert!(text.contains("\"ok\""));
        assert!(text.contains("\"data\""));
        // Should not contain meta in default mode
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_get_diagnostics_envelope_compact() {
        let (server, _temp) = create_test_server().await;
        let output = server.get_diagnostics(None).await;
        let text = extract_text_content(&output);

        // Should contain JSON envelope structure
        assert!(text.contains("\"ok\""));
        assert!(text.contains("\"data\""));
        // Should not contain meta in default mode
        assert!(!text.contains("\"meta\""));
    }
}

/// Wrapper that filters tools based on configuration.
///
/// This allows dynamically enabling/disabling tools at runtime based on
/// the configuration file without modifying the underlying tool implementations.
pub struct FilteredToolHandler<T> {
    inner: Arc<T>,
    enabled_tools: HashSet<String>,
    output_mode: OutputMode,
}

impl<T> FilteredToolHandler<T> {
    /// Create a new filtered handler wrapping the inner handler.
    pub fn new(inner: Arc<T>, enabled_tools: HashSet<String>, output_mode: OutputMode) -> Self {
        Self {
            inner,
            enabled_tools,
            output_mode,
        }
    }
}

impl<T: ToolHandler + Send + Sync> ToolHandler for FilteredToolHandler<T> {
    fn list_tools(
        &self,
        ctx: &Context<'_>,
    ) -> impl std::future::Future<Output = Result<Vec<Tool>, McpError>> + Send {
        let inner = Arc::clone(&self.inner);
        let enabled = self.enabled_tools.clone();
        async move {
            let all_tools = inner.list_tools(ctx).await?;
            Ok(all_tools
                .into_iter()
                .filter(|tool| enabled.contains(&tool.name))
                .collect())
        }
    }

    fn call_tool(
        &self,
        name: &str,
        args: Value,
        ctx: &Context<'_>,
    ) -> impl std::future::Future<Output = Result<ToolOutput, McpError>> + Send {
        let inner = Arc::clone(&self.inner);
        let enabled = self.enabled_tools.clone();
        let output_mode = self.output_mode;
        let name = name.to_string();
        async move {
            if !enabled.contains(&name) {
                return Ok(ToolOutput::text(tool_disabled_error(&name, output_mode)));
            }
            inner.call_tool(&name, args, ctx).await
        }
    }
}

/// Create and run the LSP MCP server over stdio
pub async fn run_server(manager: Arc<Manager>, config: &LspMcpConfig) -> Result<(), McpError> {
    let server_instance = Arc::new(LspMcpServer::new(manager, config));
    let enabled_tools = config.enabled_tools();

    info!(
        "Starting MCP server with {} tools enabled (preset: {:?})",
        enabled_tools.len(),
        config.tools.preset
    );

    let filtered_handler = FilteredToolHandler::new(
        Arc::clone(&server_instance),
        enabled_tools,
        config.output_mode(),
    );

    let transport = StdioTransport::new();
    let server = ServerBuilder::new(Arc::clone(&server_instance))
        .with_tools(filtered_handler)
        .build();
    server.serve(transport).await
}
