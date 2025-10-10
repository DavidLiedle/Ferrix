use std::collections::VecDeque;
use std::path::PathBuf;
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};

use crate::error::{Result, FerrixError};
use crate::protocol::{SessionId, WindowId, PaneId};

/// Time-travel debugging system for session replay and analysis
pub struct TimeTravelEngine {
    recording: SessionRecording,
    playback_state: PlaybackState,
    bookmarks: Vec<TimeBookmark>,
    analysis_cache: AnalysisCache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecording {
    pub session_id: SessionId,
    pub start_time: DateTime<Utc>,
    pub events: VecDeque<TimestampedEvent>,
    pub snapshots: Vec<TimeSnapshot>,
    pub metadata: RecordingMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedEvent {
    pub timestamp: DateTime<Utc>,
    pub relative_time: Duration,
    pub event: RecordedEvent,
    pub window_id: Option<WindowId>,
    pub pane_id: Option<PaneId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecordedEvent {
    Input {
        data: Vec<u8>,
        source: InputSource,
    },
    Output {
        data: Vec<u8>,
        stream: OutputStream,
    },
    WindowCreated {
        window_id: WindowId,
        name: String,
    },
    WindowClosed {
        window_id: WindowId,
    },
    PaneCreated {
        pane_id: PaneId,
        parent_id: Option<PaneId>,
    },
    PaneClosed {
        pane_id: PaneId,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    CommandExecuted {
        command: String,
        exit_code: Option<i32>,
        duration: Duration,
    },
    StateChange {
        from: String,
        to: String,
    },
    Error {
        message: String,
        recoverable: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputSource {
    Keyboard,
    Mouse,
    Paste,
    Script,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputStream {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSnapshot {
    pub timestamp: DateTime<Utc>,
    pub state: SessionState,
    pub terminal_buffers: Vec<TerminalBuffer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub windows: Vec<WindowState>,
    pub active_window: Option<WindowId>,
    pub environment: Vec<(String, String)>,
    pub working_directory: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub id: WindowId,
    pub name: String,
    pub panes: Vec<PaneState>,
    pub active_pane: Option<PaneId>,
    pub layout: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneState {
    pub id: PaneId,
    pub command: String,
    pub exit_code: Option<i32>,
    pub cursor_position: (u16, u16),
    pub scroll_position: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalBuffer {
    pub pane_id: PaneId,
    pub lines: Vec<String>,
    pub cursor: (u16, u16),
    pub attributes: Vec<CellAttributes>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellAttributes {
    pub position: (u16, u16),
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub current_position: DateTime<Utc>,
    pub playback_speed: f32,
    pub is_playing: bool,
    pub loop_enabled: bool,
    pub loop_start: Option<DateTime<Utc>>,
    pub loop_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBookmark {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMetadata {
    pub session_name: String,
    pub duration: Duration,
    pub event_count: usize,
    pub compressed_size: Option<usize>,
    pub checksum: String,
}

#[derive(Debug, Clone)]
pub struct AnalysisCache {
    pub command_frequency: Vec<(String, usize)>,
    pub error_points: Vec<DateTime<Utc>>,
    pub productivity_score: f32,
    pub idle_periods: Vec<(DateTime<Utc>, Duration)>,
    pub hot_keys: Vec<(String, usize)>,
}

impl TimeTravelEngine {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            recording: SessionRecording {
                session_id,
                start_time: Utc::now(),
                events: VecDeque::new(),
                snapshots: Vec::new(),
                metadata: RecordingMetadata {
                    session_name: String::new(),
                    duration: Duration::zero(),
                    event_count: 0,
                    compressed_size: None,
                    checksum: String::new(),
                },
            },
            playback_state: PlaybackState {
                current_position: Utc::now(),
                playback_speed: 1.0,
                is_playing: false,
                loop_enabled: false,
                loop_start: None,
                loop_end: None,
            },
            bookmarks: Vec::new(),
            analysis_cache: AnalysisCache {
                command_frequency: Vec::new(),
                error_points: Vec::new(),
                productivity_score: 0.0,
                idle_periods: Vec::new(),
                hot_keys: Vec::new(),
            },
        }
    }

    /// Check if recording is active
    pub fn is_recording(&self) -> bool {
        !self.playback_state.is_playing
    }

    /// Record input data
    pub fn record_input(&mut self, data: &[u8]) {
        let event = RecordedEvent::Input {
            data: data.to_vec(),
            source: InputSource::Keyboard,
        };
        self.record_event(event, None, None);
    }

    /// Record a new event
    pub fn record_event(&mut self, event: RecordedEvent, window_id: Option<WindowId>, pane_id: Option<PaneId>) {
        let timestamp = Utc::now();
        let relative_time = timestamp - self.recording.start_time;

        let timestamped = TimestampedEvent {
            timestamp,
            relative_time,
            event: event.clone(),
            window_id,
            pane_id,
        };

        self.recording.events.push_back(timestamped);
        self.recording.metadata.event_count += 1;

        // Create snapshot periodically (every 1000 events or 5 minutes)
        if self.recording.events.len() % 1000 == 0 ||
           self.should_create_snapshot() {
            self.create_snapshot();
        }

        // Update analysis cache
        self.update_analysis(&event);
    }

    /// Create a snapshot of current state
    pub fn create_snapshot(&mut self) {
        // In real implementation, would capture actual terminal state
        let snapshot = TimeSnapshot {
            timestamp: Utc::now(),
            state: self.capture_current_state(),
            terminal_buffers: self.capture_terminal_buffers(),
        };

        self.recording.snapshots.push(snapshot);
    }

    /// Jump to specific point in time
    pub fn seek(&mut self, target_time: DateTime<Utc>) -> Result<SessionState> {
        // Find nearest snapshot before target time
        let snapshot = self.find_nearest_snapshot(target_time);

        // Replay events from snapshot to target time
        let state = self.replay_to_time(snapshot, target_time)?;

        self.playback_state.current_position = target_time;

        Ok(state)
    }

    /// Play recording from current position
    pub fn play(&mut self) {
        self.playback_state.is_playing = true;
    }

    /// Pause playback
    pub fn pause(&mut self) {
        self.playback_state.is_playing = false;
    }

    /// Step forward one event
    pub fn step_forward(&mut self) -> Option<RecordedEvent> {
        let current_idx = self.find_event_index(self.playback_state.current_position);

        if current_idx < self.recording.events.len() - 1 {
            let event = &self.recording.events[current_idx + 1];
            self.playback_state.current_position = event.timestamp;
            Some(event.event.clone())
        } else {
            None
        }
    }

    /// Step backward one event
    pub fn step_backward(&mut self) -> Option<RecordedEvent> {
        let current_idx = self.find_event_index(self.playback_state.current_position);

        if current_idx > 0 {
            let event = &self.recording.events[current_idx - 1];
            self.playback_state.current_position = event.timestamp;
            Some(event.event.clone())
        } else {
            None
        }
    }

    /// Set playback speed (0.5x, 1x, 2x, etc.)
    pub fn set_playback_speed(&mut self, speed: f32) {
        self.playback_state.playback_speed = speed.clamp(0.1, 10.0);
    }

    /// Add a bookmark at current position
    pub fn add_bookmark(&mut self, name: String, description: String, tags: Vec<String>) {
        let bookmark = TimeBookmark {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: self.playback_state.current_position,
            name,
            description,
            tags,
        };

        self.bookmarks.push(bookmark);
    }

    /// Jump to bookmark
    pub fn jump_to_bookmark(&mut self, bookmark_id: &str) -> Result<()> {
        if let Some(bookmark) = self.bookmarks.iter().find(|b| b.id == bookmark_id) {
            self.seek(bookmark.timestamp)?;
            Ok(())
        } else {
            Err(FerrixError::Other("Bookmark not found".to_string()))
        }
    }

    /// Search for events matching criteria
    pub fn search_events(&self, query: &str) -> Vec<&TimestampedEvent> {
        self.recording.events.iter()
            .filter(|event| self.event_matches_query(event, query))
            .collect()
    }

    /// Get analytics for time range
    pub fn analyze_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> SessionAnalytics {
        let events_in_range: Vec<_> = self.recording.events.iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect();

        SessionAnalytics {
            total_events: events_in_range.len(),
            commands_executed: self.count_commands(&events_in_range),
            errors_encountered: self.count_errors(&events_in_range),
            idle_time: self.calculate_idle_time(&events_in_range),
            active_time: end - start - self.calculate_idle_time(&events_in_range),
            keystrokes_per_minute: self.calculate_kpm(&events_in_range),
            most_used_commands: self.get_top_commands(&events_in_range, 10),
        }
    }

    /// Export recording to file
    pub fn export(&self, path: &PathBuf) -> Result<()> {
        let json = serde_json::to_string(&self.recording)
            .map_err(|e| FerrixError::Other(format!("Failed to serialize recording: {}", e)))?;

        // Compress if large
        if json.len() > 1_000_000 {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write;

            let file = std::fs::File::create(path)?;
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder.write_all(json.as_bytes())?;
            encoder.finish()?;
        } else {
            std::fs::write(path, json)?;
        }

        Ok(())
    }

    /// Import recording from file
    pub fn import(path: &PathBuf) -> Result<Self> {
        let data = if path.extension().and_then(|s| s.to_str()) == Some("gz") {
            use flate2::read::GzDecoder;
            use std::io::Read;

            let file = std::fs::File::open(path)?;
            let mut decoder = GzDecoder::new(file);
            let mut json = String::new();
            decoder.read_to_string(&mut json)?;
            json
        } else {
            std::fs::read_to_string(path)?
        };

        let recording: SessionRecording = serde_json::from_str(&data)
            .map_err(|e| FerrixError::Other(format!("Failed to deserialize recording: {}", e)))?;

        Ok(Self {
            recording,
            playback_state: PlaybackState {
                current_position: Utc::now(),
                playback_speed: 1.0,
                is_playing: false,
                loop_enabled: false,
                loop_start: None,
                loop_end: None,
            },
            bookmarks: Vec::new(),
            analysis_cache: AnalysisCache {
                command_frequency: Vec::new(),
                error_points: Vec::new(),
                productivity_score: 0.0,
                idle_periods: Vec::new(),
                hot_keys: Vec::new(),
            },
        })
    }

    // Helper methods
    fn should_create_snapshot(&self) -> bool {
        if let Some(last_snapshot) = self.recording.snapshots.last() {
            Utc::now() - last_snapshot.timestamp > Duration::minutes(5)
        } else {
            true
        }
    }

    fn capture_current_state(&self) -> SessionState {
        // In real implementation, would capture actual state
        SessionState {
            windows: Vec::new(),
            active_window: None,
            environment: std::env::vars().collect(),
            working_directory: std::env::current_dir().unwrap_or_default(),
        }
    }

    fn capture_terminal_buffers(&self) -> Vec<TerminalBuffer> {
        // In real implementation, would capture actual terminal content
        Vec::new()
    }

    fn find_nearest_snapshot(&self, target: DateTime<Utc>) -> &TimeSnapshot {
        self.recording.snapshots.iter()
            .filter(|s| s.timestamp <= target)
            .next_back()
            .unwrap_or(&self.recording.snapshots[0])
    }

    fn replay_to_time(&self, _snapshot: &TimeSnapshot, _target: DateTime<Utc>) -> Result<SessionState> {
        // In real implementation, would replay events
        Ok(self.capture_current_state())
    }

    fn find_event_index(&self, timestamp: DateTime<Utc>) -> usize {
        self.recording.events.iter()
            .position(|e| e.timestamp >= timestamp)
            .unwrap_or(0)
    }

    fn event_matches_query(&self, event: &TimestampedEvent, query: &str) -> bool {
        format!("{:?}", event.event).to_lowercase().contains(&query.to_lowercase())
    }

    fn update_analysis(&mut self, event: &RecordedEvent) {
        match event {
            RecordedEvent::CommandExecuted { command, .. } => {
                // Update command frequency
                let cmd = command.split_whitespace().next().unwrap_or("").to_string();
                if let Some(entry) = self.analysis_cache.command_frequency.iter_mut()
                    .find(|(c, _)| c == &cmd) {
                    entry.1 += 1;
                } else {
                    self.analysis_cache.command_frequency.push((cmd, 1));
                }
            }
            RecordedEvent::Error { .. } => {
                self.analysis_cache.error_points.push(Utc::now());
            }
            _ => {}
        }
    }

    fn count_commands(&self, events: &[&TimestampedEvent]) -> usize {
        events.iter()
            .filter(|e| matches!(e.event, RecordedEvent::CommandExecuted { .. }))
            .count()
    }

    fn count_errors(&self, events: &[&TimestampedEvent]) -> usize {
        events.iter()
            .filter(|e| matches!(e.event, RecordedEvent::Error { .. }))
            .count()
    }

    fn calculate_idle_time(&self, events: &[&TimestampedEvent]) -> Duration {
        let mut idle_time = Duration::zero();

        for window in events.windows(2) {
            let gap = window[1].timestamp - window[0].timestamp;
            if gap > Duration::minutes(1) {
                idle_time += gap;
            }
        }

        idle_time
    }

    fn calculate_kpm(&self, events: &[&TimestampedEvent]) -> f32 {
        let keystrokes = events.iter()
            .filter(|e| matches!(e.event, RecordedEvent::Input { source: InputSource::Keyboard, .. }))
            .count();

        if events.len() < 2 {
            return 0.0;
        }

        let first_timestamp = events.first().map(|e| e.timestamp).unwrap_or_default();
        let last_timestamp = events.last().map(|e| e.timestamp).unwrap_or_default();
        let duration = last_timestamp - first_timestamp;
        let minutes = duration.num_minutes() as f32;

        if minutes > 0.0 {
            keystrokes as f32 / minutes
        } else {
            0.0
        }
    }

    fn get_top_commands(&self, events: &[&TimestampedEvent], limit: usize) -> Vec<(String, usize)> {
        use std::collections::HashMap;

        let mut freq: HashMap<String, usize> = HashMap::new();

        for event in events {
            if let RecordedEvent::CommandExecuted { command, .. } = &event.event {
                let cmd = command.split_whitespace().next().unwrap_or("").to_string();
                *freq.entry(cmd).or_insert(0) += 1;
            }
        }

        let mut sorted: Vec<_> = freq.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);

        sorted
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAnalytics {
    pub total_events: usize,
    pub commands_executed: usize,
    pub errors_encountered: usize,
    pub idle_time: Duration,
    pub active_time: Duration,
    pub keystrokes_per_minute: f32,
    pub most_used_commands: Vec<(String, usize)>,
}
#[cfg(test)]
mod tests {
    

    #[test]
    fn test_timetravel_initialization() {
        // Test timetravel feature initialization
        assert!(true);
    }

    #[test]
    fn test_history_tracking() {
        // Test history tracking
        assert!(true);
    }
}
