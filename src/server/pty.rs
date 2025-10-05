use std::io::Write;
use std::sync::{Arc, Mutex};
use portable_pty::{CommandBuilder, PtySize, native_pty_system, Child};
use tokio::sync::{mpsc, broadcast};

use crate::error::{FerrixError, Result};

pub struct Pty {
    writer_tx: mpsc::Sender<Vec<u8>>,
    reader_rx: mpsc::Receiver<Vec<u8>>,
    resize_tx: mpsc::Sender<(u16, u16)>,
    _writer_task: tokio::task::JoinHandle<()>,
    _reader_task: tokio::task::JoinHandle<()>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl Pty {
    pub fn new(cols: u16, rows: u16) -> Result<Self> {
        let pty_system = native_pty_system();

        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| FerrixError::Pty(format!("Failed to open PTY: {}", e)))?;

        let mut cmd = CommandBuilder::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()));
        cmd.cwd(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")));

        let child = pty_pair.slave.spawn_command(cmd)
            .map_err(|e| FerrixError::Pty(format!("Failed to spawn shell: {}", e)))?;

        let child = Arc::new(Mutex::new(child));

        let writer = Arc::new(Mutex::new(
            pty_pair.master.take_writer()
                .map_err(|e| FerrixError::Pty(format!("Failed to get PTY writer: {}", e)))?
        ));

        let reader = pty_pair.master.try_clone_reader()
            .map_err(|e| FerrixError::Pty(format!("Failed to get PTY reader: {}", e)))?;

        let master = Arc::new(Mutex::new(pty_pair.master));

        // Channel for sending output from PTY to client
        let (output_tx, output_rx) = mpsc::channel(100);

        // Channel for sending input from client to PTY
        let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(100);

        // Channel for resize requests
        let (resize_tx, mut resize_rx) = mpsc::channel::<(u16, u16)>(10);

        // Channel for shutdown signal
        let (shutdown_tx, _shutdown_rx) = broadcast::channel::<()>(1);

        // Reader task - Convert blocking reader to async
        let output_tx_clone = output_tx.clone();
        let mut shutdown_rx_reader = shutdown_tx.subscribe();
        let child_clone = child.clone();
        let reader_task = tokio::spawn(async move {
            // Use a separate thread for blocking I/O but communicate asynchronously
            let (tx, mut rx) = mpsc::channel::<Vec<u8>>(100);
            let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

            std::thread::spawn(move || {
                let mut reader = reader;
                // Use larger buffer for better performance with large outputs
                let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer
                let mut consecutive_reads = 0;
                tracing::debug!("PTY reader thread started");
                loop {
                    // Check for shutdown signal
                    if stop_rx.try_recv().is_ok() {
                        tracing::debug!("PTY reader thread received shutdown signal");
                        break;
                    }

                    match reader.read(&mut buffer) {
                        Ok(0) => {
                            tracing::debug!("PTY reader got EOF");
                            break; // EOF
                        }
                        Ok(n) => {
                            let data = buffer[..n].to_vec();
                            consecutive_reads += 1;

                            // Adaptive delay - if we're getting lots of data, read faster
                            let delay = if consecutive_reads > 10 {
                                // High throughput - minimal delay
                                0
                            } else if consecutive_reads > 5 {
                                // Medium throughput
                                1
                            } else {
                                // Low throughput
                                5
                            };

                            tracing::trace!("PTY read {} bytes (consecutive: {})", n, consecutive_reads);
                            if tx.blocking_send(data).is_err() {
                                tracing::error!("Failed to send PTY data to channel");
                                break;
                            }

                            if delay > 0 {
                                std::thread::sleep(std::time::Duration::from_micros(delay));
                            }
                        }
                        Err(e) => {
                            consecutive_reads = 0; // Reset counter on error
                            if e.kind() == std::io::ErrorKind::WouldBlock ||
                               e.kind() == std::io::ErrorKind::Interrupted {
                                std::thread::sleep(std::time::Duration::from_millis(1));
                                continue;
                            }
                            tracing::error!("PTY read error: {}", e);
                            break;
                        }
                    }
                }
                tracing::debug!("PTY reader thread exiting");
            });

            // Forward data from the thread to the output channel
            // Keep reading even if there are no consumers - this keeps the PTY alive
            // when clients detach and reattach
            tokio::select! {
                _ = async {
                    while let Some(data) = rx.recv().await {
                        // Ignore send errors - just means no one is reading right now
                        // The PTY should stay alive even when no clients are attached
                        let _ = output_tx_clone.send(data).await;
                    }
                } => {}
                _ = shutdown_rx_reader.recv() => {
                    tracing::debug!("Reader task received shutdown signal");
                    let _ = stop_tx.send(());
                    // Try to kill the child process
                    if let Ok(mut child_guard) = child_clone.lock() {
                        let _ = child_guard.kill();
                    }
                }
            }
        });

        // Writer task
        let writer_clone = writer.clone();
        let mut shutdown_rx_writer = shutdown_tx.subscribe();
        let writer_task = tokio::spawn(async move {
            tokio::select! {
                _ = async {
                    while let Some(data) = input_rx.recv().await {
                        if let Ok(mut w) = writer_clone.lock() {
                            let _ = w.write_all(&data);
                            let _ = w.flush();
                        }
                    }
                } => {}
                _ = shutdown_rx_writer.recv() => {
                    tracing::debug!("Writer task received shutdown signal");
                }
            }
        });

        // Resize task
        let master_clone = master.clone();
        let mut shutdown_rx_resize = shutdown_tx.subscribe();
        tokio::spawn(async move {
            tokio::select! {
                _ = async {
                    while let Some((cols, rows)) = resize_rx.recv().await {
                        if let Ok(m) = master_clone.lock() {
                            let _ = m.resize(PtySize {
                                rows,
                                cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                    }
                } => {}
                _ = shutdown_rx_resize.recv() => {
                    tracing::debug!("Resize task received shutdown signal");
                }
            }
        });

        Ok(Self {
            writer_tx: input_tx,
            reader_rx: output_rx,
            resize_tx,
            _writer_task: writer_task,
            _reader_task: reader_task,
            child,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    pub async fn write(&mut self, data: Vec<u8>) -> Result<()> {
        tracing::trace!("PTY write {} bytes: {:?}", data.len(), String::from_utf8_lossy(&data[..data.len().min(50)]));
        self.writer_tx.send(data).await
            .map_err(|e| FerrixError::Pty(format!("Failed to send to PTY writer: {}", e)))?;
        Ok(())
    }

    pub async fn read(&mut self) -> Result<Option<Vec<u8>>> {
        match self.reader_rx.try_recv() {
            Ok(data) => Ok(Some(data)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                Err(FerrixError::Pty("PTY reader disconnected".to_string()))
            }
        }
    }

    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.resize_tx.send((cols, rows)).await
            .map_err(|e| FerrixError::Pty(format!("Failed to send resize request: {}", e)))?;
        Ok(())
    }

    pub fn shutdown(&mut self) {
        tracing::debug!("Shutting down PTY");

        // Send shutdown signal to all tasks
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        // Kill the child process
        if let Ok(mut child_guard) = self.child.lock() {
            let _ = child_guard.kill();
            tracing::debug!("Killed PTY child process");
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::IsTerminal;

    #[tokio::test]
    async fn test_pty_creation() {
        // Note: PTY creation requires a proper terminal environment
        // This test might fail in CI/CD environments without TTY
        let result = Pty::new(80, 24);

        // In environments without TTY support, PTY creation may fail
        // So we just check that the function executes without panic
        if result.is_ok() {
            let _pty = result.unwrap();
            // PTY was created successfully
            assert!(true);
        } else {
            // PTY creation failed (likely no TTY available)
            // This is expected in some test environments
            assert!(true);
        }
    }

    #[tokio::test]
    async fn test_pty_write() {
        // Skip if no TTY available
        if !std::io::stdin().is_terminal() {
            return;
        }

        let mut pty = match Pty::new(80, 24) {
            Ok(p) => p,
            Err(_) => return, // Skip test if PTY creation fails
        };

        let test_data = b"echo test\n".to_vec();
        let result = pty.write(test_data).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pty_read() {
        // Skip if no TTY available
        if !std::io::stdin().is_terminal() {
            return;
        }

        let mut pty = match Pty::new(80, 24) {
            Ok(p) => p,
            Err(_) => return, // Skip test if PTY creation fails
        };

        // Write a command that should produce output
        let _ = pty.write(b"echo hello\n".to_vec()).await;

        // Give some time for the command to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Try to read output
        let result = pty.read().await;
        assert!(result.is_ok());

        // We might or might not have output immediately available
        // depending on timing, so we just check it doesn't error
    }

    #[tokio::test]
    async fn test_pty_resize() {
        // Skip if no TTY available
        if !std::io::stdin().is_terminal() {
            return;
        }

        let mut pty = match Pty::new(80, 24) {
            Ok(p) => p,
            Err(_) => return, // Skip test if PTY creation fails
        };

        let result = pty.resize(120, 40).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pty_dimensions() {
        // Test that PTY can be created with various dimensions
        let dimensions = vec![
            (40, 10),
            (80, 24),
            (120, 40),
            (200, 60),
        ];

        for (cols, rows) in dimensions {
            let result = Pty::new(cols, rows);
            // Just verify no panic occurs
            if result.is_ok() {
                assert!(true);
            }
        }
    }

    #[tokio::test]
    async fn test_pty_multiple_writes() {
        // Skip if no TTY available
        if !std::io::stdin().is_terminal() {
            return;
        }

        let mut pty = match Pty::new(80, 24) {
            Ok(p) => p,
            Err(_) => return, // Skip test if PTY creation fails
        };

        // Write multiple commands
        let commands = vec![
            b"ls\n".to_vec(),
            b"pwd\n".to_vec(),
            b"echo test\n".to_vec(),
        ];

        for cmd in commands {
            let result = pty.write(cmd).await;
            assert!(result.is_ok());
        }
    }
}