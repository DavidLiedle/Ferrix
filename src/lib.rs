// Allow clippy lints that would require significant refactoring
#![allow(clippy::inherent_to_string)]
#![allow(clippy::type_complexity)]
#![allow(clippy::if_same_then_else)]

// ============================================================================
// CORE MODULES (always available)
// ============================================================================
pub mod cli;
pub mod client;
pub mod config;
pub mod error;
pub mod format;
pub mod input;
pub mod protocol;
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

// ============================================================================
// TEST MODULES
// ============================================================================
#[cfg(test)]
pub mod tests;

// ============================================================================
// PUBLIC API
// ============================================================================
pub use error::{FerrixError, Result};
