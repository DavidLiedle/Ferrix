//! Daemon Process Management
//!
//! Handles daemonization of the Ferrix server on Unix systems.
//! On Windows, provides graceful degradation with warnings.

use std::path::PathBuf;
use crate::error::{Result, FerrixError};

/// Daemonize the current process if not in foreground mode
///
/// This must be called BEFORE creating the tokio async runtime to avoid
/// issues with file descriptors and process forking.
///
/// # Arguments
/// * `foreground` - If true, skip daemonization and run in foreground
///
/// # Platform Support
/// - **Unix**: Full daemonization with PID file, stdout/stderr redirection
/// - **Windows**: Prints warning and continues in foreground mode
#[cfg(unix)]
pub fn daemonize_if_needed(foreground: bool) -> Result<()> {
    if foreground {
        return Ok(());
    }

    use daemonize::Daemonize;
    use std::fs::File;

    println!("Starting Ferrix server as daemon...");

    // Create directories for daemon files if they don't exist
    let ferrix_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("ferrix");
    std::fs::create_dir_all(&ferrix_dir)
        .map_err(|e| FerrixError::Other(
            format!("Failed to create ferrix directory: {}", e)
        ))?;

    let stdout_file = File::create(ferrix_dir.join("ferrix.out"))
        .map_err(|e| FerrixError::Other(
            format!("Failed to create stdout log file: {}", e)
        ))?;

    let stderr_file = File::create(ferrix_dir.join("ferrix.err"))
        .map_err(|e| FerrixError::Other(
            format!("Failed to create stderr log file: {}", e)
        ))?;

    let daemon = Daemonize::new()
        .pid_file(ferrix_dir.join("ferrix.pid"))
        .chown_pid_file(true)
        .working_directory("/tmp")
        .stdout(stdout_file)
        .stderr(stderr_file)
        .privileged_action(|| "Ferrix daemon started");

    daemon.start()
        .map_err(|e| FerrixError::Other(format!("Failed to daemonize: {}", e)))?;

    println!("Ferrix server daemonized successfully");
    Ok(())
}

/// Windows version - daemonization not supported
#[cfg(not(unix))]
pub fn daemonize_if_needed(foreground: bool) -> Result<()> {
    if !foreground {
        eprintln!("Warning: Daemon mode is not supported on Windows. Running in foreground mode.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemonize_foreground_mode() {
        // Foreground mode should always succeed without actually daemonizing
        let result = daemonize_if_needed(true);
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(not(unix))]
    fn test_daemonize_windows_warning() {
        // On Windows, non-foreground should return Ok but print warning
        let result = daemonize_if_needed(false);
        assert!(result.is_ok());
    }
}
