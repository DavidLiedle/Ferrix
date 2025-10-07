use chrono::Local;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Text,
    widgets::Paragraph,
    Frame,
};
use sysinfo::System;
use std::fs;
use std::time::{Duration, Instant};
use std::collections::VecDeque;

use crate::config::{Config, StatusBarPosition};
use crate::protocol::{SessionId, WindowId};
use crate::format::FormatExpander;

#[derive(Debug, Clone)]
pub enum MessageType {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub text: String,
    pub msg_type: MessageType,
    pub timestamp: Instant,
}

pub struct StatusBar {
    config: Config,
    session_name: String,
    session_id: SessionId,
    windows: Vec<WindowInfo>,
    current_window: Option<WindowId>,
    system: System,
    git_branch: Option<String>,
    battery_level: Option<f32>,
    session_locked: bool,
    pane_sync_enabled: bool,
    current_pane_name: Option<String>,
    messages: VecDeque<Message>,
    message_timeout: Duration,
}

#[derive(Clone)]
pub struct WindowInfo {
    pub id: WindowId,
    pub index: usize,
    pub name: String,
    pub active: bool,
}

impl StatusBar {
    pub fn new(config: Config, session_name: String, session_id: SessionId) -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        Self {
            config,
            session_name,
            session_id,
            windows: Vec::new(),
            current_window: None,
            system,
            git_branch: Self::get_git_branch(),
            battery_level: Self::get_battery_level(),
            session_locked: false,
            pane_sync_enabled: false,
            current_pane_name: None,
            messages: VecDeque::new(),
            message_timeout: Duration::from_secs(3), // Show messages for 3 seconds
        }
    }

    pub fn update_windows(&mut self, windows: Vec<WindowInfo>, current: Option<WindowId>) {
        self.windows = windows;
        self.current_window = current;
    }

    /// Display a message in the status bar
    pub fn show_message(&mut self, text: String, msg_type: MessageType) {
        let message = Message {
            text,
            msg_type,
            timestamp: Instant::now(),
        };
        self.messages.push_back(message);

        // Keep only the last 5 messages
        while self.messages.len() > 5 {
            self.messages.pop_front();
        }
    }

    /// Convenience methods for different message types
    pub fn show_info(&mut self, text: String) {
        self.show_message(text, MessageType::Info);
    }

    pub fn show_success(&mut self, text: String) {
        self.show_message(text, MessageType::Success);
    }

    pub fn show_warning(&mut self, text: String) {
        self.show_message(text, MessageType::Warning);
    }

    pub fn show_error(&mut self, text: String) {
        self.show_message(text, MessageType::Error);
    }

    /// Clean up expired messages
    fn cleanup_messages(&mut self) {
        let now = Instant::now();
        self.messages.retain(|msg| now.duration_since(msg.timestamp) < self.message_timeout);
    }

    /// Get the current message to display (most recent non-expired)
    fn get_current_message(&self) -> Option<&Message> {
        let now = Instant::now();
        self.messages.iter().rev().find(|msg| {
            now.duration_since(msg.timestamp) < self.message_timeout
        })
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.config.status_bar.enabled {
            return;
        }

        // Clean up expired messages
        self.cleanup_messages();

        let status_area = match self.config.status_bar.position {
            StatusBarPosition::Top => Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: self.config.status_bar.height.min(area.height),
            },
            StatusBarPosition::Bottom => Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(self.config.status_bar.height),
                width: area.width,
                height: self.config.status_bar.height.min(area.height),
            },
        };

        // Parse and render the status bar sections
        let left_format = self.config.status_bar.left.clone();
        let center_format = self.config.status_bar.center.clone();
        let right_format = self.config.status_bar.right.clone();

        let left_text = self.parse_format(&left_format);
        let right_text = self.parse_format(&right_format);

        // If there's a message, show it in the center; otherwise show normal center content
        let (center_text, center_style) = if let Some(message) = self.get_current_message() {
            let style = match message.msg_type {
                MessageType::Info => Style::default().fg(Color::Cyan),
                MessageType::Success => Style::default().fg(Color::Green),
                MessageType::Warning => Style::default().fg(Color::Yellow),
                MessageType::Error => Style::default().fg(Color::Red),
            };
            (Text::from(message.text.clone()), style)
        } else {
            (self.parse_format(&center_format), self.get_status_style())
        };

        // Create layout for three sections
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                    Constraint::Percentage(33),
                ]
                .as_ref(),
            )
            .split(status_area);

        // Render left section
        let left_paragraph = Paragraph::new(left_text)
            .style(self.get_status_style())
            .alignment(Alignment::Left);
        frame.render_widget(left_paragraph, chunks[0]);

        // Render center section (with message if present)
        let center_paragraph = Paragraph::new(center_text)
            .style(center_style)
            .alignment(Alignment::Center);
        frame.render_widget(center_paragraph, chunks[1]);

        // Render right section
        let right_paragraph = Paragraph::new(right_text)
            .style(self.get_status_style())
            .alignment(Alignment::Right);
        frame.render_widget(right_paragraph, chunks[2]);
    }

    fn parse_format(&mut self, format: &str) -> Text<'static> {
        let mut expander = FormatExpander::new();

        // Populate format variables
        self.populate_format_variables(&mut expander);

        // Expand the format string
        let result = expander.expand(format).unwrap_or_else(|e| {
            tracing::warn!("Format expansion error: {}", e);
            format.to_string()
        });

        Text::from(result)
    }

    fn populate_format_variables(&mut self, expander: &mut FormatExpander) {
        // Session variables
        expander.set_variable("session_name", self.session_name.clone());
        expander.set_variable("session_id", self.session_id.0.to_string());
        expander.set_variable("session_windows", self.windows.len() as i64);
        expander.set_variable("session_locked", self.session_locked);
        expander.set_variable("pane_synchronized", self.pane_sync_enabled);

        // Window variables
        expander.set_variable("window_count", self.windows.len() as i64);
        if let Some(current_window) = self.windows.iter().find(|w| Some(&w.id) == self.current_window.as_ref()) {
            expander.set_variable("window_name", current_window.name.clone());
            expander.set_variable("window_index", current_window.index as i64);
            expander.set_variable("window_active", true);
        }

        // Pane variables
        if let Some(pane_name) = &self.current_pane_name {
            expander.set_variable("pane_title", pane_name.clone());
        }

        // Time variables
        let now = Local::now();
        expander.set_variable("time", now.format("%H:%M:%S").to_string());
        expander.set_variable("date", now.format("%Y-%m-%d").to_string());
        expander.set_variable("datetime", now.to_rfc3339());

        // System variables
        expander.set_variable("host", hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string()));
        expander.set_variable("user", std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()));

        // Git branch
        if let Some(branch) = &self.git_branch {
            expander.set_variable("git_branch", branch.clone());
        }

        // Battery
        if let Some(level) = self.battery_level {
            expander.set_variable("battery_percentage", level as i64);
            expander.set_variable("battery", self.format_battery());
        }

        // System info
        expander.set_variable("cpu", self.format_cpu());
        expander.set_variable("memory", self.format_memory());
        expander.set_variable("uptime", self.format_uptime());
        expander.set_variable("load", self.format_load_average());

        // Legacy compatibility - support both {var} and #{var} syntax
        expander.set_variable("windows", self.format_windows());
        expander.set_variable("session", self.session_name.clone());
    }

    #[allow(dead_code)]
    fn get_variable_value(&mut self, var_name: &str) -> String {
        // Handle time formatting
        if let Some(format) = var_name.strip_prefix("time:") {
            return Local::now().format(format).to_string();
        }

        match var_name {
            "session" => self.session_name.clone(),
            "session_id" => format!("{}", self.session_id.0),
            "windows" => self.format_windows(),
            "window_count" => self.windows.len().to_string(),
            "time" => Local::now().format("%H:%M:%S").to_string(),
            "date" => Local::now().format("%Y-%m-%d").to_string(),
            "host" => hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
            "user" => std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            "git_branch" => self.format_git_branch(),
            "battery" => self.format_battery(),
            "cpu" => self.format_cpu(),
            "memory" => self.format_memory(),
            "session_locked" | "lock_status" => if self.session_locked { "🔒 LOCKED" } else { "🔓" }.to_string(),
            "pane_sync" | "sync_status" => if self.pane_sync_enabled { "🔗 SYNC" } else { "🔗" }.to_string(),
            "current_pane" => self.current_pane_name.clone().unwrap_or_default(),
            "uptime" => self.format_uptime(),
            "load" => self.format_load_average(),
            "disk" => self.format_disk_usage(),
            "network" => self.format_network_status(),
            "temperature" | "temp" => self.format_temperature(),
            "processes" => self.format_process_count(),
            _ => format!("{{{}}}", var_name), // Unknown variable
        }
    }

    fn format_windows(&self) -> String {
        let window_strs: Vec<String> = self.windows
            .iter()
            .map(|w| {
                if Some(&w.id) == self.current_window.as_ref() {
                    format!("{}:{}*", w.index, w.name)
                } else {
                    format!("{}:{}", w.index, w.name)
                }
            })
            .collect();

        window_strs.join(" ")
    }

    fn format_battery(&self) -> String {
        if let Some(level) = self.battery_level {
            let icon = if level > 40.0 {
                "🔋"
            } else {
                "🪫"
            };

            // Add charging status if available (when battery feature is enabled)
            let status = {
                #[cfg(feature = "battery-status")]
                {
                    if let Ok(manager) = battery::Manager::new() {
                        if let Ok(mut batteries) = manager.batteries() {
                            if let Some(Ok(battery)) = batteries.next() {
                                match battery.state() {
                                    battery::State::Charging => "⚡",
                                    battery::State::Discharging => "",
                                    battery::State::Full => "✓",
                                    _ => "",
                                }
                            } else { "" }
                        } else { "" }
                    } else { "" }
                }
                #[cfg(not(feature = "battery-status"))]
                ""
            };

            format!("{}{} {:.0}%", icon, status, level)
        } else {
            "".to_string()
        }
    }

    fn format_cpu(&mut self) -> String {
        self.system.refresh_all();
        let usage = self.system.global_cpu_usage();

        // Add visual indicator based on usage
        let indicator = if usage > 80.0 {
            "🔴"
        } else if usage > 50.0 {
            "🟡"
        } else {
            "🟢"
        };

        format!("{}CPU: {:.1}%", indicator, usage)
    }

    fn format_memory(&mut self) -> String {
        self.system.refresh_all();
        let used = self.system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let total = self.system.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let percent = (used / total * 100.0) as u32;

        // Add visual indicator based on usage
        let indicator = if percent > 80 {
            "🔴"
        } else if percent > 60 {
            "🟡"
        } else {
            "🟢"
        };

        format!("{}MEM: {:.1}GB/{}%", indicator, used, percent)
    }

    fn get_status_style(&self) -> Style {
        Style::default()
            .fg(self.parse_color(&self.config.colors.status_fg))
            .bg(self.parse_color(&self.config.colors.status_bg))
    }

    fn parse_color(&self, color_str: &str) -> Color {
        if let Some(stripped) = color_str.strip_prefix('#') {
            // Parse hex color
            if let Ok(hex) = u32::from_str_radix(stripped, 16) {
                let r = ((hex >> 16) & 0xFF) as u8;
                let g = ((hex >> 8) & 0xFF) as u8;
                let b = (hex & 0xFF) as u8;
                return Color::Rgb(r, g, b);
            }
        }

        // Parse named colors
        match color_str.to_lowercase().as_str() {
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" => Color::Magenta,
            "cyan" => Color::Cyan,
            "white" => Color::White,
            "gray" | "grey" => Color::Gray,
            _ => Color::Reset,
        }
    }

    #[cfg(feature = "versioning")]
    fn get_git_branch() -> Option<String> {
        // Try to get current git branch
        if let Ok(repo) = git2::Repository::open_from_env() {
            if let Ok(head) = repo.head() {
                if let Some(name) = head.shorthand() {
                    return Some(name.to_string());
                }
            }
        }
        None
    }

    #[cfg(not(feature = "versioning"))]
    fn get_git_branch() -> Option<String> {
        None
    }

    #[cfg(feature = "versioning")]
    fn format_git_branch(&self) -> String {
        if let Some(branch) = &self.git_branch {
            // Try to get repository status
            let status_indicator = if let Ok(repo) = git2::Repository::open_from_env() {
                let mut has_changes = false;
                let mut has_staged = false;
                let mut has_untracked = false;

                if let Ok(statuses) = repo.statuses(None) {
                    for entry in statuses.iter() {
                        let status = entry.status();
                        if status.contains(git2::Status::INDEX_NEW) ||
                           status.contains(git2::Status::INDEX_MODIFIED) ||
                           status.contains(git2::Status::INDEX_DELETED) {
                            has_staged = true;
                        }
                        if status.contains(git2::Status::WT_MODIFIED) ||
                           status.contains(git2::Status::WT_DELETED) {
                            has_changes = true;
                        }
                        if status.contains(git2::Status::WT_NEW) {
                            has_untracked = true;
                        }
                    }
                }

                if has_staged {
                    "✓"  // Staged changes
                } else if has_changes {
                    "✗"  // Modified files
                } else if has_untracked {
                    "?"  // Untracked files
                } else {
                    ""   // Clean
                }
            } else {
                ""
            };

            format!("🌿{}{}", branch, status_indicator)
        } else {
            "".to_string()
        }
    }

    #[cfg(not(feature = "versioning"))]
    fn format_git_branch(&self) -> String {
        if let Some(branch) = &self.git_branch {
            format!("🌿{}", branch)
        } else {
            "".to_string()
        }
    }

    fn get_battery_level() -> Option<f32> {
        #[cfg(feature = "battery-status")]
        {
            // Get battery level using the battery crate (when feature is enabled)
            if let Ok(manager) = battery::Manager::new() {
                if let Ok(mut batteries) = manager.batteries() {
                    if let Some(Ok(battery)) = batteries.next() {
                        let charge = battery.state_of_charge().value * 100.0;
                        return Some(charge);
                    }
                }
            }
        }
        None
    }

    fn format_uptime(&self) -> String {
        if let Ok(uptime_data) = fs::read_to_string("/proc/uptime") {
            if let Some(uptime_str) = uptime_data.split_whitespace().next() {
                if let Ok(uptime_seconds) = uptime_str.parse::<f64>() {
                    let days = (uptime_seconds / 86400.0) as u32;
                    let hours = ((uptime_seconds % 86400.0) / 3600.0) as u32;
                    let minutes = ((uptime_seconds % 3600.0) / 60.0) as u32;

                    if days > 0 {
                        return format!("{}d {}h", days, hours);
                    } else if hours > 0 {
                        return format!("{}h {}m", hours, minutes);
                    } else {
                        return format!("{}m", minutes);
                    }
                }
            }
        }
        "N/A".to_string()
    }

    fn format_load_average(&self) -> String {
        let load_avg = System::load_average();
        format!("{:.2}", load_avg.one)
    }

    fn format_disk_usage(&self) -> String {
        // Use df command to get disk usage for root filesystem
        use std::process::Command;

        if let Ok(output) = Command::new("df")
            .args(["-h", "/"])
            .output()
        {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                let lines: Vec<&str> = output_str.lines().collect();
                if lines.len() >= 2 {
                    let parts: Vec<&str> = lines[1].split_whitespace().collect();
                    if parts.len() >= 5 {
                        let used = parts[2];
                        let total = parts[1];
                        let percent = parts[4];
                        return format!("DISK: {}/{} ({})", used, total, percent);
                    }
                }
            }
        }
        "DISK: N/A".to_string()
    }

    fn format_network_status(&self) -> String {
        // Simple network status check - can be enhanced with actual network monitoring
        use std::process::Command;

        // Check if we have network connectivity (ping a reliable server)
        if let Ok(output) = Command::new("ping")
            .args(["-c", "1", "-W", "1", "8.8.8.8"])
            .output()
        {
            if output.status.success() {
                "🌐✓"
            } else {
                "🌐✗"
            }
        } else {
            "🌐?"
        }.to_string()
    }

    fn format_temperature(&mut self) -> String {
        // Temperature monitoring is not universally available
        // This would require platform-specific implementation
        // For now, return empty string
        "".to_string()
    }

    fn format_process_count(&mut self) -> String {
        self.system.refresh_all();
        let count = self.system.processes().len();
        format!("📊{}", count)
    }

    pub fn update_session_state(&mut self, locked: bool, pane_sync: bool, current_pane: Option<String>) {
        self.session_locked = locked;
        self.pane_sync_enabled = pane_sync;
        self.current_pane_name = current_pane;
    }

    pub fn refresh(&mut self) {
        // Refresh dynamic values
        self.system.refresh_all();
        self.git_branch = Self::get_git_branch();
        self.battery_level = Self::get_battery_level();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_statusbar() -> StatusBar {
        let config = Config::default();
        let session_name = "test-session".to_string();
        let session_id = SessionId(Uuid::new_v4());
        StatusBar::new(config, session_name, session_id)
    }

    #[test]
    fn test_statusbar_initialization() {
        let statusbar = create_test_statusbar();

        // Verify initial state
        assert_eq!(statusbar.session_name, "test-session");
        assert!(statusbar.windows.is_empty());
        assert!(statusbar.current_window.is_none());
        // Git branch and battery might be None depending on environment
    }

    #[test]
    fn test_update_windows() {
        let mut statusbar = create_test_statusbar();

        // Create test window info
        let window1 = WindowInfo {
            id: WindowId(Uuid::new_v4()),
            index: 0,
            name: "bash".to_string(),
            active: true,
        };

        let window2 = WindowInfo {
            id: WindowId(Uuid::new_v4()),
            index: 1,
            name: "vim".to_string(),
            active: false,
        };

        let windows = vec![window1.clone(), window2.clone()];
        let current = Some(window1.id.clone());

        // Update windows
        statusbar.update_windows(windows.clone(), current.clone());

        // Verify the update worked
        assert_eq!(statusbar.windows.len(), 2);
        assert_eq!(statusbar.windows[0].name, "bash");
        assert_eq!(statusbar.windows[1].name, "vim");
        assert_eq!(statusbar.current_window, current);
    }

    #[test]
    fn test_format_windows() {
        let mut statusbar = create_test_statusbar();

        let window1_id = WindowId(Uuid::new_v4());
        let window2_id = WindowId(Uuid::new_v4());

        let windows = vec![
            WindowInfo {
                id: window1_id.clone(),
                index: 0,
                name: "bash".to_string(),
                active: true,
            },
            WindowInfo {
                id: window2_id.clone(),
                index: 1,
                name: "vim".to_string(),
                active: false,
            },
        ];

        statusbar.update_windows(windows, Some(window1_id));

        // Test window formatting
        let formatted = statusbar.format_windows();
        assert_eq!(formatted, "0:bash* 1:vim");
    }

    #[test]
    fn test_parse_format_with_variables() {
        let mut statusbar = create_test_statusbar();

        // Test session variable
        let result = statusbar.parse_format("#{session_name}");
        assert_eq!(result, Text::from("test-session"));

        // Test window count
        statusbar.update_windows(vec![
            WindowInfo {
                id: WindowId(Uuid::new_v4()),
                index: 0,
                name: "test".to_string(),
                active: true,
            }
        ], None);

        let result = statusbar.parse_format("#{window_count}");
        assert_eq!(result, Text::from("1"));

        // Test unknown variable (should remain as-is or empty)
        let result = statusbar.parse_format("#{unknown_var}");
        assert_eq!(result, Text::from(""));
    }

    #[test]
    fn test_get_variable_value() {
        let mut statusbar = create_test_statusbar();

        // Test user variable (should get from env or return "unknown")
        let user_val = statusbar.get_variable_value("user");
        assert!(!user_val.is_empty()); // Should be either $USER or "unknown"

        // Test session name
        let session_val = statusbar.get_variable_value("session");
        assert_eq!(session_val, "test-session");

        // Test window count
        let count_val = statusbar.get_variable_value("window_count");
        assert_eq!(count_val, "0");

        // Test host
        let host_val = statusbar.get_variable_value("host");
        assert!(!host_val.is_empty()); // Should be hostname or "unknown"

        // Test session state variables
        let lock_val = statusbar.get_variable_value("session_locked");
        assert_eq!(lock_val, "🔓"); // Should be unlocked initially

        let sync_val = statusbar.get_variable_value("pane_sync");
        assert_eq!(sync_val, "🔗"); // Should be not synced initially

        let pane_val = statusbar.get_variable_value("current_pane");
        assert_eq!(pane_val, ""); // Should be empty initially
    }

    #[test]
    fn test_parse_format_unclosed_brace() {
        let mut statusbar = create_test_statusbar();

        // Test unclosed brace - should be treated as literal
        let result = statusbar.parse_format("test {unclosed");
        assert_eq!(result, Text::from("test {unclosed"));
    }

    #[test]
    fn test_format_battery() {
        let statusbar = create_test_statusbar();

        // Battery might be None on systems without battery
        let battery_str = statusbar.format_battery();

        // Should either be empty or contain a percentage
        assert!(battery_str.is_empty() || battery_str.contains('%') || battery_str == "N/A");
    }

    #[test]
    fn test_update_session_state() {
        let mut statusbar = create_test_statusbar();

        // Update session state
        statusbar.update_session_state(true, true, Some("main".to_string()));

        // Test that state changes are reflected
        let lock_val = statusbar.get_variable_value("session_locked");
        assert_eq!(lock_val, "🔒 LOCKED");

        let sync_val = statusbar.get_variable_value("pane_sync");
        assert_eq!(sync_val, "🔗 SYNC");

        let pane_val = statusbar.get_variable_value("current_pane");
        assert_eq!(pane_val, "main");

        // Test state reset
        statusbar.update_session_state(false, false, None);

        let lock_val = statusbar.get_variable_value("session_locked");
        assert_eq!(lock_val, "🔓");

        let sync_val = statusbar.get_variable_value("pane_sync");
        assert_eq!(sync_val, "🔗");

        let pane_val = statusbar.get_variable_value("current_pane");
        assert_eq!(pane_val, "");
    }

    #[test]
    fn test_complex_format_string() {
        let mut statusbar = create_test_statusbar();

        // Add some windows
        statusbar.update_windows(vec![
            WindowInfo {
                id: WindowId(Uuid::new_v4()),
                index: 0,
                name: "bash".to_string(),
                active: true,
            },
            WindowInfo {
                id: WindowId(Uuid::new_v4()),
                index: 1,
                name: "vim".to_string(),
                active: false,
            },
        ], None);

        // Test complex format string
        let result = statusbar.parse_format("[#{session_name}] Windows: #{window_count} | #{user}@#{host}");
        let text = format!("{}", result);

        // Should contain session name
        assert!(text.contains("[test-session]"));
        assert!(text.contains("Windows: 2"));
        assert!(text.contains("@")); // user@host format
    }
}
