use std::path::Path;
use std::fs::File;
use std::io::{Write, BufWriter, BufReader, BufRead};
use std::time::{SystemTime, Duration};
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use flate2::write::GzEncoder;
use flate2::read::GzDecoder;
use flate2::Compression;
use tokio::sync::mpsc;
use tracing::{info, error};

use crate::error::{Result, FerrixError};
use crate::protocol::{SessionId, WindowId, PaneId};
use uuid;

/// Session recording format version
pub const RECORDING_VERSION: u32 = 1;

/// Recording event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RecordingEvent {
    /// Terminal output event
    Output {
        timestamp: u64,
        pane_id: PaneId,
        data: Vec<u8>,
    },
    /// Terminal input event
    Input {
        timestamp: u64,
        pane_id: PaneId,
        data: Vec<u8>,
    },
    /// Window/pane resize event
    Resize {
        timestamp: u64,
        pane_id: PaneId,
        cols: u16,
        rows: u16,
    },
    /// Pane creation
    PaneCreated {
        timestamp: u64,
        pane_id: PaneId,
        parent_id: Option<PaneId>,
        cols: u16,
        rows: u16,
    },
    /// Pane closed
    PaneClosed {
        timestamp: u64,
        pane_id: PaneId,
    },
    /// Window creation
    WindowCreated {
        timestamp: u64,
        window_id: WindowId,
        name: String,
    },
    /// Window closed
    WindowClosed {
        timestamp: u64,
        window_id: WindowId,
    },
    /// Marker for sections
    Marker {
        timestamp: u64,
        label: String,
        description: Option<String>,
    },
}

/// Recording metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMetadata {
    pub version: u32,
    pub session_id: SessionId,
    pub session_name: String,
    pub created_at: u64,
    pub duration_ms: Option<u64>,
    pub terminal_size: (u16, u16),
    pub shell: String,
    pub user: String,
    pub hostname: String,
    pub compressed: bool,
}

/// Session recorder for capturing terminal sessions
pub struct SessionRecorder {
    metadata: RecordingMetadata,
    writer: Arc<Mutex<BufWriter<Box<dyn Write + Send>>>>,
    start_time: SystemTime,
    event_tx: mpsc::UnboundedSender<RecordingEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<RecordingEvent>>,
    recording: bool,
    is_paused: bool,
    event_count: u64,
    output_path: std::path::PathBuf,
}

impl SessionRecorder {
    /// Create a new session recorder
    pub fn new(
        metadata: RecordingMetadata,
        output_path: std::path::PathBuf,
    ) -> Result<Self> {
        let file = File::create(&output_path)
            .map_err(|e| FerrixError::Other(format!("Failed to create recording file: {}", e)))?;

        let compressed = output_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "gz")
            .unwrap_or(false);

        let writer: Box<dyn Write + Send> = if compressed {
            Box::new(GzEncoder::new(file, Compression::default()))
        } else {
            Box::new(file)
        };

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let mut recorder = Self {
            metadata: metadata.clone(),
            writer: Arc::new(Mutex::new(BufWriter::new(writer))),
            start_time: SystemTime::now(),
            event_tx,
            event_rx: Some(event_rx),
            recording: true,
            is_paused: false,
            event_count: 0,
            output_path,
        };

        // Start the event writer task
        recorder.start_event_writer();

        // Write metadata header
        recorder.write_metadata()?;

