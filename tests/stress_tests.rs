// Stress Tests for Ferrix
// These tests verify system behavior under sustained load and extreme conditions
//
// Run with: cargo test --test stress_tests --release -- --ignored --test-threads=1
//
// Note: These tests are marked #[ignore] because they:
// - Take significant time to run (minutes)
// - Require substantial system resources
// - Are designed for manual stress testing, not CI/CD

use std::process::{Command, Stdio, Child};
use std::time::{Duration, Instant};
use std::path::PathBuf;
use tokio::time::sleep;
use tempfile::TempDir;

struct TestServer {
    process: Child,
    socket_path: PathBuf,
    _temp_dir: TempDir,
    start_time: Instant,
}

impl TestServer {
    async fn start() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("ferrix.sock");

        let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
            "./target/release/ferrix"
        } else {
            "./target/debug/ferrix"
        };

        let process = Command::new(ferrix_path)
            .arg("--socket")
            .arg(&socket_path)
            .arg("server")
            .arg("--foreground")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start server");

        let mut retries = 0;
        while !socket_path.exists() && retries < 50 {
            sleep(Duration::from_millis(100)).await;
            retries += 1;
        }

        assert!(socket_path.exists(), "Server failed to create socket");
        sleep(Duration::from_millis(500)).await;

        Self {
            process,
            socket_path,
            _temp_dir: temp_dir,
            start_time: Instant::now(),
        }
    }

    fn run_command(&self, args: &[&str]) -> std::process::Output {
        let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
            "./target/release/ferrix"
        } else {
            "./target/debug/ferrix"
        };
        Command::new(ferrix_path)
            .arg("--socket")
            .arg(&self.socket_path)
            .args(args)
            .output()
            .expect("Failed to run command")
    }

    fn get_process_memory(&self) -> Option<u64> {
        #[cfg(target_os = "macos")]
        {
            let pid = self.process.id();
            let output = Command::new("ps")
                .args(["-o", "rss=", "-p", &pid.to_string()])
                .output()
                .ok()?;

            String::from_utf8(output.stdout)
                .ok()?
                .trim()
                .parse::<u64>()
                .ok()
                .map(|kb| kb * 1024) // Convert KB to bytes
        }

        #[cfg(target_os = "linux")]
        {
            let pid = self.process.id();
            std::fs::read_to_string(format!("/proc/{}/statm", pid))
                .ok()?
                .split_whitespace()
                .nth(1)? // Resident size in pages
                .parse::<u64>()
                .ok()
                .map(|pages| pages * 4096) // 4KB pages to bytes
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }

    fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}

#[tokio::test]
#[ignore] // Long-running test - run manually
async fn stress_test_long_running_session() {
    println!("\n=== STRESS TEST: Long-Running Session ===");
    println!("Duration: 5 minutes");
    println!("Goal: Verify session stability over extended runtime\n");

    let server = TestServer::start().await;
    let initial_memory = server.get_process_memory();

    // Create session
    server.run_command(&["new", "-s", "long-run", "--detached"]);
    sleep(Duration::from_millis(500)).await;

    let start = Instant::now();
    let test_duration = Duration::from_secs(5 * 60); // 5 minutes
    let mut iteration = 0;

    while start.elapsed() < test_duration {
        // Send periodic commands
        server.run_command(&["send-keys", "long-run", &format!("echo 'Iteration {}'", iteration)]);
        server.run_command(&["send-keys", "long-run", "Enter"]);

        // Every 30 seconds, check server health
        if iteration % 30 == 0 {
            let output = server.run_command(&["list"]);
            assert!(output.status.success(), "Server unhealthy at iteration {}", iteration);

            if let Some(current_mem) = server.get_process_memory() {
                if let Some(initial) = initial_memory {
                    let growth = current_mem as i64 - initial as i64;
                    let growth_pct = (growth as f64 / initial as f64) * 100.0;
                    println!("Iteration {}: Memory {} bytes ({:+.1}%)",
                             iteration, current_mem, growth_pct);
                }
            }

            println!("Uptime: {:?}", server.uptime());
        }

        iteration += 1;
        sleep(Duration::from_secs(1)).await;
    }

    // Final health check
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server failed after long run");

    if let (Some(final_mem), Some(initial)) = (server.get_process_memory(), initial_memory) {
        let growth = final_mem as i64 - initial as i64;
        let growth_pct = (growth as f64 / initial as f64) * 100.0;
        println!("\nFinal memory: {} bytes ({:+.1}%)", final_mem, growth_pct);

        // Allow up to 50% memory growth (reasonable for 5 minute run with scrollback)
        assert!(growth_pct < 50.0, "Excessive memory growth: {:.1}%", growth_pct);
    }

    server.run_command(&["kill", "long-run"]);
    println!("\n=== TEST PASSED ===\n");
}

