// ABOUTME: Tool filtering wrapper for MCP server configuration.
// ABOUTME: Allows dynamic enabling/disabling of tools based on configuration.

use crate::mcp::LspMcpServer;
use crate::mcp_response::tool_disabled_message;
use crate::session::new_request_id;
use rmcp::{
    ServerHandler,
    model::{
        CallToolResult, Content, JsonObject, ListToolsResult, ServerInfo, Tool,
        CallToolRequestParam, PaginatedRequestParam,
    },
    service::{RequestContext, RoleServer},
    ErrorData as McpError,
};
use serde_json::Map;
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;
use serde_json::Value;

/// Wrapper that filters tools based on configuration.
///
/// This allows dynamically enabling/disabling tools at runtime based on
/// the configuration file without modifying the underlying tool implementations.
pub struct FilteredLspMcpServer {
    inner: LspMcpServer,
    enabled_tools: HashSet<String>,
}

impl FilteredLspMcpServer {
    /// Create a new filtered handler wrapping the inner handler.
    pub fn new(inner: LspMcpServer, enabled_tools: HashSet<String>) -> Self {
        Self {
            inner,
            enabled_tools,
        }
    }

    fn filter_tools(&self, tools: Vec<Tool>) -> Vec<Tool> {
        let enabled = &self.enabled_tools;
        tools
            .into_iter()
            .filter(|tool| enabled.contains(&tool.name.to_string()))
            .map(Self::strip_tool_schema)
            .collect()
    }

    fn strip_tool_schema(tool: Tool) -> Tool {
        let mut tool = tool;
        let mut schema = (*tool.input_schema).clone();
        Self::strip_schema_fields(&mut schema);
        tool.input_schema = Arc::new(schema);
        if let Some(description) = Self::short_tool_description(tool.name.as_ref()) {
            tool.description = Some(Cow::Borrowed(description));
        }
        tool
    }

    fn strip_schema_fields(schema: &mut JsonObject) {
        schema.remove("$schema");
        schema.remove("title");
        schema.remove("description");
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|required| {
                required
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect::<HashSet<String>>()
            });
        if let Some(Value::Object(properties)) = schema.get_mut("properties") {
            if let Some(required) = &required {
                properties.retain(|name, _| required.contains(name));
            } else {
                properties.clear();
            }
            if properties.is_empty() {
                schema.remove("properties");
                return;
            }
            Self::minimize_properties(properties);
        }
    }

    fn minimize_properties(properties: &mut Map<String, Value>) {
        for property in properties.values_mut() {
            if let Value::Object(property_schema) = property {
                let mut minimized = Map::new();
                if let Some(property_type) = property_schema.get("type") {
                    minimized.insert("type".to_string(), property_type.clone());
                }
                if let Some(items) = property_schema.get("items") {
                    let minimized_items = match items {
                        Value::Object(items_schema) => {
                            let mut items_minimized = Map::new();
                            if let Some(items_type) = items_schema.get("type") {
                                items_minimized.insert("type".to_string(), items_type.clone());
                            }
                            Value::Object(items_minimized)
                        }
                        _ => items.clone(),
                    };
                    minimized.insert("items".to_string(), minimized_items);
                }
                *property_schema = minimized;
            }
        }
    }

    fn short_tool_description(name: &str) -> Option<&'static str> {
        let description = match name {
            "callHierarchy" => "Call hierarchy at position; use to trace call flow",
            "findIdentifier" => "Identifier occurrences in a file; use for local search",
            "findReferences" => "References at position; use to see usages",
            "goToDefinition" => "Definition at position; use to jump to source",
            "getDiagnostics" => "Diagnostics for file/workspace; use for errors and warnings",
            "goToImplementation" => "Implementation at position; use for interface/trait impls",
            "documentSymbol" => "Symbols defined in a file; use to outline structure",
            "semanticSearch" => "Semantic code search; use natural language queries",
            "findReferencedSymbols" => "Symbols referenced by definition; use to see deps",
            "workspaceSymbol" => "Search symbols by name; use to locate definitions",
            "hover" => "Type/doc info at position; use for quick context",
            "listFiles" => "List workspace files; use to discover paths",
            "readSourceCode" => "Read source from file; use for exact text",
            "health" => "Service status; use to check logs/session",
            "initialSetup" => "Guided setup; use to configure languages/tools",
            _ => return None,
        };
        Some(description)
    }

    fn disabled_tool_result(&self, tool_name: &str) -> CallToolResult {
        let result = CallToolResult::error(vec![Content::text(tool_disabled_message(tool_name))]);
        let request_id = new_request_id();
        self.inner.wrap_output(request_id, result)
    }
}

impl ServerHandler for FilteredLspMcpServer {
    fn get_info(&self) -> ServerInfo {
        self.inner.get_info()
    }

