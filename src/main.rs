use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use ferrix::cli::{Cli, Commands, UserAction};
use ferrix::client::Client;
use ferrix::server::{Server, remote::{RemoteServer, PasswordAuthHandler}};
use ferrix::error::Result;
use ferrix::protocol::{ClientId, AuthCredentials};
use std::net::SocketAddr;
use std::sync::Arc;

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
        Some(Commands::Server { foreground, remote, port, tls_cert, tls_key, bind }) => {
            println!("{}", ASCII_LOGO);
            println!("Starting Ferrix server...");
            println!("Socket: {:?}", socket_path);

            if *remote {
                println!("Remote access enabled on {}:{}", bind, port);
                if tls_cert.is_some() && tls_key.is_some() {
                    println!("TLS encryption enabled");
                }
            }

            println!("The prophecy has been fulfilled! (https://github.com/cloudstreet-dev/GNU-Screen-vs-Tmux)\n");

            if !foreground {
                println!("Running in background mode (daemon)");
                // TODO: Implement proper daemonization
            }

            let server = Arc::new(Server::new(socket_path.clone()));

            if *remote {
                // Start remote server alongside local server
                let bind_addr: SocketAddr = format!("{}:{}", bind, port)
                    .parse()
                    .map_err(|e| ferrix::error::FerrixError::Other(format!("Invalid bind address: {}", e)))?;

                // Create authentication handler
                let auth_handler = Arc::new(PasswordAuthHandler::new());

                // Add a default admin user for testing
                auth_handler.add_user(
                    "admin".to_string(),
                    "password".to_string(),
                    ClientId(uuid::Uuid::new_v4())
                ).await;

                let mut remote_server = RemoteServer::new(bind_addr, server.clone(), auth_handler);

                // Configure TLS if certificates provided
                if let (Some(cert_path), Some(key_path)) = (tls_cert, tls_key) {
                    remote_server = remote_server.with_tls(
                        &std::path::PathBuf::from(cert_path),
                        &std::path::PathBuf::from(key_path)
                    )?;
                }

                // Start remote server in background
                let remote_handle = tokio::spawn(async move {
                    if let Err(e) = remote_server.start().await {
                        eprintln!("Remote server error: {}", e);
                    }
                });

                // Start local server
                let local_server = server.clone();
                let local_handle = tokio::spawn(async move {
                    let mut server = (*local_server).clone();
                    if let Err(e) = server.run().await {
                        eprintln!("Local server error: {}", e);
                    }
                });

                // Wait for both servers
                tokio::select! {
                    _ = remote_handle => {}
                    _ = local_handle => {}
                }
            } else {
                // Start only local server
                let mut local_server = (*server).clone();
                local_server.run().await?;
            }
        }

        Some(Commands::New { session, command: _, detached }) => {
            let mut client = Client::new(socket_path)?;
            client.connect().await?;

            let session_id = client.create_session(session.clone()).await?;

            if !detached {
                client.attach_session(session_id).await?;
            } else {
                println!("Session created: {}", session_id.0);
            }
        }

        Some(Commands::Attach { target }) => {
            let mut client = Client::new(socket_path)?;
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
            let mut client = Client::new(socket_path)?;
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
            let mut client = Client::new(socket_path)?;
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
            let mut client = Client::new(socket_path)?;
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
            let mut client = Client::new(socket_path)?;
            client.connect().await?;

            let session_id = client.load_snapshot(path.into()).await?;
            println!("Snapshot loaded as session: {}", session_id.0);
        }

        Some(Commands::ListSnapshots) => {
            let mut client = Client::new(socket_path)?;
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
            let mut client = Client::new(socket_path)?;
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
            println!("Note: Config hot reload is automatically handled when attached to a session.");
            println!("Use Ctrl-b r to reload config while in a session, or restart the client.");
        }

        Some(Commands::GenerateConfig { force, output }) => {
            use ferrix::config::Config;
            use std::path::PathBuf;

            let config_path = if let Some(path) = output {
                PathBuf::from(path)
            } else {
                Config::get_config_path()?
            };

            if config_path.exists() && !force {
                eprintln!("Configuration file already exists at {:?}", config_path);
                eprintln!("Use --force to overwrite");
                return Ok(());
            }

            // Create config directory if it doesn't exist
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let default_config = Config::default();
            default_config.save()?;
            println!("Generated configuration file at {:?}", config_path);
            println!("Edit this file to customize Ferrix behavior");
            println!("Key bindings can be customized in the [keybindings] section");
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

        Some(Commands::Connect { address, username, password, tls_ca, tls }) => {
            use ferrix::server::remote::RemoteClient;
            use std::io::{self, Write};

            let server_addr: SocketAddr = address.parse()
                .map_err(|e| ferrix::error::FerrixError::Other(format!("Invalid server address: {}", e)))?;

            // Get password if not provided
            let password = if let Some(pwd) = password {
                pwd.clone()
            } else {
                print!("Password for {}: ", username);
                io::stdout().flush()?;
                rpassword::read_password()
                    .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to read password: {}", e)))?
            };

            let credentials = AuthCredentials {
                username: username.clone(),
                password: Some(password),
                token: None,
                certificate: None,
            };

            let mut client = RemoteClient::new(server_addr, credentials);

            // Configure TLS if requested or certificates provided
            if *tls || tls_ca.is_some() {
                let ca_path = tls_ca.as_ref().map(|p| std::path::PathBuf::from(p));
                client = client.with_tls(ca_path.as_ref())?;
            }

            println!("Connecting to remote server at {}...", address);

            match client.connect().await {
                Ok(mut session) => {
                    println!("Connected successfully! Starting interactive session...");

                    // Create or attach to a session
                    match session.create_session(Some(format!("{}-session", username))).await {
                        Ok(session_id) => {
                            println!("Created remote session: {}", session_id.0);

                            // Simple echo loop for demonstration
                            // In a full implementation, this would integrate with the UI
                            loop {
                                print!("ferrix-remote> ");
                                io::stdout().flush()?;

                                let mut input = String::new();
                                match io::stdin().read_line(&mut input) {
                                    Ok(_) => {
                                        let input = input.trim();
                                        if input == "exit" || input == "quit" {
                                            break;
                                        }

                                        if input == "detach" {
                                            break;
                                        }

                                        // Send input to remote session
                                        if !input.is_empty() {
                                            session.send_input(input.as_bytes().to_vec()).await?;
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Error reading input: {}", e);
                                        break;
                                    }
                                }
                            }

                            session.disconnect().await?;
                            println!("Disconnected from remote server");
                        }
                        Err(e) => {
                            eprintln!("Failed to create remote session: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to connect to remote server: {}", e);
                }
            }
        }

        Some(Commands::UserManagement { action }) => {
            match action {
                UserAction::Add { username, password } => {
                    use std::io::{self, Write};

                    let password = if let Some(pwd) = password {
                        pwd.clone()
                    } else {
                        print!("Password for {}: ", username);
                        io::stdout().flush()?;
                        rpassword::read_password()
                            .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to read password: {}", e)))?
                    };

                    // TODO: Implement user storage/management
                    println!("User management not fully implemented yet.");
                    println!("Would add user '{}' with provided password", username);
                }
                UserAction::Remove { username } => {
                    println!("User management not fully implemented yet.");
                    println!("Would remove user '{}'", username);
                }
                UserAction::List => {
                    println!("User management not fully implemented yet.");
                    println!("Would list all users");
                }
                UserAction::ChangePassword { username, password } => {
                    use std::io::{self, Write};

                    let _password = if let Some(pwd) = password {
                        pwd.clone()
                    } else {
                        print!("New password for {}: ", username);
                        io::stdout().flush()?;
                        rpassword::read_password()
                            .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to read password: {}", e)))?
                    };

                    println!("User management not fully implemented yet.");
                    println!("Would change password for user '{}'", username);
                }
            }
        }

        None => {
            let mut client = Client::new(socket_path.clone())?;

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

                    let mut client = Client::new(socket_path)?;
                    client.connect().await?;
                    let session_id = client.create_session(None).await?;
                    client.attach_session(session_id).await?;
                }
            }
        }
    }

    Ok(())
}
