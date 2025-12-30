// ABOUTME: Type definitions for the service layer.
// ABOUTME: Re-exports errors and response structures.

pub mod errors;
pub mod request;
pub mod response;

pub use errors::{CallHierarchyError, PositionError, ServiceError};
pub use response::{
    CompactDefinitionResponse, FileGroup, McpDefinitionLocation, McpDefinitionResponse,
    McpIdentifierResponse, McpListFilesResponse, McpReferenceLocation, McpReferencesResponse,
    McpSymbolsResponse, TypeCounts,
};
