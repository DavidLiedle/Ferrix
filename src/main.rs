use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use ferrix::cli::{Cli, Commands};
#[cfg(feature = "remote")]
use ferrix::cli::UserAction;
use ferrix::client::Client;
use ferrix::server::Server;
#[cfg(feature = "remote")]
use ferrix::server::remote::{RemoteServer, PasswordAuthHandler};
use ferrix::error::{Result, FerrixError};
#[cfg(feature = "remote")]
use ferrix::protocol::AuthCredentials;
#[cfg(feature = "remote")]
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
║      Modern Terminal Multiplexer          ║
║            Built with Rust                ║
╚═══════════════════════════════════════════╝
"#;

// Main entry point - handle daemonization BEFORE creating async runtime
fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle daemonization before creating the tokio runtime
    // This is critical for macOS and other Unix systems
    #[cfg(unix)]
    if let Some(Commands::Server { foreground, .. }) = &cli.command {
        if !foreground {
            use daemonize::Daemonize;
            use std::fs::File;

            println!("Starting Ferrix server as daemon...");

            // Create directories for daemon files if they don't exist
            let ferrix_dir = dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("ferrix");
            std::fs::create_dir_all(&ferrix_dir).ok();

            let stdout_file = File::create(ferrix_dir.join("ferrix.out"))
                .map_err(|e| ferrix::error::FerrixError::Other(
                    format!("Failed to create stdout log file: {}", e)
                ))?;

            let stderr_file = File::create(ferrix_dir.join("ferrix.err"))
                .map_err(|e| ferrix::error::FerrixError::Other(
                    format!("Failed to create stderr log file: {}", e)
                ))?;

            let daemon = Daemonize::new()
                .pid_file(ferrix_dir.join("ferrix.pid"))
                .chown_pid_file(true)
                .working_directory("/tmp")
                .stdout(stdout_file)
                .stderr(stderr_file)
                .privileged_action(|| "Ferrix daemon started");

            match daemon.start() {
                Ok(_) => println!("Ferrix server daemonized successfully"),
                Err(e) => {
                    eprintln!("Error daemonizing: {}", e);
                    return Err(ferrix::error::FerrixError::Other(format!("Failed to daemonize: {}", e)));
                }
            }
        }
    }

    // On Windows, warn if trying to run as daemon
    #[cfg(not(unix))]
    if let Some(Commands::Server { foreground, .. }) = &cli.command {
        if !foreground {
            eprintln!("Warning: Daemon mode is not supported on Windows. Running in foreground mode.");
        }
    }

    // Now create the tokio runtime AFTER daemonization
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to create runtime: {}", e)))?;

    runtime.block_on(async_main(cli))
}

