use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock, Semaphore};
use bytes::{Bytes, BytesMut};
use tracing::{debug, trace, warn};

use crate::error::{Result, FerrixError};
use crate::protocol::PaneId;

/// Performance optimization settings
#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    /// Maximum buffer size before triggering backpressure (in bytes)
    pub max_buffer_size: usize,
    /// Batch size for output processing (in bytes)
    pub batch_size: usize,
    /// Throttle delay when buffer is near capacity (milliseconds)
    pub throttle_delay_ms: u64,
    /// Maximum concurrent output processors
    pub max_concurrent_processors: usize,
    /// Enable output compression for large transfers
    pub enable_compression: bool,
    /// Compression threshold (bytes)
    pub compression_threshold: usize,
    /// Enable adaptive batching based on throughput
    pub adaptive_batching: bool,
    /// Maximum time to wait for batch accumulation (milliseconds)
    pub batch_timeout_ms: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_buffer_size: 10 * 1024 * 1024,      // 10MB
            batch_size: 64 * 1024,                  // 64KB
            throttle_delay_ms: 10,                  // 10ms
            max_concurrent_processors: 4,
            enable_compression: true,
            compression_threshold: 1024 * 1024,     // 1MB
            adaptive_batching: true,
            batch_timeout_ms: 50,                   // 50ms
        }
    }
}

/// Output buffer with backpressure support
pub struct OutputBuffer {
    buffer: Arc<RwLock<BytesMut>>,
    config: PerformanceConfig,
    semaphore: Arc<Semaphore>,
    metrics: Arc<RwLock<BufferMetrics>>,
}

#[derive(Debug, Default, Clone)]
pub struct BufferMetrics {
    pub total_bytes_processed: u64,
    pub total_batches: u64,
    pub average_batch_size: usize,
    pub peak_buffer_size: usize,
    pub throttle_count: u64,
    pub compression_count: u64,
    pub last_throughput_mbps: f64,
}

impl OutputBuffer {
    pub fn new(config: PerformanceConfig) -> Self {
        Self {
            buffer: Arc::new(RwLock::new(BytesMut::with_capacity(config.batch_size))),
            semaphore: Arc::new(Semaphore::new(config.max_concurrent_processors)),
            metrics: Arc::new(RwLock::new(BufferMetrics::default())),
            config,
        }
    }

    /// Get current buffer usage percentage
    pub async fn buffer_usage_percent(&self) -> f64 {
        let buffer = self.buffer.read().await;
        (buffer.len() as f64 / self.config.max_buffer_size as f64) * 100.0
    }

    /// Write data to the buffer with backpressure handling
    pub async fn write(&self, data: Vec<u8>) -> Result<()> {
        let mut buffer = self.buffer.write().await;

        // Check if we need to apply backpressure
        if buffer.len() + data.len() > self.config.max_buffer_size {
            drop(buffer); // Release lock during throttle
            self.apply_backpressure().await;
            buffer = self.buffer.write().await;
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_bytes_processed += data.len() as u64;
            if buffer.len() + data.len() > metrics.peak_buffer_size {
                metrics.peak_buffer_size = buffer.len() + data.len();
            }
        }

        buffer.extend_from_slice(&data);
        Ok(())
    }

    /// Read data from the buffer in optimized batches
    pub async fn read_batch(&self) -> Option<Bytes> {
        let mut buffer = self.buffer.write().await;

        if buffer.is_empty() {
            return None;
        }

        let batch_size = if self.config.adaptive_batching {
            self.calculate_adaptive_batch_size(&buffer).await
        } else {
            self.config.batch_size.min(buffer.len())
        };

        let batch = buffer.split_to(batch_size).freeze();

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_batches += 1;
            metrics.average_batch_size =
                ((metrics.average_batch_size * (metrics.total_batches - 1) as usize) + batch_size)
                / metrics.total_batches as usize;
        }

