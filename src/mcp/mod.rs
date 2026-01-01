// ABOUTME: MCP server module for exposing LSP-based code navigation tools.
// ABOUTME: Organizes tool handlers into submodules with public server exports.

mod call_hierarchy;
mod definitions;
mod diagnostics;
mod files;
pub mod filter;
mod hover;
mod references;
mod semantic_search;
mod server;
mod symbols;
pub mod tool_params;

pub use filter::FilteredLspMcpServer;
pub use server::run_server;

use crate::config::{DebugConfig, LspMcpConfig, OutputMode};
use crate::lsp::registry::LanguageMetadata;
use crate::api_types::SupportedLanguages;
use crate::lsp::manager::Manager;
use crate::mcp::tool_params::*;
use crate::mcp_response::tool_result_success;
use crate::semantic_search::SemanticSearchManager;
use crate::service::{create_service, LspService};
use crate::session::{new_request_id, request_id_header};
use rmcp::{
    ServerHandler, tool, tool_router, tool_handler,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, RawContent, ServerInfo, Implementation,
        ServerCapabilities, ProtocolVersion,
    },
    ErrorData as McpError,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// LSP MCP Server that exposes code navigation tools for a workspace.
pub struct LspMcpServer {
    service: LspService,
    output_mode: OutputMode,
    debug_enabled: bool,
    debug_config: Option<DebugConfig>,
    workspace_root: PathBuf,
    enabled_tools: HashSet<String>,
    semantic_search_manager: Option<Arc<RwLock<SemanticSearchManager>>>,
    tool_router: ToolRouter<Self>,
}

impl LspMcpServer {
    pub fn new(manager: Arc<Manager>, config: &LspMcpConfig, workspace_root: &Path) -> Self {
        LspMcpServer {
            service: create_service(manager),
            output_mode: config.output_mode(),
            debug_enabled: config.debug_config().is_some(),
            debug_config: config.debug.clone(),
            workspace_root: workspace_root.to_path_buf(),
            enabled_tools: config.enabled_tools(),
            semantic_search_manager: None,
            tool_router: Self::tool_router(),
        }
    }

    /// Set the semantic search manager for this server.
    pub fn with_semantic_search(mut self, manager: Arc<RwLock<SemanticSearchManager>>) -> Self {
        self.semantic_search_manager = Some(manager);
        self
    }

    /// Returns whether debug mode is enabled.
    pub fn debug_enabled(&self) -> bool {
        self.debug_enabled
    }

    /// Wrap tool output with request ID header when debug is enabled.
    pub(crate) fn wrap_output(&self, request_id: Uuid, result: CallToolResult) -> CallToolResult {
        if !self.debug_enabled {
            return result;
        }

        let mut result = result;
        for content in &mut result.content {
            if let RawContent::Text(ref mut text) = content.raw {
                text.text = format!("{}{}", request_id_header(request_id), text.text);
            }
        }
        result
    }

    /// Get server instructions, with debug guidance when debug mode is enabled.
    pub fn get_instructions(&self) -> String {
        let mut instructions = if self.debug_enabled {
            r#"# LSP-MCP Usage Instructions

## Positioning
All line and character positions use **1-based indexing** (first line is 1, first character is 1).

After edits, use `getDiagnostics` to fetch the latest LSP diagnostics.

## Debug Mode Active

**Log file**: Use the `health` tool to get the current log file path.

### When to Inspect Logs
- If tool responses seem incomplete, missing data, or low quality
- If you need to read a file immediately after using an LSP tool on it
- If symbol resolution or navigation gives unexpected results

### Log Inspection Workflow
1. Call the `health` tool to get the log file path
2. Read the log file to see raw LSP responses
3. Identify discrepancies between raw data and formatted output
4. Report issues to the user

### After Each Task
Evaluate whether the lsp-mcp tools provided sufficient information.
If you used a tool and then immediately read that file, explain why.

## Log Correlation
Logs are written to `.lsp-mcp/logs/sessions/{session-id}.log`.
Each tool response includes a request ID header (`<!-- request: uuid -->`).
Use request IDs to correlate tool output with log entries."#
                .to_string()
        } else {
            r#"# LSP-MCP Usage Instructions

## Positioning
All line and character positions use **1-based indexing** (first line is 1, first character is 1).

After edits, use `getDiagnostics` to fetch the latest LSP diagnostics."#
                .to_string()
        };

        if self.enabled_tools.contains("initialSetup") {
            instructions.push_str(
                r#"

## First-Time Setup
Before using the LSP tools in this agent, run `initialSetup` to configure languages and binaries for this project."#,
            );
        }

        if self.enabled_tools.contains("semanticSearch") {
            instructions.push_str(
                r#"

## Semantic Search
Use `semanticSearch` for natural language code queries.
Optional params: `limit`, `path`, `file_pattern`, `exclude`, `min_score`, `per_file`, `rerank`, `context_lines`."#,
            );
        }

        instructions
    }

