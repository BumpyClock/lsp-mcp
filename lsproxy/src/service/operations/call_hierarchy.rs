// ABOUTME: Call hierarchy operations (prepare, incoming, outgoing calls).
// ABOUTME: Handles finding callers and callees of functions/methods.

use crate::api_types::{
    CallHierarchyDirection, CallHierarchyItemInfo, CallHierarchyResponse, CallInfo,
    IncomingCallInfo, IncomingCallsResponse, OutgoingCallInfo, OutgoingCallsResponse, Position,
    PrepareCallHierarchyResponse, Range,
};
use crate::lsp::manager::Manager;
use log::debug;
use lsp_types::Position as LspPosition;
use std::collections::HashMap;
use std::sync::Arc;

use crate::service::types::errors::ServiceError;
use crate::service::utils::transformations::call_hierarchy_item_to_info;
use crate::service::utils::external::ExternalInfo;
use lsp_types::Range as LspRange;
use std::collections::HashSet;

/// Fetches 1-line source code snippets for each call site.
///
/// For incoming calls: `call_ranges` are in the caller's file (`call.item.location.path`)
/// For outgoing calls: `call_ranges` are in the original file being analyzed (`source_file_path`)
async fn fetch_call_snippets(
    manager: &Arc<Manager>,
    mut calls: Vec<CallInfo>,
    direction: CallHierarchyDirection,
    source_file_path: &str,
) -> Vec<CallInfo> {
    for call in &mut calls {
        // For incoming calls, read from the caller's file (call.item.location.path)
        // For outgoing calls, read from the original source file being analyzed
        let file_path = match direction {
            CallHierarchyDirection::Incoming => &call.item.location.path,
            CallHierarchyDirection::Outgoing => source_file_path,
        };

        let mut snippets = Vec::with_capacity(call.call_ranges.len());
        for range in &call.call_ranges {
            let lsp_range = LspRange {
                start: LspPosition {
                    line: range.start.line.saturating_sub(1),
                    character: 0,
                },
                end: LspPosition {
                    line: range.start.line,
                    character: 0,
                },
            };
            match manager.read_source_code(file_path, Some(lsp_range)).await {
                Ok(code) => snippets.push(code.trim().to_string()),
                Err(_) => snippets.push(String::new()),
            }
        }
        call.call_snippets = Some(snippets);
    }
    calls
}

/// Prepares the call hierarchy at the given position.
pub(crate) async fn prepare_call_hierarchy_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    include_raw_response: bool,
) -> Result<PrepareCallHierarchyResponse, ServiceError> {
    let items = manager
        .prepare_call_hierarchy(
            file_path,
            LspPosition {
                line: position.line.saturating_sub(1),
                character: position.character.saturating_sub(1),
            },
        )
        .await?;

    let converted_items: Vec<CallHierarchyItemInfo> = items
        .unwrap_or_default()
        .iter()
        .map(call_hierarchy_item_to_info)
        .collect();

    let raw_response = if include_raw_response {
        serde_json::to_value(&converted_items).ok()
    } else {
        None
    };

    Ok(PrepareCallHierarchyResponse {
        raw_response,
        items: converted_items,
    })
}

/// Gets incoming calls (callers) for the function at the given position.
pub(crate) async fn incoming_calls_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    include_raw_response: bool,
) -> Result<IncomingCallsResponse, ServiceError> {
    // First prepare the call hierarchy to get the item
    let items = manager
        .prepare_call_hierarchy(
            file_path,
            LspPosition {
                line: position.line.saturating_sub(1),
                character: position.character.saturating_sub(1),
            },
        )
        .await?;

    let item = match items.and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) }) {
        Some(item) => item,
        None => {
            debug!("incoming_calls_impl: prepare_call_hierarchy returned no items");
            return Ok(IncomingCallsResponse {
                raw_response: None,
                calls: vec![],
            })
        }
    };

    let calls = manager.incoming_calls(file_path, &item).await?;
    debug!("incoming_calls_impl: got {} raw calls from LSP", calls.len());

    let workspace_files = manager.list_files().await?;
    let workspace_len = workspace_files.len();
    let workspace_set: HashSet<String> = workspace_files.into_iter().collect();
    debug!("incoming_calls_impl: workspace has {} files", workspace_len);

    let converted_calls: Vec<IncomingCallInfo> = calls
        .into_iter()
        .map(|call| {
            let mut from = call_hierarchy_item_to_info(&call.from);
            if is_external_call(&from.location.path, &workspace_set) {
                from.external = Some(true);
            }
            IncomingCallInfo {
                from,
            from_ranges: call
                .from_ranges
                .into_iter()
                .map(|r| Range {
                    start: Position {
                        line: r.start.line + 1,
                        character: r.start.character + 1,
                    },
                    end: Position {
                        line: r.end.line + 1,
                        character: r.end.character + 1,
                    },
                })
                .collect(),
            }
        })
        .collect();

    let raw_response = if include_raw_response {
        serde_json::to_value(&converted_calls).ok()
    } else {
        None
    };

    Ok(IncomingCallsResponse {
        raw_response,
        calls: converted_calls,
    })
}

