//! Test client utilities

use std::path::PathBuf;
use std::process::{Command, Output};

/// Test client that wraps CLI command execution
pub struct TestClient {
    socket_path: PathBuf,
}

impl TestClient {
    /// Create a new test client
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Run a ferrix command and return the output
    pub fn run_command(&self, args: &[&str]) -> Output {
        let ferrix_path = Self::find_ferrix_binary();

        Command::new(ferrix_path)
            .arg("--socket")
            .arg(&self.socket_path)
            .args(args)
            .output()
            .expect("Failed to run ferrix command")
    }

    /// Create a new session
    pub fn new_session(&self, name: &str, detached: bool) -> Output {
        let mut args = vec!["new", "-s", name];
        if detached {
            args.push("--detached");
        }
        self.run_command(&args)
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Output {
        self.run_command(&["list"])
    }

    /// Kill a session
    pub fn kill_session(&self, name: &str) -> Output {
        self.run_command(&["kill", name])
    }

    /// Send keys to a session
    pub fn send_keys(&self, session: &str, keys: &str) -> Output {
        self.run_command(&["send-keys", session, keys])
    }

    /// Attach to a session
    pub fn attach(&self, session: &str) -> Output {
        self.run_command(&["attach", session])
    }

    /// Save a snapshot
    pub fn save_snapshot(&self, session: &str, name: &str, description: &str) -> Output {
        self.run_command(&["save-snapshot", session, "--name", name, "--description", description])
    }

    /// List snapshots
    pub fn list_snapshots(&self) -> Output {
        self.run_command(&["list-snapshots"])
    }

    /// Enter copy mode
    pub fn enter_copy_mode(&self) -> Output {
        self.run_command(&["enter-copy-mode"])
    }

    /// Exit copy mode
    pub fn exit_copy_mode(&self) -> Output {
        self.run_command(&["exit-copy-mode"])
    }

    /// List keybindings
    pub fn list_keys(&self) -> Output {
        self.run_command(&["list-keys"])
    }

    /// Bind a key
    pub fn bind_key(&self, key: &str, action: &str) -> Output {
        self.run_command(&["bind-key", key, action])
    }

    /// Unbind a key
    pub fn unbind_key(&self, key: &str) -> Output {
        self.run_command(&["unbind-key", key])
    }

    /// Reset keybindings to defaults
    pub fn reset_keys(&self) -> Output {
        self.run_command(&["reset-keys"])
    }

    /// Create a new window
    pub fn new_window(&self, name: &str) -> Output {
        self.run_command(&["new-window", "-n", name])
    }

    /// List windows
    pub fn list_windows(&self) -> Output {
        self.run_command(&["list-windows"])
    }

    /// Rename window
    pub fn rename_window(&self, new_name: &str) -> Output {
        self.run_command(&["rename-window", new_name])
    }

    /// Parse session list output into session names
    pub fn parse_session_list(output: &Output) -> Vec<String> {
        if !output.status.success() {
            return Vec::new();
        }

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.to_string())
            .collect()
    }

    /// Find the ferrix binary (release or debug)
    fn find_ferrix_binary() -> String {
        if std::path::Path::new("./target/release/ferrix").exists() {
            "./target/release/ferrix".to_string()
        } else if std::path::Path::new("./target/debug/ferrix").exists() {
            "./target/debug/ferrix".to_string()
        } else {
            panic!("Ferrix binary not found. Run 'cargo build' first.");
        }
    }
}