    fn list_tools(
        &self,
        request: Option<PaginatedRequestParam>,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            let result = self.inner.list_tools(request, context).await?;
            let filtered_tools = self.filter_tools(result.tools);
            Ok(ListToolsResult {
                tools: filtered_tools,
                next_cursor: result.next_cursor,
                meta: result.meta,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let enabled = &self.enabled_tools;
        let tool_name = request.name.to_string();
        async move {
            if !enabled.contains(&tool_name) {
                return Ok(self.disabled_tool_result(&tool_name));
            }
            self.inner.call_tool(request, context).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DebugConfig, LspMcpConfig};
    use crate::lsp::manager::Manager;
    use rmcp::model::RawContent;
    use std::borrow::Cow;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn create_test_server(
        debug_enabled: bool,
        enabled_tools: HashSet<String>,
    ) -> (FilteredLspMcpServer, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workspace_root = temp_dir.path();

        let test_file = workspace_root.join("test.rs");
        std::fs::write(&test_file, "fn main() {}").expect("Failed to write test file");

        let manager = Manager::new(workspace_root.to_str().unwrap())
            .await
            .expect("Failed to create manager");

        let config = if debug_enabled {
            LspMcpConfig {
                debug: Some(DebugConfig {
                    enabled: true,
                    ..Default::default()
                }),
                ..Default::default()
            }
        } else {
            LspMcpConfig::default()
        };

        let server = LspMcpServer::new(Arc::new(manager), &config, workspace_root);
        let filtered_server = FilteredLspMcpServer::new(server, enabled_tools);
        (filtered_server, temp_dir)
    }

    fn extract_text_content(result: &CallToolResult) -> String {
        for content in &result.content {
            if let RawContent::Text(text_content) = &content.raw {
                return text_content.text.clone();
            }
        }
        String::new()
    }

    #[tokio::test]
    async fn test_filter_tools_excludes_disabled_tools() {
        let enabled_tools: HashSet<String> = ["hover".to_string()].into_iter().collect();
        let (server, _temp) = create_test_server(false, enabled_tools).await;

        let schema = Arc::new(serde_json::Map::new());
        let tools = vec![
            Tool {
                name: Cow::Borrowed("hover"),
                title: None,
                description: None,
                input_schema: schema.clone(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: Cow::Borrowed("health"),
                title: None,
                description: None,
                input_schema: schema,
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
        ];

        let filtered = server.filter_tools(tools);
        assert_eq!(filtered.len(), 1, "Disabled tools still present after filtering");
        assert_eq!(filtered[0].name.as_ref(), "hover", "Enabled hover tool was removed");
    }

    #[tokio::test]
    async fn test_disabled_tool_result_includes_request_id_when_debug_enabled() {
        let enabled_tools: HashSet<String> = HashSet::new();
        let (server, _temp) = create_test_server(true, enabled_tools).await;

        let result = server.disabled_tool_result("health");
        assert_eq!(result.is_error, Some(true), "Disabled tool did not return error result");
        let text = extract_text_content(&result);
        assert!(text.contains("<!-- request:"), "Request ID header missing in debug mode");
        assert!(
            text.contains(&tool_disabled_message("health")),
            "Disabled tool message missing"
        );
    }

    #[tokio::test]
    async fn test_filter_tools_strips_schema_fields() {
        let enabled_tools: HashSet<String> = ["hover".to_string()].into_iter().collect();
        let (server, _temp) = create_test_server(false, enabled_tools).await;

        let mut property_schema = serde_json::Map::new();
        property_schema.insert("type".to_string(), Value::String("integer".to_string()));
        property_schema.insert("format".to_string(), Value::String("uint32".to_string()));
        property_schema.insert("minimum".to_string(), Value::Number(0.into()));
        property_schema.insert("nullable".to_string(), Value::Bool(true));

        let mut optional_schema = serde_json::Map::new();
        optional_schema.insert("type".to_string(), Value::String("string".to_string()));

        let mut properties = serde_json::Map::new();
        properties.insert("limit".to_string(), Value::Object(property_schema));
        properties.insert("offset".to_string(), Value::Object(optional_schema));

        let mut schema = serde_json::Map::new();
        schema.insert("$schema".to_string(), Value::String("schema".to_string()));
        schema.insert("title".to_string(), Value::String("Title".to_string()));
        schema.insert("description".to_string(), Value::String("Desc".to_string()));
        schema.insert("type".to_string(), Value::String("object".to_string()));
        schema.insert("properties".to_string(), Value::Object(properties));
        schema.insert(
            "required".to_string(),
            Value::Array(vec![Value::String("limit".to_string())]),
        );

        let tools = vec![Tool {
            name: Cow::Borrowed("hover"),
            title: None,
            description: None,
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }];

        let filtered = server.filter_tools(tools);
        let schema = filtered[0].input_schema.as_ref();
        assert!(
            !schema.contains_key("$schema"),
            "Schema still included $schema"
        );
        assert!(!schema.contains_key("title"), "Schema still included title");
        assert!(
            !schema.contains_key("description"),
            "Schema still included description"
        );

        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("Schema properties missing");
        let limit_schema = properties
            .get("limit")
            .and_then(Value::as_object)
            .expect("Property schema missing");
        assert!(
            !properties.contains_key("offset"),
            "Optional property was not removed"
        );
        assert_eq!(
            limit_schema.get("type"),
            Some(&Value::String("integer".to_string())),
            "Property type was not preserved"
        );
        assert_eq!(
            limit_schema.len(),
            1,
            "Property schema includes extra fields"
        );
        assert!(
            !limit_schema.contains_key("format"),
            "Property schema still included format"
        );
        assert!(
            !limit_schema.contains_key("minimum"),
            "Property schema still included minimum"
        );
        assert!(
            !limit_schema.contains_key("nullable"),
            "Property schema still included nullable"
        );
    }

    #[tokio::test]
    async fn test_filter_tools_sets_short_description() {
        let enabled_tools: HashSet<String> = ["hover".to_string()].into_iter().collect();
        let (server, _temp) = create_test_server(false, enabled_tools).await;

        let schema = Arc::new(serde_json::Map::new());
        let tools = vec![Tool {
            name: Cow::Borrowed("hover"),
            title: None,
            description: Some(Cow::Borrowed("Long description")),
            input_schema: schema,
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }];

        let filtered = server.filter_tools(tools);
        assert_eq!(
            filtered[0].description.as_deref(),
            Some("Type/doc info at position; use for quick context"),
            "Tool description was not shortened"
        );
    }
}