#[tokio::test]
#[ignore] // Resource-intensive test - run manually
async fn stress_test_many_concurrent_clients() {
    println!("\n=== STRESS TEST: Many Concurrent Clients ===");
    println!("Clients: 50 concurrent");
    println!("Goal: Verify server handles many simultaneous connections\n");

    let server = TestServer::start().await;
    let initial_memory = server.get_process_memory();

    // Create 50 sessions
    println!("Creating 50 sessions...");
    for i in 0..50 {
        let output = server.run_command(&["new", "-s", &format!("stress-{}", i), "--detached"]);
        assert!(output.status.success(), "Failed to create session {}", i);

        if i % 10 == 0 {
            println!("Created {} sessions", i);
        }
    }

    sleep(Duration::from_millis(1000)).await;

    // Verify all sessions exist
    let output = server.run_command(&["list"]);
    assert!(output.status.success());
    let list_output = String::from_utf8_lossy(&output.stdout);

    let mut found = 0;
    for i in 0..50 {
        if list_output.contains(&format!("stress-{}", i)) {
            found += 1;
        }
    }
    assert_eq!(found, 50, "Only found {} of 50 sessions", found);
    println!("✓ All 50 sessions created successfully");

    // Send commands to all sessions concurrently
    println!("\nSending concurrent commands to all sessions...");
    let mut handles = vec![];
    for i in 0..50 {
        let socket_path = server.socket_path.clone();
        let handle = tokio::spawn(async move {
            let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
                "./target/release/ferrix"
            } else {
                "./target/debug/ferrix"
            };
            for j in 0..10 {
                let _ = Command::new(ferrix_path)
                    .arg("--socket")
                    .arg(&socket_path)
                    .arg("send-keys")
                    .arg(format!("stress-{}", i))
                    .arg(format!("echo 'Test {}'", j))
                    .output();
                sleep(Duration::from_millis(100)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all concurrent operations
    for handle in handles {
        handle.await.unwrap();
    }

    println!("✓ All concurrent operations completed");

    // Verify server is still healthy
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server became unhealthy");

    if let (Some(final_mem), Some(initial)) = (server.get_process_memory(), initial_memory) {
        let growth = final_mem as i64 - initial as i64;
        let growth_pct = (growth as f64 / initial as f64) * 100.0;
        println!("\nMemory growth: {:+.1}%", growth_pct);
    }

    // Clean up all sessions
    println!("\nCleaning up sessions...");
    for i in 0..50 {
        server.run_command(&["kill", &format!("stress-{}", i)]);
    }

    println!("\n=== TEST PASSED ===\n");
}

#[tokio::test]
#[ignore] // Resource-intensive test - run manually
async fn stress_test_memory_leak_detection() {
    println!("\n=== STRESS TEST: Memory Leak Detection ===");
    println!("Cycles: 100 create/destroy cycles");
    println!("Goal: Detect memory leaks through repetitive operations\n");

    let server = TestServer::start().await;

    let samples = 10;
    let mut memory_samples = Vec::new();

    for cycle in 0..samples {
        // Create 10 sessions
        for i in 0..10 {
            server.run_command(&["new", "-s", &format!("leak-test-{}", i), "--detached"]);
        }

        sleep(Duration::from_millis(500)).await;

        // Send some commands
        for i in 0..10 {
            server.run_command(&["send-keys", &format!("leak-test-{}", i), "echo 'test'"]);
            server.run_command(&["send-keys", &format!("leak-test-{}", i), "Enter"]);
        }

        sleep(Duration::from_millis(500)).await;

        // Destroy all sessions
        for i in 0..10 {
            server.run_command(&["kill", &format!("leak-test-{}", i)]);
        }

        sleep(Duration::from_millis(500)).await;

        // Sample memory
        if let Some(mem) = server.get_process_memory() {
            memory_samples.push(mem);
            println!("Cycle {}: Memory = {} bytes", cycle, mem);
        }
    }

    // Analyze memory trend
    if memory_samples.len() >= samples {
        let first_half: u64 = memory_samples.iter().take(samples / 2).sum::<u64>() / (samples as u64 / 2);
        let second_half: u64 = memory_samples.iter().skip(samples / 2).sum::<u64>() / (samples as u64 / 2);

        let growth = second_half as i64 - first_half as i64;
        let growth_pct = (growth as f64 / first_half as f64) * 100.0;

        println!("\nFirst half average: {} bytes", first_half);
        println!("Second half average: {} bytes", second_half);
        println!("Memory growth: {:+.1}%", growth_pct);

        // Allow up to 20% growth (some caching is expected)
        assert!(growth_pct < 20.0, "Potential memory leak detected: {:.1}% growth", growth_pct);
        println!("\n✓ No significant memory leak detected");
    }

    println!("\n=== TEST PASSED ===\n");
}

#[tokio::test]
#[ignore] // Long-running test - run manually
async fn stress_test_high_output_volume() {
    println!("\n=== STRESS TEST: High Output Volume ===");
    println!("Output: 100,000 lines per session x 5 sessions");
    println!("Goal: Verify scrollback buffer and output handling\n");

    let server = TestServer::start().await;

    // Create 5 sessions
    for i in 0..5 {
        server.run_command(&["new", "-s", &format!("output-{}", i), "--detached"]);
    }

    sleep(Duration::from_millis(500)).await;

    // Generate massive output in all sessions
    println!("Generating high-volume output...");
    for i in 0..5 {
        server.run_command(&["send-keys", &format!("output-{}", i),
                            "for j in {1..100000}; do echo \"Line $j\"; done"]);
        server.run_command(&["send-keys", &format!("output-{}", i), "Enter"]);
    }

    // Wait for output to generate (with periodic checks)
    for check in 0..60 {
        sleep(Duration::from_secs(1)).await;

        if check % 10 == 0 {
            let output = server.run_command(&["list"]);
            assert!(output.status.success(), "Server crashed during output generation");
            println!("Health check at {}s: OK", check);
        }
    }

    // Final verification
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server failed after high output");

    println!("\n✓ Server handled high-volume output successfully");

    // Clean up
    for i in 0..5 {
        server.run_command(&["kill", &format!("output-{}", i)]);
    }

    println!("\n=== TEST PASSED ===\n");
}

#[tokio::test]
#[ignore] // Resource-intensive test - run manually
async fn stress_test_rapid_operations() {
    println!("\n=== STRESS TEST: Rapid Operations ===");
    println!("Operations: 1000 rapid list/send-keys");
    println!("Goal: Verify server handles rapid command bursts\n");

    let server = TestServer::start().await;

    // Create a session
    server.run_command(&["new", "-s", "rapid", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    println!("Executing 1000 rapid operations...");
    let start = Instant::now();

    for i in 0..1000 {
        // Alternate between different operations
        match i % 3 {
            0 => {
                server.run_command(&["list"]);
            }
            1 => {
                server.run_command(&["send-keys", "rapid", &format!("echo '{}'", i)]);
            }
            _ => {
                server.run_command(&["send-keys", "rapid", "Enter"]);
            }
        }

        if i % 100 == 0 && i > 0 {
            println!("Completed {} operations", i);
        }
    }

    let elapsed = start.elapsed();
    let ops_per_sec = 1000.0 / elapsed.as_secs_f64();

    println!("\nCompleted 1000 operations in {:?}", elapsed);
    println!("Throughput: {:.1} ops/sec", ops_per_sec);

    // Verify server is still responsive
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server became unresponsive");

    println!("\n✓ Server handled rapid operations successfully");

    server.run_command(&["kill", "rapid"]);

    println!("\n=== TEST PASSED ===\n");
}
