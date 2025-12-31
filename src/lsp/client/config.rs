// ABOUTME: Configuration types for LSP client behavior
// ABOUTME: Provides timeout settings and reconnection parameters

use std::time::Duration;

/// Configuration for LSP client behavior
#[derive(Clone, Debug)]
pub struct LspClientConfig {
    /// Timeout for individual LSP requests (default: 30 seconds)
    pub request_timeout: Duration,
    /// Maximum retry attempts for reconnection (default: 3)
    pub max_reconnect_attempts: u32,
    /// Base delay between reconnection attempts (default: 1 second)
    pub reconnect_base_delay: Duration,
    /// Maximum delay between reconnection attempts (default: 30 seconds)
    pub reconnect_max_delay: Duration,
}

impl Default for LspClientConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            max_reconnect_attempts: 3,
            reconnect_base_delay: Duration::from_secs(1),
            reconnect_max_delay: Duration::from_secs(30),
        }
    }
}