/// Gets outgoing calls (callees) for the function at the given position.
pub(crate) async fn outgoing_calls_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    include_raw_response: bool,
) -> Result<OutgoingCallsResponse, ServiceError> {
    // First prepare the call hierarchy to get the item
    let items = manager
        .prepare_call_hierarchy(
            file_path,
            LspPosition {
                line: position.line.saturating_sub(1),
                character: position.character.saturating_sub(1),
            },
        )
        .await?;

    let item = match items.and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) }) {
        Some(item) => item,
        None => {
            debug!("outgoing_calls_impl: prepare_call_hierarchy returned no items");
            return Ok(OutgoingCallsResponse {
                raw_response: None,
                calls: vec![],
            })
        }
    };

    let calls = manager.outgoing_calls(file_path, &item).await?;
    debug!("outgoing_calls_impl: got {} raw calls from LSP", calls.len());

    let workspace_files = manager.list_files().await?;
    let workspace_len = workspace_files.len();
    let workspace_set: HashSet<String> = workspace_files.into_iter().collect();
    debug!("outgoing_calls_impl: workspace has {} files", workspace_len);

    let converted_calls: Vec<OutgoingCallInfo> = calls
        .into_iter()
        .map(|call| {
            let mut to = call_hierarchy_item_to_info(&call.to);
            if is_external_call(&to.location.path, &workspace_set) {
                to.external = Some(true);
            }
            OutgoingCallInfo {
                to,
            from_ranges: call
                .from_ranges
                .into_iter()
                .map(|r| Range {
                    start: Position {
                        line: r.start.line + 1,
                        character: r.start.character + 1,
                    },
                    end: Position {
                        line: r.end.line + 1,
                        character: r.end.character + 1,
                    },
                })
                .collect(),
            }
        })
        .collect();

    let raw_response = if include_raw_response {
        serde_json::to_value(&converted_calls).ok()
    } else {
        None
    };

    Ok(OutgoingCallsResponse {
        raw_response,
        calls: converted_calls,
    })
}

