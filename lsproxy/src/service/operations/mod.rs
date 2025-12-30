// ABOUTME: Operations modules for LSP-backed code navigation.
// ABOUTME: Each module handles a specific domain of operations (definitions, references, etc.).

pub(crate) mod call_hierarchy;
pub(crate) mod definitions;
pub(crate) mod diagnostics;
pub(crate) mod hover;
pub(crate) mod references;
pub(crate) mod symbols;
