// ABOUTME: Manager module for coordinating multiple LSP clients in a workspace
// ABOUTME: Re-exports Manager, LspManagerError, and language detection utilities

mod core;
mod detection;
mod error;
mod startup;

pub use core::Manager;
pub use error::LspManagerError;

#[cfg(test)]
mod language_tests;
