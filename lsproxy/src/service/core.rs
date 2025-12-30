// ABOUTME: Domain service layer for LSP-backed code navigation operations.
// ABOUTME: Provides async methods for symbol lookup, references, and file access.

use crate::api_types::{
    CallHierarchyDirection, CallHierarchyResponse, DiagnosticsResponse, HoverResponse,
    ImplementationResponse, IncomingCallsResponse, LspStatus, OutgoingCallsResponse, Position,
    PrepareCallHierarchyResponse, Range, ReferencedSymbolsResponse, SupportedLanguages,
    WorkspaceSymbolResponse,
};
use crate::lsp::manager::Manager;
use lsp_types::Range as LspRange;
use std::collections::HashMap;
use std::sync::Arc;

use super::operations::{call_hierarchy, definitions, diagnostics, hover, references, symbols};
use super::types::errors::ServiceError;
use super::types::response::{
    McpDefinitionResponse, McpIdentifierResponse, McpListFilesResponse, McpReferencesResponse,
    McpSymbolsResponse,
};

/// Provides code navigation operations over a workspace manager.
///
/// # Example
/// ```
/// use std::sync::Arc;
/// use lsproxy::lsp::manager::Manager;
/// use lsproxy::service::create_service;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = Arc::new(Manager::new("/tmp").await?);
/// let service = create_service(manager);
/// let _files = service.list_files(None, None).await?;
/// # Ok(())
/// # }
/// ```
pub struct LspService {
    manager: Arc<Manager>,
}

/// Creates a new LspService instance with the given manager.
pub fn create_service(manager: Arc<Manager>) -> LspService {
    LspService { manager }
}

