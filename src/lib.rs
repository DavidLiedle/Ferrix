pub mod ai;
pub mod cli;
pub mod client;
pub mod config;
pub mod error;
pub mod plugin;
pub mod protocol;
pub mod server;
pub mod ui;
pub mod utils;

#[cfg(test)]
pub mod tests;

pub use error::{FerrixError, Result};