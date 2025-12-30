// ABOUTME: API types module for the lsp-mcp server.
// ABOUTME: Re-exports all domain types from submodules for stable public API.

mod call_hierarchy;
mod diagnostics;
mod languages;
mod mount_dir;
mod positions;
mod responses;
mod symbols;

pub use call_hierarchy::{
    CallHierarchyDirection, CallHierarchyItemInfo, CallHierarchyResponse, CallInfo,
    IncomingCallInfo, IncomingCallsResponse, OutgoingCallInfo, OutgoingCallsResponse,
    PrepareCallHierarchyResponse,
};
pub use diagnostics::{
    Diagnostic, DiagnosticSeverity, DiagnosticsResponse, FileDiagnostics, SeverityCounts,
};
pub use languages::{ErrorResponse, HealthResponse, LspStatus, SupportedLanguages};
pub use mount_dir::{
    get_mount_dir, set_global_mount_dir, set_thread_local_mount_dir, unset_thread_local_mount_dir,
};
pub use positions::{FilePosition, FileRange, Position, Range};
pub use responses::{
    DefinitionLocation, DefinitionResponse, HoverBatchItem, HoverContents, HoverRequest,
    HoverResponse, ImplementationResponse, ReferencedSymbolsResponse, ReferencesResponse,
    WorkspaceSymbolInfo, WorkspaceSymbolResponse,
};
pub use symbols::{
    CodeContext, Identifier, IdentifierResponse, ReferenceWithSymbolDefinitions, RelatedSymbols,
    Symbol, SymbolResponse,
};
