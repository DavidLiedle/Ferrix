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

        #[arg(long, help = "Enable remote TCP/TLS access")]
        remote: bool,

        #[arg(long, default_value = "8080", help = "Port for remote connections")]
        port: u16,

        #[arg(long, help = "TLS certificate file for secure connections")]
        tls_cert: Option<String>,

        #[arg(long, help = "TLS private key file for secure connections")]
        tls_key: Option<String>,

        #[arg(long, default_value = "0.0.0.0", help = "Bind address for remote connections")]
        bind: String,
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

    #[command(about = "Connect to a remote Ferrix server")]
    Connect {
        #[arg(help = "Remote server address (host:port)")]
        address: String,

        #[arg(short, long, help = "Username for authentication")]
        username: String,

        #[arg(short, long, help = "Password for authentication (will prompt if not provided)")]
        password: Option<String>,

        #[arg(long, help = "CA certificate file for TLS verification")]
        tls_ca: Option<String>,

        #[arg(long, help = "Enable TLS (auto-detected if certificates provided)")]
        tls: bool,
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

    #[command(about = "Generate a default configuration file")]
    GenerateConfig {
        #[arg(short, long, help = "Force overwrite existing config")]
        force: bool,

        #[arg(short, long, help = "Output path for config file")]
        output: Option<String>,
    },

    #[command(about = "Validate configuration file")]
    ValidateConfig {
        #[arg(help = "Path to config file to validate")]
        path: Option<String>,
    },

    #[command(about = "Manage remote users and authentication")]
    UserManagement {
        #[command(subcommand)]
        action: UserAction,
    },
}

#[derive(Subcommand)]
pub enum UserAction {
    #[command(about = "Add a new remote user")]
    Add {
        #[arg(help = "Username")]
        username: String,

        #[arg(short, long, help = "Password (will prompt if not provided)")]
        password: Option<String>,
    },

    #[command(about = "Remove a remote user")]
    Remove {
        #[arg(help = "Username to remove")]
        username: String,
    },

    #[command(about = "List all remote users")]
    List,

    #[command(about = "Change user password")]
    ChangePassword {
        #[arg(help = "Username")]
        username: String,

        #[arg(short, long, help = "New password (will prompt if not provided)")]
        password: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_parsing_server_foreground() {
        let args = vec!["ferrix", "server", "--foreground"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::Server { foreground, .. }) => {
                assert!(foreground);
            }
            _ => panic!("Expected Server command"),
        }
    }

    #[test]
    fn test_cli_parsing_new_session() {
        let args = vec!["ferrix", "new", "--session", "test-session"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::New { session, .. }) => {
                assert_eq!(session, Some("test-session".to_string()));
            }
            _ => panic!("Expected New command"),
        }
    }

    #[test]
    fn test_cli_parsing_attach() {
        let args = vec!["ferrix", "attach", "existing-session"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::Attach { target }) => {
                assert_eq!(target, Some("existing-session".to_string()));
            }
            _ => panic!("Expected Attach command"),
        }
    }

    #[test]
    fn test_cli_parsing_list() {
        let args = vec!["ferrix", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::List) => {
                // Successfully parsed list command
            }
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn test_cli_parsing_save_snapshot() {
        let args = vec!["ferrix", "save-snapshot", "session1", "--name", "backup1"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::SaveSnapshot { session, name, .. }) => {
                assert_eq!(session, "session1");
                assert_eq!(name, Some("backup1".to_string()));
            }
            _ => panic!("Expected SaveSnapshot command"),
        }
    }

    #[test]
    fn test_cli_default_socket() {
        let args = vec!["ferrix", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.socket, "/tmp/ferrix.sock");
    }

    #[test]
    fn test_cli_custom_socket() {
        let args = vec!["ferrix", "--socket", "/custom/path.sock", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.socket, "/custom/path.sock");
    }

    #[test]
    fn test_cli_debug_flag() {
        let args = vec!["ferrix", "--debug", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert!(cli.debug);
    }

    #[test]
    fn test_cli_user_management_add() {
        let args = vec!["ferrix", "user-management", "add", "username"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::UserManagement { action }) => {
                match action {
                    UserAction::Add { username, .. } => {
                        assert_eq!(username, "username");
                    }
                    _ => panic!("Expected Add action"),
                }
            }
            _ => panic!("Expected UserManagement command"),
        }
    }
}