impl LspService {
    /// Retrieves all symbol definitions in a file with enriched metadata.
    pub async fn definitions_in_file(
        &self,
        file_path: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpSymbolsResponse, ServiceError> {
        definitions::definitions_in_file_impl(&self.manager, file_path, limit, offset).await
    }

    /// Finds the definition of a symbol at the given position.
    pub async fn find_definition(
        &self,
        file_path: &str,
        position: Position,
        include_source_code: bool,
        include_raw_response: bool,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpDefinitionResponse, ServiceError> {
        definitions::find_definition_impl(
            &self.manager,
            file_path,
            position,
            include_source_code,
            include_raw_response,
            limit,
            offset,
        )
        .await
    }

    /// Finds implementations of an interface or abstract method.
    pub async fn find_implementation(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<ImplementationResponse, ServiceError> {
        definitions::find_implementation_impl(&self.manager, file_path, position, include_raw_response)
            .await
    }

    /// Finds all references to the symbol at the given position.
    pub async fn find_references(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
        context_lines: Option<u32>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpReferencesResponse, ServiceError> {
        references::find_references_impl(
            &self.manager,
            file_path,
            position,
            include_raw_response,
            context_lines,
            limit,
            offset,
        )
        .await
    }

    /// Finds all symbols referenced at the given position and resolves their definitions.
    pub async fn find_referenced_symbols(
        &self,
        file_path: &str,
        position: Position,
        full_scan: bool,
    ) -> Result<ReferencedSymbolsResponse, ServiceError> {
        references::find_referenced_symbols_impl(&self.manager, file_path, position, full_scan).await
    }

    /// Finds identifiers matching the given name in a file.
    pub async fn find_identifier(
        &self,
        file_path: &str,
        name: &str,
        position: Option<Position>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpIdentifierResponse, ServiceError> {
        symbols::find_identifier_impl(&self.manager, file_path, name, position, limit, offset).await
    }

    /// Lists all files tracked by the workspace.
    pub async fn list_files(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<McpListFilesResponse, ServiceError> {
        symbols::list_files_impl(&self.manager, limit, offset).await
    }

    /// Reads source code from a file, optionally within a specific range.
    pub async fn read_source_code(
        &self,
        file_path: &str,
        range: Option<Range>,
    ) -> Result<String, ServiceError> {
        let lsp_range = range.map(|range| LspRange::new(range.start.into(), range.end.into()));
        Ok(self.manager.read_source_code(file_path, lsp_range).await?)
    }

    /// Returns health status for all supported language servers.
    pub async fn health(&self) -> HashMap<SupportedLanguages, LspStatus> {
        let mut languages = HashMap::new();
        for lang in [
            SupportedLanguages::Python,
            SupportedLanguages::TypeScriptJavaScript,
            SupportedLanguages::Rust,
            SupportedLanguages::CPP,
            SupportedLanguages::CSharp,
            SupportedLanguages::Java,
            SupportedLanguages::Golang,
            SupportedLanguages::PHP,
        ] {
            let status = if self.manager.get_client(lang).await.is_some() {
                LspStatus::Ready
            } else if self.manager.is_language_pending(lang).await {
                LspStatus::Initializing
            } else {
                LspStatus::Unavailable
            };
            languages.insert(lang, status);
        }
        languages
    }

    /// Gets diagnostics (errors, warnings, hints) for a file or the entire workspace.
    pub async fn get_diagnostics(
        &self,
        file_path: Option<&str>,
    ) -> Result<DiagnosticsResponse, ServiceError> {
        diagnostics::get_diagnostics_impl(&self.manager, file_path).await
    }

    /// Gets hover information (documentation, type info) for a symbol at a given position.
    pub async fn hover(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
        include_definition: bool,
    ) -> Result<HoverResponse, ServiceError> {
        hover::hover_impl(&self.manager, file_path, position, include_raw_response, include_definition)
            .await
    }

    /// Searches for symbols across the workspace.
    pub async fn workspace_symbol(
        &self,
        query: &str,
        include_raw_response: bool,
        exact: bool,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<WorkspaceSymbolResponse, ServiceError> {
        symbols::workspace_symbol_impl(&self.manager, query, include_raw_response, exact, limit, offset)
            .await
    }

    /// Prepares the call hierarchy at the given position.
    pub async fn prepare_call_hierarchy(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<PrepareCallHierarchyResponse, ServiceError> {
        call_hierarchy::prepare_call_hierarchy_impl(&self.manager, file_path, position, include_raw_response)
            .await
    }

    /// Gets incoming calls (callers) for the function at the given position.
    pub async fn incoming_calls(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<IncomingCallsResponse, ServiceError> {
        call_hierarchy::incoming_calls_impl(&self.manager, file_path, position, include_raw_response)
            .await
    }

    /// Gets outgoing calls (callees) for the function at the given position.
    pub async fn outgoing_calls(
        &self,
        file_path: &str,
        position: Position,
        include_raw_response: bool,
    ) -> Result<OutgoingCallsResponse, ServiceError> {
        call_hierarchy::outgoing_calls_impl(&self.manager, file_path, position, include_raw_response)
            .await
    }

    /// Unified method for call hierarchy traversal in either direction.
    pub async fn call_hierarchy(
        &self,
        file_path: &str,
        position: Position,
        direction: CallHierarchyDirection,
    ) -> Result<CallHierarchyResponse, ServiceError> {
        call_hierarchy::call_hierarchy_impl(&self.manager, file_path, position, direction).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::WorkspaceSymbolInfo;
    use crate::service::operations::references::{group_references_by_file, is_import_line};
    use crate::service::operations::symbols::match_kind_and_score;
    use crate::service::types::errors::CallHierarchyError;
    use crate::service::types::errors::PositionError;
    use crate::service::types::request::FindDefinitionParams;
    use crate::service::types::response::{CompactDefinitionResponse, McpDefinitionLocation, McpReferenceLocation, TypeCounts};
    use crate::service::utils::external::{parse_pnpm_package_info, ExternalInfo, PackageInfo};
    use crate::service::utils::signature::{
        batch_hover_for_signatures, filter_sibling_exports, is_internal_builder_symbol, truncate_signature,
        DEFAULT_MAX_SIGNATURE_LENGTH,
    };
    use crate::service::utils::transformations::{
        call_hierarchy_item_to_info, definition_item_from_location, reference_item_from_location,
        workspace_symbol_info_from_lsp,
    };
    use crate::api_types::{CodeContext, FilePosition, FileRange, Identifier, Symbol};
    use lsp_types::{
        CallHierarchyItem, Location, Position as LspPosition, Range as LspRange, SymbolInformation,
        SymbolKind, Url,
    };
    use crate::api_types::RelatedSymbols;
    use rand::{distr::Alphanumeric, Rng};
    use std::thread;
    use tempfile::TempDir;
    use crate::service::utils::pagination::paginate_items;

    fn random_irregular_string() -> String {
        let mut rng = rand::rng();
        let len: usize = rng.random_range(6..20);
        let mut value: String = rng
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect();
        value.push('_');
        value.push('\t');
        value
    }

    fn retry_with<T, F>(mut op: F) -> T
    where
        F: FnMut() -> Option<T>,
    {
        let mut rng = rand::rng();
        let attempts: usize = rng.random_range(2..5);
        for _ in 0..attempts {
            let result = op();
            if result.is_some() {
                return result.unwrap();
            }
        }
        let message = random_irregular_string();
        panic!("{}", message);
    }

    #[allow(deprecated)]
    #[test]
    fn test_workspace_symbol_info_kind_normalized() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let range = LspRange {
            start: LspPosition {
                line: 1,
                character: 0,
            },
            end: LspPosition {
                line: 1,
                character: 4,
            },
        };
        let sym = SymbolInformation {
            name: "Example".to_string(),
            kind: SymbolKind::ENUM_MEMBER,
            tags: None,
            deprecated: None,
            location: Location { uri, range },
            container_name: None,
        };

        let info = workspace_symbol_info_from_lsp(sym, "src/main.rs".to_string());

        assert_eq!(info.kind, "enum-member");
        assert_eq!(info.location.path, "src/main.rs");
    }

    #[test]
    fn test_call_hierarchy_kind_normalized() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let range = LspRange {
            start: LspPosition {
                line: 2,
                character: 1,
            },
            end: LspPosition {
                line: 2,
                character: 6,
            },
        };
        let item = CallHierarchyItem {
            name: "Thing".to_string(),
            kind: SymbolKind::TYPE_PARAMETER,
            tags: None,
            detail: None,
            uri,
            range: range.clone(),
            selection_range: range,
            data: None,
        };

        let info = call_hierarchy_item_to_info(&item);

        assert_eq!(info.kind, "type-parameter");
    }

    #[test]
    fn test_parse_pnpm_package_scoped_with_peer_deps() {
        let path = "node_modules/.pnpm/@reduxjs+toolkit@2.9.1_react-redux@9.2.0_react@18.3.1__react@18.3.1/node_modules/@reduxjs/toolkit/dist/index.js";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "@reduxjs/toolkit");
        assert_eq!(info.version, "2.9.1");
    }

    #[test]
    fn test_parse_pnpm_package_non_scoped_with_peer_deps() {
        let path = "node_modules/.pnpm/lodash@4.17.21_react@18.3.1/node_modules/lodash/index.js";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "lodash");
        assert_eq!(info.version, "4.17.21");
    }

    #[test]
    fn test_parse_pnpm_package_scoped_no_peer_deps() {
        let path = "node_modules/.pnpm/@types+node@20.10.0/node_modules/@types/node/index.d.ts";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "@types/node");
        assert_eq!(info.version, "20.10.0");
    }

    #[test]
    fn test_parse_pnpm_package_non_scoped_no_peer_deps() {
        let path = "node_modules/.pnpm/react@18.3.1/node_modules/react/index.js";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "react");
        assert_eq!(info.version, "18.3.1");
    }

    #[test]
    fn test_parse_pnpm_package_complex_peer_deps() {
        let path = "node_modules/.pnpm/@emotion+react@11.11.0_@types+react@18.2.0_react@18.3.1/node_modules/@emotion/react/dist/index.js";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "@emotion/react");
        assert_eq!(info.version, "11.11.0");
    }

    #[test]
    fn test_parse_pnpm_package_not_pnpm_path() {
        let path = "node_modules/react/index.js";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn it_reports_language_servers_unavailable_without_startup(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let random_suffix: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(10)
            .map(char::from)
            .collect();
        let workspace_root = temp_dir.path().join(format!("ñ{}", random_suffix));
        tokio::fs::create_dir_all(&workspace_root).await?;
        let manager = Manager::new(
            workspace_root
                .to_str()
                .ok_or("Workspace root path must be valid utf8")?,
        )
        .await?;
        let service = create_service(Arc::new(manager));

        let mut attempts_remaining = 3;
        let mut results = tokio::join!(service.health(), service.health());
        while attempts_remaining > 0
            && (results.0.values().any(|status| *status == LspStatus::Ready) || results.0 != results.1)
        {
            attempts_remaining -= 1;
            results = tokio::join!(service.health(), service.health());
        }

        let all_unavailable = results.0.values().all(|status| *status == LspStatus::Unavailable);
        let consistent = results.0 == results.1;
        assert!(
            all_unavailable && consistent,
            "Expected language servers to be unavailable and consistent but they were not"
        );

        Ok(())
    }

    #[tokio::test]
    async fn it_cannot_crash_when_language_servers_are_missing(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_left = TempDir::new()?;
        let temp_right = TempDir::new()?;
        let random_left: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(9)
            .map(char::from)
            .collect();
        let random_right: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(9)
            .map(char::from)
            .collect();
        let irregular_left = format!("ñ{}", random_left);
        let irregular_right = format!("ñ{}", random_right);
        let workspace_left = temp_left.path().join(format!("workspace_{}", random_left));
        let workspace_right = temp_right.path().join(format!("workspace_{}", random_right));
        tokio::fs::create_dir_all(&workspace_left).await?;
        tokio::fs::create_dir_all(&workspace_right).await?;
        let file_left = workspace_left.join(format!("sample_{}.py", random_left));
        let file_right = workspace_right.join(format!("sample_{}.py", random_right));
        tokio::fs::write(&file_left, format!("print('{}')", irregular_left)).await?;
        tokio::fs::write(&file_right, format!("print('{}')", irregular_right)).await?;

        let path_override_dir = TempDir::new()?;
        let path_override = path_override_dir
            .path()
            .join(format!("path_{}", rand::rng().random::<u32>()));
        tokio::fs::create_dir_all(&path_override).await?;
        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &path_override);

        let workspace_left_str = workspace_left.to_str().ok_or(irregular_left.clone())?;
        let workspace_right_str = workspace_right.to_str().ok_or(irregular_right.clone())?;
        let mut manager_left = Manager::new(workspace_left_str).await?;
        let mut manager_right = Manager::new(workspace_right_str).await?;

        let (result_left, result_right) = tokio::join!(
            retry_start(&mut manager_left, workspace_left_str),
            retry_start(&mut manager_right, workspace_right_str)
        );

        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        } else {
            std::env::remove_var("PATH");
        }

        let left_ok = result_left.is_ok();
        let right_ok = result_right.is_ok();
        assert!(
            left_ok && right_ok,
            "Did not ignore missing language servers on startup"
        );

        Ok(())
    }

    #[test]
    fn it_paginates_items_with_offset_and_truncation() {
        let mut rng = rand::rng();
        let total_len: usize = rng.random_range(6..20);
        let offset: u32 = rng.random_range(0..(total_len as u32 / 2 + 1));
        let limit: u32 = rng.random_range(1..(total_len as u32 / 2 + 2));
        let mut items = Vec::with_capacity(total_len);
        for _ in 0..total_len {
            items.push(random_irregular_string());
        }
        let expected_items = items.clone();
        let response = retry_with(|| {
            let items = items.clone();
            let handle = thread::spawn(move || paginate_items(items, Some(limit), Some(offset)));
            handle.join().ok()
        });
        let (actual_items, pagination) = response;
        let start = offset as usize;
        let end = std::cmp::min(start.saturating_add(limit as usize), expected_items.len());
        let expected_slice = expected_items[start..end].to_vec();
        assert_eq!(
            actual_items,
            expected_slice,
            "negative: paginated items mismatch"
        );
        assert_eq!(pagination.limit, limit, "negative: limit mismatch");
        assert_eq!(pagination.offset, offset, "negative: offset mismatch");
        assert_eq!(
            pagination.truncated,
            end < expected_items.len(),
            "negative: truncation mismatch"
        );
    }

    #[test]
    fn it_scores_prefix_matches_for_workspace_symbols() {
        let mut rng = rand::rng();
        let base = random_irregular_string();
        let prefix_len = rng.random_range(1..(base.len().saturating_sub(1).max(2)));
        let query: String = base.chars().take(prefix_len).collect();
        let name = format!("{}{}", base, random_irregular_string());
        let response = retry_with(|| {
            let query = query.clone();
            let name = name.clone();
            let handle = thread::spawn(move || match_kind_and_score(&query, &name));
            handle.join().ok()
        });
        let expected_kind = String::from("prefix");
        let (kind, score) = response;
        assert_eq!(kind, expected_kind, "negative: match kind mismatch");
        assert!(
            score > 0.7,
            "negative: match score did not exceed expected threshold"
        );
    }

    #[test]
    fn it_builds_definition_location_with_symbol_range_and_snippet() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        let file_name = format!("file_{}.rs", random_irregular_string());
        let file_path = temp_dir.path().join(file_name);
        let uri = Url::from_file_path(&file_path).expect("negative: uri creation failed");
        let mut rng = rand::rng();
        let start_line: u32 = rng.random_range(1..100);
        let start_char: u32 = rng.random_range(0..20);
        let end_line: u32 = start_line + rng.random_range(0..5);
        let end_char: u32 = start_char + rng.random_range(1..5);
        let location = Location {
            uri,
            range: LspRange {
                start: LspPosition {
                    line: start_line,
                    character: start_char,
                },
                end: LspPosition {
                    line: end_line,
                    character: end_char,
                },
            },
        };
        let expected_path = file_path.to_string_lossy().into_owned();
        let symbol_range = Range {
            start: Position {
                line: start_line + 1,
                character: 0,
            },
            end: Position {
                line: end_line + 2,
                character: 3,
            },
        };
        let symbol = Symbol {
            name: random_irregular_string(),
            kind: random_irregular_string(),
            identifier_position: FilePosition {
                path: expected_path.clone(),
                position: Position {
                    line: start_line,
                    character: start_char,
                },
            },
            file_range: FileRange {
                path: expected_path.clone(),
                range: symbol_range.clone(),
            },
            signature: None,
            exported: None,
            jsdoc_summary: None,
            dependencies: None,
            line_count: None,
            children: None,
        };
        let snippet = CodeContext {
            range: FileRange {
                path: expected_path.clone(),
                range: symbol_range.clone(),
            },
            source_code: random_irregular_string(),
        };
        let expected_signature = Some("fn test_function()".to_string());
        let expected_jsdoc = Some("Test documentation".to_string());
        let response = retry_with(|| {
            let location = location.clone();
            let symbol = symbol.clone();
            let snippet = snippet.clone();
            let sig = expected_signature.clone();
            let doc = expected_jsdoc.clone();
            let handle = thread::spawn(move || {
                Some(definition_item_from_location(
                    &location,
                    Some(symbol),
                    Some(snippet),
                    sig,
                    doc,
                    None,
                ))
            });
            handle.join().ok().flatten()
        });
        assert_eq!(response.path, expected_path, "negative: path mismatch");
        assert_eq!(
            response.position.line, start_line + 1,
            "negative: line mismatch"
        );
        assert_eq!(
            response.position.character, start_char + 1,
            "negative: character mismatch"
        );
        assert_eq!(
            response.definition_range, symbol_range,
            "negative: definition range mismatch"
        );
        assert_eq!(
            response.symbol_kind,
            Some(symbol.kind.clone()),
            "negative: symbol kind mismatch"
        );
        assert_eq!(
            response.signature, expected_signature,
            "negative: signature mismatch"
        );
        assert_eq!(
            response.doc, expected_jsdoc,
            "negative: doc mismatch"
        );
        assert_eq!(
            response.snippet,
            Some(snippet),
            "negative: snippet mismatch"
        );
    }

    #[test]
    fn it_builds_reference_location_with_symbol_range_and_snippet() {
        let temp_dir = TempDir::new().expect("negative: temp dir unavailable");
        let file_name = format!("ref_{}.rs", random_irregular_string());
        let file_path = temp_dir.path().join(file_name);
        let uri = Url::from_file_path(&file_path).expect("negative: uri creation failed");
        let mut rng = rand::rng();
        let start_line: u32 = rng.random_range(1..100);
        let start_char: u32 = rng.random_range(0..20);
        let end_line: u32 = start_line + rng.random_range(0..5);
        let end_char: u32 = start_char + rng.random_range(1..5);
        let location = Location {
            uri,
            range: LspRange {
                start: LspPosition {
                    line: start_line,
                    character: start_char,
                },
                end: LspPosition {
                    line: end_line,
                    character: end_char,
                },
            },
        };
        let expected_path = file_path.to_string_lossy().into_owned();
        let expected_range = Range {
            start: Position {
                line: start_line + 1,
                character: start_char + 1,
            },
            end: Position {
                line: end_line + 1,
                character: end_char + 1,
            },
        };
        let snippet = CodeContext {
            range: FileRange {
                path: expected_path.clone(),
                range: expected_range.clone(),
            },
            source_code: random_irregular_string(),
        };
        let response = retry_with(|| {
            let location = location.clone();
            let snippet = snippet.clone();
            let handle =
                thread::spawn(move || Some(reference_item_from_location(&location, Some(snippet))));
            handle.join().ok().flatten()
        });
        assert_eq!(response.path, Some(expected_path), "negative: path mismatch");
        assert_eq!(
            response.position.line, start_line + 1,
            "negative: line mismatch"
        );
        assert_eq!(
            response.position.character, start_char + 1,
            "negative: character mismatch"
        );
        assert_eq!(
            response.symbol_range, expected_range,
            "negative: reference range mismatch"
        );
        assert_eq!(
            response.snippet,
            Some(snippet),
            "negative: snippet mismatch"
        );
    }

    async fn retry_start(
        manager: &mut Manager,
        workspace_root: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut attempts_remaining = 2;
        let mut result = manager.start_langservers(workspace_root, None).await;
        while attempts_remaining > 0 && result.is_err() {
            attempts_remaining -= 1;
            result = manager.start_langservers(workspace_root, None).await;
        }
        result
    }

    #[tokio::test]
    async fn test_definitions_in_file_includes_mtime_and_path() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        let test_file = workspace_root.join("test.rs");
        tokio::fs::write(&test_file, "fn example() {}").await?;

        let manager = Manager::new(workspace_root.to_str().unwrap()).await?;
        let service = create_service(Arc::new(manager));

        let response = service.definitions_in_file("test.rs", None, None).await?;

        assert_eq!(response.path, "test.rs");
        assert!(!response.mtime.is_empty());
        assert!(chrono::DateTime::parse_from_rfc3339(&response.mtime).is_ok(),
            "mtime should be valid RFC3339: {}", response.mtime);
        assert_eq!(response.limit, 200);
        assert_eq!(response.offset, 0);
        assert!(!response.truncated);

        Ok(())
    }

