use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FerrixError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Client not found: {0}")]
    ClientNotFound(String),

    #[error("Window not found: {0}")]
    WindowNotFound(String),

    #[error("Pane not found: {0}")]
    PaneNotFound(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("PTY error: {0}")]
    Pty(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("Terminal error: {0}")]
    Terminal(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, FerrixError>;