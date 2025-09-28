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

use crate::config::{Config, StatusBarPosition};
use crate::protocol::{SessionId, WindowId};

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
        }
    }

    pub fn update_windows(&mut self, windows: Vec<WindowInfo>, current: Option<WindowId>) {
        self.windows = windows;
        self.current_window = current;
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.config.status_bar.enabled {
            return;
        }

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
        let center_text = self.parse_format(&center_format);
        let right_text = self.parse_format(&right_format);

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

        // Render center section
        let center_paragraph = Paragraph::new(center_text)
            .style(self.get_status_style())
            .alignment(Alignment::Center);
        frame.render_widget(center_paragraph, chunks[1]);

        // Render right section
        let right_paragraph = Paragraph::new(right_text)
            .style(self.get_status_style())
            .alignment(Alignment::Right);
        frame.render_widget(right_paragraph, chunks[2]);
    }

    fn parse_format(&mut self, format: &str) -> Text<'static> {
        let mut result = String::new();
        let mut chars = format.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                let mut var_name = String::new();
                let mut found_closing = false;

                while let Some(ch) = chars.next() {
                    if ch == '}' {
                        found_closing = true;
                        break;
                    }
                    var_name.push(ch);
                }

                if found_closing {
                    result.push_str(&self.get_variable_value(&var_name));
                } else {
                    result.push('{');
                    result.push_str(&var_name);
                }
            } else {
                result.push(ch);
            }
        }

        Text::from(result)
    }

    fn get_variable_value(&mut self, var_name: &str) -> String {
        // Handle time formatting
        if var_name.starts_with("time:") {
            let format = &var_name[5..];
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
            "git_branch" => self.git_branch.clone().unwrap_or_else(|| "".to_string()),
            "battery" => self.format_battery(),
            "cpu" => self.format_cpu(),
            "memory" => self.format_memory(),
            "session_locked" | "lock_status" => if self.session_locked { "🔒 LOCKED" } else { "🔓" }.to_string(),
            "pane_sync" | "sync_status" => if self.pane_sync_enabled { "🔗 SYNC" } else { "🔗" }.to_string(),
            "current_pane" => self.current_pane_name.clone().unwrap_or_else(|| "".to_string()),
            "uptime" => self.format_uptime(),
            "load" => self.format_load_average(),
            "disk" => self.format_disk_usage(),
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
            let icon = if level > 80.0 {
                "🔋"
            } else if level > 50.0 {
                "🔋"
            } else if level > 20.0 {
                "🪫"
            } else {
                "🪫"
            };
            format!("{} {:.0}%", icon, level)
        } else {
            "".to_string()
        }
    }

    fn format_cpu(&mut self) -> String {
        self.system.refresh_all();
        let usage = self.system.global_cpu_usage();
        format!("CPU: {:.1}%", usage)
    }

    fn format_memory(&mut self) -> String {
        self.system.refresh_all();
        let used = self.system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let total = self.system.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        format!("MEM: {:.1}/{:.1}GB", used, total)
    }

    fn get_status_style(&self) -> Style {
        Style::default()
            .fg(self.parse_color(&self.config.colors.status_fg))
            .bg(self.parse_color(&self.config.colors.status_bg))
    }

    fn parse_color(&self, color_str: &str) -> Color {
        if color_str.starts_with('#') {
            // Parse hex color
            if let Ok(hex) = u32::from_str_radix(&color_str[1..], 16) {
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

    fn get_battery_level() -> Option<f32> {
        // Get battery level using the battery crate
        if let Ok(manager) = battery::Manager::new() {
            if let Ok(mut batteries) = manager.batteries() {
                if let Some(Ok(battery)) = batteries.next() {
                    let charge = battery.state_of_charge().value * 100.0;
                    return Some(charge);
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
            .args(&["-h", "/"])
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
        let result = statusbar.parse_format("{session}");
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

        let result = statusbar.parse_format("{window_count}");
        assert_eq!(result, Text::from("1"));

        // Test unknown variable (should remain as-is)
        let result = statusbar.parse_format("{unknown_var}");
        assert_eq!(result, Text::from("{unknown_var}"));
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
        let result = statusbar.parse_format("[{session}] Windows: {window_count} | {user}@{host}");
        let text = format!("{}", result);

        // Should contain session name
        assert!(text.contains("[test-session]"));
        assert!(text.contains("Windows: 2"));
        assert!(text.contains("@")); // user@host format
    }
}