    #[tokio::test]
    async fn test_definitions_in_file_enriches_symbols() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        let test_file = workspace_root.join("test.rs");
        let source = r#"/// This function does something
pub fn example(x: i32) -> String {
    format!("{}", x)
}

fn internal_helper() {
    println!("internal");
}"#;
        tokio::fs::write(&test_file, source).await?;

        let manager = Manager::new(workspace_root.to_str().unwrap()).await?;
        let service = create_service(Arc::new(manager));

        let response = service.definitions_in_file("test.rs", None, None).await?;

        let pub_fn = response.symbols.iter()
            .find(|s| s.name == "example")
            .expect("Should find 'example' function");

        assert!(pub_fn.line_count.is_some(), "line_count should be populated");
        let line_count = pub_fn.line_count.unwrap();
        assert!(line_count >= 3, "Function should span at least 3 lines, got {}", line_count);
        assert!(pub_fn.exported.is_some(), "exported should be populated");

        let internal_fn = response.symbols.iter()
            .find(|s| s.name == "internal_helper")
            .expect("Should find 'internal_helper' function");

        assert!(internal_fn.line_count.is_some(), "line_count should be populated for all symbols");

        Ok(())
    }

    #[test]
    fn test_group_references_by_file() {
        let refs = vec![
            McpReferenceLocation {
                path: Some("src/main.rs".to_string()),
                position: Position { line: 1, character: 5 },
                symbol_range: Range {
                    start: Position { line: 1, character: 5 },
                    end: Position { line: 1, character: 9 },
                },
                snippet: None,
            },
            McpReferenceLocation {
                path: Some("src/lib.rs".to_string()),
                position: Position { line: 2, character: 10 },
                symbol_range: Range {
                    start: Position { line: 2, character: 10 },
                    end: Position { line: 2, character: 14 },
                },
                snippet: None,
            },
            McpReferenceLocation {
                path: Some("src/main.rs".to_string()),
                position: Position { line: 5, character: 3 },
                symbol_range: Range {
                    start: Position { line: 5, character: 3 },
                    end: Position { line: 5, character: 7 },
                },
                snippet: None,
            },
        ];

        let groups = group_references_by_file(&refs);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].path, "src/lib.rs");
        assert_eq!(groups[0].count, 1);
        assert_eq!(groups[0].refs.len(), 1);
        // Verify paths are cleared after grouping
        assert!(groups[0].refs[0].path.is_none());
        assert_eq!(groups[1].path, "src/main.rs");
        assert_eq!(groups[1].count, 2);
        assert_eq!(groups[1].refs.len(), 2);
        // Verify paths are cleared after grouping
        assert!(groups[1].refs[0].path.is_none());
        assert!(groups[1].refs[1].path.is_none());
    }

    #[test]
    fn test_is_import_line() {
        assert!(is_import_line("import os"));
        assert!(is_import_line("  import { useState } from 'react'"));
        assert!(is_import_line("use std::collections::HashMap;"));
        assert!(is_import_line("const fs = require('fs');"));
        assert!(is_import_line("from datetime import datetime"));
        assert!(is_import_line("from \"@/lib/utils\" import { cn }"));

        assert!(!is_import_line("let x = greet('hello')"));
        assert!(!is_import_line("const result = calculate()"));
        assert!(!is_import_line("// import this later"));
    }

    #[test]
    fn test_type_counts_default() {
        let counts = TypeCounts::default();
        assert_eq!(counts.import, 0);
        assert_eq!(counts.call, 0);
    }

    #[test]
    fn test_mcp_references_response_contains_by_file_with_snippets() {
        let snippet = CodeContext {
            range: FileRange {
                path: "src/main.rs".to_string(),
                range: Range {
                    start: Position { line: 10, character: 5 },
                    end: Position { line: 12, character: 10 },
                },
            },
            source_code: "fn example() {}".to_string(),
        };

        let response = McpReferencesResponse {
            raw_response: None,
            selected_identifier: Identifier {
                name: "test".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 5 },
                    },
                },
                kind: Some("function".to_string()),
            },
            limit: 200,
            offset: 0,
            truncated: false,
            total_count: 15,
            by_file: vec![
                crate::service::types::response::FileGroup {
                    path: "src/main.rs".to_string(),
                    count: 10,
                    refs: vec![
                        McpReferenceLocation {
                            path: None, // Path omitted since FileGroup provides it
                            position: Position { line: 10, character: 5 },
                            symbol_range: Range {
                                start: Position { line: 10, character: 5 },
                                end: Position { line: 10, character: 9 },
                            },
                            snippet: Some(snippet.clone()),
                        },
                    ],
                },
                crate::service::types::response::FileGroup {
                    path: "src/lib.rs".to_string(),
                    count: 5,
                    refs: vec![],
                },
            ],
            by_type: TypeCounts {
                import: 3,
                call: 12,
            },
        };

        assert_eq!(response.by_file.len(), 2);
        assert_eq!(response.by_file[0].count, 10);
        assert_eq!(response.by_file[0].refs.len(), 1);
        assert!(response.by_file[0].refs[0].snippet.is_some());

        let ref_snippet = response.by_file[0].refs[0].snippet.as_ref().unwrap();
        assert_eq!(ref_snippet.source_code, "fn example() {}");

        assert_eq!(response.total_count, 15);
        assert_eq!(response.by_type.import, 3);
        assert_eq!(response.by_type.call, 12);

        assert_eq!(
            response.by_file[0].count + response.by_file[1].count,
            15,
            "by_file counts should sum to total_count"
        );
        assert_eq!(
            response.by_type.import + response.by_type.call,
            15,
            "by_type counts should sum to total_count"
        );
    }

    #[test]
    fn test_mcp_definition_response_has_related_field() {
        let response = McpDefinitionResponse {
            raw_response: None,
            definitions: vec![],
            source_code_context: None,
            selected_identifier: Identifier {
                name: "test_fn".to_string(),
                file_range: FileRange {
                    path: "src/lib.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 8 },
                    },
                },
                kind: Some("function".to_string()),
            },
            related: Some(RelatedSymbols::default()),
            limit: 200,
            offset: 0,
            truncated: false,
        };

        assert!(response.related.is_some(), "related field must be present");
        let related = response.related.unwrap();
        assert!(related.sibling_exports.is_empty(), "default sibling_exports must be empty");
    }

    #[test]
    fn test_mcp_definition_response_related_with_siblings() {
        let sibling = Symbol {
            name: "helper_fn".to_string(),
            kind: "function".to_string(),
            identifier_position: FilePosition {
                path: "src/lib.rs".to_string(),
                position: Position { line: 20, character: 4 },
            },
            file_range: FileRange {
                path: "src/lib.rs".to_string(),
                range: Range {
                    start: Position { line: 20, character: 1 },
                    end: Position { line: 25, character: 1 },
                },
            },
            ..Default::default()
        };

        let related = RelatedSymbols {
            sibling_exports: vec![sibling.clone()],
            ..Default::default()
        };

        let response = McpDefinitionResponse {
            raw_response: None,
            definitions: vec![],
            source_code_context: None,
            selected_identifier: Identifier {
                name: "main_fn".to_string(),
                file_range: FileRange {
                    path: "src/lib.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 8 },
                    },
                },
                kind: Some("function".to_string()),
            },
            related: Some(related),
            limit: 200,
            offset: 0,
            truncated: false,
        };

        let related = response.related.expect("related field must be present");
        assert_eq!(related.sibling_exports.len(), 1, "sibling_exports must have one entry");
        assert_eq!(related.sibling_exports[0].name, "helper_fn", "sibling name must match");
    }

    #[test]
    fn test_mcp_definition_response_serialization_skips_empty_related() {
        let response = McpDefinitionResponse {
            raw_response: None,
            definitions: vec![],
            source_code_context: None,
            selected_identifier: Identifier {
                name: "test".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 5 },
                    },
                },
                kind: Some("identifier".to_string()),
            },
            related: None,
            limit: 200,
            offset: 0,
            truncated: false,
        };

        let json = serde_json::to_value(&response).expect("serialization failed");
        assert!(json.get("related").is_none(), "None related must be skipped in serialization");
    }

    #[test]
    fn it_creates_position_error_with_suggestions() {
        let closest = vec![
            Identifier {
                name: "my_function".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 5, character: 1 },
                        end: Position { line: 5, character: 12 },
                    },
                },
                kind: Some("function".to_string()),
            },
        ];

        let error = PositionError::IdentifierNotFound { closest: closest.clone() };
        let suggestions = error.suggestions();

        assert!(!suggestions.is_empty(), "negative: IdentifierNotFound should provide suggestions");
        assert!(
            suggestions.iter().any(|s| s.contains("definitions_in_file")),
            "negative: suggestions should mention definitions_in_file tool"
        );
    }

    #[test]
    fn it_creates_position_error_with_closest_identifiers_in_suggestions() {
        let closest = vec![
            Identifier {
                name: "nearby_fn".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 10, character: 1 },
                        end: Position { line: 10, character: 10 },
                    },
                },
                kind: Some("function".to_string()),
            },
            Identifier {
                name: "another_fn".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 15, character: 1 },
                        end: Position { line: 15, character: 11 },
                    },
                },
                kind: Some("function".to_string()),
            },
        ];

        let error = PositionError::IdentifierNotFound { closest: closest.clone() };
        let suggestions = error.suggestions();

        assert!(
            suggestions.iter().any(|s| s.contains("nearby_fn")),
            "negative: suggestions should include closest identifier names"
        );
    }

    #[test]
    fn it_creates_call_hierarchy_error_with_suggestions() {
        let nearby = vec![
            Symbol {
                name: "some_function".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "test.rs".to_string(),
                    position: Position { line: 10, character: 4 },
                },
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 10, character: 1 },
                        end: Position { line: 15, character: 1 },
                    },
                },
                ..Default::default()
            },
        ];

        let error = CallHierarchyError::NoItemAtPosition { nearby_callables: nearby };
        let suggestions = error.suggestions();

        assert!(!suggestions.is_empty(), "negative: NoItemAtPosition should provide suggestions");
        assert!(
            suggestions.iter().any(|s| s.contains("function") || s.contains("method")),
            "negative: suggestions should mention function/method positioning"
        );
    }

    #[test]
    fn it_includes_nearby_callables_in_call_hierarchy_error_suggestions() {
        let nearby = vec![
            Symbol {
                name: "callable_fn".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "test.rs".to_string(),
                    position: Position { line: 5, character: 4 },
                },
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 5, character: 1 },
                        end: Position { line: 10, character: 1 },
                    },
                },
                ..Default::default()
            },
        ];

        let error = CallHierarchyError::NoItemAtPosition { nearby_callables: nearby.clone() };
        let suggestions = error.suggestions();

        assert!(
            suggestions.iter().any(|s| s.contains("callable_fn")),
            "negative: suggestions should include nearby callable names"
        );
    }

    #[test]
    fn it_formats_service_error_with_suggestions() {
        let closest = vec![
            Identifier {
                name: "test_id".to_string(),
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 1, character: 1 },
                        end: Position { line: 1, character: 8 },
                    },
                },
                kind: Some("identifier".to_string()),
            },
        ];

        let error = ServiceError::IdentifierSelection(
            PositionError::IdentifierNotFound { closest }
        );

        let suggestions = error.suggestions();
        assert!(
            !suggestions.is_empty(),
            "negative: ServiceError should expose suggestions from inner error"
        );
    }

    #[test]
    fn it_formats_call_hierarchy_service_error_with_suggestions() {
        let nearby = vec![
            Symbol {
                name: "method_name".to_string(),
                kind: "method".to_string(),
                identifier_position: FilePosition {
                    path: "test.rs".to_string(),
                    position: Position { line: 20, character: 8 },
                },
                file_range: FileRange {
                    path: "test.rs".to_string(),
                    range: Range {
                        start: Position { line: 20, character: 1 },
                        end: Position { line: 25, character: 1 },
                    },
                },
                ..Default::default()
            },
        ];

        let error = ServiceError::CallHierarchy(
            CallHierarchyError::NoItemAtPosition { nearby_callables: nearby }
        );

        let suggestions = error.suggestions();
        assert!(
            !suggestions.is_empty(),
            "negative: ServiceError should expose suggestions from CallHierarchyError"
        );
    }

    #[test]
    fn test_mcp_definition_location_includes_signature() {
        let def_location = McpDefinitionLocation {
            path: "src/service.ts".to_string(),
            position: Position { line: 82, character: 5 },
            definition_range: Range {
                start: Position { line: 82, character: 1 },
                end: Position { line: 90, character: 1 },
            },
            symbol_kind: Some("function".to_string()),
            snippet: None,
            signature: Some("(args: {classId: number}) => UseQueryResult<ClassDetails>".to_string()),
            doc: Some("Query hook for fetching class details by ID".to_string()),
            external: None,
            package: None,
            reference_count: None,
        };

        assert!(def_location.signature.is_some(), "definition location must include signature");
        assert!(def_location.doc.is_some(), "definition location must include doc");
    }

    #[test]
    fn test_mcp_definition_location_serializes_signature_and_doc() {
        let def_location = McpDefinitionLocation {
            path: "src/api.ts".to_string(),
            position: Position { line: 10, character: 5 },
            definition_range: Range {
                start: Position { line: 10, character: 1 },
                end: Position { line: 15, character: 1 },
            },
            symbol_kind: Some("function".to_string()),
            snippet: None,
            signature: Some("fn example(x: i32) -> String".to_string()),
            doc: Some("Example function documentation".to_string()),
            external: None,
            package: None,
            reference_count: None,
        };

        let json = serde_json::to_value(&def_location).expect("serialization failed");

        assert!(json.get("signature").is_some(), "signature must be present in serialization");
        assert!(json.get("doc").is_some(), "doc must be present in serialization");
        assert_eq!(json["signature"], "fn example(x: i32) -> String", "signature content must match");
    }

    #[test]
    fn test_mcp_definition_location_skips_none_signature_and_doc() {
        let def_location = McpDefinitionLocation {
            path: "src/api.ts".to_string(),
            position: Position { line: 10, character: 5 },
            definition_range: Range {
                start: Position { line: 10, character: 1 },
                end: Position { line: 15, character: 1 },
            },
            symbol_kind: Some("function".to_string()),
            snippet: None,
            signature: None,
            doc: None,
            external: None,
            package: None,
            reference_count: None,
        };

        let json = serde_json::to_value(&def_location).expect("serialization failed");

        assert!(json.get("signature").is_none(), "None signature must be skipped in serialization");
        assert!(json.get("doc").is_none(), "None doc must be skipped in serialization");
    }

    #[test]
    fn test_mcp_definition_location_includes_external_fields() {
        let def_location = McpDefinitionLocation {
            path: "node_modules/.pnpm/@reduxjs+toolkit@2.0.0/node_modules/@reduxjs/toolkit/dist/index.d.ts".to_string(),
            position: Position { line: 100, character: 5 },
            definition_range: Range {
                start: Position { line: 100, character: 1 },
                end: Position { line: 110, character: 1 },
            },
            symbol_kind: Some("function".to_string()),
            snippet: None,
            signature: Some("fn configureStore<S>() -> Store<S>".to_string()),
            doc: None,
            external: Some(true),
            package: Some(PackageInfo {
                name: "@reduxjs/toolkit".to_string(),
                version: "2.0.0".to_string(),
            }),
            reference_count: Some(42),
        };

        let json = serde_json::to_value(&def_location).expect("serialization failed");

        assert_eq!(json["external"], true, "external must be true");
        assert_eq!(json["package"]["name"], "@reduxjs/toolkit", "package name must match");
        assert_eq!(json["package"]["version"], "2.0.0", "package version must match");
        assert_eq!(json["reference_count"], 42, "reference_count must match");
    }

    #[test]
    fn test_external_info_creation_for_node_modules_path() {
        let path = "node_modules/.pnpm/@reduxjs+toolkit@2.0.0/node_modules/@reduxjs/toolkit/dist/query/react/buildHooks.d.ts";
        let external_info = ExternalInfo::from_path(path);

        assert!(external_info.is_some(), "external info must be detected for node_modules path");

        let info = external_info.unwrap();
        assert!(info.external, "external flag must be true");
        assert!(info.package.is_some(), "package info must be present");

        let pkg = info.package.unwrap();
        assert_eq!(pkg.name, "@reduxjs/toolkit", "package name must be parsed");
        assert_eq!(pkg.version, "2.0.0", "package version must be parsed");
    }

    #[test]
    fn test_external_info_none_for_workspace_path() {
        let path = "src/components/Button.tsx";
        let external_info = ExternalInfo::from_path(path);

        assert!(external_info.is_none(), "external info must be None for workspace paths");
    }

    #[test]
    fn test_external_info_serialization() {
        let info = ExternalInfo {
            external: true,
            package: Some(PackageInfo {
                name: "react".to_string(),
                version: "18.2.0".to_string(),
            }),
        };

        let json = serde_json::to_value(&info).expect("serialization failed");

        assert_eq!(json["external"], true, "external flag must serialize");
        assert!(json.get("package").is_some(), "package must be present");
        assert_eq!(json["package"]["name"], "react", "package name must match");
        assert_eq!(json["package"]["version"], "18.2.0", "package version must match");
    }

    #[test]
    fn test_compact_definition_response_format() {
        let compact = CompactDefinitionResponse {
            name: "useGetClassDetailsQuery".to_string(),
            sig: "(args: {classId: number}) => UseQueryResult".to_string(),
            loc: "src/app/service/classManagementService.ts:82".to_string(),
            ext: false,
        };

        let json = serde_json::to_string(&compact).expect("serialization failed");

        assert!(json.len() < 250, "compact format must be under 250 chars, got {} chars", json.len());

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse failed");
        assert!(parsed.get("name").is_some(), "name must be present");
        assert!(parsed.get("sig").is_some(), "sig must be present");
        assert!(parsed.get("loc").is_some(), "loc must be present");
        assert!(parsed.get("ext").is_some(), "ext must be present");
    }

    #[test]
    fn test_compact_definition_response_abbreviations() {
        let compact = CompactDefinitionResponse {
            name: "myFunction".to_string(),
            sig: "(x: number) => string".to_string(),
            loc: "src/lib.ts:42".to_string(),
            ext: true,
        };

        let json = serde_json::to_value(&compact).expect("serialization failed");

        assert!(json.get("sig").is_some(), "must use 'sig' not 'signature'");
        assert!(json.get("loc").is_some(), "must use 'loc' not 'location'");
        assert!(json.get("ext").is_some(), "must use 'ext' not 'external'");
    }

    #[test]
    fn test_find_definition_params_include_siblings_default_false() {
        let params = FindDefinitionParams::default();
        assert!(!params.include_siblings, "include_siblings must default to false");
    }

    #[test]
    fn test_find_definition_params_include_compact_default_false() {
        let params = FindDefinitionParams::default();
        assert!(!params.compact, "compact mode must default to false");
    }

    #[test]
    fn test_find_definition_params_siblings_limit_default() {
        let params = FindDefinitionParams::default();
        assert_eq!(params.siblings_limit.unwrap_or(5), 5, "siblings_limit must default to 5");
    }

    #[test]
    fn test_is_internal_builder_symbol() {
        assert!(is_internal_builder_symbol("_baseEndpointQuery"), "underscore prefix indicates internal");
        assert!(is_internal_builder_symbol("providesTags"), "RTK builder function");
        assert!(is_internal_builder_symbol("invalidatesTags"), "RTK builder function");
        assert!(is_internal_builder_symbol("query"), "generic builder method");
        assert!(is_internal_builder_symbol("mutation"), "generic builder method");
        assert!(is_internal_builder_symbol("endpoints"), "RTK builder config");

        assert!(!is_internal_builder_symbol("useGetUserQuery"), "user hook export");
        assert!(!is_internal_builder_symbol("UserService"), "user service export");
        assert!(!is_internal_builder_symbol("getUserById"), "user function export");
    }

    #[test]
    fn test_filter_sibling_exports() {
        let siblings = vec![
            Symbol {
                name: "useGetUserQuery".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "src/api.ts".to_string(),
                    position: Position { line: 10, character: 5 },
                },
                file_range: FileRange {
                    path: "src/api.ts".to_string(),
                    range: Range {
                        start: Position { line: 10, character: 1 },
                        end: Position { line: 15, character: 1 },
                    },
                },
                ..Default::default()
            },
            Symbol {
                name: "providesTags".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "src/api.ts".to_string(),
                    position: Position { line: 20, character: 5 },
                },
                file_range: FileRange {
                    path: "src/api.ts".to_string(),
                    range: Range {
                        start: Position { line: 20, character: 1 },
                        end: Position { line: 25, character: 1 },
                    },
                },
                ..Default::default()
            },
            Symbol {
                name: "_internalHelper".to_string(),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "src/api.ts".to_string(),
                    position: Position { line: 30, character: 5 },
                },
                file_range: FileRange {
                    path: "src/api.ts".to_string(),
                    range: Range {
                        start: Position { line: 30, character: 1 },
                        end: Position { line: 35, character: 1 },
                    },
                },
                ..Default::default()
            },
        ];

        let filtered = filter_sibling_exports(siblings, 10);

        assert_eq!(filtered.len(), 1, "must filter out internal builder symbols");
        assert_eq!(filtered[0].name, "useGetUserQuery", "must keep user exports");
    }

    #[test]
    fn test_filter_sibling_exports_respects_limit() {
        let siblings: Vec<Symbol> = (0..10)
            .map(|i| Symbol {
                name: format!("userExport{}", i),
                kind: "function".to_string(),
                identifier_position: FilePosition {
                    path: "src/api.ts".to_string(),
                    position: Position { line: i * 10, character: 5 },
                },
                file_range: FileRange {
                    path: "src/api.ts".to_string(),
                    range: Range {
                        start: Position { line: i * 10, character: 1 },
                        end: Position { line: i * 10 + 5, character: 1 },
                    },
                },
                ..Default::default()
            })
            .collect();

        let filtered = filter_sibling_exports(siblings, 5);

        assert_eq!(filtered.len(), 5, "must respect siblings limit");
    }

    #[test]
    fn test_truncate_signature_short_string_unchanged() {
        let sig = "fn foo(x: i32) -> bool";
        let result = truncate_signature(sig, Some(50));
        assert_eq!(result, sig);
    }

    #[test]
    fn test_truncate_signature_truncates_at_generic_opener() {
        let sig = "fn configure_store(options: StoreOptions<State, Middleware, Enhancers>) -> EnhancedStore<State>";
        let result = truncate_signature(sig, Some(50));
        assert_eq!(result, "fn configure_store(options: StoreOptions...");
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_signature_normalizes_whitespace() {
        let sig = "const store: EnhancedStore<{\n    member: CombinedState<{\n        foo: Bar\n    }>\n}>";
        let result = truncate_signature(sig, Some(40));
        assert_eq!(result, "const store: EnhancedStore...");
        assert!(!result.contains('\n'));
    }

    #[test]
    fn test_truncate_signature_default_length() {
        let sig = "a".repeat(250);
        let result = truncate_signature(&sig, None);
        assert!(result.len() <= DEFAULT_MAX_SIGNATURE_LENGTH);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_signature_unicode_safe() {
        let sig = "fn 测试函数<T>(参数: 类型) -> 返回值";
        let result = truncate_signature(sig, Some(20));
        assert_eq!(result, "fn 测试函数...");
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_signature_simple_generic_preserved() {
        let sig = "Vec<String>";
        let result = truncate_signature(sig, Some(100));
        assert_eq!(result, "Vec<String>");
    }

    #[test]
    fn test_truncate_signature_fallback_byte_truncation() {
        let sig = "fn very_long_function_name_without_generics(arg1: Type1, arg2: Type2, arg3: Type3)";
        let result = truncate_signature(sig, Some(50));
        assert!(result.len() <= 50);
        assert!(result.ends_with("..."));
    }

    #[allow(deprecated)]
    #[test]
    fn workspace_symbol_info_from_lsp_initializes_signature_as_none() {
        let uri = Url::from_file_path("/tmp/test.rs").expect("Expected file path url");
        let range = LspRange {
            start: LspPosition {
                line: 5,
                character: 2,
            },
            end: LspPosition {
                line: 5,
                character: 10,
            },
        };
        let sym = SymbolInformation {
            name: "test_func".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: Location { uri, range },
            container_name: None,
        };

        let info = workspace_symbol_info_from_lsp(sym, "src/lib.rs".to_string());

        assert!(
            info.signature.is_none(),
            "signature field must be initialized as None in workspace_symbol_info_from_lsp"
        );
    }

    #[tokio::test]
    async fn batch_hover_for_signatures_returns_vec_of_same_length_as_input() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_path = temp_dir.path().to_str().expect("Expected valid path");

        let manager = Manager::new(workspace_path)
            .await
            .expect("Failed to create manager");

        let positions = vec![
            ("test_file.rs", Position { line: 1, character: 1 }),
            ("another.rs", Position { line: 2, character: 3 }),
        ];

        let results = batch_hover_for_signatures(&manager, positions.clone()).await;

        assert_eq!(
            results.len(),
            positions.len(),
            "batch_hover_for_signatures must return same number of results as input positions"
        );
    }

    #[test]
    fn workspace_symbol_assigns_signatures_to_filtered_symbols() {
        let mut symbol1 = WorkspaceSymbolInfo {
            name: "func1".to_string(),
            kind: "function".to_string(),
            location: FilePosition {
                path: "src/lib.rs".to_string(),
                position: Position { line: 10, character: 5 },
            },
            container_name: None,
            match_kind: Some("exact".to_string()),
            match_score: Some(1.0),
            signature: None,
        };

        let signatures = vec![Some("fn func1() -> i32".to_string())];

        for (symbol, sig) in std::iter::once(&mut symbol1).zip(signatures.into_iter()) {
            symbol.signature = sig;
        }

        assert_eq!(
            symbol1.signature,
            Some("fn func1() -> i32".to_string()),
            "signature must be assigned to symbol from batch hover results"
        );
    }
}
