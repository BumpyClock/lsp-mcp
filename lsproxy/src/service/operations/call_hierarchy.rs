// ABOUTME: Call hierarchy operations (prepare, incoming, outgoing calls).
// ABOUTME: Handles finding callers and callees of functions/methods.

use crate::api_types::{
    CallHierarchyDirection, CallHierarchyItemInfo, CallHierarchyResponse, CallInfo,
    IncomingCallInfo, IncomingCallsResponse, OutgoingCallInfo, OutgoingCallsResponse, Position,
    PrepareCallHierarchyResponse, Range,
};
use crate::lsp::manager::Manager;
use crate::utils::file_utils::uri_to_relative_path_string;
use log::debug;
use lsp_types::Position as LspPosition;
use std::sync::Arc;

use crate::service::types::errors::ServiceError;
use crate::service::utils::transformations::call_hierarchy_item_to_info;

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
    debug!("incoming_calls_impl: workspace has {} files", workspace_files.len());

    let converted_calls: Vec<IncomingCallInfo> = calls
        .into_iter()
        .filter(|call| {
            let path = uri_to_relative_path_string(&call.from.uri);
            let in_workspace = workspace_files.contains(&path);
            if !in_workspace {
                debug!("incoming_calls_impl: filtering out call from {} (not in workspace)", path);
            }
            in_workspace
        })
        .map(|call| IncomingCallInfo {
            from: call_hierarchy_item_to_info(&call.from),
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
    debug!("outgoing_calls_impl: workspace has {} files", workspace_files.len());

    let converted_calls: Vec<OutgoingCallInfo> = calls
        .into_iter()
        .filter(|call| {
            let path = uri_to_relative_path_string(&call.to.uri);
            let in_workspace = workspace_files.contains(&path);
            if !in_workspace {
                debug!("outgoing_calls_impl: filtering out call to {} (not in workspace)", path);
            }
            in_workspace
        })
        .map(|call| OutgoingCallInfo {
            to: call_hierarchy_item_to_info(&call.to),
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
pub(crate) async fn call_hierarchy_impl(
    manager: &Arc<Manager>,
    file_path: &str,
    position: Position,
    direction: CallHierarchyDirection,
) -> Result<CallHierarchyResponse, ServiceError> {
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
            return Ok(CallHierarchyResponse {
                direction,
                raw_response: None,
                calls: vec![],
            })
        }
    };

    let workspace_files = manager.list_files().await?;

    let calls = match direction {
        CallHierarchyDirection::Incoming => {
            let lsp_calls = manager.incoming_calls(file_path, &item).await?;
            lsp_calls
                .into_iter()
                .filter(|call| {
                    let path = uri_to_relative_path_string(&call.from.uri);
                    workspace_files.contains(&path)
                })
                .map(|call| CallInfo {
                    item: call_hierarchy_item_to_info(&call.from),
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
                })
                .collect()
        }
        CallHierarchyDirection::Outgoing => {
            let lsp_calls = manager.outgoing_calls(file_path, &item).await?;
            lsp_calls
                .into_iter()
                .filter(|call| {
                    let path = uri_to_relative_path_string(&call.to.uri);
                    workspace_files.contains(&path)
                })
                .map(|call| CallInfo {
                    item: call_hierarchy_item_to_info(&call.to),
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
                })
                .collect()
        }
    };

    Ok(CallHierarchyResponse {
        direction,
        raw_response: None, // MCP layer handles verbose mode
        calls,
    })
}