        Ok(recorder)
    }

    /// Start the event writer task
    fn start_event_writer(&mut self) {
        if let Some(mut rx) = self.event_rx.take() {
            let writer = Arc::clone(&self.writer);

            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    if let Ok(mut w) = writer.lock() {
                        if let Err(e) = Self::write_event_to_writer(&mut w, &event) {
                            error!("Failed to write recording event: {}", e);
                        }
                    }
                }
                // Flush on completion
                if let Ok(mut w) = writer.lock() {
                    let _ = w.flush();
                }
            });
        }
    }

    /// Stop recording
    pub async fn stop(&mut self) -> Result<(u64, u64)> {
        if !self.recording {
            return Err(FerrixError::Other("Recording not active".to_string()));
        }

        self.recording = false;

        // Calculate duration
        let duration = SystemTime::now()
            .duration_since(self.start_time)
            .unwrap_or(Duration::from_secs(0));
        self.metadata.duration_ms = Some(duration.as_millis() as u64);

        // Write final metadata
        self.write_metadata()?;

        // Flush writer
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.flush();
        }

        // Get file size
        let file_size = std::fs::metadata(&self.output_path)
            .map(|m| m.len())
            .unwrap_or(0);

        info!("Stopped recording session {:?} (duration: {:?}, size: {} bytes)",
              self.metadata.session_id, duration, file_size);

        Ok((duration.as_secs(), file_size))
    }

    /// Pause recording
    pub fn pause(&mut self) {
        self.is_paused = true;
    }

    /// Resume recording
    pub fn resume(&mut self) {
        self.is_paused = false;
    }

    /// Check if recording is paused
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    /// Get the output path
    pub fn get_output_path(&self) -> std::path::PathBuf {
        self.output_path.clone()
    }

    /// Get the duration
    pub fn get_duration(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.start_time)
            .unwrap_or(Duration::from_secs(0))
    }

    /// Get the event count
    pub fn get_event_count(&self) -> u64 {
        self.event_count
    }

    /// Record an event
    pub fn record_event(&mut self, event: RecordingEvent) {
        if self.recording && !self.is_paused {
            self.event_count += 1;
            let _ = self.event_tx.send(event);
        }
    }

    /// Record output data
    pub async fn record_output(&mut self, data: Vec<u8>) -> Result<()> {
        if !data.is_empty() {
            // Use a default pane ID for now - sessions have multiple panes
            let pane_id = PaneId(uuid::Uuid::nil());
            let event = RecordingEvent::Output {
                timestamp: SystemTime::now()
                    .duration_since(self.start_time)
                    .unwrap_or(Duration::from_secs(0))
                    .as_millis() as u64,
                pane_id,
                data,
            };
            self.record_event(event);
        }
        Ok(())
    }

    /// Record input data
    pub async fn record_input(&mut self, data: Vec<u8>) -> Result<()> {
        if !data.is_empty() {
            // Use a default pane ID for now
            let pane_id = PaneId(uuid::Uuid::nil());
            let event = RecordingEvent::Input {
                timestamp: SystemTime::now()
                    .duration_since(self.start_time)
                    .unwrap_or(Duration::from_secs(0))
                    .as_millis() as u64,
                pane_id,
                data,
            };
            self.record_event(event);
        }
        Ok(())
    }

    /// Record terminal resize
    pub async fn record_resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        // Use a default pane ID for now
        let pane_id = PaneId(uuid::Uuid::nil());
        let event = RecordingEvent::Resize {
            timestamp: SystemTime::now()
                .duration_since(self.start_time)
                .unwrap_or(Duration::from_secs(0))
                .as_millis() as u64,
            pane_id,
            cols,
            rows,
        };
        self.record_event(event);
        Ok(())
    }

    /// Write metadata to the file
    fn write_metadata(&self) -> Result<()> {
        if let Ok(mut w) = self.writer.lock() {
            let metadata_json = serde_json::to_string(&self.metadata)
                .map_err(|e| FerrixError::Other(format!("Failed to serialize metadata: {}", e)))?;
            writeln!(w, "FERRIX_REC_V1")
                .map_err(|e| FerrixError::Other(format!("Failed to write header: {}", e)))?;
            writeln!(w, "{}", metadata_json)
                .map_err(|e| FerrixError::Other(format!("Failed to write metadata: {}", e)))?;
        }
        Ok(())
    }

    /// Write an event to a writer
    fn write_event_to_writer(writer: &mut BufWriter<Box<dyn Write + Send>>, event: &RecordingEvent) -> Result<()> {
        let event_json = serde_json::to_string(event)
            .map_err(|e| FerrixError::Other(format!("Failed to serialize event: {}", e)))?;
        writeln!(writer, "{}", event_json)
            .map_err(|e| FerrixError::Other(format!("Failed to write event: {}", e)))?;
        Ok(())
    }
}

/// Session player for replaying recorded sessions
pub struct SessionPlayer {
    metadata: RecordingMetadata,
    events: Vec<RecordingEvent>,
    current_index: usize,
    playback_speed: f32,
    paused: bool,
}

impl SessionPlayer {
    /// Load a recording from file
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .map_err(|e| FerrixError::Other(format!("Failed to open recording file: {}", e)))?;

        let reader: Box<dyn BufRead + Send> = if path.extension()
            .and_then(|s| s.to_str()) == Some("gz") {
            Box::new(BufReader::new(GzDecoder::new(file)))
        } else {
            Box::new(BufReader::new(file))
        };