/// Unified method for call hierarchy traversal in either direction.
///
/// This method handles both incoming (callers) and outgoing (callees) call hierarchy
/// requests based on the `direction` parameter.
///
/// When `internal_only` is true (default), external dependencies are filtered out.
pub(crate) async fn call_hierarchy_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    direction: CallHierarchyDirection,
    internal_only: bool,
) -> Result<CallHierarchyResponse, ServiceError> {
    debug!(
        "call_hierarchy_impl: file={}, position=({},{}), direction={:?}",
        file_path, position.line, position.character, direction
    );

    // First prepare the call hierarchy to get the item
    let items = manager
        .prepare_call_hierarchy(
            file_path,
            LspPosition {
                line: position.line.saturating_sub(1),
                character: position.character.saturating_sub(1),
            },
        )
        .await?;

    let item = match items.and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) }) {
        Some(item) => item,
        None => {
            debug!("call_hierarchy_impl: prepare_call_hierarchy returned no items");
            return Ok(CallHierarchyResponse {
                direction,
                raw_response: None,
                calls: vec![],
            })
        }
    };

    debug!("call_hierarchy_impl: got item name={}", item.name);

    let workspace_files = manager.list_files().await?;
    let workspace_len = workspace_files.len();
    let workspace_set: HashSet<String> = workspace_files.into_iter().collect();
    debug!("call_hierarchy_impl: workspace has {} files", workspace_len);

    let calls: Vec<CallInfo> = match direction {
        CallHierarchyDirection::Incoming => {
            let lsp_calls = manager.incoming_calls(file_path, &item).await?;
            lsp_calls
                .into_iter()
                .map(|call| {
                    let mut item = call_hierarchy_item_to_info(&call.from);
                    if is_external_call(&item.location.path, &workspace_set) {
                        item.external = Some(true);
                    }
                    CallInfo {
                        item: item.clone(),
                        call_ranges: call
                            .from_ranges
                            .into_iter()
                            .map(|r| Range {
                                start: Position {
                                    line: r.start.line + 1,
                                    character: r.start.character + 1,
                                },
                                end: Position {
                                    line: r.end.line + 1,
                                    character: r.end.character + 1,
                                },
                            })
                            .collect(),
                        call_snippets: None,
                    }
                })
                .collect()
        }
        CallHierarchyDirection::Outgoing => {
            let lsp_calls = manager.outgoing_calls(file_path, &item).await?;
            lsp_calls
                .into_iter()
                .map(|call| {
                    let mut item = call_hierarchy_item_to_info(&call.to);
                    if is_external_call(&item.location.path, &workspace_set) {
                        item.external = Some(true);
                    }
                    CallInfo {
                        item: item.clone(),
                        call_ranges: call
                            .from_ranges
                            .into_iter()
                            .map(|r| Range {
                                start: Position {
                                    line: r.start.line + 1,
                                    character: r.start.character + 1,
                                },
                                end: Position {
                                    line: r.end.line + 1,
                                    character: r.end.character + 1,
                                },
                            })
                            .collect(),
                        call_snippets: None,
                    }
                })
                .collect()
        }
    };

    debug!(
        "call_hierarchy_impl: converted {} calls before filtering",
        calls.len()
    );

    let calls = if internal_only {
        filter_internal_calls(calls)
    } else {
        calls
    };
    debug!(
        "call_hierarchy_impl: {} calls after internal_only filter (internal_only={})",
        calls.len(),
        internal_only
    );

    let calls = dedupe_calls(calls);
    debug!(
        "call_hierarchy_impl: {} calls after deduplication",
        calls.len()
    );

    for (i, call) in calls.iter().enumerate() {
        debug!(
            "  call[{}]: name={}, path={}, external={:?}",
            i, call.item.name, call.item.location.path, call.item.external
        );
    }

    let calls = fetch_call_snippets(manager, calls, direction, file_path).await;

    debug!(
        "call_hierarchy_impl: returning {} calls after fetch_snippets",
        calls.len()
    );

    Ok(CallHierarchyResponse {
        direction,
        raw_response: None, // MCP layer handles verbose mode
        calls,
    })
}

/// Deduplicates call hierarchy entries by (file_path, line_number).
/// When duplicates are found, merges call_ranges and call_snippets.
pub(crate) fn dedupe_calls(calls: Vec<CallInfo>) -> Vec<CallInfo> {
    let mut seen: HashMap<(String, u32), CallInfo> = HashMap::new();
    for call in calls {
        let key = (
            call.item.location.path.clone(),
            call.item.location.position.line,
        );
        seen.entry(key)
            .and_modify(|existing| {
                let mut merged_ranges = existing.call_ranges.clone();
                for r in &call.call_ranges {
                    let dup = merged_ranges.iter().any(|er| {
                        er.start.line == r.start.line && er.start.character == r.start.character
                    });
                    if !dup {
                        merged_ranges.push(r.clone());
                    }
                }
                existing.call_ranges = merged_ranges;

                if let (Some(existing_snippets), Some(new_snippets)) =
                    (&mut existing.call_snippets, &call.call_snippets)
                {
                    for snippet in new_snippets {
                        if !existing_snippets.contains(snippet) {
                            existing_snippets.push(snippet.clone());
                        }
                    }
                }
            })
            .or_insert(call);
    }
    seen.into_values().collect()
}

