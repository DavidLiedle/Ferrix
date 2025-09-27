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
║  |  ___|_   _|  _ \|  _ \|_ _|\ \/ /     ║
║  | |_    | | | |_) | |_) || |  \  /      ║
║  |  _|   | | |  _ <|  _ < | |  /  \      ║
║  |_|     |_| |_| \_\_| \_\___/_/\_\      ║
║                                           ║
║  Blazingly fast terminal multiplexer      ║
║           Rewritten in Rust               ║
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
            println!("The prophecy from the GNU Screen vs Tmux book has been fulfilled!\n");

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

        Some(Commands::Send { .. }) => {
            eprintln!("Send command not yet implemented");
        }

        Some(Commands::Info { .. }) => {
            eprintln!("Info command not yet implemented");
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
