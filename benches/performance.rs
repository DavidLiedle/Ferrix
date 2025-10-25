// Performance benchmarks for Ferrix
// Run with: cargo bench
//
// These benchmarks validate performance optimizations including:
// - Lock-free config access (v0.22.0)
// - Clone reduction in rendering (v0.22.0)
// - Keystroke latency
// - Render throughput

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use ferrix::client::ansi_parser::AnsiParser;
use ferrix::protocol::{ClientMessage, ServerMessage, SessionId, WindowId, PaneId};
use ferrix::server::snapshot::{SessionSnapshot, SessionState, WindowState, PaneState, SnapshotMetadata};
use ferrix::config::Config;
use ferrix::config::keybindings::KeyBindingManager;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::Arc;
use std::time::Duration;
use chrono::Utc;
use std::path::PathBuf;

fn bench_ansi_parser_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("ansi_parser");

    // Test different data sizes
    for size in [100, 1000, 10000, 100000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let mut parser = AnsiParser::new(80, 24);
            let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();

            b.iter(|| {
                parser.process(black_box(&data));
            });
        });
    }

    group.finish();
}

fn bench_ansi_parser_escape_sequences(c: &mut Criterion) {
    let mut parser = AnsiParser::new(80, 24);

    c.bench_function("ansi_escape_processing", |b| {
        let data = b"\x1b[31mRed text\x1b[0m\x1b[1mBold\x1b[0m\x1b[2J\x1b[H";
        b.iter(|| {
            parser.process(black_box(data));
        });
    });
}

fn bench_protocol_serialization(c: &mut Criterion) {
    c.bench_function("serialize_client_message", |b| {
        let msg = ClientMessage::CreateSession {
            name: Some("benchmark-session".to_string()),
        };

        b.iter(|| {
            black_box(bincode::serialize(&msg).unwrap());
        });
    });

    c.bench_function("serialize_server_message", |b| {
        use uuid::Uuid;
        let msg = ServerMessage::SessionCreated {
            session_id: SessionId(Uuid::new_v4()),
            name: "benchmark-session".to_string(),
        };

        b.iter(|| {
            black_box(bincode::serialize(&msg).unwrap());
        });
    });
}

fn bench_snapshot_operations(c: &mut Criterion) {
    use uuid::Uuid;

    c.bench_function("create_snapshot", |b| {
        b.iter(|| {
            let session_id = SessionId(Uuid::new_v4());
            let window_id = WindowId(Uuid::new_v4());
            let pane_id = PaneId(Uuid::new_v4());
            let now = Utc::now();

            let snapshot = SessionSnapshot {
                metadata: SnapshotMetadata {
                    id: Uuid::new_v4(),
                    name: "benchmark".to_string(),
                    description: "Benchmark snapshot".to_string(),
                    created_at: now,
                    ferrix_version: "0.10.2".to_string(),
                    checksum: None,
                },
                session: SessionState {
                    id: session_id.clone(),
                    name: "benchmark".to_string(),
                    working_directory: std::path::PathBuf::from("/tmp"),
                    created_at: now,
                    current_window: Some(window_id.clone()),
                    environment: vec![],
                },
                windows: vec![
                    WindowState {
                        id: window_id.clone(),
                        session_id: session_id.clone(),
                        name: "main".to_string(),
                        index: 0,
                        layout: ferrix::server::layout::Layout::new(pane_id.clone()),
                        current_pane: Some(pane_id.clone()),
                        width: 80,
                        height: 24,
                        panes: std::collections::HashMap::new(),
                    }
                ],
                panes: vec![
                    PaneState {
                        id: pane_id.clone(),
                        window_id: window_id.clone(),
                        working_directory: PathBuf::from("/tmp"),
                        command: "bash".to_string(),
                        cols: 80,
                        rows: 24,
                        scrollback: vec![],
                        cursor_position: (0, 0),
                    }
                ],
                created_at: now,
                environment: std::collections::HashMap::new(),
                config: None,
            };

            black_box(snapshot);
        });
    });
}

