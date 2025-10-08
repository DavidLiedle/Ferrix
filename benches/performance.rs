// Performance benchmarks for Ferrix
// Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use ferrix::client::ansi_parser::AnsiParser;
use ferrix::protocol::{ClientMessage, ServerMessage, SessionId, WindowId, PaneId};
use ferrix::server::snapshot::{SessionSnapshot, SessionState, WindowState, PaneState, SnapshotMetadata};
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
        bench_multiple_panes
}

criterion_main!(benches);