/// Filters calls to include only internal (non-external) entries.
pub(crate) fn filter_internal_calls(calls: Vec<CallInfo>) -> Vec<CallInfo> {
    calls
        .into_iter()
        .filter(|call| call.item.external != Some(true))
        .collect()
}

/// Normalizes a path for consistent comparison.
/// Strips leading `./` and trailing `/` to ensure paths from different sources match.
fn normalize_path_for_comparison(path: &str) -> String {
    path.trim_start_matches("./")
        .trim_end_matches('/')
        .to_string()
}

fn is_external_call(path: &str, workspace_files: &HashSet<String>) -> bool {
    if ExternalInfo::from_path(path).is_some() {
        return true;
    }
    if workspace_files.is_empty() {
        return false;
    }
    // Normalize the incoming path for comparison
    let normalized_path = normalize_path_for_comparison(path);
    // Check if any workspace file matches after normalization
    !workspace_files.iter().any(|f| normalize_path_for_comparison(f) == normalized_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::FilePosition;
    use rand::{distr::Alphanumeric, Rng};

    fn make_call_info(path: &str, line: u32, name: &str, external: Option<bool>) -> CallInfo {
        CallInfo {
            item: CallHierarchyItemInfo {
                name: name.to_string(),
                kind: "function".to_string(),
                location: FilePosition {
                    path: path.to_string(),
                    position: Position { line, character: 1 },
                },
                range: Range {
                    start: Position { line, character: 1 },
                    end: Position {
                        line: line + 10,
                        character: 1,
                    },
                },
                detail: None,
                external,
            },
            call_ranges: vec![Range {
                start: Position { line: line + 2, character: 5 },
                end: Position { line: line + 2, character: 20 },
            }],
            call_snippets: None,
        }
    }

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

    #[test]
    fn dedupe_calls_removes_duplicate_entries_by_file_and_line() {
        let call1 = make_call_info("src/lib.rs", 10, "foo", None);
        let call2 = make_call_info("src/lib.rs", 10, "foo_overload", None);
        let call3 = make_call_info("src/lib.rs", 20, "bar", None);

        let calls = vec![call1, call2, call3];
        let result = dedupe_calls(calls);

        assert_eq!(result.len(), 2, "duplicate entries at same file:line must be merged");
    }

    #[test]
    fn dedupe_calls_merges_call_ranges_from_duplicates() {
        let mut call1 = make_call_info("src/lib.rs", 10, "foo", None);
        call1.call_ranges = vec![Range {
            start: Position { line: 12, character: 5 },
            end: Position { line: 12, character: 10 },
        }];

        let mut call2 = make_call_info("src/lib.rs", 10, "foo", None);
        call2.call_ranges = vec![Range {
            start: Position { line: 15, character: 5 },
            end: Position { line: 15, character: 10 },
        }];

        let calls = vec![call1, call2];
        let result = dedupe_calls(calls);

        assert_eq!(result.len(), 1, "entries must be merged");
        assert_eq!(
            result[0].call_ranges.len(),
            2,
            "call_ranges from both entries must be merged"
        );
    }

    #[test]
    fn dedupe_calls_avoids_duplicate_ranges_within_merged_entry() {
        let mut call1 = make_call_info("src/lib.rs", 10, "foo", None);
        call1.call_ranges = vec![Range {
            start: Position { line: 12, character: 5 },
            end: Position { line: 12, character: 10 },
        }];

        let mut call2 = make_call_info("src/lib.rs", 10, "foo", None);
        call2.call_ranges = vec![Range {
            start: Position { line: 12, character: 5 },
            end: Position { line: 12, character: 10 },
        }];

        let calls = vec![call1, call2];
        let result = dedupe_calls(calls);

        assert_eq!(result.len(), 1, "entries must be merged");
        assert_eq!(
            result[0].call_ranges.len(),
            1,
            "identical ranges must not be duplicated"
        );
    }

    #[test]
    fn dedupe_calls_preserves_distinct_entries() {
        let call1 = make_call_info("src/foo.rs", 10, "foo", None);
        let call2 = make_call_info("src/bar.rs", 10, "bar", None);
        let call3 = make_call_info("src/foo.rs", 20, "baz", None);

        let calls = vec![call1, call2, call3];
        let result = dedupe_calls(calls);

        assert_eq!(result.len(), 3, "distinct entries must be preserved");
    }

    #[test]
    fn filter_internal_calls_removes_external_entries() {
        let internal1 = make_call_info("src/lib.rs", 10, "internal_fn", None);
        let internal2 = make_call_info("src/lib.rs", 20, "internal_fn2", Some(false));
        let external = make_call_info("node_modules/lib.d.ts", 100, "trim", Some(true));

        let calls = vec![internal1, internal2, external];
        let result = filter_internal_calls(calls);

        assert_eq!(result.len(), 2, "external entries must be filtered out");
        for call in &result {
            assert_ne!(
                call.item.external,
                Some(true),
                "no external entries must remain"
            );
        }
    }

    #[test]
    fn filter_internal_calls_keeps_all_when_no_external() {
        let call1 = make_call_info("src/lib.rs", 10, "foo", None);
        let call2 = make_call_info("src/lib.rs", 20, "bar", Some(false));

        let calls = vec![call1, call2];
        let result = filter_internal_calls(calls);

        assert_eq!(result.len(), 2, "all internal entries must be kept");
    }

    #[test]
    fn filter_internal_calls_returns_empty_when_all_external() {
        let call1 = make_call_info("node_modules/a.d.ts", 10, "trim", Some(true));
        let call2 = make_call_info("node_modules/b.d.ts", 20, "toLowerCase", Some(true));

        let calls = vec![call1, call2];
        let result = filter_internal_calls(calls);

        assert_eq!(result.len(), 0, "all external entries must be filtered");
    }

    #[test]
    fn is_external_call_detects_node_modules_even_when_listed() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let path = format!(
            "node_modules/{}/index.d.ts",
            format!("pkg_{}{}", unicode, random_irregular_string())
        );
        let workspace_files = vec![path.clone()].into_iter().collect();

        let result = is_external_call(&path, &workspace_files);

        assert!(result, "negative: node_modules paths must be external");
    }

    #[test]
    fn is_external_call_treats_workspace_paths_as_internal() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let path = format!(
            "src/{}_file.rs",
            format!("name_{}{}", unicode, random_irregular_string())
        );
        let workspace_files = vec![path.clone()].into_iter().collect();

        let result = is_external_call(&path, &workspace_files);

        assert!(!result, "negative: workspace paths must be internal");
    }

    #[test]
    fn is_external_call_falls_back_to_internal_when_list_is_empty() {
        let unicode = char::from_u32(241).expect("negative: unicode should be valid");
        let path = format!(
            "src/{}_file.rs",
            format!("name_{}{}", unicode, random_irregular_string())
        );
        let workspace_files = std::collections::HashSet::new();

        let result = is_external_call(&path, &workspace_files);

        assert!(
            !result,
            "negative: empty workspace list must not mark internal files external"
        );
    }

    #[test]
    fn normalize_path_for_comparison_strips_leading_dot_slash() {
        assert_eq!(normalize_path_for_comparison("./src/main.rs"), "src/main.rs");
        assert_eq!(normalize_path_for_comparison("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn normalize_path_for_comparison_strips_trailing_slash() {
        assert_eq!(normalize_path_for_comparison("src/dir/"), "src/dir");
        assert_eq!(normalize_path_for_comparison("src/dir"), "src/dir");
    }

    #[test]
    fn is_external_call_matches_with_leading_dot_slash_mismatch() {
        let workspace_files: HashSet<String> = vec!["src/lib.rs".to_string()].into_iter().collect();

        // Path with leading ./ should match workspace file without it
        let result = is_external_call("./src/lib.rs", &workspace_files);
        assert!(!result, "path with leading ./ must match workspace file without it");
    }

    #[test]
    fn is_external_call_matches_with_trailing_slash_mismatch() {
        let workspace_files: HashSet<String> = vec!["src/lib.rs".to_string()].into_iter().collect();

        // This is edge case - files shouldn't have trailing slash but testing normalization
        let result = is_external_call("src/lib.rs/", &workspace_files);
        assert!(!result, "path normalization must handle trailing slash");
    }
}
