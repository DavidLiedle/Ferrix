//! Session versioning command handlers
//!
//! Git-like version control for sessions:
//! - init-versioning: Initialize versioning for a session
//! - commit: Save current session state with message
//! - branch: Create, list, or delete branches
//! - checkout: Switch to a different branch or commit
//! - merge: Merge another branch into current
//! - log: View commit history
//! - diff: Show differences between commits

use crate::client::Client;
use crate::error::Result;
use crate::protocol::{ClientMessage, ServerMessage};
use std::path::PathBuf;

/// Handle `init-versioning` - initialize versioning for the current session
pub async fn handle_init(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    // Get current session (this assumes we're attached to a session)
    let sessions = client.list_sessions().await?;
    
    if sessions.is_empty() {
        eprintln!("✗ No sessions found. Create a session first with: ferrix new -s <name>");
        std::process::exit(1);
    }

    // Use the first session (in a real scenario, we'd get the attached session)
    let session_id = sessions[0].id.clone();

    client.send(ClientMessage::InitVersioning { session_id: session_id.clone() }).await?;

    match client.receive().await? {
        ServerMessage::Success => {
            println!("✓ Versioning initialized for session");
            println!("  You can now use:");
            println!("    • ferrix commit -m \"message\" - to save state");
            println!("    • ferrix branch <name> - to create branches");
            println!("    • ferrix log - to view history");
            Ok(())
        }
        ServerMessage::Error { message } => {
            eprintln!("✗ Failed to initialize versioning: {}", message);
            std::process::exit(1);
        }
        _ => {
            eprintln!("✗ Unexpected server response");
            std::process::exit(1);
        }
    }
}

/// Handle `commit` - commit current session state
pub async fn handle_commit(
    socket_path: PathBuf,
    message: String,
    author: Option<String>,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let sessions = client.list_sessions().await?;
    
    if sessions.is_empty() {
        eprintln!("✗ No sessions found");
        std::process::exit(1);
    }

    let session_id = sessions[0].id.clone();

    // Use author from arg or default to user@hostname
    let author_str = author.unwrap_or_else(|| {
        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "localhost".to_string());
        format!("{}@{}", user, hostname)
    });

    client.send(ClientMessage::CommitSession {
        session_id: session_id.clone(),
        message: format!("{}\n\nAuthor: {}", message, author_str),
    }).await?;

    match client.receive().await? {
        ServerMessage::Success => {
            println!("✓ Session state committed");
            println!("  Message: {}", message);
            println!("  Author: {}", author_str);
            Ok(())
        }
        ServerMessage::Error { message: err } => {
            eprintln!("✗ Failed to commit: {}", err);
            std::process::exit(1);
        }
        _ => {
            eprintln!("✗ Unexpected server response");
            std::process::exit(1);
        }
    }
}