        // Compress if needed
        if self.config.enable_compression && batch.len() > self.config.compression_threshold {
            self.compress_batch(batch).await
        } else {
            Some(batch)
        }
    }

    /// Apply backpressure when buffer is near capacity
    async fn apply_backpressure(&self) {
        warn!("Applying backpressure - buffer near capacity");

        // Update throttle count
        {
            let mut metrics = self.metrics.write().await;
            metrics.throttle_count += 1;
        }

        // Wait for buffer to drain
        tokio::time::sleep(Duration::from_millis(self.config.throttle_delay_ms)).await;
    }

    /// Calculate adaptive batch size based on current throughput
    async fn calculate_adaptive_batch_size(&self, buffer: &BytesMut) -> usize {
        let metrics = self.metrics.read().await;

        // Start with base batch size
        let mut batch_size = self.config.batch_size;

        // Adjust based on throughput
        if metrics.last_throughput_mbps > 100.0 {
            // High throughput - increase batch size
            batch_size = (batch_size as f64 * 1.5) as usize;
        } else if metrics.last_throughput_mbps < 10.0 {
            // Low throughput - decrease batch size for lower latency
            batch_size = (batch_size as f64 * 0.75) as usize;
        }

        // Ensure reasonable bounds
        batch_size
            .clamp(4096, 1024 * 1024)  // 4KB to 1MB
            .min(buffer.len())  // Don't exceed buffer size
    }

    /// Compress a batch of data
    async fn compress_batch(&self, batch: Bytes) -> Option<Bytes> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let _permit = self.semaphore.acquire().await.ok()?;

        // Update compression count
        {
            let mut metrics = self.metrics.write().await;
            metrics.compression_count += 1;
        }

        let encoder = GzEncoder::new(Vec::new(), Compression::fast());
        let mut encoder = encoder;

        if encoder.write_all(&batch).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                if compressed.len() < batch.len() {
                    debug!("Compressed {} bytes to {} bytes ({}% reduction)",
                           batch.len(), compressed.len(),
                           100 - (compressed.len() * 100 / batch.len()));
                    return Some(Bytes::from(compressed));
                }
            }
        }

        Some(batch)
    }

    /// Get current buffer metrics
    pub async fn get_metrics(&self) -> BufferMetrics {
        let metrics = self.metrics.read().await;
        BufferMetrics {
            total_bytes_processed: metrics.total_bytes_processed,
            total_batches: metrics.total_batches,
            average_batch_size: metrics.average_batch_size,
            peak_buffer_size: metrics.peak_buffer_size,
            throttle_count: metrics.throttle_count,
            compression_count: metrics.compression_count,
            last_throughput_mbps: metrics.last_throughput_mbps,
        }
    }

    /// Update throughput metric
    pub async fn update_throughput(&self, bytes_per_second: f64) {
        let mut metrics = self.metrics.write().await;
        metrics.last_throughput_mbps = bytes_per_second / 1_000_000.0;
    }
}

/// Optimized output processor for handling large terminal outputs
pub struct OutputProcessor {
    pane_id: PaneId,
    buffer: OutputBuffer,
    output_tx: mpsc::Sender<(PaneId, Vec<u8>)>,
    stats: Arc<RwLock<ProcessorStats>>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessorStats {
    pub total_bytes: u64,
    pub total_chunks: u64,
    pub processing_time_ms: u64,
    pub last_update: Option<Instant>,
}

impl OutputProcessor {
    pub fn new(
        pane_id: PaneId,
        output_tx: mpsc::Sender<(PaneId, Vec<u8>)>,
        config: PerformanceConfig,
    ) -> Self {
        Self {
            pane_id,
            buffer: OutputBuffer::new(config),
            output_tx,
            stats: Arc::new(RwLock::new(ProcessorStats::default())),
        }
    }

    /// Process incoming data with optimizations
    pub async fn process(&self, data: Vec<u8>) -> Result<()> {
        let start = Instant::now();

        // Write to buffer
        self.buffer.write(data.clone()).await?;

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.total_bytes += data.len() as u64;
            stats.total_chunks += 1;
            stats.last_update = Some(Instant::now());
        }