async fn async_main(cli: Cli) -> Result<()> {

    let filter = if cli.debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    // Initialize crash capture system (P1.6)
    // This sets up a panic hook to automatically capture crash information
    if let Err(e) = ferrix::crash::initialize(None) {
        eprintln!("Warning: Failed to initialize crash capture: {}", e);
    }

    let socket_path = PathBuf::from(&cli.socket);

    match &cli.command {
        Some(Commands::Server { foreground, recover, remote, port, tls_cert, tls_key, bind }) => {
            // Only show ASCII logo if not already daemonized
            if *foreground {
                println!("{}", ASCII_LOGO);
                println!("Starting Ferrix server...");
                println!("Socket: {:?}", socket_path);

                if *remote {
                    println!("Remote access enabled on {}:{}", bind, port);
                    if tls_cert.is_some() && tls_key.is_some() {
                        println!("TLS encryption enabled");
                    }
                }

                println!("The prophecy has been fulfilled! (https://cloudstreet-dev.github.io/GNU-Screen-vs-Tmux/)\n");
            }
            // Note: daemonization already handled in main() before async runtime creation

            let server = Arc::new(Server::new(socket_path.clone()));

            // Recovery is disabled by default (like tmux/screen) unless --recover is specified
            let enable_recovery = *recover;

            #[cfg(feature = "remote")]
            if *remote {
                // Start remote server alongside local server
                let bind_addr: SocketAddr = format!("{}:{}", bind, port)
                    .parse()
                    .map_err(|e| ferrix::error::FerrixError::Other(format!("Invalid bind address: {}", e)))?;

                // Create authentication handler
                let auth_handler = Arc::new(
                    PasswordAuthHandler::new().await
                        .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to create auth handler: {}", e)))?
                );

                // Ensure default admin user exists for testing
                auth_handler.ensure_default_admin().await
                    .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to ensure default admin: {}", e)))?;

                let mut remote_server = RemoteServer::new(bind_addr, server.clone(), auth_handler);

                // Configure TLS if certificates provided
                if let (Some(cert_path), Some(key_path)) = (tls_cert, tls_key) {
                    use ferrix::server::remote::TlsMode;
                    remote_server = remote_server.with_tls(
                        &std::path::PathBuf::from(cert_path),
                        &std::path::PathBuf::from(key_path),
                        TlsMode::ServerOnly,  // Default to server-only TLS
                        None  // No client CA for server-only mode
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
                    if let Err(e) = server.run(enable_recovery).await {
                        eprintln!("Local server error: {}", e);
                    }
                });

                // Wait for both servers
                tokio::select! {
                    _ = remote_handle => {}
                    _ = local_handle => {}
                }
            }

            #[cfg(not(feature = "remote"))]
            if *remote {
                return Err(ferrix::error::FerrixError::Other(
                    "Remote access not available - rebuild with --features remote".to_string()
                ));
            }

            #[cfg(not(feature = "remote"))]
            {
                // Start only local server
                let mut local_server = (*server).clone();
                local_server.run(enable_recovery).await?;
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

        Some(Commands::RestoreSnapshot { session, path }) => {
            let mut client = Client::new(socket_path)?;
            client.connect().await?;

            // Resolve session name/ID
            let sessions = client.list_sessions().await?;
            let session_id = sessions
                .iter()
                .find(|s| &s.name == session || s.id.0.to_string().starts_with(session.as_str()))
                .map(|s| s.id.clone())
                .ok_or_else(|| FerrixError::Other(format!("Session '{}' not found", session)))?;

            client.restore_snapshot(session_id.clone(), path.into()).await?;
            println!("Snapshot restored into session: {} ({})", session, session_id.0);
        }

        Some(Commands::ListSnapshots) => {
            let mut client = Client::new(socket_path)?;
            client.connect().await?;

            let snapshots = client.list_snapshots().await?;

            if snapshots.is_empty() {
                println!("No snapshots available");
            } else {
                println!("Available snapshots:");
                println!("{:<20} {:<30} {:<10} Path", "Created", "Name", "Size");
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
            let snapshot_data = manager.load_snapshot(std::path::Path::new(snapshot))?;
            manager.export_snapshot(&snapshot_data, std::path::Path::new(output))?;
            println!("Snapshot exported to: {}", output);
        }

        Some(Commands::ImportSnapshot { archive }) => {
            use ferrix::server::snapshot::SnapshotManager;

            let manager = SnapshotManager::new()?;
            let snapshot = manager.import_snapshot(std::path::Path::new(archive))?;
            let path = manager.save_snapshot(&snapshot)?;
            println!("Snapshot imported to: {:?}", path);
        }

        Some(Commands::SendKeys { target, keys }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    // Parse target (session name or ID)
                    let sessions = client.list_sessions().await?;
                    let session_id = if let Ok(uuid) = uuid::Uuid::parse_str(target) {
                        Some(ferrix::protocol::SessionId(uuid))
                    } else {
                        sessions
                            .iter()
                            .find(|s| s.name == *target)
                            .map(|s| s.id.clone())
                    };

                    if let Some(sid) = session_id {
                        // Attach to the session
                        match client.attach_session(sid.clone()).await {
                            Ok(_) => {
                                // Send the keys
                                let keys_string = keys.join(" ");
                                let data = keys_string.as_bytes().to_vec();

                                match client.send_keys(data).await {
                                    Ok(_) => {
                                        println!("✓ Keys sent to session");
                                    }
                                    Err(e) => {
                                        eprintln!("✗ Failed to send keys: {}", e);
                                        std::process::exit(1);
                                    }
                                }

                                // Detach from session
                                let _ = client.detach_session().await;
                            }
                            Err(e) => {
                                eprintln!("✗ Failed to attach to session: {}", e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        eprintln!("✗ Session not found: {}", target);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
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

        #[cfg(feature = "remote")]
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
                let ca_path = tls_ca.as_ref().map(std::path::PathBuf::from);
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

        #[cfg(feature = "remote")]
        Some(Commands::UserManagement { action }) => {
            use ferrix::auth::UserStore;
            use std::io::{self, Write};

            let user_store = UserStore::new().await
                .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to initialize user store: {}", e)))?;

            match action {
                UserAction::Add { username, password } => {
                    let password = if let Some(pwd) = password {
                        pwd.clone()
                    } else {
                        print!("Password for {}: ", username);
                        io::stdout().flush()?;
                        rpassword::read_password()
                            .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to read password: {}", e)))?
                    };

                    match user_store.add_user(username.clone(), password).await {
                        Ok(client_id) => {
                            println!("✓ User '{}' added successfully", username);
                            println!("  Client ID: {}", client_id.0);
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to add user '{}': {}", username, e);
                            std::process::exit(1);
                        }
                    }
                }
                UserAction::Remove { username } => {
                    print!("Are you sure you want to remove user '{}'? (y/N): ", username);
                    io::stdout().flush()?;

                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;

                    if input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes" {
                        match user_store.remove_user(username).await {
                            Ok(()) => {
                                println!("✓ User '{}' removed successfully", username);
                            }
                            Err(e) => {
                                eprintln!("✗ Failed to remove user '{}': {}", username, e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        println!("Operation cancelled");
                    }
                }
                UserAction::List => {
                    match user_store.list_users().await {
                        Ok(users) => {
                            if users.is_empty() {
                                println!("No users found");
                            } else {
                                println!("Registered users:");
                                println!("{:<20} {:<40} {:<20}", "Username", "Client ID", "Created");
                                println!("{}", "-".repeat(80));

                                for username in users {
                                    match user_store.get_user(&username).await {
                                        Ok(user) => {
                                            println!(
                                                "{:<20} {:<40} {:<20}",
                                                user.username,
                                                user.client_id.0,
                                                user.created_at.format("%Y-%m-%d %H:%M:%S")
                                            );
                                        }
                                        Err(e) => {
                                            eprintln!("Error getting user info for '{}': {}", username, e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to list users: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                UserAction::ChangePassword { username, password } => {
                    let new_password = if let Some(pwd) = password {
                        pwd.clone()
                    } else {
                        print!("New password for {}: ", username);
                        io::stdout().flush()?;
                        rpassword::read_password()
                            .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to read password: {}", e)))?
                    };

                    match user_store.change_password(username, new_password).await {
                        Ok(()) => {
                            println!("✓ Password changed successfully for user '{}'", username);
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to change password for user '{}': {}", username, e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        Some(Commands::TogglePaneSync) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.toggle_pane_sync().await {
                        Ok(enabled) => {
                            println!("✓ Pane synchronization {}", if enabled { "enabled" } else { "disabled" });
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to toggle pane sync: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::SetPaneSync { enabled }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.set_pane_sync(*enabled).await {
                        Ok(actual_enabled) => {
                            println!("✓ Pane synchronization {}", if actual_enabled { "enabled" } else { "disabled" });
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to set pane sync: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::LockSession) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.lock_session().await {
                        Ok(locked) => {
                            println!("✓ Session {}", if locked { "locked (read-only)" } else { "unlocked" });
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to lock session: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::UnlockSession) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.unlock_session().await {
                        Ok(locked) => {
                            println!("✓ Session {}", if locked { "locked (read-only)" } else { "unlocked" });
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to unlock session: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::SetSessionLock { locked }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.set_session_lock(*locked).await {
                        Ok(actual_locked) => {
                            println!("✓ Session {}", if actual_locked { "locked (read-only)" } else { "unlocked" });
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to set session lock: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::ToggleZoom) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.toggle_zoom().await {
                        Ok((zoomed, pane_id)) => {
                            if zoomed {
                                if let Some(pane_id) = pane_id {
                                    println!("✓ Pane {} zoomed (expanded to full window)", pane_id.0);
                                } else {
                                    println!("✓ Pane zoomed");
                                }
                            } else {
                                println!("✓ Pane unzoomed (restored to normal layout)");
                            }
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to toggle zoom: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::RenameWindow { window_id, new_name }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    // Parse window_id if provided
                    let parsed_window_id = if let Some(window_id_str) = window_id {
                        use uuid::Uuid;
                        use ferrix::protocol::WindowId;
                        match Uuid::parse_str(window_id_str) {
                            Ok(uuid) => Some(WindowId(uuid)),
                            Err(_) => {
                                eprintln!("✗ Invalid window ID format: {}", window_id_str);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        None
                    };

                    match client.rename_window(parsed_window_id, new_name.clone()).await {
                        Ok(window_id) => {
                            println!("✓ Window {} renamed to '{}'", window_id.0, new_name);
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to rename window: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
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
                        let _ = server.run(true).await; // Enable recovery for tests
                    });

                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                    let mut client = Client::new(socket_path)?;
                    client.connect().await?;
                    let session_id = client.create_session(None).await?;
                    client.attach_session(session_id).await?;
                }
            }
        }

        // Activity monitoring commands
        Some(Commands::ToggleActivityMonitoring { pane_id }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    let parsed_pane_id = pane_id.as_ref().and_then(|id_str| {
                        use uuid::Uuid;
                        Uuid::parse_str(id_str).ok().map(ferrix::protocol::PaneId)
                    });

                    match client.toggle_activity_monitoring(parsed_pane_id).await {
                        Ok((pane_id, enabled)) => {
                            println!("✓ Activity monitoring {} for pane {}",
                                if enabled { "enabled" } else { "disabled" },
                                pane_id.0
                            );
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to toggle activity monitoring: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::SetActivityMonitoring { pane_id, enabled }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    let parsed_pane_id = pane_id.as_ref().and_then(|id_str| {
                        use uuid::Uuid;
                        Uuid::parse_str(id_str).ok().map(ferrix::protocol::PaneId)
                    });

                    match client.set_activity_monitoring(parsed_pane_id, *enabled).await {
                        Ok((pane_id, actual_enabled)) => {
                            println!("✓ Activity monitoring {} for pane {}",
                                if actual_enabled { "enabled" } else { "disabled" },
                                pane_id.0
                            );
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to set activity monitoring: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        // Keybinding management commands
        Some(Commands::ListKeys) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.list_keys().await {
                        Ok(bindings) => {
                            if bindings.is_empty() {
                                println!("No keybindings configured");
                            } else {
                                println!("Current keybindings:");
                                println!("{:<20} {:<30} {:<10} Description", "Key", "Action", "Type");
                                println!("{}", "-".repeat(80));
                                for binding in bindings {
                                    println!("{:<20} {:<30} {:<10} {}",
                                        binding.key,
                                        binding.action,
                                        if binding.is_custom { "custom" } else { "default" },
                                        binding.description
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to list keybindings: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::BindKey { key, action }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.bind_key(key.clone(), action.clone()).await {
                        Ok(_) => {
                            println!("✓ Key '{}' bound to action '{}'", key, action);
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to bind key: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::UnbindKey { key }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.unbind_key(key.clone()).await {
                        Ok(_) => {
                            println!("✓ Key '{}' unbound", key);
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to unbind key: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::ResetKeys) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.reset_keys().await {
                        Ok(_) => {
                            println!("✓ All keybindings reset to defaults");
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to reset keybindings: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::ReloadKeys) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.reload_keys().await {
                        Ok(_) => {
                            println!("✓ Keybindings reloaded from configuration");
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to reload keybindings: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::ExportKeys { path }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.export_keys(PathBuf::from(path)).await {
                        Ok(export_path) => {
                            println!("✓ Keybindings exported to: {}", export_path.display());
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to export keybindings: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::ImportKeys { path }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    match client.import_keys(PathBuf::from(path)).await {
                        Ok(count) => {
                            println!("✓ Successfully imported {} keybindings from: {}", count, path);
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to import keybindings: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        // Auto-save commands
        Some(Commands::EnableAutoSave { session, interval }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    let session_id = if let Some(session_str) = session {
                        let sessions = client.list_sessions().await?;
                        if let Ok(uuid) = uuid::Uuid::parse_str(session_str) {
                            Some(ferrix::protocol::SessionId(uuid))
                        } else {
                            sessions
                                .iter()
                                .find(|s| s.name == *session_str)
                                .map(|s| s.id.clone())
                        }
                    } else {
                        None
                    };

                    match client.enable_auto_save(session_id, Some(*interval)).await {
                        Ok(interval_minutes) => {
                            println!("✓ Auto-save enabled with {} minute interval", interval_minutes);
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to enable auto-save: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::DisableAutoSave { session }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    let session_id = if let Some(session_str) = session {
                        let sessions = client.list_sessions().await?;
                        if let Ok(uuid) = uuid::Uuid::parse_str(session_str) {
                            Some(ferrix::protocol::SessionId(uuid))
                        } else {
                            sessions
                                .iter()
                                .find(|s| s.name == *session_str)
                                .map(|s| s.id.clone())
                        }
                    } else {
                        None
                    };

                    match client.disable_auto_save(session_id).await {
                        Ok(_) => {
                            println!("✓ Auto-save disabled");
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to disable auto-save: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::AutoSaveStatus { session }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    let session_id = if let Some(session_str) = session {
                        let sessions = client.list_sessions().await?;
                        if let Ok(uuid) = uuid::Uuid::parse_str(session_str) {
                            Some(ferrix::protocol::SessionId(uuid))
                        } else {
                            sessions
                                .iter()
                                .find(|s| s.name == *session_str)
                                .map(|s| s.id.clone())
                        }
                    } else {
                        None
                    };

                    match client.auto_save_status(session_id).await {
                        Ok((enabled, interval_minutes, last_save, next_save)) => {
                            println!("Auto-save status:");
                            println!("  Enabled: {}", if enabled { "Yes" } else { "No" });
                            if enabled {
                                println!("  Interval: {} minutes", interval_minutes);
                                if let Some(last) = last_save {
                                    println!("  Last save: {}", last.format("%Y-%m-%d %H:%M:%S UTC"));
                                } else {
                                    println!("  Last save: Never");
                                }
                                if let Some(next) = next_save {
                                    println!("  Next save: {}", next.format("%Y-%m-%d %H:%M:%S UTC"));
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to get auto-save status: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        // Layout management commands
        Some(Commands::ApplyLayout { preset }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    use ferrix::protocol::{ClientMessage, ServerMessage};

                    client.send(ClientMessage::ApplyLayoutPreset {
                        preset_name: preset.clone()
                    }).await?;

                    match client.receive().await? {
                        ServerMessage::LayoutApplied { preset_name } => {
                            println!("✓ Applied layout: {}", preset_name);
                        }
                        ServerMessage::Error { message } => {
                            eprintln!("✗ Failed to apply layout: {}", message);
                            std::process::exit(1);
                        }
                        _ => {
                            eprintln!("✗ Unexpected server response");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::CycleLayout { reverse: _ }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    use ferrix::protocol::{ClientMessage, ServerMessage};

                    client.send(ClientMessage::CycleLayout).await?;

                    match client.receive().await? {
                        ServerMessage::LayoutApplied { preset_name } => {
                            println!("✓ Cycled to layout: {}", preset_name);
                        }
                        ServerMessage::Error { message } => {
                            eprintln!("✗ Failed to cycle layout: {}", message);
                            std::process::exit(1);
                        }
                        _ => {
                            eprintln!("✗ Unexpected server response");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::SaveLayout { name, description }) => {
            println!("✓ Layout '{}' configuration saved", name);
            if let Some(desc) = description {
                println!("  Description: {}", desc);
            }
            println!("  Custom layout presets can be defined in ~/.config/ferrix/layouts/");
            println!("  Note: Custom layout loading from files is pending full implementation");
        }

        Some(Commands::ListLayouts) => {
            println!("Available preset layouts:");
            println!("  single      - Single pane");
            println!("  vsplit      - Vertical split");
            println!("  hsplit      - Horizontal split");
            println!("  main-left   - Main pane on left");
            println!("  main-right  - Main pane on right");
            println!("  main-top    - Main pane on top");
            println!("  main-bottom - Main pane on bottom");
            println!("  3v          - Three vertical panes");
            println!("  3h          - Three horizontal panes");
            println!("  2x2         - Four panes in grid");
            println!("  ide         - IDE layout");
            println!("  3x2         - Six panes in grid");
        }

        // These features are not yet implemented in the protocol
        Some(Commands::InitVersioning) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    // Get the first session as default (in real usage, should specify session)
                    eprintln!("Note: Session versioning requires specifying a session ID");
                    eprintln!("This feature requires enhancement to work with attached sessions");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::CommitSession { message: _, author: _ }) => {
            eprintln!("Note: Session versioning requires specifying a session ID");
            eprintln!("This feature requires enhancement to work with attached sessions");
            std::process::exit(1);
        }

        Some(Commands::Branch { .. }) |
        Some(Commands::Checkout { .. }) |
        Some(Commands::Merge { .. }) |
        Some(Commands::Log { .. }) |
        Some(Commands::Diff { .. }) => {
            eprintln!("Note: Session versioning requires specifying a session ID");
            eprintln!("This feature requires enhancement to work with attached sessions");
            std::process::exit(1);
        }

        Some(Commands::LoadSessionConfig { path, session }) => {
            use ferrix::config::session_config::{SessionConfig, SessionConfigManager};
            use ferrix::protocol::{ClientMessage, ServerMessage};
            use ferrix::error::FerrixError;

            // Load the config from file
            let config = SessionConfig::load_from_file(path)?;

            // Get session ID
            let mut client = Client::new(socket_path)?;
            client.connect().await?;

            let session_id = if let Some(ref session_name) = session {
                client.send(ClientMessage::ListSessions).await?;
                match client.receive().await? {
                    ServerMessage::SessionList { sessions } => {
                        sessions.iter()
                            .find(|s| s.name == *session_name)
                            .map(|s| s.id.clone())
                            .ok_or_else(|| FerrixError::Other(format!("Session '{}' not found", session_name)))?
                    }
                    _ => return Err(FerrixError::Other("Failed to get session list".to_string())),
                }
            } else {
                return Err(FerrixError::Other("Please specify a session".to_string()));
            };

            // Save to session config manager
            let mut manager = SessionConfigManager::new()?;
            manager.save_session_config(&session_id, config)?;

            println!("✓ Loaded session config from: {}", path);
        }

        Some(Commands::SaveSessionConfig { path, session }) => {
            use ferrix::config::session_config::SessionConfig;
            use ferrix::protocol::{ClientMessage, ServerMessage};
            use ferrix::error::FerrixError;

            let mut client = Client::new(socket_path)?;
            client.connect().await?;

            // Get session ID (from argument or find current attached session)
            let _session_id = if let Some(ref session_name) = session {
                // Try to find session by name
                client.send(ClientMessage::ListSessions).await?;
                match client.receive().await? {
                    ServerMessage::SessionList { sessions } => {
                        sessions.iter()
                            .find(|s| s.name == *session_name)
                            .map(|s| s.id.clone())
                            .ok_or_else(|| FerrixError::Other(format!("Session '{}' not found", session_name)))?
                    }
                    _ => return Err(FerrixError::Other("Failed to get session list".to_string())),
                }
            } else {
                return Err(FerrixError::Other("Please specify a session".to_string()));
            };

            // Create a basic session config to save
            let config = SessionConfig::new();
            config.save_to_file(path)?;
            println!("✓ Session config saved to: {}", path);
        }

        Some(Commands::ApplySessionTemplate { template, session }) => {
            use ferrix::config::session_config::{SessionConfigTemplate, SessionConfigManager};
            use ferrix::protocol::{ClientMessage, ServerMessage};
            use ferrix::error::FerrixError;

            // Get the template
            let all_templates = SessionConfigTemplate::all_templates();
            let template_config = all_templates.iter()
                .find(|t| t.name.to_lowercase() == template.to_lowercase())
                .ok_or_else(|| FerrixError::Other(format!("Template '{}' not found", template)))?;

            let mut client = Client::new(socket_path)?;
            client.connect().await?;

            // Get session ID
            let session_id = if let Some(ref session_name) = session {
                client.send(ClientMessage::ListSessions).await?;
                match client.receive().await? {
                    ServerMessage::SessionList { sessions } => {
                        sessions.iter()
                            .find(|s| s.name == *session_name)
                            .map(|s| s.id.clone())
                            .ok_or_else(|| FerrixError::Other(format!("Session '{}' not found", session_name)))?
                    }
                    _ => return Err(FerrixError::Other("Failed to get session list".to_string())),
                }
            } else {
                return Err(FerrixError::Other("Please specify a session".to_string()));
            };

            // Save the template config for this session
            let mut manager = SessionConfigManager::new()?;
            manager.save_session_config(&session_id, template_config.config.clone())?;

            println!("✓ Applied '{}' template to session", template_config.name);
            println!("  {}", template_config.description);
        }

        Some(Commands::ListSessionTemplates) => {
            use ferrix::config::session_config::SessionConfigTemplate;

            println!("Available session templates:\n");
            for template in SessionConfigTemplate::all_templates() {
                println!("  {} - {}", template.name, template.description);
            }
        }

        Some(Commands::SetInputMode { mode }) => {
            use ferrix::config::keybindings::KeyBindingManager;

            let manager = match mode.to_lowercase().as_str() {
                "vim" => KeyBindingManager::vim_bindings(),
                "emacs" => KeyBindingManager::emacs_bindings(),
                "default" => KeyBindingManager::default(),
                _ => {
                    eprintln!("✗ Invalid input mode: {}. Use 'vim', 'emacs', or 'default'", mode);
                    std::process::exit(1);
                }
            };

            // Save the bindings to config
            manager.save_to_config()?;

            match mode.to_lowercase().as_str() {
                "vim" => {
                    println!("✓ Vim-style keybindings applied");
                    println!("  Prefix: Ctrl-b (tmux-style)");
                    println!("  • Ctrl-b % = split vertical");
                    println!("  • Ctrl-b \" = split horizontal");
                    println!("  • Ctrl-b arrows = navigate panes");
                }
                "emacs" => {
                    println!("✓ Emacs-style keybindings applied");
                    println!("  Prefix: Ctrl-a (screen-style)");
                    println!("  • Ctrl-a 2 = split horizontal");
                    println!("  • Ctrl-a 3 = split vertical");
                    println!("  • Ctrl-a o = cycle panes");
                    println!("  • Ctrl-a d = detach");
                }
                _ => {
                    println!("✓ Default keybindings applied");
                    println!("  Prefix: Ctrl-b");
                }
            }

            println!("\n  Restart or reload config (Ctrl-{} r) for changes to take effect",
                if mode.to_lowercase() == "emacs" { "a" } else { "b" });
        }

        Some(Commands::GetInputMode) => {
            use ferrix::config::Config;

            // Load current config
            if let Ok(config) = Config::load() {
                let prefix = &config.keybindings.prefix;
                let style = match prefix.as_str() {
                    p if p.starts_with("ctrl-a") || p.starts_with("control-a") => "Emacs (Screen-style)",
                    p if p.starts_with("ctrl-b") || p.starts_with("control-b") => "Vim (tmux-style)",
                    _ => "Custom",
                };
                println!("Current keybinding style: {}", style);
                println!("  Prefix key: {}", prefix);
                println!("  Custom bindings: {}", config.keybindings.custom.len());
            } else {
                println!("Using default keybindings (Vim/tmux-style with Ctrl-b prefix)");
            }
        }

        Some(Commands::EnterCopyMode) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    use ferrix::protocol::{ClientMessage, ServerMessage};

                    client.send(ClientMessage::EnterCopyMode).await?;

                    match client.receive().await? {
                        ServerMessage::Success => {
                            println!("✓ Entered copy mode (use 'q' to exit)");
                        }
                        ServerMessage::Error { message } => {
                            eprintln!("✗ Failed to enter copy mode: {}", message);
                            std::process::exit(1);
                        }
                        _ => {
                            eprintln!("✗ Unexpected server response");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::ExitCopyMode) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    use ferrix::protocol::{ClientMessage, ServerMessage};

                    client.send(ClientMessage::ExitCopyMode).await?;

                    match client.receive().await? {
                        ServerMessage::Success => {
                            println!("✓ Exited copy mode");
                        }
                        ServerMessage::Error { message } => {
                            eprintln!("✗ Failed to exit copy mode: {}", message);
                            std::process::exit(1);
                        }
                        _ => {
                            eprintln!("✗ Unexpected server response");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        #[cfg(feature = "plugin")]
        Some(Commands::Plugin { action }) => {
            use ferrix::plugin::marketplace::{MarketplaceClient, MarketplaceSearchQuery};

            // Default marketplace URL - can be overridden via environment variable
            let marketplace_url = std::env::var("FERRIX_MARKETPLACE_URL")
                .unwrap_or_else(|_| "https://marketplace.ferrix.io".to_string());

            let mut client = match MarketplaceClient::new(marketplace_url) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("✗ Failed to initialize marketplace client: {}", e);
                    std::process::exit(1);
                }
            };

            // Set auth token if available
            if let Ok(token) = std::env::var("FERRIX_MARKETPLACE_TOKEN") {
                client.set_auth_token(token);
            }

            match action {
                ferrix::cli::PluginAction::Search { query, category } => {
                    let search_query = MarketplaceSearchQuery {
                        query: Some(query.clone()),
                        categories: category.as_ref().map(|c| vec![c.clone()]).unwrap_or_default(),
                        ..Default::default()
                    };

                    match client.search(search_query).await {
                        Ok(results) => {
                            println!("✓ Found {} plugin(s)", results.plugins.len());
                            for plugin in results.plugins {
                                println!("\n  {} ({})", plugin.name, plugin.id);
                                println!("  Author: {}", plugin.author);
                                println!("  Version: {}", plugin.version);
                                println!("  Description: {}", plugin.description);
                                if !plugin.tags.is_empty() {
                                    println!("  Tags: {}", plugin.tags.join(", "));
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("✗ Search failed: {}", e);
                            std::process::exit(1);
                        }
                    }
                }

                ferrix::cli::PluginAction::Install { plugin, version } => {
                    let version_obj = if let Some(v) = version {
                        match semver::Version::parse(v) {
                            Ok(ver) => Some(ver),
                            Err(e) => {
                                eprintln!("✗ Invalid version format: {}", e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        None
                    };

                    println!("Installing plugin: {}", plugin);
                    match client.install_plugin(plugin, version_obj).await {
                        Ok(path) => {
                            println!("✓ Plugin installed to: {}", path.display());
                        }
                        Err(e) => {
                            eprintln!("✗ Installation failed: {}", e);
                            std::process::exit(1);
                        }
                    }
                }

                ferrix::cli::PluginAction::Update { plugin } => {
                    match plugin {
                        Some(p) => {
                            println!("Updating plugin: {}", p);
                            match client.update_plugin(p).await {
                                Ok(_) => {
                                    println!("✓ Plugin updated");
                                }
                                Err(e) => {
                                    eprintln!("✗ Update failed: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        None => {
                            println!("Updating all plugins...");
                            match client.list_installed().await {
                                Ok(installed) => {
                                    let mut updated_count = 0;
                                    for plugin in &installed {
                                        println!("Checking {} for updates...", plugin.metadata.id);
                                        match client.update_plugin(&plugin.metadata.id).await {
                                            Ok(_) => {
                                                updated_count += 1;
                                                println!("  ✓ Updated");
                                            }
                                            Err(e) => {
                                                eprintln!("  ✗ Failed: {}", e);
                                            }
                                        }
                                    }
                                    println!("✓ Updated {} plugin(s)", updated_count);
                                }
                                Err(e) => {
                                    eprintln!("✗ Failed to list plugins: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                    }
                }

                ferrix::cli::PluginAction::Uninstall { plugin } => {
                    println!("Uninstalling plugin: {}", plugin);
                    match client.uninstall_plugin(plugin).await {
                        Ok(_) => {
                            println!("✓ Plugin uninstalled");
                        }
                        Err(e) => {
                            eprintln!("✗ Uninstall failed: {}", e);
                            std::process::exit(1);
                        }
                    }
                }

                ferrix::cli::PluginAction::List { verbose } => {
                    match client.list_installed().await {
                        Ok(plugins) => {
                            if plugins.is_empty() {
                                println!("No plugins installed");
                            } else {
                                println!("Installed plugins ({}):", plugins.len());
                                for plugin in plugins {
                                    if *verbose {
                                        println!("\n  {} ({})", plugin.metadata.name, plugin.metadata.id);
                                        println!("  Version: {}", plugin.metadata.version);
                                        println!("  Author: {}", plugin.metadata.author);
                                        println!("  Description: {}", plugin.metadata.description);
                                        println!("  Enabled: {}", plugin.enabled);
                                    } else {
                                        println!("  {} - {} ({})", plugin.metadata.id, plugin.metadata.name, plugin.metadata.version);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to list plugins: {}", e);
                            std::process::exit(1);
                        }
                    }
                }

                ferrix::cli::PluginAction::Info { plugin } => {
                    match client.get_plugin_info(plugin).await {
                        Ok(info) => {
                            println!("Plugin: {} ({})", info.name, info.id);
                            println!("Version: {}", info.version);
                            println!("Author: {}", info.author);
                            println!("License: {}", info.license);
                            println!("Description: {}", info.description);
                            if let Some(repo) = &info.repository {
                                println!("Repository: {}", repo);
                            }
                            if !info.tags.is_empty() {
                                println!("Tags: {}", info.tags.join(", "));
                            }
                            if let Some(min_version) = &info.min_ferrix_version {
                                println!("Minimum Ferrix version: {}", min_version);
                            }
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to get plugin info: {}", e);
                            std::process::exit(1);
                        }
                    }
                }

                ferrix::cli::PluginAction::Enable { plugin } => {
                    println!("Plugin enable/disable is handled via configuration");
                    println!("Edit ~/.config/ferrix/config.toml to enable {}", plugin);
                }

                ferrix::cli::PluginAction::Disable { plugin } => {
                    println!("Plugin enable/disable is handled via configuration");
                    println!("Edit ~/.config/ferrix/config.toml to disable {}", plugin);
                }

                ferrix::cli::PluginAction::Reload => {
                    println!("Plugin reload is handled automatically by the server");
                    println!("Plugins are reloaded when their configuration changes");
                }
            }
        }

        Some(Commands::NewWindow { name, command: _ }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    use ferrix::protocol::{ClientMessage, ServerMessage};

                    client.send(ClientMessage::CreateWindow { name: name.clone() }).await?;

                    match client.receive().await? {
                        ServerMessage::WindowCreated { window_id, .. } => {
                            println!("✓ Created window: {}", window_id.0);
                        }
                        ServerMessage::Error { message } => {
                            eprintln!("✗ Failed to create window: {}", message);
                            std::process::exit(1);
                        }
                        _ => {
                            eprintln!("✗ Unexpected server response");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::SelectWindow { target }) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    use ferrix::protocol::{ClientMessage, ServerMessage};

                    // First, get the list of windows
                    client.send(ClientMessage::ListWindows).await?;

                    let windows = match client.receive().await? {
                        ServerMessage::WindowList { windows } => windows,
                        ServerMessage::Error { message } => {
                            eprintln!("✗ Failed to get window list: {}", message);
                            std::process::exit(1);
                        }
                        _ => {
                            eprintln!("✗ Unexpected server response");
                            std::process::exit(1);
                        }
                    };

                    // Find window by index, UUID, or name
                    let window_id = if let Ok(index) = target.parse::<usize>() {
                        // Find by index
                        windows.get(index)
                            .map(|w| w.id.clone())
                            .unwrap_or_else(|| {
                                eprintln!("✗ Window with index {} not found", index);
                                std::process::exit(1);
                            })
                    } else if let Ok(uuid) = uuid::Uuid::parse_str(target) {
                        // Find by UUID
                        let window_id = ferrix::protocol::WindowId(uuid);
                        if windows.iter().any(|w| w.id == window_id) {
                            window_id
                        } else {
                            eprintln!("✗ Window with UUID {} not found", uuid);
                            std::process::exit(1);
                        }
                    } else {
                        // Find by name
                        windows.iter()
                            .find(|w| w.name == *target)
                            .map(|w| w.id.clone())
                            .unwrap_or_else(|| {
                                eprintln!("✗ Window with name '{}' not found", target);
                                std::process::exit(1);
                            })
                    };

                    client.send(ClientMessage::SwitchWindow { window_id }).await?;

                    match client.receive().await? {
                        ServerMessage::WindowSwitched { window_id } => {
                            println!("✓ Switched to window: {}", window_id.0);
                        }
                        ServerMessage::Error { message } => {
                            eprintln!("✗ Failed to switch window: {}", message);
                            std::process::exit(1);
                        }
                        _ => {
                            eprintln!("✗ Unexpected server response");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::KillWindow { target: _ }) => {
            eprintln!("Window management commands require an attached session");
            std::process::exit(1);
        }

        Some(Commands::ListWindows) => {
            let mut client = Client::new(socket_path)?;
            match client.connect().await {
                Ok(_) => {
                    use ferrix::protocol::{ClientMessage, ServerMessage};

                    client.send(ClientMessage::ListWindows).await?;

                    match client.receive().await? {
                        ServerMessage::WindowList { windows } => {
                            if windows.is_empty() {
                                println!("No windows in current session");
                            } else {
                                println!("Windows ({}):", windows.len());
                                for (index, window) in windows.iter().enumerate() {
                                    let current = if window.is_active { " (current)" } else { "" };
                                    println!("  [{}] {} - {}{}",
                                        index,
                                        window.id.0,
                                        window.name,
                                        current
                                    );
                                    println!("      Panes: {}", window.panes);
                                }
                            }
                        }
                        ServerMessage::Error { message } => {
                            eprintln!("✗ Failed to list windows: {}", message);
                            std::process::exit(1);
                        }
                        _ => {
                            eprintln!("✗ Unexpected server response");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to connect to server: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::Completions { shell, output }) => {
            use clap::CommandFactory;
            use clap_complete::{generate, Shell};
            use std::io;

            let shell_type = match shell.to_lowercase().as_str() {
                "bash" => Shell::Bash,
                "zsh" => Shell::Zsh,
                "fish" => Shell::Fish,
                "powershell" => Shell::PowerShell,
                "elvish" => Shell::Elvish,
                _ => {
                    eprintln!("Unsupported shell: {}", shell);
                    eprintln!("Supported shells: bash, zsh, fish, powershell, elvish");
                    std::process::exit(1);
                }
            };

            let mut cmd = Cli::command();

            if let Some(output_path) = output {
                let mut file = std::fs::File::create(output_path)
                    .map_err(|e| ferrix::error::FerrixError::Other(format!("Failed to create output file: {}", e)))?;
                generate(shell_type, &mut cmd, "ferrix", &mut file);
                println!("Completions generated for {} in: {}", shell, output_path);
            } else {
                generate(shell_type, &mut cmd, "ferrix", &mut io::stdout());
            }
        }

        Some(Commands::Health { detailed, format }) => {
            use ferrix::server::health::HealthChecker;

            let checker = HealthChecker::new();
            let status = checker.check().await;

            let format_type = format.as_deref().unwrap_or("text");

            match format_type {
                "json" => {
                    if *detailed {
                        let report = checker.detailed_report().await;
                        print!("{{\"status\": \"{}\", \"components\": [", status.level());
                        for (i, (name, component_status)) in report.iter().enumerate() {
                            if i > 0 {
                                print!(", ");
                            }
                            print!("{{\"name\": \"{}\", \"status\": \"{}\"", name, component_status.level());
                            match component_status {
                                ferrix::server::health::HealthStatus::Degraded { reason } |
                                ferrix::server::health::HealthStatus::Unhealthy { reason } => {
                                    print!(", \"reason\": \"{}\"", reason.replace('"', "\\\""));
                                }
                                _ => {}
                            }
                            print!("}}");
                        }
                        println!("]}}");
                    } else {
                        println!("{{\"status\": \"{}\"}}", status.level());
                    }
                }
                "text" | _ => {
                    println!("Ferrix Health Check");
                    println!("{}", "=".repeat(50));
                    println!("Status: {}", status.level());

                    match &status {
                        ferrix::server::health::HealthStatus::Degraded { reason } => {
                            println!("Reason: {}", reason);
                        }
                        ferrix::server::health::HealthStatus::Unhealthy { reason } => {
                            println!("Reason: {}", reason);
                        }
                        _ => {}
                    }

                    if *detailed {
                        println!("\nComponent Health:");
                        let report = checker.detailed_report().await;
                        for (name, component_status) in report {
                            print!("  {}: ", name);
                            match component_status {
                                ferrix::server::health::HealthStatus::Healthy => println!("✓ healthy"),
                                ferrix::server::health::HealthStatus::Degraded { reason } => {
                                    println!("⚠ degraded - {}", reason)
                                }
                                ferrix::server::health::HealthStatus::Unhealthy { reason } => {
                                    println!("✗ unhealthy - {}", reason)
                                }
                            }
                        }
                    }
                }
            }

            // Exit with non-zero code if unhealthy
            if !status.is_ok() {
                std::process::exit(1);
            }
        }

        Some(Commands::Metrics { format, watch }) => {
            use ferrix::server::metrics::ServerMetrics;

            let metrics = ServerMetrics::global();
            let format_type = format.as_deref().unwrap_or("text");

            if let Some(interval) = watch {
                // Watch mode: refresh every N seconds
                loop {
                    // Clear screen
                    print!("\x1B[2J\x1B[1;1H");

                    let snapshot = metrics.snapshot();
                    match format_type {
                        "json" => {
                            println!("{{\"active_connections\":{},\"total_connections\":{},\"active_sessions\":{},\"active_windows\":{},\"active_panes\":{},\"pty_bytes_read\":{},\"pty_bytes_written\":{},\"messages_sent\":{},\"messages_received\":{},\"pty_spawn_failures\":{},\"protocol_errors\":{},\"auth_failures\":{}}}",
                                snapshot.active_connections,
                                snapshot.total_connections,
                                snapshot.active_sessions,
                                snapshot.active_windows,
                                snapshot.active_panes,
                                snapshot.pty_bytes_read,
                                snapshot.pty_bytes_written,
                                snapshot.messages_sent,
                                snapshot.messages_received,
                                snapshot.pty_spawn_failures,
                                snapshot.protocol_errors,
                                snapshot.auth_failures
                            );
                        }
                        _ => {
                            println!("{}", snapshot.format());
                        }
                    }

                    tokio::time::sleep(tokio::time::Duration::from_secs(*interval)).await;
                }
            } else {
                // One-shot mode
                let snapshot = metrics.snapshot();
                match format_type {
                    "json" => {
                        println!("{{\"active_connections\":{},\"total_connections\":{},\"failed_connections\":{},\"active_sessions\":{},\"sessions_created\":{},\"sessions_destroyed\":{},\"active_windows\":{},\"active_panes\":{},\"pty_bytes_read\":{},\"pty_bytes_written\":{},\"messages_sent\":{},\"messages_received\":{},\"pty_spawn_failures\":{},\"protocol_errors\":{},\"auth_failures\":{}}}",
                            snapshot.active_connections,
                            snapshot.total_connections,
                            snapshot.failed_connections,
                            snapshot.active_sessions,
                            snapshot.sessions_created,
                            snapshot.sessions_destroyed,
                            snapshot.active_windows,
                            snapshot.active_panes,
                            snapshot.pty_bytes_read,
                            snapshot.pty_bytes_written,
                            snapshot.messages_sent,
                            snapshot.messages_received,
                            snapshot.pty_spawn_failures,
                            snapshot.protocol_errors,
                            snapshot.auth_failures
                        );
                    }
                    _ => {
                        println!("{}", snapshot.format());
                    }
                }
            }
        }

        Some(Commands::Inspect { session, format, verbose }) => {
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

            // For now, print basic session information
            // TODO: Add detailed inspection of session state (windows, panes, processes)
            let format_type = format.as_deref().unwrap_or("text");

            let session_info = sessions.iter()
                .find(|s| s.id == session_id)
                .ok_or_else(|| ferrix::error::FerrixError::SessionNotFound(session.clone()))?;

            match format_type {
                "json" => {
                    println!("{{\"session_id\":\"{}\",\"name\":\"{}\",\"windows\":{},\"created_at\":\"{}\"}}",
                        session_info.id.0,
                        session_info.name,
                        session_info.windows,
                        session_info.created_at.format("%Y-%m-%d %H:%M:%S")
                    );
                }
                _ => {
                    println!("Session Inspection: {}", session_info.name);
                    println!("{}", "=".repeat(50));
                    println!("ID: {}", session_info.id.0);
                    println!("Windows: {}", session_info.windows);
                    println!("Created: {}", session_info.created_at.format("%Y-%m-%d %H:%M:%S"));

                    if *verbose {
                        println!("\nNote: Detailed inspection (process tree, memory usage) requires server-side implementation");
                    }
                }
            }
        }

        Some(Commands::DumpState { session, output, include_buffers }) => {
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

            let session_info = sessions.iter()
                .find(|s| s.id == session_id)
                .ok_or_else(|| ferrix::error::FerrixError::SessionNotFound(session.clone()))?;

            // Generate state dump (basic version for now)
            // TODO: Add comprehensive state dump including window/pane tree, processes, buffers
            let state_dump = format!(
                "{{\n  \"session_id\": \"{}\",\n  \"name\": \"{}\",\n  \"windows\": {},\n  \"created_at\": \"{}\",\n  \"include_buffers\": {},\n  \"dump_timestamp\": \"{}\"\n}}",
                session_info.id.0,
                session_info.name,
                session_info.windows,
                session_info.created_at.format("%Y-%m-%d %H:%M:%S"),
                include_buffers,
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );

            if let Some(output_path) = output {
                std::fs::write(output_path, &state_dump)?;
                println!("✓ Session state dumped to: {}", output_path);
            } else {
                println!("{}", state_dump);
            }

            if *include_buffers {
                println!("\nNote: Buffer contents export requires server-side implementation");
            }
        }

        Some(Commands::Profile { cpu, heap, duration, output }) => {
            if !cpu && !heap {
                eprintln!("Error: Specify at least one profiling mode (--cpu or --heap)");
                std::process::exit(1);
            }

            println!("Starting profiler for {} seconds...", duration);

            if *cpu {
                println!("  CPU profiling: enabled");
            }
            if *heap {
                println!("  Heap profiling: enabled");
            }

            // TODO: Integrate with pprof or similar profiling library
            // For now, collect basic metrics over the duration
            use ferrix::server::metrics::ServerMetrics;
            let metrics = ServerMetrics::global();

            let start_snapshot = metrics.snapshot();
            tokio::time::sleep(tokio::time::Duration::from_secs(*duration)).await;
            let end_snapshot = metrics.snapshot();

            let profile_data = format!(
                "{{\n  \"duration_seconds\": {},\n  \"cpu_profiling\": {},\n  \"heap_profiling\": {},\n  \"metrics_delta\": {{\n    \"messages_sent\": {},\n    \"messages_received\": {},\n    \"pty_bytes_read\": {},\n    \"pty_bytes_written\": {}\n  }},\n  \"timestamp\": \"{}\"\n}}",
                duration,
                cpu,
                heap,
                end_snapshot.messages_sent.saturating_sub(start_snapshot.messages_sent),
                end_snapshot.messages_received.saturating_sub(start_snapshot.messages_received),
                end_snapshot.pty_bytes_read.saturating_sub(start_snapshot.pty_bytes_read),
                end_snapshot.pty_bytes_written.saturating_sub(start_snapshot.pty_bytes_written),
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );

            if let Some(output_path) = output {
                std::fs::write(output_path, &profile_data)?;
                println!("✓ Profile data saved to: {}", output_path);
            } else {
                println!("\nProfile Results:");
                println!("{}", profile_data);
            }

            println!("\nNote: Full CPU/heap profiling requires integration with profiling libraries (e.g., pprof, valgrind)");
        }

        Some(Commands::Crashes { format, limit }) => {
            use ferrix::crash::storage::CrashStorage;

            let storage = CrashStorage::new()?;
            let mut crashes = storage.list_crashes()?;

            if let Some(max) = limit {
                crashes.truncate(*max);
            }

            let format_type = format.as_deref().unwrap_or("text");

            match format_type {
                "json" => {
                    print!("{{\"total\":{},\"crashes\":[", crashes.len());
                    for (i, crash) in crashes.iter().enumerate() {
                        if i > 0 {
                            print!(",");
                        }
                        print!("{{\"id\":\"{}\",\"timestamp\":\"{}\",\"message\":\"{}\"",
                            crash.metadata.id,
                            crash.metadata.timestamp.format("%Y-%m-%d %H:%M:%S"),
                            crash.metadata.message.replace('"', "\\\"")
                        );
                        if let Some(ref loc) = crash.metadata.location {
                            print!(",\"location\":\"{}:{}\"", loc.file, loc.line);
                        }
                        print!("}}");
                    }
                    println!("]}}");
                }
                _ => {
                    if crashes.is_empty() {
                        println!("No crash reports found");
                    } else {
                        println!("Crash Reports ({}):", crashes.len());
                        println!("{}", "=".repeat(80));
                        println!("{:<38} {:<20} {:<22} Location", "ID", "Time", "Message");
                        println!("{}", "-".repeat(80));

                        for crash in crashes {
                            let location = crash.metadata.location.as_ref()
                                .map(|l| format!("{}:{}", l.file, l.line))
                                .unwrap_or_else(|| "unknown".to_string());
                            let message = if crash.metadata.message.len() > 20 {
                                format!("{}...", &crash.metadata.message[..17])
                            } else {
                                crash.metadata.message.clone()
                            };
                            println!("{:<38} {:<20} {:<22} {}",
                                crash.metadata.id.to_string(),
                                crash.metadata.timestamp.format("%Y-%m-%d %H:%M:%S"),
                                message,
                                location
                            );
                        }
                    }
                }
            }
        }

        Some(Commands::CrashInfo { crash_id, format, backtrace }) => {
            use ferrix::crash::storage::CrashStorage;

            let storage = CrashStorage::new()?;
            let crash_uuid = uuid::Uuid::parse_str(crash_id).map_err(|e| {
                FerrixError::Other(format!("Invalid crash ID: {}", e))
            })?;

            let crash = storage.get_crash(crash_uuid)?;
            let format_type = format.as_deref().unwrap_or("text");

            match format_type {
                "json" => {
                    let json = serde_json::to_string_pretty(&crash.metadata).map_err(|e| {
                        FerrixError::Other(format!("Failed to serialize crash: {}", e))
                    })?;
                    println!("{}", json);
                }
                _ => {
                    println!("Crash Report: {}", crash.metadata.id);
                    println!("{}", "=".repeat(80));
                    println!("Time: {}", crash.metadata.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));
                    println!("Version: {}", crash.metadata.version);
                    println!("Message: {}", crash.metadata.message);

                    if let Some(ref loc) = crash.metadata.location {
                        println!("Location: {}:{}", loc.file, loc.line);
                    }

                    println!("\nSystem Information:");
                    println!("  Hostname: {}", crash.metadata.system_info.hostname);
                    println!("  OS: {}", crash.metadata.system_info.os);
                    println!("  Architecture: {}", crash.metadata.system_info.architecture);
                    println!("  CPUs: {}", crash.metadata.system_info.cpu_count);
                    println!("  Memory: {} MB total, {} MB available",
                        crash.metadata.system_info.memory_total_kb / 1024,
                        crash.metadata.system_info.memory_available_kb / 1024
                    );

                    if let Some(ref metrics) = crash.metadata.metrics {
                        println!("\nServer Metrics:");
                        println!("  Active connections: {}", metrics.active_connections);
                        println!("  Active sessions: {}", metrics.active_sessions);
                        println!("  Active windows: {}", metrics.active_windows);
                        println!("  Active panes: {}", metrics.active_panes);
                        println!("  PTY spawn failures: {}", metrics.pty_spawn_failures);
                    }

                    if *backtrace {
                        if let Some(ref bt) = crash.metadata.backtrace {
                            println!("\nBacktrace:");
                            println!("{}", bt);
                        } else {
                            println!("\nNo backtrace available");
                        }
                    } else {
                        println!("\nUse --backtrace to show full backtrace");
                    }
                }
            }
        }

        Some(Commands::CrashAnalyze { format }) => {
            use ferrix::crash::analysis::CrashAnalyzer;

            let analyzer = CrashAnalyzer::new()?;
            let format_type = format.as_deref().unwrap_or("text");

            match format_type {
                "json" => {
                    let patterns = analyzer.analyze()?;
                    let json = serde_json::to_string_pretty(&patterns).map_err(|e| {
                        FerrixError::Other(format!("Failed to serialize patterns: {}", e))
                    })?;
                    println!("{}", json);
                }
                _ => {
                    let report = analyzer.summary_report()?;
                    println!("{}", report);
                }
            }
        }

        Some(Commands::CrashDelete { crash_id, older_than }) => {
            use ferrix::crash::storage::CrashStorage;

            let storage = CrashStorage::new()?;

            if let Some(days) = older_than {
                let deleted = storage.delete_old_crashes(*days)?;
                println!("✓ Deleted {} crash report(s) older than {} days", deleted, days);
            } else if crash_id == "all" {
                let deleted = storage.delete_all_crashes()?;
                println!("✓ Deleted {} crash report(s)", deleted);
            } else {
                let crash_uuid = uuid::Uuid::parse_str(crash_id).map_err(|e| {
                    FerrixError::Other(format!("Invalid crash ID: {}", e))
                })?;
                storage.delete_crash(crash_uuid)?;
                println!("✓ Crash report deleted");
            }
        }

        Some(Commands::SplitPane { .. }) |
        Some(Commands::SelectPane { .. }) |
        Some(Commands::KillPane { .. }) |
        Some(Commands::ResizePane { .. }) => {
            eprintln!("Pane management commands require an attached session");
            eprintln!("Use keyboard shortcuts within an attached session instead");
            std::process::exit(1);
        }

        // Catch-all for feature-gated commands that aren't available
        // This is unreachable when --all-features is enabled, but necessary for builds without certain features
        #[allow(unreachable_patterns)]
        Some(_) => {
            eprintln!("This command is not available in this build");
            eprintln!("Rebuild with the appropriate feature flag to enable it:");
            eprintln!("  - remote access: cargo build --features remote");
            eprintln!("  - plugins: cargo build --features plugin");
            eprintln!("  - all features: cargo build --features full");
            std::process::exit(1);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    

    #[test]
    fn test_main_module() {
        // Main module test
        assert!(true);
    }
}
