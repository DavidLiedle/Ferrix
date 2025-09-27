use std::io::Write;
use std::sync::{Arc, Mutex};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio::sync::mpsc;

use crate::error::{FerrixError, Result};

pub struct Pty {
    writer_tx: mpsc::Sender<Vec<u8>>,
    reader_rx: mpsc::Receiver<Vec<u8>>,
    resize_tx: mpsc::Sender<(u16, u16)>,
    _writer_task: tokio::task::JoinHandle<()>,
    _reader_task: tokio::task::JoinHandle<()>,
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

        let _child = pty_pair.slave.spawn_command(cmd)
            .map_err(|e| FerrixError::Pty(format!("Failed to spawn shell: {}", e)))?;

        let writer = Arc::new(Mutex::new(
            pty_pair.master.take_writer()
                .map_err(|e| FerrixError::Pty(format!("Failed to get PTY writer: {}", e)))?
        ));

        let mut reader = pty_pair.master.try_clone_reader()
            .map_err(|e| FerrixError::Pty(format!("Failed to get PTY reader: {}", e)))?;

        let master = Arc::new(Mutex::new(pty_pair.master));

        // Channel for sending output from PTY to client
        let (output_tx, output_rx) = mpsc::channel(100);

        // Channel for sending input from client to PTY
        let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(100);

        // Channel for resize requests
        let (resize_tx, mut resize_rx) = mpsc::channel::<(u16, u16)>(10);

        // Reader task
        let reader_task = tokio::task::spawn_blocking(move || {
            let mut buffer = vec![0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = buffer[..n].to_vec();
                        let tx = output_tx.clone();
                        tokio::runtime::Handle::current().block_on(async {
                            let _ = tx.send(data).await;
                        });
                    }
                    Err(e) => {
                        if e.kind() != std::io::ErrorKind::WouldBlock {
                            tracing::error!("PTY read error: {}", e);
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            }
        });

        // Writer task
        let writer_clone = writer.clone();
        let writer_task = tokio::spawn(async move {
            while let Some(data) = input_rx.recv().await {
                if let Ok(mut w) = writer_clone.lock() {
                    let _ = w.write_all(&data);
                    let _ = w.flush();
                }
            }
        });

        // Resize task
        let master_clone = master.clone();
        tokio::spawn(async move {
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
        });

        Ok(Self {
            writer_tx: input_tx,
            reader_rx: output_rx,
            resize_tx,
            _writer_task: writer_task,
            _reader_task: reader_task,
        })
    }

    pub async fn write(&mut self, data: Vec<u8>) -> Result<()> {
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
}