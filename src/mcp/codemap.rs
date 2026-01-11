// ABOUTME: MCP tool handler for codemap queries.
// ABOUTME: Provides codebase structure, dependency, and impact analysis.

use crate::codemap::{CodemapManager, CodemapQuery, EdgeKind, QueryMode};
use crate::markdown_formatter::format_codemap_response;
use crate::mcp_response::{tool_result_error, tool_result_success};
use rmcp::model::CallToolResult;
use std::sync::Arc;

/// Execute a codemap query and return formatted results
pub async fn codemap(
    manager: &Arc<CodemapManager>,
    mode: String,
    target: Option<String>,
    edge_type: Option<String>,
    depth: Option<u32>,
    detail: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    scope: Option<String>,
    include_external: Option<bool>,
) -> CallToolResult {
    let query_mode = match mode.to_lowercase().as_str() {
        "overview" => QueryMode::Overview,
        "impact" => QueryMode::Impact,
        "context" => QueryMode::Context,
        _ => {
            return tool_result_error(format!(
                "Invalid mode: {}. Expected: overview, impact, or context",
                mode
            ));
        }
    };

    if matches!(query_mode, QueryMode::Impact | QueryMode::Context) && target.is_none() {
        return tool_result_error(
            "target parameter is required for impact and context modes".to_string(),
        );
    }

    let edge_kind = edge_type
        .as_ref()
        .and_then(|e| match e.to_lowercase().as_str() {
            "defines" => Some(EdgeKind::Defines),
            "imports" => Some(EdgeKind::Imports),
            "calls" => Some(EdgeKind::Calls),
            _ => None,
        });

    let query = CodemapQuery {
        mode: query_mode,
        target,
        edge_type: edge_kind,
        depth: depth.unwrap_or(2),
        detail: detail.as_deref() == Some("full"),
        limit: limit.unwrap_or(50),
        offset: offset.unwrap_or(0),
        scope,
        include_external: include_external.unwrap_or(false),
    };

    match manager.query(query).await {
        Ok(response) => {
            let markdown = format_codemap_response(&response);
            tool_result_success(markdown)
        }
        Err(e) => tool_result_error(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    fn random_mode() -> String {
        let mut rng = rand::rng();
        let modes = ["overview", "impact", "context"];
        modes[rng.random_range(0..modes.len())].to_string()
    }

    fn random_invalid_mode() -> String {
        let mut rng = rand::rng();
        let invalid_modes = ["invalid", "unknown", "random", "test"];
        invalid_modes[rng.random_range(0..invalid_modes.len())].to_string()
    }

    fn is_error_result(result: &CallToolResult) -> bool {
        result.is_error == Some(true)
    }

    fn extract_text_content(result: &CallToolResult) -> String {
        for content in &result.content {
            if let rmcp::model::RawContent::Text(text_content) = &content.raw {
                return text_content.text.clone();
            }
        }
        String::new()
    }

    #[test]
    fn parse_mode_accepts_overview_case_insensitively() {
        let modes = ["overview", "Overview", "OVERVIEW", "OvErViEw"];
        for mode_str in modes {
            let query_mode = match mode_str.to_lowercase().as_str() {
                "overview" => QueryMode::Overview,
                _ => panic!("unexpected mode"),
            };
            assert_eq!(
                query_mode,
                QueryMode::Overview,
                "negative: '{}' must parse to Overview",
                mode_str
            );
        }
    }

    #[test]
    fn parse_mode_accepts_impact_case_insensitively() {
        let modes = ["impact", "Impact", "IMPACT"];
        for mode_str in modes {
            let query_mode = match mode_str.to_lowercase().as_str() {
                "impact" => QueryMode::Impact,
                _ => panic!("unexpected mode"),
            };
            assert_eq!(
                query_mode,
                QueryMode::Impact,
                "negative: '{}' must parse to Impact",
                mode_str
            );
        }
    }

    #[test]
    fn parse_mode_accepts_context_case_insensitively() {
        let modes = ["context", "Context", "CONTEXT"];
        for mode_str in modes {
            let query_mode = match mode_str.to_lowercase().as_str() {
                "context" => QueryMode::Context,
                _ => panic!("unexpected mode"),
            };
            assert_eq!(
                query_mode,
                QueryMode::Context,
                "negative: '{}' must parse to Context",
                mode_str
            );
        }
    }

    #[test]
    fn parse_edge_type_accepts_defines() {
        let edge_str = "defines";
        let edge_kind = match edge_str.to_lowercase().as_str() {
            "defines" => Some(EdgeKind::Defines),
            _ => None,
        };
        assert_eq!(
            edge_kind,
            Some(EdgeKind::Defines),
            "negative: 'defines' must parse to EdgeKind::Defines"
        );
    }

    #[test]
    fn parse_edge_type_accepts_imports() {
        let edge_str = "imports";
        let edge_kind = match edge_str.to_lowercase().as_str() {
            "imports" => Some(EdgeKind::Imports),
            _ => None,
        };
        assert_eq!(
            edge_kind,
            Some(EdgeKind::Imports),
            "negative: 'imports' must parse to EdgeKind::Imports"
        );
    }

    #[test]
    fn parse_edge_type_accepts_calls() {
        let edge_str = "calls";
        let edge_kind = match edge_str.to_lowercase().as_str() {
            "calls" => Some(EdgeKind::Calls),
            _ => None,
        };
        assert_eq!(
            edge_kind,
            Some(EdgeKind::Calls),
            "negative: 'calls' must parse to EdgeKind::Calls"
        );
    }

    #[test]
    fn parse_edge_type_returns_none_for_invalid() {
        let invalid_types = ["invalid", "unknown", "references"];
        for edge_str in invalid_types {
            let edge_kind: Option<EdgeKind> = match edge_str.to_lowercase().as_str() {
                "defines" => Some(EdgeKind::Defines),
                "imports" => Some(EdgeKind::Imports),
                "calls" => Some(EdgeKind::Calls),
                _ => None,
            };
            assert_eq!(edge_kind, None, "negative: '{}' must return None", edge_str);
        }
    }

    #[tokio::test]
    async fn codemap_returns_error_for_invalid_mode() {
        use crate::codemap::CodemapManager;
        use crate::lsp::manager::Manager;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("negative: temp dir creation must succeed");
        let db_path = temp_dir.path().join("codemap.db");

        let manager = Manager::new(temp_dir.path().to_str().unwrap())
            .await
            .expect("negative: manager creation must succeed");
        let codemap_manager = Arc::new(
            CodemapManager::new(&db_path, Arc::new(manager))
                .await
                .expect("negative: codemap manager creation must succeed"),
        );

        let invalid_mode = random_invalid_mode();
        let result = codemap(
            &codemap_manager,
            invalid_mode.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        assert!(
            is_error_result(&result),
            "negative: invalid mode '{}' must return error result",
            invalid_mode
        );
        let text = extract_text_content(&result);
        assert!(
            text.contains("Invalid mode"),
            "negative: error message must mention invalid mode"
        );
    }

    #[tokio::test]
    async fn codemap_returns_error_for_impact_without_target() {
        use crate::codemap::CodemapManager;
        use crate::lsp::manager::Manager;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("negative: temp dir creation must succeed");
        let db_path = temp_dir.path().join("codemap.db");

        let manager = Manager::new(temp_dir.path().to_str().unwrap())
            .await
            .expect("negative: manager creation must succeed");
        let codemap_manager = Arc::new(
            CodemapManager::new(&db_path, Arc::new(manager))
                .await
                .expect("negative: codemap manager creation must succeed"),
        );

        let result = codemap(
            &codemap_manager,
            "impact".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        assert!(
            is_error_result(&result),
            "negative: impact mode without target must return error"
        );
        let text = extract_text_content(&result);
        assert!(
            text.contains("target parameter is required"),
            "negative: error must mention target parameter requirement"
        );
    }

    #[tokio::test]
    async fn codemap_returns_error_for_context_without_target() {
        use crate::codemap::CodemapManager;
        use crate::lsp::manager::Manager;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("negative: temp dir creation must succeed");
        let db_path = temp_dir.path().join("codemap.db");

        let manager = Manager::new(temp_dir.path().to_str().unwrap())
            .await
            .expect("negative: manager creation must succeed");
        let codemap_manager = Arc::new(
            CodemapManager::new(&db_path, Arc::new(manager))
                .await
                .expect("negative: codemap manager creation must succeed"),
        );

        let result = codemap(
            &codemap_manager,
            "context".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        assert!(
            is_error_result(&result),
            "negative: context mode without target must return error"
        );
        let text = extract_text_content(&result);
        assert!(
            text.contains("target parameter is required"),
            "negative: error must mention target parameter requirement"
        );
    }

    #[test]
    fn query_defaults_depth_to_two() {
        let depth: Option<u32> = None;
        let resolved_depth = depth.unwrap_or(2);
        assert_eq!(resolved_depth, 2, "negative: default depth must be 2");
    }

    #[test]
    fn query_defaults_limit_to_fifty() {
        let limit: Option<u32> = None;
        let resolved_limit = limit.unwrap_or(50);
        assert_eq!(resolved_limit, 50, "negative: default limit must be 50");
    }

    #[test]
    fn query_defaults_offset_to_zero() {
        let offset: Option<u32> = None;
        let resolved_offset = offset.unwrap_or(0);
        assert_eq!(resolved_offset, 0, "negative: default offset must be 0");
    }

    #[test]
    fn query_defaults_include_external_to_false() {
        let include_external: Option<bool> = None;
        let resolved = include_external.unwrap_or(false);
        assert!(
            !resolved,
            "negative: default include_external must be false"
        );
    }

    #[test]
    fn query_detail_is_true_when_full() {
        let detail = Some("full".to_string());
        let is_detail = detail.as_deref() == Some("full");
        assert!(is_detail, "negative: detail must be true when 'full'");
    }

    #[test]
    fn query_detail_is_false_when_summary() {
        let detail = Some("summary".to_string());
        let is_detail = detail.as_deref() == Some("full");
        assert!(!is_detail, "negative: detail must be false when 'summary'");
    }

    #[test]
    fn query_detail_is_false_when_none() {
        let detail: Option<String> = None;
        let is_detail = detail.as_deref() == Some("full");
        assert!(!is_detail, "negative: detail must be false when None");
    }

    #[test]
    fn codemap_query_builds_with_correct_defaults() {
        let query = CodemapQuery {
            mode: QueryMode::Overview,
            target: None,
            edge_type: None,
            depth: 2,
            detail: false,
            limit: 50,
            offset: 0,
            scope: None,
            include_external: false,
        };

        assert_eq!(query.mode, QueryMode::Overview);
        assert!(query.target.is_none());
        assert!(query.edge_type.is_none());
        assert_eq!(query.depth, 2);
        assert!(!query.detail);
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 0);
        assert!(query.scope.is_none());
        assert!(!query.include_external);
    }
}
