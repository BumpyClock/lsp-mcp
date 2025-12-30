// ABOUTME: Request parameter types for service layer operations.
// ABOUTME: Contains FindDefinitionParams for configuring definition lookups.

/// Parameters for find_definition with optimization options
#[derive(Debug, Clone, Default)]
pub struct FindDefinitionParams {
    pub compact: bool,
    pub include_siblings: bool,
    pub siblings_limit: Option<u32>,
}
