// ============================================================================
// CORE MODULES (always available)
// ============================================================================
pub mod cli;
pub mod client;
pub mod config;
pub mod crash;
pub mod error;
pub mod format;
pub mod input;
pub mod protocol;
pub mod resilience;
pub mod server;
pub mod ui;
pub mod utils;

// ============================================================================
// OPTIONAL MODULES (feature-gated)
// ============================================================================
#[cfg(feature = "ai-assist")]
pub mod ai;

#[cfg(feature = "remote")]
pub mod auth;

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(any(feature = "ssh", feature = "mosh"))]
pub mod transport;

// ============================================================================
// TEST MODULES
// ============================================================================
#[cfg(test)]
pub mod tests;

// ============================================================================
// PUBLIC API
// ============================================================================
pub use error::{FerrixError, Result};
