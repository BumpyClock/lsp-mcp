// ABOUTME: Error types for the service layer operations.
// ABOUTME: Defines ServiceError, PositionError, and CallHierarchyError with suggestions.

use crate::api_types::{Identifier, Symbol};
use crate::lsp::manager::LspManagerError;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum ServiceError {
    Lsp(LspManagerError),
    IdentifierSelection(PositionError),
    CallHierarchy(CallHierarchyError),
    Serialization(String),
    InvalidPath(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::Lsp(e) => write!(f, "Operation failed because {e}"),
            ServiceError::IdentifierSelection(e) => {
                write!(f, "Identifier selection failed because {e}")
            }
            ServiceError::CallHierarchy(e) => {
                write!(f, "Call hierarchy failed because {e}")
            }
            ServiceError::Serialization(message) => {
                write!(f, "Serialization failed because {message}")
            }
            ServiceError::InvalidPath(message) => {
                write!(f, "Invalid path: {message}")
            }
        }
    }
}

impl ServiceError {
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            ServiceError::IdentifierSelection(e) => e.suggestions(),
            ServiceError::CallHierarchy(e) => e.suggestions(),
            ServiceError::Lsp(_) | ServiceError::Serialization(_) | ServiceError::InvalidPath(_) => vec![],
        }
    }
}

impl Error for ServiceError {}

impl From<LspManagerError> for ServiceError {
    fn from(err: LspManagerError) -> Self {
        ServiceError::Lsp(err)
    }
}

impl From<PositionError> for ServiceError {
    fn from(err: PositionError) -> Self {
        ServiceError::IdentifierSelection(err)
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(err: serde_json::Error) -> Self {
        ServiceError::Serialization(err.to_string())
    }
}

impl From<CallHierarchyError> for ServiceError {
    fn from(err: CallHierarchyError) -> Self {
        ServiceError::CallHierarchy(err)
    }
}

#[derive(Debug)]
pub enum PositionError {
    IdentifierNotFound { closest: Vec<Identifier> },
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PositionError::IdentifierNotFound { closest } => write!(
                f,
                "No identifier found at position with {} nearby matches",
                closest.len()
            ),
        }
    }
}

impl Error for PositionError {}

impl PositionError {
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            PositionError::IdentifierNotFound { closest } => {
                let mut suggestions = vec![
                    "Use documentSymbol to see available symbols in this file".to_string(),
                ];
                if !closest.is_empty() {
                    let names: Vec<&str> = closest.iter().take(3).map(|id| id.name.as_str()).collect();
                    suggestions.push(format!("Nearby identifiers: {}", names.join(", ")));
                }
                suggestions
            }
        }
    }
}

#[derive(Debug)]
pub enum CallHierarchyError {
    NoItemAtPosition { nearby_callables: Vec<Symbol> },
}

impl fmt::Display for CallHierarchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallHierarchyError::NoItemAtPosition { nearby_callables } => write!(
                f,
                "No call hierarchy item at position with {} nearby callables",
                nearby_callables.len()
            ),
        }
    }
}

impl Error for CallHierarchyError {}

impl CallHierarchyError {
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            CallHierarchyError::NoItemAtPosition { nearby_callables } => {
                let mut suggestions = vec![
                    "Position must be on a function or method name".to_string(),
                ];
                if !nearby_callables.is_empty() {
                    let names: Vec<&str> = nearby_callables
                        .iter()
                        .take(3)
                        .map(|s| s.name.as_str())
                        .collect();
                    suggestions.push(format!("Nearby callables: {}", names.join(", ")));
                }
                suggestions
            }
        }
    }

    pub fn nearby_callables(&self) -> &[Symbol] {
        match self {
            CallHierarchyError::NoItemAtPosition { nearby_callables } => nearby_callables,
        }
    }
}
