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

use crate::config::{DebugConfig, InitialSetupMode, LspMcpConfig, OutputMode};
use crate::lsp::registry::LanguageMetadata;
use crate::api_types::SupportedLanguages;
use crate::lsp::manager::Manager;
use crate::service::{create_service, LspService};
use crate::session::{new_request_id, request_id_header};
use mcpkit::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// LSP MCP Server that exposes code navigation tools for a workspace.
pub struct LspMcpServer {
    service: LspService,
    output_mode: OutputMode,
    debug_enabled: bool,
    debug_config: Option<DebugConfig>,
    workspace_root: PathBuf,
    project_config_present: bool,
    initial_setup_mode: InitialSetupMode,
    enabled_tools: HashSet<String>,
}

impl LspMcpServer {
    pub fn new(manager: Arc<Manager>, config: &LspMcpConfig, workspace_root: &Path) -> Self {
        LspMcpServer {
            service: create_service(manager),
            output_mode: config.output_mode(),
            debug_enabled: config.debug_config().is_some(),
            debug_config: config.debug.clone(),
            workspace_root: workspace_root.to_path_buf(),
            project_config_present: config.project_config_present,
            initial_setup_mode: config.tools.initial_setup,
            enabled_tools: config.enabled_tools(),
        }
    }

    /// Returns whether debug mode is enabled.
    pub fn debug_enabled(&self) -> bool {
        self.debug_enabled
    }

    /// Wrap tool output with request ID header when debug is enabled.
    fn wrap_output(&self, request_id: Uuid, output: ToolOutput) -> ToolOutput {
        if !self.debug_enabled {
            return output;
        }

        match output {
            ToolOutput::Success(mut result) => {
                for content in &mut result.content {
                    if let mcpkit::types::Content::Text(text) = content {
                        text.text = format!("{}{}", request_id_header(request_id), text.text);
                    }
                }
                ToolOutput::Success(result)
            }
            other => other,
        }
    }

    /// Get server instructions, with debug guidance when debug mode is enabled.
    pub fn get_instructions(&self) -> String {
        const BASE_INSTRUCTIONS: &str = "All line and character positions use 1-based indexing (first line is 1, first character is 1). This matches what editors display to users.";

        let mut instructions = if self.debug_enabled {
            format!(
                r#"{}

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

Logs are written to `.lsp-mcp/logs/sessions/{{session-id}}.log`.
Each tool response includes a request ID header for correlation."#,
                BASE_INSTRUCTIONS
            )
        } else {
            BASE_INSTRUCTIONS.to_string()
        };

        if self.project_config_present && self.initial_setup_mode == InitialSetupMode::Auto {
            instructions.push_str(
                r#"

## Initial Setup Tool Disabled
A project `.lsp-mcp.json` was detected, so the `initialSetup` tool is disabled by default.
To keep it enabled, set `"tools": { "initial_setup": "enabled" }` in your config and restart the agent."#,
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

#[mcp_server(
    name = "lsp-mcp",
    version = "0.4.4"
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
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "documentSymbol",
            path = %path,
            "Processing tool request"
        );

        let output = definitions::definitions_in_file(
            &self.service,
            self.output_mode,
            path,
            include_locals,
            limit,
            offset,
            context_lines,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            success = !matches!(&output, ToolOutput::RecoverableError { .. }),
            "Tool request completed"
        );

        self.wrap_output(request_id, output)
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
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "goToDefinition",
            path = %path,
            line = line,
            character = character,
            "Processing tool request"
        );

        let output = definitions::find_definition(
            &self.service,
            self.output_mode,
            path,
            line,
            character,
            limit,
            offset,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            success = !matches!(&output, ToolOutput::RecoverableError { .. }),
            "Tool request completed"
        );

        self.wrap_output(request_id, output)
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
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "findReferences",
            path = %path,
            line = line,
            character = character,
            "Processing tool request"
        );

        let output = references::find_references(
            &self.service,
            self.output_mode,
            path,
            line,
            character,
            context_lines,
            limit,
            offset,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            success = !matches!(&output, ToolOutput::RecoverableError { .. }),
            "Tool request completed"
        );