fn bench_multiple_panes(c: &mut Criterion) {
    let mut group = c.benchmark_group("multiple_panes");

    for pane_count in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(pane_count), pane_count, |b, &count| {
            b.iter(|| {
                let mut parsers: Vec<_> = (0..count)
                    .map(|_| AnsiParser::new(80, 24))
                    .collect();

                let data = b"Hello, world!\n";
                for parser in &mut parsers {
                    parser.process(black_box(data));
                }
            });
        });
    }

    group.finish();
}

// Benchmark lock-free keybinding access (v0.22.0 optimization)
fn bench_keybinding_access(c: &mut Criterion) {
    let manager = Arc::new(KeyBindingManager::new());
    let key_event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

    c.bench_function("lock_free_keybinding_lookup", |b| {
        b.iter(|| {
            // Direct Arc access - no async RwLock acquisition
            black_box(manager.get_action(black_box(&key_event), false))
        })
    });
}

// Benchmark string clone reduction (v0.22.0 optimization)
fn bench_statusbar_rendering(c: &mut Criterion) {
    let mut group = c.benchmark_group("statusbar_rendering");

    let format_string = "#{session_name} - #{window_index}:#{window_name}".to_string();

    // Cached approach (optimized)
    group.bench_function("cached_format_strings", |b| {
        let cached_left = format_string.clone();
        let cached_center = format_string.clone();
        let cached_right = format_string.clone();

        b.iter(|| {
            // Clone cached strings once per render
            let left = black_box(cached_left.clone());
            let center = black_box(cached_center.clone());
            let right = black_box(cached_right.clone());
            black_box((left, center, right))
        })
    });

    // Nested access approach (pre-optimization)
    group.bench_function("nested_config_access", |b| {
        let config = Config::default();

        b.iter(|| {
            // Access nested config.status_bar.{left,center,right} each time
            let left = black_box(config.status_bar.left.clone());
            let center = black_box(config.status_bar.center.clone());
            let right = black_box(config.status_bar.right.clone());
            black_box((left, center, right))
        })
    });

    group.finish();
}

// Benchmark copy mode search optimization (v0.22.0)
fn bench_copy_mode_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("copy_mode_search");

    let search_query = "test_search_pattern".to_string();

    // Optimized: no clone
    group.bench_function("search_no_clone", |b| {
        let mut query = search_query.clone();

        b.iter(|| {
            // Modify query directly without cloning
            query.push('x');
            query.pop();
            black_box(&query)
        })
    });

    // Pre-optimization: clone each time
    group.bench_function("search_with_clone", |b| {
        let mut query = search_query.clone();

        b.iter(|| {
            query.push('x');
            let cloned = black_box(query.clone()); // Unnecessary clone
            query.pop();
            black_box(cloned)
        })
    });

    group.finish();
}

// Benchmark keystroke latency (target: <1ms)
fn bench_keystroke_latency(c: &mut Criterion) {
    let manager = Arc::new(KeyBindingManager::new());

    c.bench_function("keystroke_end_to_end", |b| {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        b.iter(|| {
            // Simulate full keystroke processing path
            let action = manager.get_action(&key, false);
            black_box(action)
        })
    });
}

// Benchmark render throughput (target: 60 FPS = 16.67ms per frame)
fn bench_render_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_throughput");
    group.measurement_time(Duration::from_secs(5));

    let mut parser = AnsiParser::new(80, 24);

    // Simulate a single frame render
    group.bench_function("single_frame_render", |b| {
        let frame_data = b"[test@host ~]$ ls\nfile1  file2  file3\n[test@host ~]$ ";

        b.iter(|| {
            parser.process(black_box(frame_data));
            black_box(())
        })
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets =
        bench_ansi_parser_throughput,
        bench_ansi_parser_escape_sequences,
        bench_protocol_serialization,
        bench_snapshot_operations,
        bench_multiple_panes,
        bench_keybinding_access,
        bench_statusbar_rendering,
        bench_copy_mode_search,
        bench_keystroke_latency,
        bench_render_frame
}

criterion_main!(benches);
