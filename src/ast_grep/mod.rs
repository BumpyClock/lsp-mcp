//! Tree-sitter based code analysis and pattern matching.
//!
//! This module provides functionality for analyzing source code using tree-sitter,
//! including pattern matching, symbol lookup, and code navigation.
//!
//! ## Public API
//!
//! - [`filters`]: Post-processing filters for tree-sitter query captures.
//!   These filter out unwanted matches that tree-sitter queries alone can't exclude.
//! - [`query_registry`]: Registry of compiled tree-sitter queries organized by
//!   language and query type (symbol, identifier, reference).

pub(crate) mod client;
pub mod filters;
pub mod query_registry;
pub(crate) mod types;
