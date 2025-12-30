// ABOUTME: Call hierarchy types for function/method call analysis.
// ABOUTME: Includes direction, call info, and response types for incoming/outgoing calls.

use super::{FilePosition, Range};
use serde::{Deserialize, Serialize};

/// A call hierarchy item representing a function/method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHierarchyItemInfo {
    /// The name of the function/method
    pub name: String,
    /// The kind (function, method, constructor, etc.)
    pub kind: String,
    /// Location of the function/method identifier
    pub location: FilePosition,
    /// The full range of the function/method
    pub range: Range,
    /// Detail information (e.g., signature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Response to prepareCallHierarchy request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareCallHierarchyResponse {
    /// The raw response from the langserver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<serde_json::Value>,
    /// The call hierarchy items at the position
    pub items: Vec<CallHierarchyItemInfo>,
}

/// An incoming call (caller) in the call hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingCallInfo {
    /// The calling function/method
    pub from: CallHierarchyItemInfo,
    /// The ranges where the call occurs within the calling function
    pub from_ranges: Vec<Range>,
}

/// Response to incomingCalls request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingCallsResponse {
    /// The raw response from the langserver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<serde_json::Value>,
    /// The incoming calls (callers)
    pub calls: Vec<IncomingCallInfo>,
}

/// An outgoing call (callee) in the call hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingCallInfo {
    /// The called function/method
    pub to: CallHierarchyItemInfo,
    /// The ranges where the call occurs
    pub from_ranges: Vec<Range>,
}

/// Response to outgoingCalls request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingCallsResponse {
    /// The raw response from the langserver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<serde_json::Value>,
    /// The outgoing calls (callees)
    pub calls: Vec<OutgoingCallInfo>,
}

/// Direction for call hierarchy traversal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallHierarchyDirection {
    /// Find callers of the function (incoming calls)
    Incoming,
    /// Find callees of the function (outgoing calls)
    Outgoing,
}

/// A call in the call hierarchy (either incoming or outgoing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallInfo {
    /// The function/method involved in the call (caller for incoming, callee for outgoing)
    pub item: CallHierarchyItemInfo,
    /// The ranges where the call occurs
    pub call_ranges: Vec<Range>,
}