        // Process batch if ready
        if let Some(batch) = self.buffer.read_batch().await {
            self.send_batch(batch).await?;
        }

        // Update processing time
        {
            let mut stats = self.stats.write().await;
            stats.processing_time_ms += start.elapsed().as_millis() as u64;
        }

        Ok(())
    }

    /// Send a batch of data to the output channel
    async fn send_batch(&self, batch: Bytes) -> Result<()> {
        // Convert Bytes back to Vec<u8> for sending
        let data = batch.to_vec();

        self.output_tx
            .send((self.pane_id.clone(), data))
            .await
            .map_err(|_| FerrixError::Other("Output channel closed".to_string()))?;

        Ok(())
    }

    /// Flush any remaining data in the buffer
    pub async fn flush(&self) -> Result<()> {
        while let Some(batch) = self.buffer.read_batch().await {
            self.send_batch(batch).await?;
        }
        Ok(())
    }

    /// Get processor statistics
    pub async fn get_stats(&self) -> ProcessorStats {
        *self.stats.read().await
    }
}

/// ANSI sequence optimizer for reducing redundant escape sequences
pub struct AnsiOptimizer {
    last_style: Option<AnsiStyle>,
    buffer: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
struct AnsiStyle {
    fg_color: Option<u32>,
    bg_color: Option<u32>,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl Default for AnsiOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsiOptimizer {
    pub fn new() -> Self {
        Self {
            last_style: None,
            buffer: Vec::with_capacity(4096),
        }
    }

    /// Optimize ANSI sequences by removing redundant style changes
    pub fn optimize(&mut self, data: &[u8]) -> Vec<u8> {
        self.buffer.clear();

        let mut i = 0;
        let mut current_style = self.last_style.clone();

        while i < data.len() {
            if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'[' {
                // Found escape sequence
                if let Some((seq_end, new_style)) = self.parse_ansi_sequence(&data[i..]) {
                    // Only emit if style actually changed
                    if current_style != Some(new_style.clone()) {
                        self.buffer.extend_from_slice(&data[i..i + seq_end]);
                        current_style = Some(new_style);
                    }
                    i += seq_end;
                    continue;
                }
            }

            self.buffer.push(data[i]);
            i += 1;
        }

        self.last_style = current_style;
        self.buffer.clone()
    }

    /// Parse ANSI sequence and return its length and style
    fn parse_ansi_sequence(&self, data: &[u8]) -> Option<(usize, AnsiStyle)> {
        // Simple ANSI parser - would need full implementation
        // This is a placeholder for the concept
        if data.len() < 3 {
            return None;
        }

        // Find sequence end
        let mut end = 2;
        while end < data.len() && !data[end].is_ascii_alphabetic() {
            end += 1;
        }

        if end >= data.len() {
            return None;
        }

        // Parse style (simplified)
        let style = AnsiStyle {
            fg_color: None,
            bg_color: None,
            bold: false,
            italic: false,
            underline: false,
        };

        Some((end + 1, style))
    }
}

/// Delta compression for incremental updates
pub struct DeltaCompressor {
    last_frame: Vec<u8>,
    compression_enabled: bool,
}

impl DeltaCompressor {
    pub fn new(compression_enabled: bool) -> Self {
        Self {
            last_frame: Vec::new(),
            compression_enabled,
        }
    }

    /// Compress data using delta encoding
    pub fn compress(&mut self, data: &[u8]) -> Vec<u8> {
        if !self.compression_enabled || self.last_frame.is_empty() {
            self.last_frame = data.to_vec();
            return data.to_vec();
        }

        // Simple XOR delta compression
        let mut delta = Vec::with_capacity(data.len());
        let min_len = self.last_frame.len().min(data.len());

        // XOR with previous frame
        for (&d, &lf) in data.iter().zip(self.last_frame.iter()).take(min_len) {
            delta.push(d ^ lf);
        }

        // Append any additional bytes
        if data.len() > min_len {
            delta.extend_from_slice(&data[min_len..]);
        }

        self.last_frame = data.to_vec();

        // Check if delta is actually smaller
        let compression_ratio = delta.iter().filter(|&&b| b == 0).count() as f32 / delta.len() as f32;

        if compression_ratio > 0.3 {
            // Good compression ratio - use delta
            trace!("Delta compression ratio: {:.1}%", compression_ratio * 100.0);
            delta
        } else {
            // Poor compression - send full frame
            data.to_vec()
        }
    }

