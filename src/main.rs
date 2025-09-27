use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use ferrix::cli::{Cli, Commands};
use ferrix::client::Client;
use ferrix::server::Server;
use ferrix::error::Result;

const ASCII_LOGO: &str = r#"
╔═══════════════════════════════════════════╗
║   _____ _____ ____  ____  ___ __  __     ║
║  |  ___| ____|  _ \|  _ \|_ _|\ \/ /     ║
║  | |_  |  _| | |_) | |_) || |  \  /      ║
║  |  _| | |___|  _ <|  _ < | |  /  \      ║
║  |_|   |_____|_| \_\_| \_\___/_/\_\      ║
║                                           ║
║  Revolutionary Terminal Multiplexer        ║
║         Built with Rust                   ║
╚═══════════════════════════════════════════╝
"#;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = if cli.debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    let socket_path = PathBuf::from(&cli.socket);

    match &cli.command {
        Some(Commands::Server { foreground }) => {
            println!("{}", ASCII_LOGO);
            println!("Starting Ferrix server...");
            println!("Socket: {:?}", socket_path);
            println!("The prophecy has been fulfilled! (https://github.com/cloudstreet-dev/GNU-Screen-vs-Tmux)\n");

            if !foreground {
                println!("Running in background mode (daemon)");
                // TODO: Implement proper daemonization
            }

            let mut server = Server::new(socket_path);
            server.run().await?;
        }

        Some(Commands::New { session, command: _, detached }) => {
            let mut client = Client::new(socket_path);
            client.connect().await?;

            let session_id = client.create_session(session.clone()).await?;

            if !detached {
                client.attach_session(session_id).await?;
            } else {
                println!("Session created: {}", session_id.0);
            }
        }

        Some(Commands::Attach { target }) => {
            let mut client = Client::new(socket_path);
            client.connect().await?;

            if let Some(target_str) = target {
                let sessions = client.list_sessions().await?;

                let session_id = if let Ok(uuid) = uuid::Uuid::parse_str(target_str) {
                    ferrix::protocol::SessionId(uuid)
                } else {
                    sessions
                        .iter()
                        .find(|s| s.name == *target_str)
                        .map(|s| s.id.clone())
                        .ok_or_else(|| ferrix::error::FerrixError::SessionNotFound(target_str.clone()))?
                };

                client.attach_session(session_id).await?;
            } else {
                let sessions = client.list_sessions().await?;

                if sessions.is_empty() {
                    eprintln!("No sessions available");
                } else {
                    let first_session = &sessions[0];
                    client.attach_session(first_session.id.clone()).await?;
                }
            }
        }

        Some(Commands::List) => {
            let mut client = Client::new(socket_path);
            client.connect().await?;

            let sessions = client.list_sessions().await?;

            if sessions.is_empty() {
                println!("No active sessions");
            } else {
                println!("Active sessions:");
                for session in sessions {
                    println!("  {} ({}) - {} windows - created at {}",
                        session.name,
                        session.id.0,
                        session.windows,
                        session.created_at.format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }
        }

        Some(Commands::Kill { target }) => {
            let mut client = Client::new(socket_path);
            client.connect().await?;

            let sessions = client.list_sessions().await?;

            let session_id = if let Ok(uuid) = uuid::Uuid::parse_str(target) {
                ferrix::protocol::SessionId(uuid)
            } else {
                sessions
                    .iter()
                    .find(|s| s.name == *target)
                    .map(|s| s.id.clone())
                    .ok_or_else(|| ferrix::error::FerrixError::SessionNotFound(target.clone()))?
            };

            client.kill_session(session_id).await?;
            println!("Session killed");
        }

        Some(Commands::Detach) => {
            eprintln!("Detach must be used from within an attached session (Ctrl-b d)");
        }

        Some(Commands::SaveSnapshot { session, name, description }) => {
            let mut client = Client::new(socket_path);
            client.connect().await?;

            let sessions = client.list_sessions().await?;

            let session_id = if let Ok(uuid) = uuid::Uuid::parse_str(session) {
                ferrix::protocol::SessionId(uuid)
            } else {
                sessions
                    .iter()
                    .find(|s| s.name == *session)
                    .map(|s| s.id.clone())
                    .ok_or_else(|| ferrix::error::FerrixError::SessionNotFound(session.clone()))?
            };

            let path = client.save_snapshot(session_id, name.clone(), description.clone()).await?;
            println!("Snapshot saved to: {:?}", path);
        }

        Some(Commands::LoadSnapshot { path }) => {
            let mut client = Client::new(socket_path);
            client.connect().await?;

            let session_id = client.load_snapshot(path.into()).await?;
            println!("Snapshot loaded as session: {}", session_id.0);
        }

        Some(Commands::ListSnapshots) => {
            let mut client = Client::new(socket_path);
            client.connect().await?;

            let snapshots = client.list_snapshots().await?;

            if snapshots.is_empty() {
                println!("No snapshots available");
            } else {
                println!("Available snapshots:");
                println!("{:<20} {:<30} {:<10} {}", "Created", "Name", "Size", "Path");
                println!("{}", "-".repeat(80));

                for snapshot in snapshots {
                    let size_mb = snapshot.size as f64 / 1024.0 / 1024.0;
                    println!(
                        "{:<20} {:<30} {:<10.2}MB {}",
                        snapshot.created_at.format("%Y-%m-%d %H:%M:%S"),
                        snapshot.name,
                        size_mb,
                        snapshot.path.display()
                    );
                }
            }
        }

        Some(Commands::DeleteSnapshot { path }) => {
            let mut client = Client::new(socket_path);
            client.connect().await?;

            client.delete_snapshot(path.into()).await?;
            println!("Snapshot deleted");
        }

        Some(Commands::ExportSnapshot { snapshot, output }) => {
            use ferrix::server::snapshot::SnapshotManager;

            let manager = SnapshotManager::new()?;
            let snapshot_data = manager.load_snapshot(&std::path::Path::new(snapshot))?;
            manager.export_snapshot(&snapshot_data, &std::path::Path::new(output))?;
            println!("Snapshot exported to: {}", output);
        }

        Some(Commands::ImportSnapshot { archive }) => {
            use ferrix::server::snapshot::SnapshotManager;

            let manager = SnapshotManager::new()?;
            let snapshot = manager.import_snapshot(&std::path::Path::new(archive))?;
            let path = manager.save_snapshot(&snapshot)?;
            println!("Snapshot imported to: {:?}", path);
        }

        Some(Commands::SendKeys { .. }) => {
            eprintln!("SendKeys command not yet implemented");
        }

        Some(Commands::ReloadConfig) => {
            eprintln!("ReloadConfig command not yet implemented");
        }

        Some(Commands::GenerateConfig { force, output }) => {
            use ferrix::config::ferrixrc::FerrixRc;
            use std::path::PathBuf;

            let config_path = if let Some(path) = output {
                PathBuf::from(path)
            } else if let Some(home) = dirs::home_dir() {
                home.join(".ferrixrc")
            } else {
                eprintln!("Could not determine home directory");
                return Ok(());
            };

            if config_path.exists() && !force {
                eprintln!("Configuration file already exists at {:?}", config_path);
                eprintln!("Use --force to overwrite");
                return Ok(());
            }

            let sample_config = FerrixRc::generate_sample();
            std::fs::write(&config_path, sample_config)?;
            println!("Generated configuration file at {:?}", config_path);
            println!("Edit this file to customize Ferrix behavior");
        }

        Some(Commands::ValidateConfig { path }) => {
            use ferrix::config::ferrixrc::FerrixRc;
            use std::path::PathBuf;

            let config_path = if let Some(p) = path {
                PathBuf::from(p)
            } else if let Ok(p) = std::env::var("FERRIXRC") {
                PathBuf::from(p)
            } else if let Some(home) = dirs::home_dir() {
                home.join(".ferrixrc")
            } else {
                eprintln!("Could not determine config file location");
                return Ok(());
            };

            if !config_path.exists() {
                eprintln!("Configuration file not found at {:?}", config_path);
                return Ok(());
            }

            println!("Validating configuration file: {:?}", config_path);

            match FerrixRc::load() {
                Ok(config) => {
                    println!("✓ Configuration is valid");
                    println!("  - {} keybindings defined", config.keybindings.len());
                    println!("  - {} hooks registered", config.hooks.len());
                    println!("  - {} aliases configured", config.aliases.len());
                    println!("  - {} startup commands", config.startup_commands.len());
                    println!("  - {} plugins configured", config.settings.plugins.len());
                }
                Err(e) => {
                    eprintln!("✗ Configuration validation failed:");
                    eprintln!("  {}", e);
                }
            }
        }

        None => {
            let mut client = Client::new(socket_path.clone());

            match client.connect().await {
                Ok(_) => {
                    let sessions = client.list_sessions().await?;
                    if sessions.is_empty() {
                        let session_id = client.create_session(None).await?;
                        client.attach_session(session_id).await?;
                    } else {
                        client.attach_session(sessions[0].id.clone()).await?;
                    }
                }
                Err(_) => {
                    println!("{}", ASCII_LOGO);
                    println!("No server found. Starting new server and session...\n");

                    let server_socket = socket_path.clone();
                    tokio::spawn(async move {
                        let mut server = Server::new(server_socket);
                        let _ = server.run().await;
                    });

                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                    let mut client = Client::new(socket_path);
                    client.connect().await?;
                    let session_id = client.create_session(None).await?;
                    client.attach_session(session_id).await?;
                }
            }
        }
    }

    Ok(())
}