/// Unified response for call hierarchy requests (both incoming and outgoing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHierarchyResponse {
    /// The direction of the call hierarchy traversal
    pub direction: CallHierarchyDirection,
    /// The raw response from the langserver (always None at service layer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<serde_json::Value>,
    /// The calls found
    pub calls: Vec<CallInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::Position;

    #[test]
    fn test_call_hierarchy_direction_serializes_to_lowercase() {
        let incoming = CallHierarchyDirection::Incoming;
        let outgoing = CallHierarchyDirection::Outgoing;

        let incoming_json = serde_json::to_string(&incoming).expect("failed to serialize incoming");
        let outgoing_json = serde_json::to_string(&outgoing).expect("failed to serialize outgoing");

        assert_eq!(
            incoming_json, "\"incoming\"",
            "incoming direction must serialize to lowercase"
        );
        assert_eq!(
            outgoing_json, "\"outgoing\"",
            "outgoing direction must serialize to lowercase"
        );
    }

    #[test]
    fn test_call_hierarchy_direction_deserializes_from_lowercase() {
        let incoming: CallHierarchyDirection =
            serde_json::from_str("\"incoming\"").expect("failed to deserialize incoming");
        let outgoing: CallHierarchyDirection =
            serde_json::from_str("\"outgoing\"").expect("failed to deserialize outgoing");

        assert_eq!(
            incoming,
            CallHierarchyDirection::Incoming,
            "incoming string must deserialize to Incoming variant"
        );
        assert_eq!(
            outgoing,
            CallHierarchyDirection::Outgoing,
            "outgoing string must deserialize to Outgoing variant"
        );
    }

    #[test]
    fn test_call_hierarchy_direction_equality() {
        let incoming1 = CallHierarchyDirection::Incoming;
        let incoming2 = CallHierarchyDirection::Incoming;
        let outgoing = CallHierarchyDirection::Outgoing;

        assert_eq!(incoming1, incoming2, "same variants must be equal");
        assert_ne!(
            incoming1, outgoing,
            "different variants must not be equal"
        );
    }

    #[test]
    fn test_call_info_serializes_with_item_and_call_ranges() {
        let call_info = CallInfo {
            item: CallHierarchyItemInfo {
                name: "test_function".to_string(),
                kind: "function".to_string(),
                location: FilePosition {
                    path: "src/test.rs".to_string(),
                    position: Position {
                        line: 10,
                        character: 5,
                    },
                },
                range: Range {
                    start: Position {
                        line: 10,
                        character: 1,
                    },
                    end: Position {
                        line: 20,
                        character: 1,
                    },
                },
                detail: Some("fn test_function()".to_string()),
            },
            call_ranges: vec![Range {
                start: Position {
                    line: 15,
                    character: 10,
                },
                end: Position {
                    line: 15,
                    character: 25,
                },
            }],
        };

        let json = serde_json::to_value(&call_info).expect("failed to serialize call info");

        assert!(json.get("item").is_some(), "item field must be present");
        assert!(
            json.get("call_ranges").is_some(),
            "call_ranges field must be present"
        );
        assert_eq!(
            json["item"]["name"], "test_function",
            "item name must match"
        );
    }

    #[test]
    fn test_call_hierarchy_response_serializes_incoming_calls() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Incoming,
            raw_response: None,
            calls: vec![CallInfo {
                item: CallHierarchyItemInfo {
                    name: "caller_fn".to_string(),
                    kind: "function".to_string(),
                    location: FilePosition {
                        path: "src/caller.rs".to_string(),
                        position: Position { line: 5, character: 1 },
                    },
                    range: Range {
                        start: Position { line: 5, character: 1 },
                        end: Position { line: 10, character: 1 },
                    },
                    detail: None,
                },
                call_ranges: vec![Range {
                    start: Position { line: 7, character: 5 },
                    end: Position { line: 7, character: 20 },
                }],
            }],
        };

        let json = serde_json::to_value(&response).expect("failed to serialize response");

        assert_eq!(
            json["direction"], "incoming",
            "direction must be lowercase incoming"
        );
        assert!(
            json.get("raw_response").is_none(),
            "raw_response must be skipped when None"
        );
        assert_eq!(json["calls"].as_array().unwrap().len(), 1, "must have one call");
    }

    #[test]
    fn test_call_hierarchy_response_serializes_outgoing_calls() {
        let response = CallHierarchyResponse {
            direction: CallHierarchyDirection::Outgoing,
            raw_response: Some(serde_json::json!({"test": "data"})),
            calls: vec![],
        };

        let json = serde_json::to_value(&response).expect("failed to serialize response");

        assert_eq!(
            json["direction"], "outgoing",
            "direction must be lowercase outgoing"
        );
        assert!(
            json.get("raw_response").is_some(),
            "raw_response must be present when Some"
        );
        assert_eq!(
            json["raw_response"]["test"], "data",
            "raw_response content must match"
        );
    }

    #[test]
    fn test_call_hierarchy_response_deserializes_correctly() {
        let json_str = r#"{
            "direction": "incoming",
            "calls": [{
                "item": {
                    "name": "test_fn",
                    "kind": "function",
                    "location": {"path": "test.rs", "position": {"line": 1, "character": 1}},
                    "range": {"start": {"line": 1, "character": 1}, "end": {"line": 5, "character": 1}}
                },
                "call_ranges": []
            }]
        }"#;

        let response: CallHierarchyResponse =
            serde_json::from_str(json_str).expect("failed to deserialize response");

        assert_eq!(
            response.direction,
            CallHierarchyDirection::Incoming,
            "direction must be Incoming"
        );
        assert_eq!(response.calls.len(), 1, "must have one call");
        assert_eq!(
            response.calls[0].item.name, "test_fn",
            "item name must match"
        );
    }

    #[test]
    fn test_call_info_with_multiple_call_ranges() {
        let call_info = CallInfo {
            item: CallHierarchyItemInfo {
                name: "multiply_called".to_string(),
                kind: "method".to_string(),
                location: FilePosition {
                    path: "src/lib.rs".to_string(),
                    position: Position { line: 100, character: 10 },
                },
                range: Range {
                    start: Position { line: 100, character: 1 },
                    end: Position { line: 110, character: 1 },
                },
                detail: None,
            },
            call_ranges: vec![
                Range {
                    start: Position { line: 102, character: 5 },
                    end: Position { line: 102, character: 20 },
                },
                Range {
                    start: Position { line: 105, character: 5 },
                    end: Position { line: 105, character: 20 },
                },
                Range {
                    start: Position { line: 108, character: 5 },
                    end: Position { line: 108, character: 20 },
                },
            ],
        };

        let json = serde_json::to_value(&call_info).expect("failed to serialize");
        let call_ranges = json["call_ranges"].as_array().unwrap();

        assert_eq!(
            call_ranges.len(),
            3,
            "must serialize all three call ranges"
        );
    }
}