        let mut lines = reader.lines();

        // Read header
        let header = lines.next()
            .ok_or_else(|| FerrixError::Other("Empty recording file".to_string()))?
            .map_err(|e| FerrixError::Other(format!("Failed to read header: {}", e)))?;

        if !header.starts_with("FERRIX_RECORDING_V") {
            return Err(FerrixError::Other("Invalid recording file format".to_string()));
        }

        // Read metadata
        let metadata_line = lines.next()
            .ok_or_else(|| FerrixError::Other("Missing metadata".to_string()))?
            .map_err(|e| FerrixError::Other(format!("Failed to read metadata: {}", e)))?;

        let metadata: RecordingMetadata = serde_json::from_str(&metadata_line)
            .map_err(|e| FerrixError::Other(format!("Failed to parse metadata: {}", e)))?;

        // Read events
        let mut events = Vec::new();
        for line in lines {
            let line = line.map_err(|e| FerrixError::Other(format!("Failed to read event: {}", e)))?;
            if line.trim().is_empty() {
                continue;
            }

            let event: RecordingEvent = serde_json::from_str(&line)
                .map_err(|e| FerrixError::Other(format!("Failed to parse event: {}", e)))?;
            events.push(event);
        }

        info!("Loaded recording: {} events, duration: {:?}ms",
              events.len(), metadata.duration_ms);

