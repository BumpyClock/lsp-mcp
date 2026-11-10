// ABOUTME: LSP client module providing trait and configuration for language server communication
// ABOUTME: Re-exports LspClient trait, LspClientConfig, and default capabilities

mod capabilities;
mod config;
mod connection;
mod trait_def;

pub use capabilities::create_default_capabilities;
pub use config::{LspClientConfig, MAX_IN_FLIGHT_REQUESTS};
pub use connection::{Connection, LspResponse};
pub use trait_def::LspClient;
