use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use ferrix::server::performance::{OutputBuffer, PerformanceConfig, AnsiOptimizer, DeltaCompressor};
use ferrix::server::scrollback::LineScrollback;
use tokio::runtime::Runtime;

fn bench_output_buffer(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let mut group = c.benchmark_group("output_buffer");

    // Test different data sizes
    for size in &[1024, 10240, 102400, 1024000] {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("write", size),
            size,
            |b, &size| {
                b.to_async(&runtime).iter(|| async {
                    let config = PerformanceConfig::default();
                    let buffer = OutputBuffer::new(config);
                    let data = vec![0u8; size];
                    buffer.write(black_box(data)).await.unwrap();
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("write_and_read", size),
            size,
            |b, &size| {
                b.to_async(&runtime).iter(|| async {
                    let config = PerformanceConfig::default();
                    let buffer = OutputBuffer::new(config);
                    let data = vec![0u8; size];
                    buffer.write(black_box(data)).await.unwrap();
                    while buffer.read_batch().await.is_some() {}
                });
            },
        );
    }

    group.finish();
}

fn bench_scrollback_buffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("scrollback_buffer");

    // Test different line counts
    for lines in &[100, 1000, 10000, 50000] {
        group.bench_with_input(
            BenchmarkId::new("push_lines", lines),
            lines,
            |b, &lines| {
                b.iter(|| {
                    let mut buffer = LineScrollback::new(lines);
                    for i in 0..lines {
                        let line = format!("Line {} with some terminal output data", i);
                        buffer.push(black_box(line));
                    }
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("search", lines),
            lines,
            |b, &lines| {
                let mut buffer = LineScrollback::new(lines);
                for i in 0..lines {
                    let line = format!("Line {} with some terminal output data", i);
                    buffer.push(line);
                }

                b.iter(|| {
                    buffer.search(black_box("terminal"), false);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("last_n", lines),
            lines,
            |b, &lines| {
                let mut buffer = LineScrollback::new(lines);
                for i in 0..lines {
                    let line = format!("Line {} with some terminal output data", i);
                    buffer.push(line);
                }

                b.iter(|| {
                    let _: Vec<_> = buffer.last_n(black_box(100)).collect();
                });
            },
        );
    }

    group.finish();
}

fn bench_ansi_optimizer(c: &mut Criterion) {
    let mut group = c.benchmark_group("ansi_optimizer");

    // Create test data with ANSI sequences
    let simple_data = b"Hello World\n".to_vec();
    let ansi_data = b"\x1b[31mRed\x1b[32mGreen\x1b[34mBlue\x1b[0mNormal\n".to_vec();
    let heavy_ansi_data = {
        let mut data = Vec::new();
        for i in 0..100 {
            data.extend_from_slice(
                format!("\x1b[{}mColor{}\x1b[0m ", 30 + (i % 8), i).as_bytes()
            );
        }
        data
    };

    group.bench_function("simple_text", |b| {
        let mut optimizer = AnsiOptimizer::new();
        b.iter(|| {
            optimizer.optimize(black_box(&simple_data))
        });
    });

    group.bench_function("moderate_ansi", |b| {
        let mut optimizer = AnsiOptimizer::new();
        b.iter(|| {
            optimizer.optimize(black_box(&ansi_data))
        });
    });

    group.bench_function("heavy_ansi", |b| {
        let mut optimizer = AnsiOptimizer::new();
        b.iter(|| {
            optimizer.optimize(black_box(&heavy_ansi_data))
        });
    });

    group.finish();
}

fn bench_delta_compressor(c: &mut Criterion) {
    let mut group = c.benchmark_group("delta_compressor");

    // Test data with varying similarity
    let frame1 = vec![1u8; 10000];
    let frame2_identical = frame1.clone();
    let mut frame2_minor_change = frame1.clone();
    frame2_minor_change[5000] = 2;

    let frame2_major_change: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

    group.bench_function("identical_frames", |b| {
        let mut compressor = DeltaCompressor::new(true);
        compressor.compress(&frame1);
        b.iter(|| {
            compressor.compress(black_box(&frame2_identical))
        });
    });

    group.bench_function("minor_changes", |b| {
        let mut compressor = DeltaCompressor::new(true);
        compressor.compress(&frame1);
        b.iter(|| {
            compressor.compress(black_box(&frame2_minor_change))
        });
    });

    group.bench_function("major_changes", |b| {
        let mut compressor = DeltaCompressor::new(true);
        compressor.compress(&frame1);
        b.iter(|| {
            compressor.compress(black_box(&frame2_major_change))
        });
    });

    group.finish();
}

fn bench_large_output_scenario(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let mut group = c.benchmark_group("real_world_scenario");

    // Simulate `find /` or similar command with large output
    group.bench_function("find_command_simulation", |b| {
        b.to_async(&runtime).iter(|| async {
            let config = PerformanceConfig {
                batch_size: 64 * 1024,
                adaptive_batching: true,
                enable_compression: true,
                compression_threshold: 100 * 1024,
                ..PerformanceConfig::default()
            };

            let buffer = OutputBuffer::new(config);

            // Simulate 1MB of find output
            for _ in 0..100 {
                let chunk = vec![b'/' ; 10240]; // 10KB chunks
                buffer.write(black_box(chunk)).await.unwrap();
            }

            // Read all batches
            while buffer.read_batch().await.is_some() {}
        });
    });

    // Simulate `cat large_file.log` with continuous output
    group.bench_function("cat_large_log", |b| {
        b.to_async(&runtime).iter(|| async {
            let config = PerformanceConfig::default();
            let buffer = OutputBuffer::new(config);
            let mut scrollback = LineScrollback::new(10000);

            // Simulate 5MB log file
            for i in 0..5000 {
                let line = format!(
                    "[2024-01-01 12:00:{:02}] INFO: Processing request {} with data...\n",
                    i % 60, i
                );
                let data = line.as_bytes().to_vec();

                // Write to buffer
                buffer.write(black_box(data.clone())).await.unwrap();

                // Also update scrollback
                scrollback.push(line);
            }

            // Process all batches
            while buffer.read_batch().await.is_some() {}
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_output_buffer,
    bench_scrollback_buffer,
    bench_ansi_optimizer,
    bench_delta_compressor,
    bench_large_output_scenario
);
criterion_main!(benches);