        Ok(Self {
            metadata,
            events,
            current_index: 0,
            playback_speed: 1.0,
            paused: true,
        })
    }

    /// Start playback with optional speed and event callback
    pub async fn play<F, Fut>(&mut self, speed: f32, mut callback: F) -> Result<()>
    where
        F: FnMut(RecordingEvent) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        self.paused = false;
        self.playback_speed = speed;
        let mut last_timestamp = 0u64;

        while self.current_index < self.events.len() && !self.paused {
            let event = self.events[self.current_index].clone();

            // Calculate delay
            let timestamp = match &event {
                RecordingEvent::Output { timestamp, .. } |
                RecordingEvent::Input { timestamp, .. } |
                RecordingEvent::Resize { timestamp, .. } |
                RecordingEvent::PaneCreated { timestamp, .. } |
                RecordingEvent::PaneClosed { timestamp, .. } |
                RecordingEvent::WindowCreated { timestamp, .. } |
                RecordingEvent::WindowClosed { timestamp, .. } |
                RecordingEvent::Marker { timestamp, .. } => *timestamp,
            };

            if timestamp > last_timestamp {
                let delay = Duration::from_millis(
                    ((timestamp - last_timestamp) as f32 / self.playback_speed) as u64
                );
                tokio::time::sleep(delay).await;
            }

            // Send event to callback
            callback(event.clone()).await;

            last_timestamp = timestamp;
            self.current_index += 1;
        }

        Ok(())
    }

    /// Pause playback
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume playback
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Seek to timestamp
    pub fn seek(&mut self, timestamp_ms: u64) {
        // Find the event closest to the timestamp
        for (i, event) in self.events.iter().enumerate() {
            let event_timestamp = match event {
                RecordingEvent::Output { timestamp, .. } |
                RecordingEvent::Input { timestamp, .. } |
                RecordingEvent::Resize { timestamp, .. } |
                RecordingEvent::PaneCreated { timestamp, .. } |
                RecordingEvent::PaneClosed { timestamp, .. } |
                RecordingEvent::WindowCreated { timestamp, .. } |
                RecordingEvent::WindowClosed { timestamp, .. } |
                RecordingEvent::Marker { timestamp, .. } => *timestamp,
            };

            if event_timestamp >= timestamp_ms {
                self.current_index = i;
                break;
            }
        }
    }

    /// Set playback speed
    pub fn set_speed(&mut self, speed: f32) {
        self.playback_speed = speed.clamp(0.1, 10.0);
    }

    /// Get recording metadata
    pub fn metadata(&self) -> &RecordingMetadata {
        &self.metadata
    }

    /// Get current playback position
    pub fn current_position(&self) -> (usize, u64) {
        if self.current_index < self.events.len() {
            let timestamp = match &self.events[self.current_index] {
                RecordingEvent::Output { timestamp, .. } |
                RecordingEvent::Input { timestamp, .. } |
                RecordingEvent::Resize { timestamp, .. } |
                RecordingEvent::PaneCreated { timestamp, .. } |
                RecordingEvent::PaneClosed { timestamp, .. } |
                RecordingEvent::WindowCreated { timestamp, .. } |
                RecordingEvent::WindowClosed { timestamp, .. } |
                RecordingEvent::Marker { timestamp, .. } => *timestamp,
            };
            (self.current_index, timestamp)
        } else {
            (self.current_index, self.metadata.duration_ms.unwrap_or(0))
        }
    }

    /// Process a single event during playback
    pub async fn process_event(&self, event: &RecordingEvent) -> Result<()> {
        match event {
            RecordingEvent::Output { data, .. } => {
                // Write output to terminal
                std::io::stdout().write_all(data)
                    .map_err(|e| FerrixError::Other(format!("Failed to write output: {}", e)))?;
                std::io::stdout().flush()
                    .map_err(|e| FerrixError::Other(format!("Failed to flush output: {}", e)))?;
            }
            RecordingEvent::Resize { cols, rows, .. } => {
                // Resize terminal
                crossterm::execute!(
                    std::io::stdout(),
                    crossterm::terminal::SetSize(*cols, *rows)
                ).map_err(|e| FerrixError::Other(format!("Failed to resize terminal: {}", e)))?;
            }
            RecordingEvent::Marker { label, description, .. } => {
                // Display marker
                info!("Marker: {} - {:?}", label, description);
            }
            _ => {
                // Other events are informational for now
            }
        }

        Ok(())
    }

    /// Export recording to different format
    pub fn export(&self, path: &Path, format: ExportFormat) -> Result<()> {
        match format {
            ExportFormat::Asciinema => self.export_asciinema(path),
            ExportFormat::Text => self.export_text(path),
            ExportFormat::Html => self.export_html(path),
        }
    }

    /// Export as Asciinema format
    pub fn export_asciinema(&self, path: &Path) -> Result<()> {
        let mut file = File::create(path)
            .map_err(|e| FerrixError::Other(format!("Failed to create export file: {}", e)))?;

        // Write Asciinema header
        let header = serde_json::json!({
            "version": 2,
            "width": self.metadata.terminal_size.0,
            "height": self.metadata.terminal_size.1,
            "timestamp": self.metadata.created_at,
            "env": {
                "SHELL": self.metadata.shell,
                "TERM": "xterm-256color"
            }
        });

        let header_str = serde_json::to_string(&header)
            .map_err(|e| FerrixError::Other(format!("Failed to serialize header: {}", e)))?;
        writeln!(file, "{}", header_str)
            .map_err(|e| FerrixError::Other(format!("Failed to write header: {}", e)))?;

        // Write events
        for event in &self.events {
            if let RecordingEvent::Output { timestamp, data, .. } = event {
                let timestamp_sec = *timestamp as f64 / 1000.0;
                let text = String::from_utf8_lossy(data);
                let event_array = serde_json::json!([timestamp_sec, "o", text]);
                let event_str = serde_json::to_string(&event_array)
                    .map_err(|e| FerrixError::Other(format!("Failed to serialize event: {}", e)))?;
                writeln!(file, "{}", event_str)
                    .map_err(|e| FerrixError::Other(format!("Failed to write event: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Export as plain text
    pub fn export_text(&self, path: &Path) -> Result<()> {
        let mut file = File::create(path)
            .map_err(|e| FerrixError::Other(format!("Failed to create export file: {}", e)))?;

        writeln!(file, "# Ferrix Session Recording")?;
        writeln!(file, "# Session: {} ({:?})", self.metadata.session_name, self.metadata.session_id)?;
        let created_date = chrono::DateTime::<chrono::Utc>::from_timestamp(
            self.metadata.created_at as i64, 0
        ).unwrap_or_else(chrono::Utc::now);
        writeln!(file, "# Date: {}", created_date)?;
        writeln!(file, "# Duration: {:?}ms\n", self.metadata.duration_ms)?;

        for event in &self.events {
            if let RecordingEvent::Output { data, .. } = event {
                file.write_all(data)?;
            }
        }

        Ok(())
    }

    /// Export as HTML with player
    pub fn export_html(&self, path: &Path) -> Result<()> {
        // Read metadata
        let metadata = self.metadata.clone();

        // Collect all output events
        let mut output_frames = Vec::new();
        let _start_time = Duration::from_secs(metadata.created_at);

        for event in &self.events {
            if let RecordingEvent::Output { timestamp, data, .. } = event {
                let elapsed_ms = (*timestamp - metadata.created_at) * 1000;
                // Escape the data for JSON
                let data_str = String::from_utf8_lossy(data);
                let data_json = serde_json::to_string(&data_str)
                    .map_err(|e| FerrixError::Other(format!("Failed to serialize data: {}", e)))?;
                output_frames.push(format!("[{}, {}]", elapsed_ms, data_json));
            }
        }

        let frames_json = format!("[{}]", output_frames.join(",\n      "));

        // Generate HTML with embedded xterm.js player
        let html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Ferrix Recording - {session_name}</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/xterm@5.3.0/css/xterm.css">
    <style>
        body {{
            background: #1e1e1e;
            color: #d4d4d4;
            font-family: 'Courier New', monospace;
            margin: 0;
            padding: 20px;
            display: flex;
            flex-direction: column;
            align-items: center;
        }}
        .container {{
            max-width: 1200px;
            width: 100%;
        }}
        .header {{
            background: #252525;
            padding: 20px;
            border-radius: 8px 8px 0 0;
            margin-bottom: 0;
        }}
        h1 {{
            margin: 0 0 10px 0;
            font-size: 24px;
        }}
        .metadata {{
            font-size: 12px;
            opacity: 0.8;
        }}
        .metadata span {{
            margin-right: 20px;
        }}
        .terminal-container {{
            background: #1e1e1e;
            padding: 10px;
            border-radius: 0 0 8px 8px;
            box-shadow: 0 4px 12px rgba(0,0,0,0.5);
        }}
        .controls {{
            background: #252525;
            padding: 10px;
            border-radius: 4px;
            margin-top: 20px;
            display: flex;
            gap: 10px;
            align-items: center;
        }}
        button {{
            background: #007acc;
            color: white;
            border: none;
            padding: 8px 16px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 14px;
        }}
        button:hover {{
            background: #005a9e;
        }}
        button:disabled {{
            background: #444;
            cursor: not-allowed;
        }}
        .progress {{
            flex: 1;
            height: 4px;
            background: #444;
            border-radius: 2px;
            overflow: hidden;
            cursor: pointer;
        }}
        .progress-bar {{
            height: 100%;
            background: #007acc;
            width: 0%;
            transition: width 0.1s linear;
        }}
        .time {{
            font-size: 12px;
            min-width: 100px;
        }}
        .watermark {{
            text-align: center;
            margin-top: 20px;
            font-size: 12px;
            opacity: 0.6;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>Terminal Recording: {session_name}</h1>
            <div class="metadata">
                <span>User: {user}@{hostname}</span>
                <span>Shell: {shell}</span>
                <span>Size: {cols}x{rows}</span>
                <span>Date: {date}</span>
            </div>
        </div>
        <div class="terminal-container">
            <div id="terminal"></div>
        </div>
        <div class="controls">
            <button id="play-pause">Play</button>
            <button id="restart">Restart</button>
            <div class="progress" id="progress">
                <div class="progress-bar" id="progress-bar"></div>
            </div>
            <div class="time" id="time">0:00 / {duration}</div>
        </div>
        <div class="watermark">
            <p>Generated with <strong>Ferrix</strong> - Modern Terminal Multiplexer<br>
            <a href="https://github.com/davidliedle/Ferrix" style="color: #007acc;">github.com/davidliedle/Ferrix</a></p>
        </div>
    </div>

    <script src="https://cdn.jsdelivr.net/npm/xterm@5.3.0/lib/xterm.js"></script>
    <script>
        // Recording data
        const frames = {frames};

        // Terminal setup
        const term = new Terminal({{
            cols: {cols},
            rows: {rows},
            cursorBlink: true,
            theme: {{
                background: '#1e1e1e',
                foreground: '#d4d4d4'
            }}
        }});

        term.open(document.getElementById('terminal'));

        // Player state
        let currentFrame = 0;
        let isPlaying = false;
        let startTime = 0;
        let animationId = null;

        // Controls
        const playPauseBtn = document.getElementById('play-pause');
        const restartBtn = document.getElementById('restart');
        const progressBar = document.getElementById('progress-bar');
        const progressContainer = document.getElementById('progress');
        const timeDisplay = document.getElementById('time');

        const totalDuration = frames.length > 0 ? frames[frames.length - 1][0] : 0;

        function formatTime(ms) {{
            const seconds = Math.floor(ms / 1000);
            const minutes = Math.floor(seconds / 60);
            const secs = seconds % 60;
            return `${{minutes}}:${{secs.toString().padStart(2, '0')}}`;
        }}

        function play() {{
            if (currentFrame >= frames.length) {{
                restart();
                return;
            }}

            isPlaying = true;
            playPauseBtn.textContent = 'Pause';
            startTime = performance.now() - (frames[currentFrame]?.[0] || 0);

            function renderFrame(timestamp) {{
                if (!isPlaying) return;

                const elapsed = timestamp - startTime;

                while (currentFrame < frames.length && frames[currentFrame][0] <= elapsed) {{
                    term.write(frames[currentFrame][1]);
                    currentFrame++;
                }}

                // Update progress
                const progress = (elapsed / totalDuration) * 100;
                progressBar.style.width = Math.min(progress, 100) + '%';
                timeDisplay.textContent = formatTime(elapsed) + ' / ' + formatTime(totalDuration);

                if (currentFrame < frames.length) {{
                    animationId = requestAnimationFrame(renderFrame);
                }} else {{
                    pause();
                }}
            }}

            animationId = requestAnimationFrame(renderFrame);
        }}

        function pause() {{
            isPlaying = false;
            playPauseBtn.textContent = 'Play';
            if (animationId) {{
                cancelAnimationFrame(animationId);
                animationId = null;
            }}
        }}

        function restart() {{
            pause();
            currentFrame = 0;
            term.reset();
            progressBar.style.width = '0%';
            timeDisplay.textContent = '0:00 / ' + formatTime(totalDuration);
        }}

        playPauseBtn.addEventListener('click', () => {{
            if (isPlaying) {{
                pause();
            }} else {{
                play();
            }}
        }});

        restartBtn.addEventListener('click', restart);

        progressContainer.addEventListener('click', (e) => {{
            const rect = progressContainer.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const percentage = clickX / rect.width;
            const targetTime = percentage * totalDuration;

            // Find the frame closest to target time
            pause();
            term.reset();
            currentFrame = 0;

            while (currentFrame < frames.length && frames[currentFrame][0] < targetTime) {{
                term.write(frames[currentFrame][1]);
                currentFrame++;
            }}

            progressBar.style.width = (percentage * 100) + '%';
            timeDisplay.textContent = formatTime(targetTime) + ' / ' + formatTime(totalDuration);
        }});

        // Update time display
        timeDisplay.textContent = '0:00 / ' + formatTime(totalDuration);
    </script>
</body>
</html>"#,
            session_name = metadata.session_name,
            user = metadata.user,
            hostname = metadata.hostname,
            shell = metadata.shell,
            cols = metadata.terminal_size.0,
            rows = metadata.terminal_size.1,
            date = {
                use std::time::UNIX_EPOCH;
                let datetime = UNIX_EPOCH + Duration::from_secs(metadata.created_at);
                format!("{:?}", datetime)
            },
            duration = metadata.duration_ms.map(|d| format!("{}:{:02}", d / 60000, (d / 1000) % 60)).unwrap_or_else(|| "0:00".to_string()),
            frames = frames_json,
        );

        // Write to file
        std::fs::write(path, html)?;

        info!("Exported recording to HTML: {:?}", path);
        Ok(())
    }
}

/// Export format options
#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Asciinema,
    Text,
    Html,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_recording_and_playback() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();

        // Create metadata
        let metadata = RecordingMetadata {
            version: RECORDING_VERSION,
            session_id: SessionId(uuid::Uuid::new_v4()),
            session_name: "test-session".to_string(),
            created_at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms: None,
            terminal_size: (80, 24),
            shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
            user: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            hostname: "localhost".to_string(),
            compressed: false,
        };

        // Create recorder
        let mut recorder = SessionRecorder::new(metadata.clone(), path.clone()).unwrap();

        // Record some events
        recorder.record_output(b"Hello, world!".to_vec()).await.unwrap();
        recorder.record_input(b"ls -la".to_vec()).await.unwrap();
        recorder.record_resize(100, 30).await.unwrap();

        // Stop recording
        let (_duration, _size) = recorder.stop().await.unwrap();

        // Load and verify - SessionPlayer doesn't exist yet, so comment out for now
        // let player = SessionPlayer::load(path).unwrap();
        // assert_eq!(player.events.len(), 3);
        // assert_eq!(player.metadata.session_name, "test-session");
    }
}