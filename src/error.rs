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

    #[error("Not connected to server")]
    NotConnected,

    #[error("Terminal error: {0}")]
    Terminal(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, FerrixError>;

impl FerrixError {
    /// Get helpful suggestion for this error
    pub fn suggestion(&self) -> Option<String> {
        match self {
            FerrixError::SessionNotFound(name) => {
                Some(format!(
                    "Session '{}' not found. Try:\n  • ferrix list - to see available sessions\n  • ferrix new -s {} - to create this session",
                    name, name
                ))
            }
            FerrixError::NotConnected => {
                Some(
                    "Not connected to Ferrix server. Try:\n  • ferrix server - to start the server\n  • Check if the server is running: ps aux | grep ferrix".to_string()
                )
            }
            FerrixError::Config(msg) if msg.contains("not found") || msg.contains("No such file") => {
                Some(
                    "Configuration file not found. Try:\n  • ferrix generate-config - to create a default config\n  • Check ~/.config/ferrix/ferrix.toml".to_string()
                )
            }
            FerrixError::Pty(msg) if msg.contains("shell") || msg.contains("spawn") => {
                Some(
                    "Failed to spawn shell. Try:\n  • Check SHELL environment variable: echo $SHELL\n  • Verify shell exists: which $SHELL\n  • Try a different shell: SHELL=/bin/bash ferrix ...".to_string()
                )
            }
            FerrixError::WindowNotFound(_) => {
                Some(
                    "Window not found. Try:\n  • Ctrl-b w - to list windows\n  • Ctrl-b c - to create a new window".to_string()
                )
            }
            FerrixError::PaneNotFound(_) => {
                Some(
                    "Pane not found. Try:\n  • Ctrl-b q - to display pane numbers\n  • Check if panes were closed".to_string()
                )
            }
            FerrixError::Ipc(msg) if msg.contains("socket") || msg.contains("connection") => {
                Some(
                    "Cannot connect to server. Try:\n  • ferrix server - to start the server\n  • rm /tmp/ferrix-*.sock - to clean up stale sockets\n  • Check permissions on /tmp".to_string()
                )
            }
            _ => None,
        }
    }

    /// Format error with suggestion for user display
    pub fn display_with_suggestion(&self) -> String {
        let base_msg = format!("Error: {}", self);
        if let Some(suggestion) = self.suggestion() {
            format!("{}\n\n{}", base_msg, suggestion)
        } else {
            base_msg
        }
    }
}

/// Extension trait for Result to add context to errors
pub trait ResultExt<T> {
    /// Add context to an error
    fn context(self, context: impl Into<String>) -> Result<T>;

    /// Add context with lazy evaluation
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T, E: std::error::Error> ResultExt<T> for std::result::Result<T, E> {
    fn context(self, context: impl Into<String>) -> Result<T> {
        self.map_err(|e| FerrixError::Other(format!("{}: {}", context.into(), e)))
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| FerrixError::Other(format!("{}: {}", f(), e)))
    }
}

#[cfg(feature = "plugin")]
impl From<wasmtime::Error> for FerrixError {
    fn from(err: wasmtime::Error) -> Self {
        FerrixError::Plugin(format!("WASM runtime error: {}", err))
    }
}

#[cfg(feature = "gpu")]
impl From<wgpu::SurfaceError> for FerrixError {
    fn from(err: wgpu::SurfaceError) -> Self {
        FerrixError::Other(format!("GPU surface error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_error_from_io() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "File not found");
        let ferrix_error = FerrixError::from(io_error);

        match ferrix_error {
            FerrixError::Io(_) => {
                // Successfully converted
            }
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn test_session_not_found_error() {
        let error = FerrixError::SessionNotFound("test-session".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("Session not found: test-session"));
    }

    #[test]
    fn test_client_not_found_error() {
        let error = FerrixError::ClientNotFound("client-123".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("Client not found: client-123"));
    }

    #[test]
    fn test_window_not_found_error() {
        let error = FerrixError::WindowNotFound("window-456".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("Window not found: window-456"));
    }

    #[test]
    fn test_pane_not_found_error() {
        let error = FerrixError::PaneNotFound("pane-789".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("Pane not found: pane-789"));
    }

    #[test]
    fn test_protocol_error() {
        let error = FerrixError::Protocol("Invalid message format".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("Protocol error: Invalid message format"));
    }

    #[test]
    fn test_config_error() {
        let error = FerrixError::Config("Missing configuration file".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("Configuration error: Missing configuration file"));
    }

    #[test]
    fn test_pty_error() {
        let error = FerrixError::Pty("Failed to spawn shell".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("PTY error: Failed to spawn shell"));
    }

    #[test]
    fn test_ipc_error() {
        let error = FerrixError::Ipc("Socket connection failed".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("IPC error: Socket connection failed"));
    }

    #[test]
    fn test_not_connected_error() {
        let error = FerrixError::NotConnected;
        let error_string = format!("{}", error);
        assert_eq!(error_string, "Not connected to server");
    }

    #[test]
    fn test_terminal_error() {
        let error = FerrixError::Terminal("Terminal initialization failed".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("Terminal error: Terminal initialization failed"));
    }

    #[test]
    fn test_plugin_error() {
        let error = FerrixError::Plugin("Plugin loading failed".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("Plugin error: Plugin loading failed"));
    }

    #[test]
    fn test_other_error() {
        let error = FerrixError::Other("Unknown error occurred".to_string());
        let error_string = format!("{}", error);
        assert!(error_string.contains("Other error: Unknown error occurred"));
    }

    #[test]
    fn test_result_type_alias() {
        let success: Result<i32> = Ok(42);
        let failure: Result<i32> = Err(FerrixError::NotConnected);

        assert!(success.is_ok());
        assert!(failure.is_err());
        assert_eq!(success.unwrap(), 42);
    }

    #[test]
    fn test_error_debug_format() {
        let error = FerrixError::SessionNotFound("debug-session".to_string());
        let debug_string = format!("{:?}", error);
        assert!(debug_string.contains("SessionNotFound"));
        assert!(debug_string.contains("debug-session"));
    }
}