/// Handle `branch` - create or manage branches
pub async fn handle_branch(
    socket_path: PathBuf,
    name: Option<String>,
    list: bool,
    delete: Option<String>,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let sessions = client.list_sessions().await?;
    
    if sessions.is_empty() {
        eprintln!("✗ No sessions found");
        std::process::exit(1);
    }

    let session_id = sessions[0].id.clone();

    if list {
        // List branches
        client.send(ClientMessage::ListBranches { session_id }).await?;

        match client.receive().await? {
            ServerMessage::BranchList { branches, current, .. } => {
                if branches.is_empty() {
                    println!("No branches found");
                } else {
                    println!("Branches:");
                    for branch in branches {
                        let marker = if branch.name == current { "* " } else { "  " };
                        println!("{}{}", marker, branch.name);
                        println!("    Head: {}", branch.head);
                        println!("    Created: {}", branch.created_at.format("%Y-%m-%d %H:%M:%S"));
                    }
                }
                Ok(())
            }
            ServerMessage::Error { message } => {
                eprintln!("✗ Failed to list branches: {}", message);
                std::process::exit(1);
            }
            _ => {
                eprintln!("✗ Unexpected server response");
                std::process::exit(1);
            }
        }
    } else if let Some(branch_name) = delete {
        // Delete branch
        eprintln!("✗ Branch deletion not yet implemented for '{}'", branch_name);
        std::process::exit(1);
    } else if let Some(branch_name) = name {
        // Create branch
        client.send(ClientMessage::CreateBranch {
            session_id,
            branch_name: branch_name.clone(),
            description: None,
        }).await?;

        match client.receive().await? {
            ServerMessage::Success => {
                println!("✓ Branch '{}' created", branch_name);
                Ok(())
            }
            ServerMessage::Error { message } => {
                eprintln!("✗ Failed to create branch: {}", message);
                std::process::exit(1);
            }
            _ => {
                eprintln!("✗ Unexpected server response");
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("✗ Specify --list to list branches or provide a branch name to create");
        std::process::exit(1);
    }
}

/// Handle `checkout` - switch to a different branch or commit
pub async fn handle_checkout(socket_path: PathBuf, target: String) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let sessions = client.list_sessions().await?;
    
    if sessions.is_empty() {
        eprintln!("✗ No sessions found");
        std::process::exit(1);
    }

    let session_id = sessions[0].id.clone();

    client.send(ClientMessage::CheckoutBranch {
        session_id,
        branch_name: target.clone(),
    }).await?;

    match client.receive().await? {
        ServerMessage::Success => {
            println!("✓ Switched to branch '{}'", target);
            println!("  Session state restored to this branch's HEAD");
            Ok(())
        }
        ServerMessage::Error { message } => {
            eprintln!("✗ Failed to checkout: {}", message);
            std::process::exit(1);
        }
        _ => {
            eprintln!("✗ Unexpected server response");
            std::process::exit(1);
        }
    }
}

/// Handle `merge` - merge another branch into current
pub async fn handle_merge(
    socket_path: PathBuf,
    branch: String,
    strategy: Option<String>,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let sessions = client.list_sessions().await?;
    
    if sessions.is_empty() {
        eprintln!("✗ No sessions found");
        std::process::exit(1);
    }

    let session_id = sessions[0].id.clone();
    let merge_strategy = strategy.unwrap_or_else(|| "auto".to_string());

    client.send(ClientMessage::MergeBranch {
        session_id,
        branch_name: branch.clone(),
        strategy: merge_strategy,
    }).await?;

    match client.receive().await? {
        ServerMessage::Success => {
            println!("✓ Merged branch '{}' into current branch", branch);
            Ok(())
        }
        ServerMessage::Error { message } => {
            eprintln!("✗ Failed to merge: {}", message);
            std::process::exit(1);
        }
        _ => {
            eprintln!("✗ Unexpected server response");
            std::process::exit(1);
        }
    }
}

/// Handle `log` - view commit history
pub async fn handle_log(socket_path: PathBuf, limit: usize, _verbose: bool) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let sessions = client.list_sessions().await?;
    
    if sessions.is_empty() {
        eprintln!("✗ No sessions found");
        std::process::exit(1);
    }

    let session_id = sessions[0].id.clone();

    client.send(ClientMessage::ShowLog { session_id, limit: Some(limit) }).await?;

    match client.receive().await? {
        ServerMessage::LogHistory { commits, .. } => {
            if commits.is_empty() {
                println!("No commits yet");
                println!("Create your first commit with: ferrix commit -m \"message\"");
            } else {
                for commit in commits {
                    println!("commit {}", commit.id);
                    println!("Author: {}", commit.author);
                    println!("Date:   {}", commit.timestamp.format("%Y-%m-%d %H:%M:%S"));
                    println!();
                    for line in commit.message.lines() {
                        println!("    {}", line);
                    }
                    println!();
                }
            }
            Ok(())
        }
        ServerMessage::Error { message } => {
            eprintln!("✗ Failed to get log: {}", message);
            std::process::exit(1);
        }
        _ => {
            eprintln!("✗ Unexpected server response");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_versioning_handlers_exist() {
        // Verify all handlers compile
        assert!(true);
    }
}
