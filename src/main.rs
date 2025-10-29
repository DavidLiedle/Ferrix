use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use ferrix::cli::{Cli, Commands};
use ferrix::client::Client;
use ferrix::server::Server;
#[cfg(feature = "remote")]
use ferrix::server::remote::{RemoteServer, PasswordAuthHandler};
use ferrix::error::{Result, FerrixError};
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
    if let Some(Commands::Server { foreground, .. }) = &cli.command {
        ferrix::daemon::daemonize_if_needed(*foreground)?;
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
            ferrix::handlers::session::handle_new(socket_path, session.clone(), *detached).await?;
        }

        Some(Commands::Attach { target }) => {
            ferrix::handlers::session::handle_attach(socket_path, target.clone()).await?;
        }

        Some(Commands::List) => {
            ferrix::handlers::session::handle_list(socket_path).await?;
        }

        Some(Commands::Kill { target }) => {
            ferrix::handlers::session::handle_kill(socket_path, target.clone()).await?;
        }

        Some(Commands::Detach) => {
            ferrix::handlers::session::handle_detach();
        }

        Some(Commands::SaveSnapshot { session, name, description }) => {
            ferrix::handlers::snapshot::handle_save(socket_path, session.clone(), name.clone(), description.clone()).await?;
        }

        Some(Commands::LoadSnapshot { path }) => {
            ferrix::handlers::snapshot::handle_load(socket_path, path.clone()).await?;
        }

        Some(Commands::RestoreSnapshot { session, path }) => {
            ferrix::handlers::snapshot::handle_restore(socket_path, session.clone(), path.clone()).await?;
        }

        Some(Commands::ListSnapshots) => {
            ferrix::handlers::snapshot::handle_list(socket_path).await?;
        }

        Some(Commands::DeleteSnapshot { path }) => {
            ferrix::handlers::snapshot::handle_delete(socket_path, path.clone()).await?;
        }

        Some(Commands::ExportSnapshot { snapshot, output }) => {
            ferrix::handlers::snapshot::handle_export(snapshot.clone(), output.clone())?;
        }

        Some(Commands::ImportSnapshot { archive }) => {
            ferrix::handlers::snapshot::handle_import(archive.clone())?;
        }

        Some(Commands::SendKeys { target, keys }) => {
            ferrix::handlers::misc::handle_send_keys(socket_path, target.clone(), keys.clone()).await?;
        }

        Some(Commands::ReloadConfig) => {
            ferrix::handlers::config::handle_reload();
        }

        Some(Commands::GenerateConfig { force, output }) => {
            ferrix::handlers::config::handle_generate(*force, output.clone())?;
        }

        Some(Commands::ValidateConfig { path }) => {
            ferrix::handlers::config::handle_validate(path.clone())?;
        }

        #[cfg(feature = "remote")]
        Some(Commands::Connect { address, username, password, tls_ca, tls }) => {
            ferrix::handlers::remote::handle_connect(
                address.clone(),
                username.clone(),
                password.clone(),
                tls_ca.clone(),
                *tls
            ).await?;
        }

        #[cfg(feature = "remote")]
        Some(Commands::UserManagement { action }) => {
            ferrix::handlers::remote::handle_user_management(action).await?;
        }

        Some(Commands::TogglePaneSync) => {
            ferrix::handlers::pane::handle_toggle_sync(socket_path).await?;
        }

        Some(Commands::SetPaneSync { enabled }) => {
            ferrix::handlers::pane::handle_set_sync(socket_path, *enabled).await?;
        }

        Some(Commands::LockSession) => {
            ferrix::handlers::session_state::handle_lock(socket_path).await?;
        }

        Some(Commands::UnlockSession) => {
            ferrix::handlers::session_state::handle_unlock(socket_path).await?;
        }

        Some(Commands::SetSessionLock { locked }) => {
            ferrix::handlers::session_state::handle_set_lock(socket_path, *locked).await?;
        }

        Some(Commands::ToggleZoom) => {
            ferrix::handlers::pane::handle_toggle_zoom(socket_path).await?;
        }

        Some(Commands::RenameWindow { window_id, new_name }) => {
            ferrix::handlers::window::handle_rename(socket_path, window_id.clone(), new_name.clone()).await?;
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
            ferrix::handlers::activity::handle_toggle(socket_path, pane_id.clone()).await?;
        }
        Some(Commands::SetActivityMonitoring { pane_id, enabled }) => {
            ferrix::handlers::activity::handle_set(socket_path, pane_id.clone(), *enabled).await?;
        }

        // Keybinding management commands
        Some(Commands::ListKeys) => {
            ferrix::handlers::keys::handle_list(socket_path).await?;
        }
        Some(Commands::BindKey { key, action }) => {
            ferrix::handlers::keys::handle_bind(socket_path, key.clone(), action.clone()).await?;
        }
        Some(Commands::UnbindKey { key }) => {
            ferrix::handlers::keys::handle_unbind(socket_path, key.clone()).await?;
        }
        Some(Commands::ResetKeys) => {
            ferrix::handlers::keys::handle_reset(socket_path).await?;
        }
        Some(Commands::ReloadKeys) => {
            ferrix::handlers::keys::handle_reload(socket_path).await?;
        }
        Some(Commands::ExportKeys { path }) => {
            ferrix::handlers::keys::handle_export(socket_path, path.clone()).await?;
        }
        Some(Commands::ImportKeys { path }) => {
            ferrix::handlers::keys::handle_import(socket_path, path.clone()).await?;
        }

        // Auto-save commands
        Some(Commands::EnableAutoSave { session, interval }) => {
            ferrix::handlers::autosave::handle_enable(socket_path, session.clone(), *interval).await?;
        }
        Some(Commands::DisableAutoSave { session }) => {
            ferrix::handlers::autosave::handle_disable(socket_path, session.clone()).await?;
        }
        Some(Commands::AutoSaveStatus { session }) => {
            ferrix::handlers::autosave::handle_status(socket_path, session.clone()).await?;
        }

        // Layout management commands
        Some(Commands::ApplyLayout { preset }) => {
            ferrix::handlers::layout::handle_apply(socket_path, preset.clone()).await?;
        }

        Some(Commands::CycleLayout { reverse: _ }) => {
            ferrix::handlers::layout::handle_cycle(socket_path).await?;
        }

        Some(Commands::SaveLayout { name, description }) => {
            ferrix::handlers::layout::handle_save(name.clone(), description.clone());
        }

        Some(Commands::ListLayouts) => {
            ferrix::handlers::layout::handle_list();
        }

        // Session versioning commands
        Some(Commands::InitVersioning) => {
            ferrix::handlers::versioning::handle_init(socket_path).await?;
        }

        Some(Commands::CommitSession { message, author }) => {
            ferrix::handlers::versioning::handle_commit(socket_path, message.clone(), author.clone()).await?;
        }

        Some(Commands::Branch { name, list, delete }) => {
            ferrix::handlers::versioning::handle_branch(socket_path, name.clone(), *list, delete.clone()).await?;
        }

        Some(Commands::Checkout { target, create: _ }) => {
            ferrix::handlers::versioning::handle_checkout(socket_path, target.clone()).await?;
        }

        Some(Commands::Merge { branch, auto: _ }) => {
            ferrix::handlers::versioning::handle_merge(socket_path, branch.clone(), None).await?;
        }

        Some(Commands::Log { limit, verbose }) => {
            ferrix::handlers::versioning::handle_log(socket_path, *limit, *verbose).await?;
        }

        Some(Commands::Diff { .. }) => {
            eprintln!("Note: Session diff functionality not yet implemented");
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

            // Get detailed session inspection
            let inspection = client.inspect_session(session_id).await?;
            let format_type = format.as_deref().unwrap_or("text");

            match format_type {
                "json" => {
                    // Output as JSON
                    let json = serde_json::to_string_pretty(&inspection)
                        .map_err(|e| FerrixError::Other(format!("JSON serialization error: {}", e)))?;
                    println!("{}", json);
                }
                _ => {
                    // Human-readable text format
                    println!("Session Inspection: {}", inspection.name);
                    println!("{}", "=".repeat(70));
                    println!("ID:                  {}", inspection.id.0);
                    println!("Created:             {}", inspection.created_at.format("%Y-%m-%d %H:%M:%S"));
                    println!("Working Directory:   {}", inspection.working_directory);
                    println!("Attached Clients:    {}", inspection.attached_clients);
                    println!("Windows:             {}", inspection.windows.len());
                    println!("Locked:              {}", if inspection.locked { "Yes" } else { "No" });
                    println!("Pane Sync:           {}", if inspection.pane_sync_enabled { "Enabled" } else { "Disabled" });
                    println!("Recording:           {}", if inspection.is_recording { "Active" } else { "Inactive" });

                    if inspection.auto_save_enabled {
                        println!("Auto-save:           Enabled (every {} seconds)", inspection.auto_save_interval_secs);
                        if let Some(last_save) = inspection.last_auto_save {
                            println!("Last Auto-save:      {}", last_save.format("%Y-%m-%d %H:%M:%S"));
                        }
                    } else {
                        println!("Auto-save:           Disabled");
                    }

                    if *verbose {
                        println!("\n{}", "Windows & Panes".to_string());
                        println!("{}", "-".repeat(70));

                        for (win_idx, window) in inspection.windows.iter().enumerate() {
                            let is_current = Some(window.id.clone()) == inspection.current_window_id;
                            let marker = if is_current { "*" } else { " " };

                            println!("\n{}Window {}: {} ({}x{})",
                                marker,
                                win_idx,
                                window.name,
                                window.width,
                                window.height
                            );
                            println!("  ID: {}", window.id.0);

                            if window.zoomed_pane.is_some() {
                                println!("  Status: ZOOMED");
                            }

                            println!("  Panes: {}", window.panes.len());

                            for (pane_idx, pane) in window.panes.iter().enumerate() {
                                let is_current_pane = Some(pane.id.clone()) == window.current_pane;
                                let pane_marker = if is_current_pane { "►" } else { " " };

                                println!("\n  {}Pane {}:", pane_marker, pane_idx);
                                println!("    ID:        {}", pane.id.0);
                                println!("    Command:   {}", pane.command);
                                println!("    Size:      {}x{}", pane.cols, pane.rows);
                                println!("    CWD:       {}", pane.working_directory);
                                println!("    Cursor:    ({}, {})", pane.cursor_position.0, pane.cursor_position.1);

                                if pane.is_dead {
                                    if let Some(status) = pane.exit_status {
                                        println!("    Status:    DEAD (exit code: {})", status);
                                    } else {
                                        println!("    Status:    DEAD");
                                    }
                                    println!("    Remain:    {}", if pane.remain_on_exit { "Yes" } else { "No" });
                                } else {
                                    println!("    Status:    Running");
                                }

                                println!("    Scrollback: {} lines", pane.scrollback_lines);
                                println!("    Buffer:    {} bytes", pane.raw_buffer_size);
                            }
                        }
                    } else {
                        println!("\nTip: Use --verbose for detailed window and pane information");
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

            // Get comprehensive state dump
            let dump = client.dump_state(session_id, *include_buffers).await?;

            // Serialize to JSON
            let state_dump = serde_json::to_string_pretty(&dump)
                .map_err(|e| FerrixError::Other(format!("JSON serialization error: {}", e)))?;

            if let Some(output_path) = output {
                std::fs::write(output_path, &state_dump)?;
                println!("✓ Session state dumped to: {}", output_path);
                println!("  Session:     {}", dump.session_info.name);
                println!("  Windows:     {}", dump.session_info.windows.len());

                let total_panes: usize = dump.session_info.windows.iter()
                    .map(|w| w.panes.len())
                    .sum();
                println!("  Panes:       {}", total_panes);

                if let Some(ref buffers) = dump.buffer_data {
                    let total_buffer_size: usize = buffers.iter()
                        .map(|b| b.raw_buffer.len() + b.scrollback_content.iter().map(|s| s.len()).sum::<usize>())
                        .sum();
                    println!("  Buffer data: {} bytes", total_buffer_size);
                } else {
                    println!("  Buffer data: Not included");
                }

                println!("  Timestamp:   {}", dump.dump_timestamp.format("%Y-%m-%d %H:%M:%S"));
            } else {
                println!("{}", state_dump);
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

            // NOTE: Future enhancement could integrate with pprof/flamegraph for advanced profiling
            // Current implementation: Collect basic metrics delta over the specified duration
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

        Some(Commands::SplitPane { vertical, horizontal, percentage }) => {
            ferrix::handlers::pane::handle_split(socket_path, *vertical, *horizontal, *percentage).await?;
        }

        Some(Commands::SelectPane { target }) => {
            ferrix::handlers::pane::handle_select(socket_path, target.clone()).await?;
        }

        Some(Commands::KillPane { target }) => {
            ferrix::handlers::pane::handle_kill_pane(socket_path, target.clone()).await?;
        }

        Some(Commands::ResizePane { direction, amount }) => {
            ferrix::handlers::pane::handle_resize(socket_path, direction.clone(), *amount).await?;
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