    fn format_initial_setup_language_list(&self) -> String {
        let mut lines = Vec::new();
        for metadata in LanguageMetadata::all() {
            let id = match metadata.id {
                SupportedLanguages::TypeScriptJavaScript => "typescript|javascript".to_string(),
                _ => metadata.id.to_string(),
            };
            lines.push(format!(
                "- `{}` ({}) — `{}`",
                id, metadata.name, metadata.default_binary
            ));
        }
        lines.join("\n")
    }
}

#[tool_router]
impl LspMcpServer {
    #[tool(name = "documentSymbol", description = "Symbols defined in a file (top-level only by default; set include_children for nesting)")]
    async fn definitions_in_file(
        &self,
        Parameters(params): Parameters<DocumentSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "documentSymbol",
            path = %params.path,
            "Processing tool request"
        );

        let output = definitions::definitions_in_file(
            &self.service,
            self.output_mode,
            params.path,
            params.include_locals,
            params.include_children,
            params.limit,
            params.offset,
            params.context_lines,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }

    #[tool(name = "goToDefinition", description = "Definition of symbol at position. Returns signature, source code (first ~100 lines), and related symbols (max 5).")]
    async fn find_definition(
        &self,
        Parameters(params): Parameters<GoToDefinitionParams>,
    ) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "goToDefinition",
            path = %params.path,
            line = params.line,
            character = params.character,
            "Processing tool request"
        );

        let output = definitions::find_definition(
            &self.service,
            self.output_mode,
            params.path,
            params.line,
            params.character,
            params.limit,
            params.offset,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }

    #[tool(name = "findReferences", description = "References to symbol at position")]
    async fn find_references(
        &self,
        Parameters(params): Parameters<FindReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "findReferences",
            path = %params.path,
            line = params.line,
            character = params.character,
            "Processing tool request"
        );

        let output = references::find_references(
            &self.service,
            self.output_mode,
            params.path,
            params.line,
            params.character,
            params.context_lines,
            params.limit,
            params.offset,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }

    #[tool(name = "hover", description = "Hover info at position. Use include_definition to also get definition location. Use 'requests' for batch mode with array of {path, line, character}")]
    async fn hover(
        &self,
        Parameters(params): Parameters<HoverParams>,
    ) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "hover",
            "Processing tool request"
        );

        let output = hover::hover(
            &self.service,
            self.output_mode,
            params.path,
            params.line,
            params.character,
            params.include_definition,
            params.requests,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }

    #[tool(name = "workspaceSymbol", description = "Search symbols by name")]
    async fn workspace_symbol(
        &self,
        Parameters(params): Parameters<WorkspaceSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "workspaceSymbol",
            query = %params.query,
            "Processing tool request"
        );

        let output = symbols::workspace_symbol(
            &self.service,
            self.output_mode,
            params.query,
            params.exact,
            params.limit,
            params.offset,
            params.context_lines,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }

    #[tool(name = "goToImplementation", description = "Implementations of interface/trait")]
    async fn go_to_implementation(
        &self,
        Parameters(params): Parameters<GoToImplementationParams>,
    ) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "goToImplementation",
            path = %params.path,
            line = params.line,
            character = params.character,
            "Processing tool request"
        );

        let output = call_hierarchy::go_to_implementation(&self.service, self.output_mode, params.path, params.line, params.character)
            .await;

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }

    #[tool(name = "callHierarchy", description = "Incoming or outgoing calls at position. External deps included by default. Set externals=false to exclude.")]
    async fn call_hierarchy(
        &self,
        Parameters(params): Parameters<CallHierarchyParams>,
    ) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "callHierarchy",
            path = %params.path,
            line = params.line,
            character = params.character,
            direction = %params.direction,
            "Processing tool request"
        );

        let output = call_hierarchy::call_hierarchy(
            &self.service,
            self.output_mode,
            params.path,
            params.line,
            params.character,
            params.direction,
            params.externals,
            params.context_lines,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }

    #[tool(name = "findReferencedSymbols", description = "Symbols referenced by definition. External deps included by default. Set externals=false to exclude.")]
    async fn find_referenced_symbols(
        &self,
        Parameters(params): Parameters<FindReferencedSymbolsParams>,
    ) -> Result<CallToolResult, McpError> {
        let output = references::find_referenced_symbols(
            &self.service,
            self.output_mode,
            params.path,
            params.line,
            params.character,
            params.full_scan,
            params.externals,
        )
        .await;
        Ok(output)
    }

    #[tool(name = "findIdentifier", description = "Identifiers by name in file")]
    async fn find_identifier(
        &self,
        Parameters(params): Parameters<FindIdentifierParams>,
    ) -> Result<CallToolResult, McpError> {
        let output = symbols::find_identifier(
            &self.service,
            self.output_mode,
            params.path,
            params.name,
            params.line,
            params.character,
            params.limit,
            params.offset,
        )
        .await;
        Ok(output)
    }

    #[tool(name = "listFiles", description = "List workspace files")]
    async fn list_files(
        &self,
        Parameters(params): Parameters<ListFilesParams>,
    ) -> Result<CallToolResult, McpError> {
        let output = files::list_files(&self.service, self.output_mode, params.limit, params.offset).await;
        Ok(output)
    }

    #[tool(name = "readSourceCode", description = "Read source code from file")]
    async fn read_source_code(
        &self,
        Parameters(params): Parameters<ReadSourceCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        let output = files::read_source_code(
            &self.service,
            self.output_mode,
            params.path,
            params.start_line,
            params.start_character,
            params.end_line,
            params.end_character,
        )
        .await;
        Ok(output)
    }

    #[tool(name = "health", description = "Service status")]
    async fn health(&self) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "health",
            "Processing tool request"
        );

        let output = diagnostics::health(
            &self.service,
            self.output_mode,
            self.debug_config.as_ref(),
            &self.workspace_root,
            self.semantic_search_manager.clone(),
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }

    #[tool(name = "getDiagnostics", description = "Diagnostics for file or workspace")]
    async fn get_diagnostics(
        &self,
        Parameters(params): Parameters<GetDiagnosticsParams>,
    ) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "getDiagnostics",
            "Processing tool request"
        );

        let output = diagnostics::get_diagnostics(&self.service, self.output_mode, params.file_path).await;

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }

    #[tool(
        name = "initialSetup",
        description = "Guided setup for configuring languages, binaries, and tools."
    )]
    async fn initial_setup(&self) -> Result<CallToolResult, McpError> {
        let language_list = self.format_initial_setup_language_list();
        let instructions = format!(
            r#"# LSP-MCP Initial Setup

Use this tool to configure language servers and `.lsp-mcp.json` for the current project.
It is enabled by default only in the standard tool preset.

## 1) Auto-detect languages and confirm choices
Ask the user to auto-detect languages by scanning the workspace (file extensions) and present the detected set.
Then give them options: enable all detected languages, select a subset, or add additional languages manually.
If `languages` is omitted, LSP-MCP will auto-detect on startup.

**Supported languages + default server binaries:**
{}

## 2) Create `.lsp-mcp.json`
```json
{{
  "languages": ["rust", "typescript"],
  "binaries": {{
    "rust": "/opt/rust-analyzer"
  }},
  "tools": {{
    "preset": "standard"
  }}
}}
```

## 3) Configure semantic search (optional)
Add this to `.lsp-mcp.json` to enable semantic search:
```json
{{
  "tools": {{
    "enable": ["semanticSearch"]
  }},
  "semantic_search": {{
    "enabled": true,
    "embedder": {{
      "provider": "fastembed"
    }}
  }}
}}
```

If you prefer OpenAI embeddings, use (prefer `api_key_env`; `api_key` is supported if needed):
```json
{{
  "semantic_search": {{
    "enabled": true,
    "embedder": {{
      "provider": "openai",
      "api_key_env": "OPENAI_API_KEY"
    }}
  }}
}}
```

Restart the agent after updating the config. Indexing runs in the background; semanticSearch returns status until it is ready.

## 4) Install language servers
Ensure the default binaries above are on `PATH`, or set `binaries` per language with absolute paths.
If you want install commands, tell the agent your OS/package manager and target languages.
The agent should verify that each configured language server binary is available before continuing.
Check `PATH` with `which` (macOS/Linux) or `where` (Windows).
If a binary is missing, the agent should provide install steps for the user's OS/package manager.

## 5) Disable this tool after setup
Add `initialSetup` to the disabled list and restart your agent:
```json
{{
  "tools": {{
    "disable": ["initialSetup"]
  }}
}}
```

If you need to keep this tool enabled after creating a project config, set:
```json
{{
  "tools": {{
    "initial_setup": "enabled"
  }}
}}
```
Restart your agent for new settings to take effect."#,
            language_list
        );

        Ok(tool_result_success(instructions))
    }

    #[tool(
        name = "semanticSearch",
        description = "Search code semantically using natural language queries. Returns ranked code chunks based on embedding similarity."
    )]
    async fn semantic_search_tool(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "semanticSearch",
            query = %params.query,
            "Processing tool request"
        );

        let output = match &self.semantic_search_manager {
            Some(manager) => {
                semantic_search::semantic_search(
                    manager,
                    params.query,
                    params.limit,
                    params.path,
                    params.file_pattern,
                    params.exclude,
                    params.min_score,
                    params.per_file,
                    params.rerank,
                    params.context_lines,
                )
                .await
            }
            None => {
                tool_result_success(
                    "Semantic search is disabled. Enable it in `.lsp-mcp.json`:\n\n```json\n{\n  \"tools\": {\n    \"enable\": [\"semanticSearch\"]\n  },\n  \"semantic_search\": {\n    \"enabled\": true,\n    \"embedder\": {\n      \"provider\": \"fastembed\"\n    }\n  }\n}\n```".to_string()
                )
            }
        };

        tracing::debug!(
            request_id = %request_id,
            "Tool request completed"
        );

        Ok(self.wrap_output(request_id, output))
    }
}

