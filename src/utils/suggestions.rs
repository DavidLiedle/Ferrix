//! Error suggestion engine
//!
//! Provides contextual suggestions and "did you mean" functionality for errors.

/// Calculate Levenshtein distance between two strings
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a.chars().nth(i - 1) == b.chars().nth(j - 1) {
                0
            } else {
                1
            };

            matrix[i][j] = std::cmp::min(
                std::cmp::min(
                    matrix[i - 1][j] + 1,      // deletion
                    matrix[i][j - 1] + 1,      // insertion
                ),
                matrix[i - 1][j - 1] + cost,   // substitution
            );
        }
    }

    matrix[a_len][b_len]
}

/// Find the closest match to a given string from a list of candidates
pub fn find_closest_match<'a>(target: &str, candidates: &[&'a str]) -> Option<&'a str> {
    if candidates.is_empty() {
        return None;
    }

    let mut best_match = candidates[0];
    let mut best_distance = levenshtein_distance(target, best_match);

    for &candidate in &candidates[1..] {
        let distance = levenshtein_distance(target, candidate);
        if distance < best_distance {
            best_distance = distance;
            best_match = candidate;
        }
    }

    // Only suggest if distance is reasonable (less than half the target length)
    if best_distance <= target.len() / 2 || best_distance <= 2 {
        Some(best_match)
    } else {
        None
    }
}

/// Find all close matches within a threshold
pub fn find_close_matches<'a>(
    target: &str,
    candidates: &[&'a str],
    max_distance: usize,
) -> Vec<&'a str> {
    candidates
        .iter()
        .filter(|&&candidate| {
            let distance = levenshtein_distance(target, candidate);
            distance <= max_distance
        })
        .copied()
        .collect()
}

/// Generate a "did you mean" suggestion
pub fn did_you_mean(target: &str, candidates: &[&str]) -> Option<String> {
    find_closest_match(target, candidates).map(|suggestion| {
        format!("Did you mean '{}'?", suggestion)
    })
}

/// Generate suggestions for session names
pub fn suggest_session(target: &str, available_sessions: &[String]) -> Option<String> {
    let candidates: Vec<&str> = available_sessions.iter().map(|s| s.as_str()).collect();

    if let Some(closest) = find_closest_match(target, &candidates) {
        let mut suggestions = vec![format!("Did you mean '{}'?", closest)];

        if !available_sessions.is_empty() {
            suggestions.push("\nAvailable sessions:".to_string());
            for session in available_sessions.iter().take(5) {
                suggestions.push(format!("  - {}", session));
            }
            if available_sessions.len() > 5 {
                suggestions.push(format!("  ... and {} more", available_sessions.len() - 5));
            }
        }

        Some(suggestions.join("\n"))
    } else if !available_sessions.is_empty() {
        let mut msg = "Available sessions:".to_string();
        for session in available_sessions.iter().take(5) {
            msg.push_str(&format!("\n  - {}", session));
        }
        if available_sessions.len() > 5 {
            msg.push_str(&format!("\n  ... and {} more", available_sessions.len() - 5));
        }
        Some(msg)
    } else {
        Some("No sessions available. Create one with: ferrix new -s <name>".to_string())
    }
}

/// Generate suggestions for command names
pub fn suggest_command(target: &str) -> Option<String> {
    let valid_commands = [
        "new", "attach", "list", "kill", "detach",
        "save-snapshot", "load-snapshot", "restore-snapshot", "list-snapshots",
        "split-pane", "select-pane", "kill-pane", "resize-pane",
        "toggle-zoom", "toggle-pane-sync", "set-pane-sync",
        "rename-window", "new-window", "list-windows",
        "list-keys", "bind-key", "unbind-key", "reset-keys",
        "enter-copy-mode", "exit-copy-mode",
        "generate-config", "reload-config", "validate-config",
    ];

    did_you_mean(target, &valid_commands)
}

/// Common error recovery suggestions
pub fn recovery_suggestions(error_type: &str) -> Vec<String> {
    let mut suggestions = Vec::new();

    match error_type {
        "connection_failed" => {
            suggestions.push("• Check if the server is running with: ferrix list".to_string());
            suggestions.push("• Start a new server with: ferrix server".to_string());
            suggestions.push("• Verify socket path with: echo $FERRIX_SOCKET".to_string());
        }
        "session_not_found" => {
            suggestions.push("• List all sessions with: ferrix list".to_string());
            suggestions.push("• Create a new session with: ferrix new -s <name>".to_string());
        }
        "permission_denied" => {
            suggestions.push("• Check socket permissions".to_string());
            suggestions.push("• Ensure you have access to ~/.ferrix/".to_string());
            suggestions.push("• Try running with appropriate permissions".to_string());
        }
        "invalid_config" => {
            suggestions.push("• Validate your config: ferrix validate-config".to_string());
            suggestions.push("• Generate a fresh config: ferrix generate-config --force".to_string());
            suggestions.push("• Check config location: ~/.ferrixrc".to_string());
        }
        _ => {}
    }

    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("a", ""), 1);
        assert_eq!(levenshtein_distance("", "a"), 1);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("saturday", "sunday"), 3);
    }

    #[test]
    fn test_find_closest_match() {
        let candidates = vec!["attach", "detach", "list", "kill"];

        assert_eq!(find_closest_match("atach", &candidates), Some("attach"));
        assert_eq!(find_closest_match("detatch", &candidates), Some("detach"));
        assert_eq!(find_closest_match("lst", &candidates), Some("list"));
        assert_eq!(find_closest_match("xyz", &candidates), None);
    }

    #[test]
    fn test_did_you_mean() {
        let candidates = vec!["new", "attach", "list"];

        assert_eq!(
            did_you_mean("atach", &candidates),
            Some("Did you mean 'attach'?".to_string())
        );
        assert_eq!(
            did_you_mean("nwe", &candidates),
            Some("Did you mean 'new'?".to_string())
        );
    }

    #[test]
    fn test_suggest_command() {
        assert!(suggest_command("atach").is_some());
        assert!(suggest_command("lst").is_some());
        assert!(suggest_command("completelywrong").is_none());
    }

    #[test]
    fn test_suggest_session() {
        let sessions = vec!["test-session".to_string(), "demo-session".to_string()];

        let suggestion = suggest_session("test-sesion", &sessions);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("test-session"));
    }

    #[test]
    fn test_recovery_suggestions() {
        let suggestions = recovery_suggestions("connection_failed");
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].contains("server"));
    }
}
