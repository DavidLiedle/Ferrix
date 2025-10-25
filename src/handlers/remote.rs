//! Remote access command handlers
//!
//! Handles remote server connections and user management (feature-gated).
//! These handlers are only available when compiled with the "remote" feature.
//!
//! - connect: Connect to remote Ferrix server
//! - user_management: Add, remove, list users and change passwords

#[cfg(feature = "remote")]
use crate::protocol::AuthCredentials;
#[cfg(feature = "remote")]
use crate::server::remote::RemoteClient;
#[cfg(feature = "remote")]
use crate::auth::UserStore;
#[cfg(feature = "remote")]
use crate::cli::UserAction;
use crate::error::Result;
#[cfg(feature = "remote")]
use std::io::{self, Write};
#[cfg(feature = "remote")]
use std::net::SocketAddr;

/// Handle `connect` - connect to a remote Ferrix server
#[cfg(feature = "remote")]
pub async fn handle_connect(
    address: String,
    username: String,
    password: Option<String>,
    tls_ca: Option<String>,
    tls: bool,
) -> Result<()> {
    let server_addr: SocketAddr = address.parse()
        .map_err(|e| crate::error::FerrixError::Other(format!("Invalid server address: {}", e)))?;

    // Get password if not provided
    let password = if let Some(pwd) = password {
        pwd
    } else {
        print!("Password for {}: ", username);
        io::stdout().flush()?;
        rpassword::read_password()
            .map_err(|e| crate::error::FerrixError::Other(format!("Failed to read password: {}", e)))?
    };

    let credentials = AuthCredentials {
        username: username.clone(),
        password: Some(password),
        token: None,
        certificate: None,
    };

    let mut client = RemoteClient::new(server_addr, credentials);

    // Configure TLS if requested or certificates provided
    if tls || tls_ca.is_some() {
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

    Ok(())
}

/// Handle `user-management` - manage remote access users
#[cfg(feature = "remote")]
pub async fn handle_user_management(action: &UserAction) -> Result<()> {
    let user_store = UserStore::new().await
        .map_err(|e| crate::error::FerrixError::Other(format!("Failed to initialize user store: {}", e)))?;

    match action {
        UserAction::Add { username, password } => {
            let password = if let Some(pwd) = password {
                pwd.clone()
            } else {
                print!("Password for {}: ", username);
                io::stdout().flush()?;
                rpassword::read_password()
                    .map_err(|e| crate::error::FerrixError::Other(format!("Failed to read password: {}", e)))?
            };

            match user_store.add_user(username.clone(), password.clone()).await {
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
                match user_store.remove_user(&username).await {
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
                    .map_err(|e| crate::error::FerrixError::Other(format!("Failed to read password: {}", e)))?
            };

            match user_store.change_password(&username, new_password.clone()).await {
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

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_remote_handlers_exist() {
        // Verify module compiles
        assert!(true);
    }
}
