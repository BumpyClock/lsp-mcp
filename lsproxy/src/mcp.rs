// ABOUTME: MCP server tools and handler for exposing LSP-based code navigation.
// ABOUTME: Provides stdio MCP tool definitions and request handling for a workspace manager.
use crate::api_types::{HealthResponse, Position, Range};
use crate::lsp::manager::Manager;
use crate::service::{create_service, LspService};
use mcpkit::prelude::*;
use mcpkit::transport::stdio::StdioTransport;
use std::sync::Arc;

/// LSP MCP Server that exposes code navigation tools for a workspace.
pub struct LspMcpServer {
    service: LspService,
}

impl LspMcpServer {
    pub fn new(manager: Arc<Manager>) -> Self {
        LspMcpServer {
            service: create_service(manager),
        }
    }
}

#[mcp_server(
    name = "lsp-mcp",
    version = "0.4.4"
)]
impl LspMcpServer {
    #[tool(description = "Returns symbols defined in a file relative to the workspace root")]
    async fn definitions_in_file(&self, file_path: String) -> ToolOutput {
        match self.service.definitions_in_file(&file_path).await {
            Ok(symbols) => {
                let summary = format!("Found {} symbols", symbols.len());
                match serde_json::to_string_pretty(&symbols) {
                    Ok(json) => ToolOutput::text(format!("{}\n\n{}", summary, json)),
                    Err(e) => ToolOutput::error(format!("Serialization error: {}", e)),
                }
            }
            Err(e) => ToolOutput::error(format!("Error: {}", e)),
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
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .find_definition(
                &path,
                pos,
                include_source_code.unwrap_or(false),
                include_raw_response.unwrap_or(false),
            )
            .await
        {
            Ok(response) => {
                let summary = format!("Found {} definitions", response.definitions.len());
                match serde_json::to_string_pretty(&response) {
                    Ok(json) => ToolOutput::text(format!("{}\n\n{}", summary, json)),
                    Err(e) => ToolOutput::error(format!("Serialization error: {}", e)),
                }
            }
            Err(e) => ToolOutput::error(format!("Error: {}", e)),
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
    ) -> ToolOutput {
        let pos = Position { line, character };
        match self
            .service
            .find_references(
                &path,
                pos,
                include_raw_response.unwrap_or(false),
                include_code_context_lines,
            )
            .await
        {
            Ok(response) => {
                let summary = format!("Found {} references", response.references.len());
                match serde_json::to_string_pretty(&response) {
                    Ok(json) => ToolOutput::text(format!("{}\n\n{}", summary, json)),
                    Err(e) => ToolOutput::error(format!("Serialization error: {}", e)),
                }
            }
            Err(e) => ToolOutput::error(format!("Error: {}", e)),
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
                let summary = format!(
                    "Found {} workspace symbols and {} external symbols",
                    response.workspace_symbols.len(),
                    response.external_symbols.len()
                );
                match serde_json::to_string_pretty(&response) {
                    Ok(json) => ToolOutput::text(format!("{}\n\n{}", summary, json)),
                    Err(e) => ToolOutput::error(format!("Serialization error: {}", e)),
                }
            }
            Err(e) => ToolOutput::error(format!("Error: {}", e)),
        }
    }

    #[tool(description = "Finds identifiers by name in a file with an optional position")]
    async fn find_identifier(
        &self,
        path: String,
        name: String,
        line: Option<u32>,
        character: Option<u32>,
    ) -> ToolOutput {
        let position = match (line, character) {
            (Some(l), Some(c)) => Some(Position {
                line: l,
                character: c,
            }),
            _ => None,
        };
        match self.service.find_identifier(&path, &name, position).await {
            Ok(response) => {
                let summary = format!("Found {} identifiers", response.identifiers.len());
                match serde_json::to_string_pretty(&response) {
                    Ok(json) => ToolOutput::text(format!("{}\n\n{}", summary, json)),
                    Err(e) => ToolOutput::error(format!("Serialization error: {}", e)),
                }
            }
            Err(e) => ToolOutput::error(format!("Error: {}", e)),
        }
    }

    #[tool(description = "Lists files in the workspace")]
    async fn list_files(&self) -> ToolOutput {
        match self.service.list_files().await {
            Ok(files) => {
                let summary = format!("Found {} files", files.len());
                match serde_json::to_string_pretty(&files) {
                    Ok(json) => ToolOutput::text(format!("{}\n\n{}", summary, json)),
                    Err(e) => ToolOutput::error(format!("Serialization error: {}", e)),
                }
            }
            Err(e) => ToolOutput::error(format!("Error: {}", e)),
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
                let summary = format!("Read {} characters", source_code.len());
                ToolOutput::text(format!("{}\n\n{}", summary, source_code))
            }
            Err(e) => ToolOutput::error(format!("Error: {}", e)),
        }
    }

    #[tool(description = "Returns service status and supported language availability")]
    async fn health(&self) -> ToolOutput {
        let response = HealthResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            languages: self.service.health().await,
        };
        match serde_json::to_string_pretty(&response) {
            Ok(json) => ToolOutput::text(format!("Service status ok\n\n{}", json)),
            Err(e) => ToolOutput::error(format!("Serialization error: {}", e)),
        }
    }

    #[tool(description = "Returns diagnostics (errors, warnings, hints) for a file or the entire workspace. Diagnostics are pushed by language servers and cached. Pass a file path for single file diagnostics, or omit for all workspace diagnostics.")]
    async fn get_diagnostics(&self, file_path: Option<String>) -> ToolOutput {
        match self.service.get_diagnostics(file_path.as_deref()).await {
            Ok(response) => {
                let summary = if response.total_count == 0 {
                    "No diagnostics found".to_string()
                } else {
                    format!(
                        "Found {} diagnostics across {} files",
                        response.total_count,
                        response.files.len()
                    )
                };
                match serde_json::to_string_pretty(&response) {
                    Ok(json) => ToolOutput::text(format!("{}\n\n{}", summary, json)),
                    Err(e) => ToolOutput::error(format!("Serialization error: {}", e)),
                }
            }
            Err(e) => ToolOutput::error(format!("Error: {}", e)),
        }
    }
}

/// Create and run the LSP MCP server over stdio
pub async fn run_server(manager: Arc<Manager>) -> Result<(), McpError> {
    let server_instance = Arc::new(LspMcpServer::new(manager));
    let transport = StdioTransport::new();
    let server = ServerBuilder::new(Arc::clone(&server_instance))
        .with_tools(Arc::clone(&server_instance))
        .build();
    server.serve(transport).await
}
