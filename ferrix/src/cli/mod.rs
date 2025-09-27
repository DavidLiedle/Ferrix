pub mod commands;

use clap::{Parser, Subcommand};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "ferrix")]
#[command(about = "A revolutionary Rust-based terminal multiplexer", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to the server socket
    #[arg(short = 'S', long, default_value = "/tmp/ferrix.sock")]
    pub socket: String,

    /// Enable debug logging
    #[arg(short, long)]
    pub debug: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new session
    New {
        /// Session name
        #[arg(short = 's', long)]
        session: Option<String>,

        /// Command to run (default: shell)
        #[arg(short, long)]
        command: Option<String>,

        /// Detach after creating
        #[arg(short, long)]
        detached: bool,
    },

    /// Attach to an existing session
    Attach {
        /// Session ID or name
        #[arg(short = 't', long)]
        target: Option<String>,
    },

    /// Detach from current session
    Detach,

    /// List all sessions
    #[command(alias = "ls")]
    List,

    /// Kill a session
    Kill {
        /// Session ID or name
        #[arg(short = 't', long)]
        target: String,
    },

    /// Start the server daemon
    Server {
        /// Run in foreground
        #[arg(short, long)]
        foreground: bool,
    },

    /// Send a command to the server
    Send {
        /// Session ID or name
        #[arg(short = 't', long)]
        target: String,

        /// Command to send
        command: String,
    },

    /// Show session info
    Info {
        /// Session ID or name
        #[arg(short = 't', long)]
        target: Option<String>,
    },
}

impl Cli {
    pub fn parse_target(&self, target: &str) -> Option<crate::protocol::SessionId> {
        if let Ok(uuid) = Uuid::parse_str(target) {
            Some(crate::protocol::SessionId(uuid))
        } else {
            None
        }
    }
}