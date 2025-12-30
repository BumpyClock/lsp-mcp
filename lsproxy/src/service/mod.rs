// ABOUTME: Service layer module for LSP-backed code navigation.
// ABOUTME: Organizes types and utilities into sub-modules with public re-exports.

mod core;
pub(crate) mod operations;
pub mod types;
pub mod utils;

pub use core::{create_service, LspService};
pub use types::errors::{CallHierarchyError, PositionError, ServiceError};
pub use types::response::{
    CompactDefinitionResponse, FileGroup, McpDefinitionLocation, McpDefinitionResponse,
    McpIdentifierResponse, McpListFilesResponse, McpReferenceLocation, McpReferencesResponse,
    McpSymbolsResponse, TypeCounts,
};
pub use utils::external::{ExternalInfo, PackageInfo};
pub use utils::signature::{
    filter_sibling_exports, is_internal_builder_symbol, truncate_signature,
    DEFAULT_MAX_SIGNATURE_LENGTH,
};
