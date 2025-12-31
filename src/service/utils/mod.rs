// ABOUTME: Utility modules for the service layer.
// ABOUTME: Re-exports pagination, external, identifier, transformation, and signature utilities.

pub mod external;
pub(crate) mod hover_parser;
pub mod identifiers;
pub mod pagination;
pub mod signature;
pub mod transformations;

pub use external::{ExternalInfo, PackageInfo};
pub use signature::{
    extract_active_signature, filter_sibling_exports, is_internal_builder_symbol,
    truncate_signature, ActiveSignatureInfo, DEFAULT_MAX_SIGNATURE_LENGTH,
};