        self.wrap_output(request_id, output)
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
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "hover",
            "Processing tool request"
        );

        let output = hover::hover(
            &self.service,
            self.output_mode,
            path,
            line,
            character,
            include_definition,
            requests,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            success = !matches!(&output, ToolOutput::RecoverableError { .. }),
            "Tool request completed"
        );

        self.wrap_output(request_id, output)
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
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "workspaceSymbol",
            query = %query,
            "Processing tool request"
        );

        let output = symbols::workspace_symbol(
            &self.service,
            self.output_mode,
            query,
            exact,
            limit,
            offset,
            context_lines,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            success = !matches!(&output, ToolOutput::RecoverableError { .. }),
            "Tool request completed"
        );

        self.wrap_output(request_id, output)
    }

    #[tool(name = "goToImplementation", description = "Implementations of interface/trait")]
    async fn go_to_implementation(
        &self,
        path: String,
        line: u32,
        character: u32,
    ) -> ToolOutput {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "goToImplementation",
            path = %path,
            line = line,
            character = character,
            "Processing tool request"
        );

        let output = call_hierarchy::go_to_implementation(&self.service, self.output_mode, path, line, character)
            .await;

        tracing::debug!(
            request_id = %request_id,
            success = !matches!(&output, ToolOutput::RecoverableError { .. }),
            "Tool request completed"
        );

        self.wrap_output(request_id, output)
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
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "callHierarchy",
            path = %path,
            line = line,
            character = character,
            direction = %direction,
            "Processing tool request"
        );

        let output = call_hierarchy::call_hierarchy(
            &self.service,
            self.output_mode,
            path,
            line,
            character,
            direction,
            externals,
            context_lines,
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            success = !matches!(&output, ToolOutput::RecoverableError { .. }),
            "Tool request completed"
        );

        self.wrap_output(request_id, output)
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
        )
        .await;

        tracing::debug!(
            request_id = %request_id,
            success = !matches!(&output, ToolOutput::RecoverableError { .. }),
            "Tool request completed"
        );

        self.wrap_output(request_id, output)
    }

    #[tool(name = "getDiagnostics", description = "Diagnostics for file or workspace")]
    async fn get_diagnostics(&self, file_path: Option<String>) -> ToolOutput {
        let request_id = new_request_id();
        tracing::debug!(
            request_id = %request_id,
            tool = "getDiagnostics",
            "Processing tool request"
        );

        let output = diagnostics::get_diagnostics(&self.service, self.output_mode, file_path).await;

        tracing::debug!(
            request_id = %request_id,
            success = !matches!(&output, ToolOutput::RecoverableError { .. }),
            "Tool request completed"
        );

        self.wrap_output(request_id, output)
    }

    #[tool(
        name = "initialSetup",
        description = "Guided setup for configuring languages, binaries, and tools."
    )]
    async fn initial_setup(&self) -> ToolOutput {
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

## 3) Install language servers
Ensure the default binaries above are on `PATH`, or set `binaries` per language with absolute paths.
If you want install commands, tell the agent your OS/package manager and target languages.
The agent should verify that each configured language server binary is available before continuing.
Check `PATH` with `which` (macOS/Linux) or `where` (Windows).
If a binary is missing, the agent should provide install steps for the user's OS/package manager.

## 4) Disable this tool after setup
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

        ToolOutput::text(instructions)
    }

    #[tool(
        name = "initialInstructions",
        description = "Read this on startup to know how to use LSP-MCP properly."
    )]
    async fn initial_instructions(&self) -> ToolOutput {
        let mut instructions = if self.debug_enabled {
            r#"# LSP-MCP Usage Instructions

## Positioning
All line and character positions use **1-based indexing** (first line is 1, first character is 1). This matches what editors display to users.

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
Use request IDs to correlate tool output with log entries.

## Available Tools
- `documentSymbol` - Get symbols defined in a file
- `goToDefinition` - Jump to symbol definition
- `findReferences` - Find all references to a symbol
- `hover` - Get type/documentation info at a position
- `workspaceSymbol` - Search for symbols by name
- `callHierarchy` - Trace incoming/outgoing calls
- `getDiagnostics` - Get compiler errors/warnings
- `initialSetup` - Guided first-time setup for project configuration"#
            .to_string()
        } else {
            r#"# LSP-MCP Usage Instructions

## Positioning
All line and character positions use **1-based indexing** (first line is 1, first character is 1). This matches what editors display to users.

## Available Tools
- `documentSymbol` - Get symbols defined in a file
- `goToDefinition` - Jump to symbol definition
- `findReferences` - Find all references to a symbol
- `hover` - Get type/documentation info at a position
- `workspaceSymbol` - Search for symbols by name
- `callHierarchy` - Trace incoming/outgoing calls
- `getDiagnostics` - Get compiler errors/warnings
- `initialSetup` - Guided first-time setup for project configuration"#
            .to_string()
        };

        if self.enabled_tools.contains("initialSetup") {
            instructions.push_str(
                r#"

## First-Time Setup
Before using the LSP tools in this agent, run `initialSetup` to configure languages and binaries for this project."#,
            );
        }

        ToolOutput::text(instructions)
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

        let output = server.health().await;
        let text = extract_text_content(&output);

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

        let config = LspMcpConfig::default(); // No debug config
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
    async fn test_instructions_warn_when_project_config_present() {
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
            instructions.contains("Initial Setup Tool Disabled"),
            "Should warn when project config disables initialSetup by default"
        );
        assert!(
            instructions.contains("\"initial_setup\": \"enabled\""),
            "Should mention how to keep initialSetup enabled"
        );
    }

    #[tokio::test]
    async fn test_initial_instructions_prompt_initial_setup_when_enabled() {
        let (server, _temp) = create_test_server().await;
        let output = server.initial_instructions().await;
        let text = extract_text_content(&output);

        assert!(text.contains("First-Time Setup"));
        assert!(text.contains("initialSetup"));
    }
}
