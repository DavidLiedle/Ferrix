use std::collections::{HashMap, VecDeque};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::error::{Result, FerrixError};

/// AI-powered command assistant for intelligent suggestions and automation
pub struct CommandAssistant {
    command_history: VecDeque<CommandEntry>,
    patterns: HashMap<String, CommandPattern>,
    context: SessionContext,
    suggestions_cache: HashMap<String, Vec<Suggestion>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEntry {
    pub command: String,
    pub directory: String,
    pub timestamp: DateTime<Utc>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub output_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPattern {
    pub pattern: String,
    pub frequency: usize,
    pub contexts: Vec<String>,
    pub typical_next_commands: Vec<String>,
    pub success_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub current_directory: String,
    pub git_branch: Option<String>,
    pub environment_type: EnvironmentType,
    pub active_processes: Vec<String>,
    pub recent_errors: Vec<ErrorContext>,
    pub project_type: Option<ProjectType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvironmentType {
    Development,
    Testing,
    Staging,
    Production,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    Python,
    JavaScript,
    Go,
    Ruby,
    Java,
    Cpp,
    Docker,
    Kubernetes,
    Mixed(Vec<ProjectType>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    pub command: String,
    pub error_message: String,
    pub timestamp: DateTime<Utc>,
    pub suggested_fixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub command: String,
    pub description: String,
    pub confidence: f32,
    pub category: SuggestionCategory,
    pub keyboard_shortcut: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionCategory {
    NextLogicalStep,
    ErrorFix,
    Optimization,
    Alternative,
    Completion,
    Macro,
}

impl CommandAssistant {
    pub fn new() -> Self {
        Self {
            command_history: VecDeque::with_capacity(1000),
            patterns: HashMap::new(),
            context: SessionContext::default(),
            suggestions_cache: HashMap::new(),
        }
    }

    /// Learn from historical commands (for testing and initialization)
    pub fn learn_from_history(&mut self, commands: &[&str]) {
        for command in commands {
            let entry = CommandEntry {
                command: command.to_string(),
                directory: "/workspace".to_string(),
                timestamp: Utc::now(),
                exit_code: Some(0),
                duration_ms: Some(100),
                output_lines: 10,
            };
            self.record_command(entry);
        }
    }

    /// Add command to history and learn from it
    pub fn record_command(&mut self, entry: CommandEntry) {
        // Learn from command patterns
        self.learn_pattern(&entry);

        // Update context based on command
        self.update_context(&entry);

        // Add to history
        self.command_history.push_back(entry);
        if self.command_history.len() > 1000 {
            self.command_history.pop_front();
        }

        // Clear suggestions cache as context changed
        self.suggestions_cache.clear();
    }

    /// Get intelligent command suggestions based on context
    pub fn get_suggestions(&mut self, partial_command: &str) -> Vec<Suggestion> {
        // Check cache first
        if let Some(cached) = self.suggestions_cache.get(partial_command) {
            return cached.clone();
        }

        let mut suggestions = Vec::new();

        // 1. Command completion
        suggestions.extend(self.complete_command(partial_command));

        // 2. Next logical step based on patterns
        suggestions.extend(self.predict_next_command());

        // 3. Error fixes if last command failed
        suggestions.extend(self.suggest_error_fixes());

        // 4. Context-aware suggestions
        suggestions.extend(self.context_suggestions());

        // 5. Optimization suggestions
        suggestions.extend(self.suggest_optimizations());

        // Sort by confidence
        suggestions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Cache results
        self.suggestions_cache.insert(partial_command.to_string(), suggestions.clone());

        suggestions
    }

    /// Complete partial command based on history and context
    fn complete_command(&self, partial: &str) -> Vec<Suggestion> {
        let mut completions = Vec::new();

        // Search history for matching commands
        for entry in self.command_history.iter().rev() {
            if entry.command.starts_with(partial) {
                let suggestion = Suggestion {
                    command: entry.command.clone(),
                    description: format!("Previously used in {}", entry.directory),
                    confidence: 0.8,
                    category: SuggestionCategory::Completion,
                    keyboard_shortcut: None,
                };
                completions.push(suggestion);

                if completions.len() >= 3 {
                    break;
                }
            }
        }

        completions
    }

    /// Predict likely next command based on patterns
    fn predict_next_command(&self) -> Vec<Suggestion> {
        let mut predictions = Vec::new();

        if let Some(last_command) = self.command_history.back() {
            // Look for patterns matching the last command
            for pattern in self.patterns.values() {
                if Self::matches_pattern(&last_command.command, &pattern.pattern) {
                    for next_cmd in &pattern.typical_next_commands {
                        predictions.push(Suggestion {
                            command: next_cmd.clone(),
                            description: format!("Often follows '{}'", last_command.command),
                            confidence: pattern.success_rate,
                            category: SuggestionCategory::NextLogicalStep,
                            keyboard_shortcut: None,
                        });
                    }
                }
            }
        }

        // Project-specific predictions
        match &self.context.project_type {
            Some(ProjectType::Rust) => {
                if self.last_command_was("cargo build") {
                    predictions.push(Suggestion {
                        command: "cargo test".to_string(),
                        description: "Run tests after building".to_string(),
                        confidence: 0.9,
                        category: SuggestionCategory::NextLogicalStep,
                        keyboard_shortcut: Some("Ctrl+T".to_string()),
                    });
                }
            }
            Some(ProjectType::Python) => {
                if self.last_command_was("pip install") {
                    predictions.push(Suggestion {
                        command: "pip freeze > requirements.txt".to_string(),
                        description: "Update requirements file".to_string(),
                        confidence: 0.7,
                        category: SuggestionCategory::NextLogicalStep,
                        keyboard_shortcut: None,
                    });
                }
            }
            _ => {}
        }

        predictions
    }

    /// Suggest fixes for recent errors
    fn suggest_error_fixes(&self) -> Vec<Suggestion> {
        let mut fixes = Vec::new();

        for error in &self.context.recent_errors {
            // Pattern match common errors
            if error.error_message.contains("command not found") {
                let cmd = error.command.split_whitespace().next().unwrap_or("");
                fixes.push(Suggestion {
                    command: format!("apt install {}", cmd),
                    description: "Install missing command".to_string(),
                    confidence: 0.6,
                    category: SuggestionCategory::ErrorFix,
                    keyboard_shortcut: None,
                });
            }

            if error.error_message.contains("Permission denied") {
                fixes.push(Suggestion {
                    command: format!("sudo {}", error.command),
                    description: "Retry with sudo".to_string(),
                    confidence: 0.8,
                    category: SuggestionCategory::ErrorFix,
                    keyboard_shortcut: Some("Ctrl+S".to_string()),
                });
            }

            if error.error_message.contains("No such file or directory") {
                fixes.push(Suggestion {
                    command: "ls -la".to_string(),
                    description: "Check available files".to_string(),
                    confidence: 0.7,
                    category: SuggestionCategory::ErrorFix,
                    keyboard_shortcut: None,
                });
            }
        }

        fixes
    }

    /// Context-aware suggestions based on current environment
    fn context_suggestions(&self) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Git-aware suggestions
        if let Some(branch) = &self.context.git_branch {
            if branch != "main" && branch != "master" {
                suggestions.push(Suggestion {
                    command: "git status".to_string(),
                    description: format!("Check status on branch '{}'", branch),
                    confidence: 0.6,
                    category: SuggestionCategory::NextLogicalStep,
                    keyboard_shortcut: Some("Ctrl+G".to_string()),
                });
            }
        }

        // Project-specific suggestions
        if let Some(project_type) = &self.context.project_type {
            match project_type {
                ProjectType::Rust => {
                    suggestions.push(Suggestion {
                        command: "cargo clippy".to_string(),
                        description: "Run linter".to_string(),
                        confidence: 0.5,
                        category: SuggestionCategory::Optimization,
                        keyboard_shortcut: None,
                    });
                }
                ProjectType::Docker => {
                    suggestions.push(Suggestion {
                        command: "docker ps".to_string(),
                        description: "List running containers".to_string(),
                        confidence: 0.5,
                        category: SuggestionCategory::NextLogicalStep,
                        keyboard_shortcut: None,
                    });
                }
                _ => {}
            }
        }

        suggestions
    }

    /// Suggest command optimizations
    fn suggest_optimizations(&self) -> Vec<Suggestion> {
        let mut optimizations = Vec::new();

        // Look for inefficient patterns in recent history
        let recent: Vec<_> = self.command_history.iter().rev().take(5).collect();

        // Check for multiple ls commands
        if recent.iter().filter(|e| e.command.starts_with("ls")).count() > 2 {
            optimizations.push(Suggestion {
                command: "watch ls -la".to_string(),
                description: "Use watch for continuous monitoring".to_string(),
                confidence: 0.6,
                category: SuggestionCategory::Optimization,
                keyboard_shortcut: None,
            });
        }

        // Check for repeated grep patterns
        let grep_commands: Vec<_> = recent.iter()
            .filter(|e| e.command.contains("grep"))
            .collect();

        if grep_commands.len() >= 2 {
            optimizations.push(Suggestion {
                command: "rg".to_string(),
                description: "Use ripgrep for faster searching".to_string(),
                confidence: 0.7,
                category: SuggestionCategory::Alternative,
                keyboard_shortcut: None,
            });
        }

        optimizations
    }

    /// Learn patterns from command history
    fn learn_pattern(&mut self, entry: &CommandEntry) {
        let key = Self::extract_pattern_key(&entry.command);

        let pattern = self.patterns.entry(key.clone()).or_insert(CommandPattern {
            pattern: key,
            frequency: 0,
            contexts: Vec::new(),
            typical_next_commands: Vec::new(),
            success_rate: 1.0,
        });

        pattern.frequency += 1;

        if !pattern.contexts.contains(&entry.directory) {
            pattern.contexts.push(entry.directory.clone());
        }

        // Update success rate based on exit code
        if let Some(exit_code) = entry.exit_code {
            let success = if exit_code == 0 { 1.0 } else { 0.0 };
            pattern.success_rate = (pattern.success_rate * 0.9) + (success * 0.1);
        }

        // Learn command sequences
        if self.command_history.len() > 1 {
            let prev_entry = &self.command_history[self.command_history.len() - 1];
            let prev_key = Self::extract_pattern_key(&prev_entry.command);

            if let Some(prev_pattern) = self.patterns.get_mut(&prev_key) {
                if !prev_pattern.typical_next_commands.contains(&entry.command) {
                    prev_pattern.typical_next_commands.push(entry.command.clone());
                    if prev_pattern.typical_next_commands.len() > 5 {
                        prev_pattern.typical_next_commands.remove(0);
                    }
                }
            }
        }
    }

    /// Update session context based on command
    fn update_context(&mut self, entry: &CommandEntry) {
        // Update current directory if cd command
        if entry.command.starts_with("cd ") {
            if let Some(dir) = entry.command.split_whitespace().nth(1) {
                self.context.current_directory = dir.to_string();
            }
        }

        // Detect git operations
        if entry.command.starts_with("git ") {
            // Try to extract branch info
            if entry.command.contains("checkout") {
                if let Some(branch) = entry.command.split_whitespace().last() {
                    self.context.git_branch = Some(branch.to_string());
                }
            }
        }

        // Detect project type
        if entry.command.contains("cargo") {
            self.context.project_type = Some(ProjectType::Rust);
        } else if entry.command.contains("pip") || entry.command.contains("python") {
            self.context.project_type = Some(ProjectType::Python);
        } else if entry.command.contains("npm") || entry.command.contains("node") {
            self.context.project_type = Some(ProjectType::JavaScript);
        } else if entry.command.contains("docker") {
            self.context.project_type = Some(ProjectType::Docker);
        }

        // Track errors
        if let Some(exit_code) = entry.exit_code {
            if exit_code != 0 {
                self.context.recent_errors.push(ErrorContext {
                    command: entry.command.clone(),
                    error_message: format!("Command failed with exit code {}", exit_code),
                    timestamp: entry.timestamp,
                    suggested_fixes: Vec::new(),
                });

                // Keep only recent errors
                if self.context.recent_errors.len() > 5 {
                    self.context.recent_errors.remove(0);
                }
            }
        }
    }

    fn extract_pattern_key(command: &str) -> String {
        // Extract the command pattern (first word typically)
        command.split_whitespace()
            .next()
            .unwrap_or(command)
            .to_string()
    }

    fn matches_pattern(command: &str, pattern: &str) -> bool {
        command.starts_with(pattern)
    }

    fn last_command_was(&self, prefix: &str) -> bool {
        self.command_history
            .back()
            .map(|e| e.command.starts_with(prefix))
            .unwrap_or(false)
    }
}

impl Default for SessionContext {
    fn default() -> Self {
        Self {
            current_directory: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            git_branch: None,
            environment_type: EnvironmentType::Local,
            active_processes: Vec::new(),
            recent_errors: Vec::new(),
            project_type: None,
        }
    }
}