// ABOUTME: Process health state tracking for LSP server lifecycle monitoring
// ABOUTME: Provides health states to track whether LSP process is healthy, unhealthy, or dead

/// Represents the current health state of the LSP process
#[derive(Clone, Debug, PartialEq)]
pub enum ProcessHealth {
    Healthy,
    Unhealthy(String),
    Dead,
}
