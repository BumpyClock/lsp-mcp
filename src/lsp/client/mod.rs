// ABOUTME: LSP client module providing trait and configuration for language server communication
// ABOUTME: Re-exports LspClient trait, LspClientConfig, and default capabilities

mod capabilities;
mod config;
mod trait_def;

pub use capabilities::create_default_capabilities;
pub use config::LspClientConfig;
pub use trait_def::LspClient;
