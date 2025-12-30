// ABOUTME: MCP server tools and handler for exposing LSP-based code navigation.
// ABOUTME: Provides stdio MCP tool definitions and request handling for a workspace manager.
use crate::api_types::{CallHierarchyDirection, HealthResponse, HoverBatchItem, HoverRequest, Position, Range};
use crate::config::{LspMcpConfig, OutputMode};
use crate::lsp::manager::Manager;
use crate::mcp_response::{format_error, success_response, tool_disabled_message};
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
    #[tool(description = "Symbols defined in a file")]
    async fn definitions_in_file(
        &self,
        path: String,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        match self
            .service
            .definitions_in_file(&path, limit, offset)
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        return ToolOutput::error(format_error(&err));
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
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "Definition of symbol at position. Returns signature/type info from LSP hover. Set include_siblings=true to get other exports from same file (filtered, max siblings_limit=5).")]
    async fn find_definition(
        &self,
        path: String,
        line: u32,
        character: u32,
        include_source_code: Option<bool>,
        context_lines: Option<u32>,
        include_siblings: Option<bool>,
        siblings_limit: Option<u32>,
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
                self.output_mode == OutputMode::Verbose,
                context_lines,
                limit,
                offset,
            )
            .await
        {
            Ok(mut response) => {
                // Filter siblings based on parameters
                if !include_siblings.unwrap_or(false) {
                    // Remove siblings when not requested
                    if let Some(ref mut related) = response.related {
                        related.sibling_exports.clear();
                    }
                } else if let Some(ref mut related) = response.related {
                    // Filter internal builder symbols and apply limit
                    let limit = siblings_limit.unwrap_or(5);
                    related.sibling_exports = crate::service::filter_sibling_exports(
                        std::mem::take(&mut related.sibling_exports),
                        limit,
                    );
                }

                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("definitions".to_string(), response.definitions.len());
                let resp = success_response("find_definition", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "References to symbol at position")]
    async fn find_references(
        &self,
        path: String,
        line: u32,
        character: u32,
        context_lines: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .find_references(
                &path,
                pos,
                self.output_mode == OutputMode::Verbose,
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
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let mut counts = HashMap::new();
                let reference_count: usize = response.by_file.iter().map(|g| g.refs.len()).sum();
                counts.insert("references".to_string(), reference_count);
                let resp = success_response("find_references", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "Hover info at position. Use include_definition to also get definition location. Use 'requests' for batch mode with array of {path, line, character}")]
    async fn hover(
        &self,
        path: Option<String>,
        line: Option<u32>,
        character: Option<u32>,
        include_definition: Option<bool>,
        requests: Option<String>,
    ) -> ToolOutput {
        let include_def = include_definition.unwrap_or(false);
        if let Some(requests_json) = requests {
            let batch_requests: Vec<HoverRequest> = match serde_json::from_str(&requests_json) {
                Ok(r) => r,
                Err(e) => return ToolOutput::error(format!("Invalid requests JSON: {}", e)),
            };
            let mut results: Vec<HoverBatchItem> = Vec::with_capacity(batch_requests.len());
            for req in batch_requests {
                let pos = Position {
                    line: req.line,
                    character: req.character,
                };
                match self
                    .service
                    .hover(&req.path, pos, self.output_mode == OutputMode::Verbose, include_def)
                    .await
                {
                    Ok(response) => results.push(HoverBatchItem::Success(response)),
                    Err(e) => results.push(HoverBatchItem::Error {
                        error: format_error(&e),
                    }),
                }
            }
            let data = match serde_json::to_value(&results) {
                Ok(v) => v,
                Err(e) => return ToolOutput::error(format!("Serialization error: {}", e)),
            };
            let resp = success_response("hover", data, self.output_mode, None);
            return ToolOutput::text(resp);
        }
        let (path, line, character) = match (path, line, character) {
            (Some(p), Some(l), Some(c)) => (p, l, c),
            _ => return ToolOutput::error("Single mode requires path, line, and character"),
        };
        let pos = Position { line, character };
        match self
            .service
            .hover(&path, pos, self.output_mode == OutputMode::Verbose, include_def)
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let resp = success_response("hover", data, self.output_mode, None);
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "Search symbols by name")]
    async fn workspace_symbol(
        &self,
        query: String,
        exact: Option<bool>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ToolOutput {
        match self
            .service
            .workspace_symbol(
                &query,
                self.output_mode == OutputMode::Verbose,
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
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("symbols".to_string(), response.symbols.len());
                let resp = success_response("workspace_symbol", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "Implementations of interface/trait")]
    async fn go_to_implementation(
        &self,
        path: String,
        line: u32,
        character: u32,
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .find_implementation(&path, pos, self.output_mode == OutputMode::Verbose)
            .await
        {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("implementations".to_string(), response.implementations.len());
                let resp = success_response("go_to_implementation", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "Incoming or outgoing calls at position")]
    async fn call_hierarchy(
        &self,
        path: String,
        line: u32,
        character: u32,
        direction: String,
    ) -> ToolOutput {
        let dir = match direction.to_lowercase().as_str() {
            "incoming" => CallHierarchyDirection::Incoming,
            "outgoing" => CallHierarchyDirection::Outgoing,
            _ => {
                return ToolOutput::error(format!(
                    "Invalid direction '{}': must be 'incoming' or 'outgoing'",
                    direction
                ));
            }
        };
        let pos = Position { line, character };
        match self.service.call_hierarchy(&path, pos, dir).await {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("calls".to_string(), response.calls.len());
                let resp = success_response("call_hierarchy", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "Symbols referenced by definition")]
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
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("workspace_symbols".to_string(), response.workspace_symbols.len());
                counts.insert("external_symbols".to_string(), response.external_symbols.len());
                counts.insert("not_found".to_string(), response.not_found.len());
                let resp = success_response("find_referenced_symbols", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "Identifiers by name in file")]
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
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("identifiers".to_string(), response.identifiers.len());
                let resp = success_response("find_identifier", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "List workspace files")]
    async fn list_files(&self, limit: Option<u32>, offset: Option<u32>) -> ToolOutput {
        match self.service.list_files(limit, offset).await {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("files".to_string(), response.files.len());
                let resp = success_response("list_files", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "Read source code from file")]
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
            Err(e) => ToolOutput::error(format_error(&e)),
        }
    }

    #[tool(description = "Service status")]
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
                ToolOutput::error(format_error(&err))
            }
        }
    }

    #[tool(description = "Diagnostics for file or workspace")]
    async fn get_diagnostics(&self, file_path: Option<String>) -> ToolOutput {
        match self.service.get_diagnostics(file_path.as_deref()).await {
            Ok(response) => {
                let data = match serde_json::to_value(&response) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = ServiceError::Serialization(e.to_string());
                        return ToolOutput::error(format_error(&err));
                    }
                };
                let mut counts = HashMap::new();
                counts.insert("diagnostics".to_string(), response.total_count);
                counts.insert("files".to_string(), response.files.len());
                let resp = success_response("get_diagnostics", data, self.output_mode, Some(counts));
                ToolOutput::text(resp)
            }
            Err(e) => ToolOutput::error(format_error(&e)),
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

        // Either success (data directly) or MCP protocol error
        if is_error_output(&output) {
            // Error should be plain text, not JSON wrapper
            let error_msg = extract_text_content(&output);
            assert!(!error_msg.contains("\"ok\""));
            assert!(!error_msg.starts_with('{'));
        } else {
            let text = extract_text_content(&output);
            // Should NOT contain "ok" wrapper - data returned directly
            assert!(!text.contains("\"ok\""));
            // Should contain actual data fields
            assert!(text.contains("\"identifiers\""));
            // Should not contain meta in default mode
            assert!(!text.contains("\"meta\""));
            // Should be compact (no newlines)
            assert!(!text.contains('\n'));
        }
    }

    #[tokio::test]
    async fn test_list_files_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let output = server.list_files(None, None).await;
        let text = extract_text_content(&output);

        // Should NOT contain "ok" wrapper
        assert!(!text.contains("\"ok\""));
        // Should contain actual data
        assert!(text.contains("\"files\""));
        // Should not contain meta in default mode
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_list_files_verbose_has_meta_sibling() {
        let (server, _temp) = create_test_server_with_mode(OutputMode::Verbose).await;
        let output = server.list_files(None, None).await;
        let text = extract_text_content(&output);

        let parsed: serde_json::Value =
            serde_json::from_str(&text).expect("Expected JSON response");

        // Should NOT have "ok" wrapper
        assert!(parsed.get("ok").is_none());
        // Should have data fields directly
        assert!(parsed.get("files").is_some());
        // Should have meta as sibling
        assert!(parsed.get("meta").is_some());
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
    async fn test_read_source_code_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let output = server
            .read_source_code("test.rs".to_string(), None, None, None, None)
            .await;

        // Either success (data directly) or MCP protocol error
        if is_error_output(&output) {
            // Error should be plain text, not JSON wrapper
            let error_msg = extract_text_content(&output);
            assert!(!error_msg.contains("\"ok\""));
            assert!(!error_msg.starts_with('{'));
        } else {
            let text = extract_text_content(&output);
            // Should NOT contain "ok" wrapper
            assert!(!text.contains("\"ok\""));
            // Should contain source data
            assert!(text.contains("\"source\""));
            // Should not contain meta in default mode
            assert!(!text.contains("\"meta\""));
            // Should be compact (no newlines)
            assert!(!text.contains('\n'));
        }
    }

    #[tokio::test]
    async fn test_health_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let output = server.health().await;
        let text = extract_text_content(&output);

        // Should NOT contain "ok" wrapper
        assert!(!text.contains("\"ok\":true"));
        // Should contain health data fields directly
        assert!(text.contains("\"status\""));
        assert!(text.contains("\"version\""));
        // Should not contain meta in default mode
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_get_diagnostics_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let output = server.get_diagnostics(None).await;
        let text = extract_text_content(&output);

        // Should NOT contain "ok" wrapper
        assert!(!text.contains("\"ok\""));
        // Should not contain meta in default mode
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_error_uses_mcp_protocol_error() {
        let (server, _temp) = create_test_server().await;
        // Request a file that doesn't exist
        let output = server
            .read_source_code("nonexistent.rs".to_string(), None, None, None, None)
            .await;

        // Should be a RecoverableError, not a Success with JSON error
        assert!(is_error_output(&output));
        let error_message = extract_text_content(&output);
        // Error message should be plain text, not JSON
        assert!(!error_message.starts_with('{'));
    }
}

/// Wrapper that filters tools based on configuration.
///
/// This allows dynamically enabling/disabling tools at runtime based on
/// the configuration file without modifying the underlying tool implementations.
pub struct FilteredToolHandler<T> {
    inner: Arc<T>,
    enabled_tools: HashSet<String>,
}

impl<T> FilteredToolHandler<T> {
    /// Create a new filtered handler wrapping the inner handler.
    pub fn new(inner: Arc<T>, enabled_tools: HashSet<String>) -> Self {
        Self {
            inner,
            enabled_tools,
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
        let name = name.to_string();
        async move {
            if !enabled.contains(&name) {
                return Ok(ToolOutput::error(tool_disabled_message(&name)));
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
    );

    let transport = StdioTransport::new();
    let server = ServerBuilder::new(Arc::clone(&server_instance))
        .with_tools(filtered_handler)
        .build();
    server.serve(transport).await
}