    /// Decompress delta-encoded data
    pub fn decompress(&mut self, delta: &[u8]) -> Vec<u8> {
        if !self.compression_enabled || self.last_frame.is_empty() {
            self.last_frame = delta.to_vec();
            return delta.to_vec();
        }

        let mut result = Vec::with_capacity(delta.len());
        let min_len = self.last_frame.len().min(delta.len());

        // XOR with previous frame to recover data
        for (&d, &lf) in delta.iter().zip(self.last_frame.iter()).take(min_len) {
            result.push(d ^ lf);
        }

        // Append any additional bytes
        if delta.len() > min_len {
            result.extend_from_slice(&delta[min_len..]);
        }

        self.last_frame = result.clone();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_output_buffer_basic() {
        let config = PerformanceConfig::default();
        let buffer = OutputBuffer::new(config);

        // Write some data
        buffer.write(vec![1, 2, 3, 4, 5]).await.unwrap();

        // Read batch
        let batch = buffer.read_batch().await.unwrap();
        assert_eq!(batch.len(), 5);
        assert_eq!(&batch[..], &[1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_output_buffer_batching() {
        let mut config = PerformanceConfig::default();
        config.batch_size = 10;
        config.adaptive_batching = false;

        let buffer = OutputBuffer::new(config);

        // Write more than batch size
        buffer.write(vec![0; 25]).await.unwrap();

        // First batch should be 10 bytes
        let batch1 = buffer.read_batch().await.unwrap();
        assert_eq!(batch1.len(), 10);

        // Second batch should be 10 bytes
        let batch2 = buffer.read_batch().await.unwrap();
        assert_eq!(batch2.len(), 10);

        // Third batch should be 5 bytes
        let batch3 = buffer.read_batch().await.unwrap();
        assert_eq!(batch3.len(), 5);
    }

    #[test]
    fn test_ansi_optimizer() {
        let mut optimizer = AnsiOptimizer::new();

        // Test data with ANSI sequences
        let data = b"Hello \x1b[31mWorld\x1b[0m!";
        let optimized = optimizer.optimize(data);

        // Should preserve the data (basic test)
        assert!(!optimized.is_empty());
    }

    #[test]
    fn test_delta_compressor() {
        let mut compressor = DeltaCompressor::new(true);

        // First frame
        let frame1 = vec![1, 2, 3, 4, 5];
        let compressed1 = compressor.compress(&frame1);
        assert_eq!(compressed1, frame1); // First frame is stored as-is

        // Second frame with small changes
        let frame2 = vec![1, 2, 3, 4, 6]; // Only last byte changed
        let compressed2 = compressor.compress(&frame2);

        // Decompress to verify
        let mut decompressor = DeltaCompressor::new(true);
        decompressor.decompress(&compressed1);
        let decompressed2 = decompressor.decompress(&compressed2);
        assert_eq!(decompressed2, frame2);
    }

    #[tokio::test]
    async fn test_output_processor() {
        let pane_id = PaneId(uuid::Uuid::new_v4());
        let (tx, mut rx) = mpsc::channel(10);
        let config = PerformanceConfig::default();

        let processor = OutputProcessor::new(pane_id.clone(), tx, config);

        // Process some data
        processor.process(vec![1, 2, 3]).await.unwrap();
        processor.flush().await.unwrap();

        // Check that data was sent
        let (received_pane_id, received_data) = rx.recv().await.unwrap();
        assert_eq!(received_pane_id, pane_id);
        assert_eq!(received_data, vec![1, 2, 3]);

        // Check stats
        let stats = processor.get_stats().await;
        assert_eq!(stats.total_bytes, 3);
        assert_eq!(stats.total_chunks, 1);
    }
}