#[tool_handler]
impl ServerHandler for LspMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_06_18,
            server_info: Implementation {
                name: "lsp-mcp".into(),
                version: "0.4.4".into(),
                ..Default::default()
            },
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            instructions: Some(self.get_instructions()),
            ..Default::default()
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

        let test_file = workspace_root.join("test.rs");
        std::fs::write(&test_file, "fn main() {}").expect("Failed to write test file");

        let manager = Manager::new(workspace_root.to_str().unwrap())
            .await
            .expect("Failed to create manager");

        let config = LspMcpConfig {
            output: Some(OutputConfig { mode: output_mode }),
            ..Default::default()
        };
        let server = LspMcpServer::new(Arc::new(manager), &config, workspace_root);
        (server, temp_dir)
    }

    fn extract_text_content(result: &CallToolResult) -> String {
        for content in &result.content {
            if let RawContent::Text(text_content) = &content.raw {
                return text_content.text.clone();
            }
        }
        String::new()
    }

    fn is_error_result(result: &CallToolResult) -> bool {
        result.is_error == Some(true)
    }

    #[tokio::test]
    async fn test_find_identifier_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let params = Parameters(FindIdentifierParams {
            path: "test.rs".to_string(),
            name: "main".to_string(),
            line: None,
            character: None,
            limit: None,
            offset: None,
        });
        let result = server.find_identifier(params).await.unwrap();

        if is_error_result(&result) {
            let error_msg = extract_text_content(&result);
            assert!(!error_msg.contains("\"ok\""));
            assert!(!error_msg.starts_with('{'));
        } else {
            let text = extract_text_content(&result);
            assert!(!text.contains("\"ok\""));
            assert!(text.contains("Identifiers ("), "Expected markdown header");
            assert!(text.contains("main ("), "negative: identifier name missing");
            assert!(!text.contains("\"meta\""));
        }
    }

    #[tokio::test]
    async fn test_list_files_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let params = Parameters(ListFilesParams { limit: None, offset: None });
        let result = server.list_files(params).await.unwrap();
        let text = extract_text_content(&result);

        assert!(!text.contains("\"ok\""));
        assert!(text.contains("Workspace Files"), "Expected markdown header");
        assert!(text.contains("total)"), "Expected total count in header");
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_list_files_verbose_returns_markdown() {
        let (server, _temp) = create_test_server_with_mode(OutputMode::Verbose).await;
        let params = Parameters(ListFilesParams { limit: None, offset: None });
        let result = server.list_files(params).await.unwrap();
        let text = extract_text_content(&result);

        assert!(
            text.contains("Workspace Files"),
            "Expected markdown header even in verbose mode"
        );
        assert!(text.contains("total)"), "Expected total count in header");
        assert!(text.contains('\n'));
    }

    #[tokio::test]
    async fn test_read_source_code_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let params = Parameters(ReadSourceCodeParams {
            path: "test.rs".to_string(),
            start_line: None,
            start_character: None,
            end_line: None,
            end_character: None,
        });
        let result = server.read_source_code(params).await.unwrap();

        if is_error_result(&result) {
            let error_msg = extract_text_content(&result);
            assert!(!error_msg.contains("\"ok\""));
            assert!(!error_msg.starts_with('{'));
        } else {
            let text = extract_text_content(&result);
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
        let result = server.health().await.unwrap();
        let text = extract_text_content(&result);

        assert!(!text.contains("\"ok\":true"));
        assert!(text.contains("LSP-MCP Health"), "Expected markdown health header");
        assert!(text.contains("Status:"), "Expected markdown status field");
        assert!(text.contains("Version:"), "Expected markdown version field");
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_get_diagnostics_returns_data_directly() {
        let (server, _temp) = create_test_server().await;
        let params = Parameters(GetDiagnosticsParams { file_path: None });
        let result = server.get_diagnostics(params).await.unwrap();
        let text = extract_text_content(&result);

        assert!(!text.contains("\"ok\""));
        assert!(text.contains("Diagnostics ("), "Expected markdown diagnostics header");
        assert!(!text.contains("\"meta\""));
    }

    #[tokio::test]
    async fn test_error_uses_mcp_protocol_error() {
        let (server, _temp) = create_test_server().await;
        let params = Parameters(ReadSourceCodeParams {
            path: "nonexistent.rs".to_string(),
            start_line: None,
            start_character: None,
            end_line: None,
            end_character: None,
        });
        let result = server.read_source_code(params).await.unwrap();

        assert!(is_error_result(&result));
        let error_message = extract_text_content(&result);
        assert!(!error_message.starts_with('{'));
    }

    #[tokio::test]
    async fn test_debug_enabled_adds_request_id_header() {
        use crate::config::DebugConfig;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workspace_root = temp_dir.path();

        let test_file = workspace_root.join("test.rs");
        std::fs::write(&test_file, "fn main() {}").expect("Failed to write test file");

        let manager = Manager::new(workspace_root.to_str().unwrap())
            .await
            .expect("Failed to create manager");

        let config = LspMcpConfig {
            debug: Some(DebugConfig {
                enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let server = LspMcpServer::new(Arc::new(manager), &config, workspace_root);

        let result = server.health().await.unwrap();
        let text = extract_text_content(&result);

        assert!(text.contains("<!-- request:"), "Debug mode should add request ID header");
    }

    #[tokio::test]
    async fn test_instructions_include_debug_guidance_when_enabled() {
        use crate::config::DebugConfig;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workspace_root = temp_dir.path();

        let manager = Manager::new(workspace_root.to_str().unwrap())
            .await
            .expect("Failed to create manager");

        let config = LspMcpConfig {
            debug: Some(DebugConfig {
                enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let server = LspMcpServer::new(Arc::new(manager), &config, workspace_root);

        let instructions = server.get_instructions();

        assert!(
            instructions.contains("Debug Mode Active"),
            "Debug mode should include Debug Mode Active header"
        );
        assert!(
            instructions.contains("When to Inspect Logs"),
            "Debug mode should include log inspection guidance"
        );
        assert!(
            instructions.contains(".lsp-mcp/logs/sessions"),
            "Debug mode should mention log location"
        );
    }

    #[tokio::test]
    async fn test_instructions_are_basic_when_debug_disabled() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workspace_root = temp_dir.path();

        let manager = Manager::new(workspace_root.to_str().unwrap())
            .await
            .expect("Failed to create manager");

        let config = LspMcpConfig::default();
        let server = LspMcpServer::new(Arc::new(manager), &config, workspace_root);

        let instructions = server.get_instructions();

        assert!(
            instructions.contains("1-based indexing"),
            "Should contain base instructions"
        );
        assert!(
            !instructions.contains("Debug Mode Active"),
            "Should NOT contain debug mode header when disabled"
        );
        assert!(
            !instructions.contains("When to Inspect Logs"),
            "Should NOT mention log inspection when disabled"
        );
    }

    #[tokio::test]
    async fn test_instructions_do_not_warn_when_project_config_present() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workspace_root = temp_dir.path();

        let manager = Manager::new(workspace_root.to_str().unwrap())
            .await
            .expect("Failed to create manager");

        let config = LspMcpConfig {
            project_config_present: true,
            ..Default::default()
        };
        let server = LspMcpServer::new(Arc::new(manager), &config, workspace_root);

        let instructions = server.get_instructions();

        assert!(
            !instructions.contains("Initial Setup Tool Disabled"),
            "Should not warn about initialSetup being disabled"
        );
    }
}
