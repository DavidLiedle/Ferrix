use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ferrix")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, default_value = "/tmp/ferrix.sock")]
    pub socket: String,

    #[arg(short, long)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Server {
        #[arg(short, long)]
        foreground: bool,
    },

    #[command(visible_alias = "n")]
    New {
        #[arg(short, long)]
        session: Option<String>,

        #[arg(short, long)]
        command: Option<String>,

        #[arg(short, long)]
        detached: bool,
    },

    #[command(visible_alias = "a")]
    Attach {
        target: Option<String>,
    },

    #[command(visible_alias = "ls")]
    List,

    #[command(visible_alias = "k")]
    Kill {
        target: String,
    },

    #[command(visible_alias = "d")]
    Detach,

    // Snapshot commands
    #[command(about = "Save a session snapshot")]
    SaveSnapshot {
        #[arg(help = "Session ID or name to snapshot")]
        session: String,

        #[arg(short, long, help = "Name for the snapshot")]
        name: Option<String>,

        #[arg(short, long, help = "Description for the snapshot")]
        description: Option<String>,
    },

    #[command(about = "Load a session from snapshot")]
    LoadSnapshot {
        #[arg(help = "Path to snapshot file")]
        path: String,
    },

    #[command(about = "List available snapshots")]
    ListSnapshots,

    #[command(about = "Delete a snapshot")]
    DeleteSnapshot {
        #[arg(help = "Path to snapshot file to delete")]
        path: String,
    },

    #[command(about = "Export snapshot to compressed archive")]
    ExportSnapshot {
        #[arg(help = "Path to snapshot file")]
        snapshot: String,

        #[arg(help = "Path for exported archive")]
        output: String,
    },

    #[command(about = "Import snapshot from compressed archive")]
    ImportSnapshot {
        #[arg(help = "Path to compressed archive")]
        archive: String,
    },

    SendKeys {
        target: String,
        keys: Vec<String>,
    },

    #[command(visible_alias = "config")]
    ReloadConfig,
}