use crate::error::Result;
use crate::config::Config;
use tracing::{info, warn, debug};

/// Renderer selection and initialization
pub enum RendererType {
    Terminal,
    Gpu,
}

pub struct RendererSelector;

impl RendererSelector {
    /// Select the appropriate renderer based on configuration and system capabilities
    pub async fn select_renderer(config: &Config) -> RendererType {
        // Check if GPU acceleration is enabled in config
        if !config.advanced.gpu_acceleration {
            info!("GPU acceleration disabled in configuration");
            return RendererType::Terminal;
        }

        // Check if GPU is available
        if !Self::is_gpu_available().await {
            warn!("GPU acceleration requested but no suitable GPU found");
            return RendererType::Terminal;
        }

        // Check environment variable override
        if let Ok(renderer) = std::env::var("FERRIX_RENDERER") {
            match renderer.to_lowercase().as_str() {
                "gpu" | "wgpu" => {
                    info!("Using GPU renderer (environment override)");
                    return RendererType::Gpu;
                }
                "terminal" | "cpu" => {
                    info!("Using terminal renderer (environment override)");
                    return RendererType::Terminal;
                }
                _ => {
                    warn!("Unknown renderer type in FERRIX_RENDERER: {}", renderer);
                }
            }
        }

        // Check if running in SSH session (usually no GPU access)
        if Self::is_ssh_session() {
            info!("SSH session detected, using terminal renderer");
            return RendererType::Terminal;
        }

        // Check if running in container (might have limited GPU access)
        if Self::is_container() {
            info!("Container environment detected, checking GPU passthrough");
            if !Self::has_gpu_passthrough() {
                return RendererType::Terminal;
            }
        }

info!("GPU acceleration available and enabled");
        RendererType::Gpu
    }

    /// Check if a GPU is available on the system
    async fn is_gpu_available() -> bool {
        // Try to create a wgpu instance and adapter
        match Self::probe_gpu().await {
            Ok(has_gpu) => has_gpu,
            Err(e) => {
debug!("Failed to probe GPU: {}", e);
                false
            }
        }
    }

    #[cfg(feature = "gpu")]
    async fn probe_gpu() -> Result<bool> {
        use wgpu::{Instance, InstanceDescriptor, Backends};

        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await;

        if let Some(adapter) = adapter {
            let info = adapter.get_info();
            info!("GPU found: {} ({})", info.name, info.backend.to_str());

            // Check if it's a software renderer
            if info.device_type == wgpu::DeviceType::Cpu {
                info!("Software renderer detected, preferring terminal renderer");
                return Ok(false);
            }

            // Check for minimum capabilities
            let features = adapter.features();
            let limits = adapter.limits();

    debug!("GPU features: {:?}", features);
    debug!("GPU limits: max_texture_dimension_2d={}", limits.max_texture_dimension_2d);

            // Ensure minimum texture size for glyph atlas
            if limits.max_texture_dimension_2d < 2048 {
                warn!("GPU texture size limit too small for efficient rendering");
                return Ok(false);
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    #[cfg(not(feature = "gpu"))]
    async fn probe_gpu() -> Result<bool> {
        Ok(false)
    }

    /// Check if running in an SSH session
    fn is_ssh_session() -> bool {
        // Check common SSH environment variables
        std::env::var("SSH_CLIENT").is_ok() ||
        std::env::var("SSH_TTY").is_ok() ||
        std::env::var("SSH_CONNECTION").is_ok()
    }

    /// Check if running in a container
    fn is_container() -> bool {
        // Check for Docker
        std::path::Path::new("/.dockerenv").exists() ||
        // Check for containerd
        std::env::var("container").is_ok() ||
        // Check cgroup for container signatures
        Self::check_cgroup_for_container()
    }

    fn check_cgroup_for_container() -> bool {
        if let Ok(cgroup) = std::fs::read_to_string("/proc/self/cgroup") {
            cgroup.contains("/docker/") ||
            cgroup.contains("/kubepods/") ||
            cgroup.contains("/lxc/")
        } else {
            false
        }
    }

    /// Check if container has GPU passthrough
    fn has_gpu_passthrough() -> bool {
        // Check for NVIDIA GPU in container
        if std::path::Path::new("/dev/nvidia0").exists() {
            info!("NVIDIA GPU passthrough detected");
            return true;
        }

        // Check for AMD GPU
        if std::path::Path::new("/dev/dri/renderD128").exists() {
            info!("AMD GPU passthrough detected");
            return true;
        }

        // Check for Intel GPU
        if std::path::Path::new("/dev/dri/card0").exists() {
            info!("Intel GPU passthrough detected");
            return true;
        }

        false
    }

}

/// Performance profiler for renderer selection
pub struct RendererProfiler {
    start_time: std::time::Instant,
    frame_times: Vec<std::time::Duration>,
    max_samples: usize,
}

impl Default for RendererProfiler {
    fn default() -> Self {
        Self::new()
    }
}

impl RendererProfiler {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            frame_times: Vec::with_capacity(120),
            max_samples: 120,
        }
    }

    pub fn record_frame(&mut self) {
        let now = std::time::Instant::now();
        let frame_time = now - self.start_time;
        self.start_time = now;

        self.frame_times.push(frame_time);
        if self.frame_times.len() > self.max_samples {
            self.frame_times.remove(0);
        }
    }

    pub fn average_fps(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }

        let avg_frame_time = self.frame_times.iter().sum::<std::time::Duration>() / self.frame_times.len() as u32;
        if avg_frame_time.as_secs_f64() > 0.0 {
            1.0 / avg_frame_time.as_secs_f64()
        } else {
            0.0
        }
    }

    pub fn should_switch_renderer(&self, target_fps: f64) -> bool {
        let current_fps = self.average_fps();

        // If we have enough samples and FPS is consistently below target
        if self.frame_times.len() >= 60 && current_fps < target_fps * 0.8 {
            info!("Performance below target: {:.1} FPS (target: {:.1})", current_fps, target_fps);
            return true;
        }

        false
    }
}

/// GPU capability detection
pub struct GpuCapabilities {
    pub vendor: String,
    pub device: String,
    pub driver_version: String,
    pub api_version: String,
    pub max_texture_size: u32,
    pub max_compute_workgroups: u32,
    pub supports_surface_textures: bool,
    pub supports_bindless_textures: bool,
    pub dedicated_video_memory: Option<u64>,
    pub shared_system_memory: Option<u64>,
}

#[cfg(feature = "gpu")]
impl GpuCapabilities {
    pub async fn detect() -> Option<Self> {
        use wgpu::{Instance, InstanceDescriptor, Backends};

        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await?;

        let info = adapter.get_info();
        let limits = adapter.limits();
        let _features = adapter.features();

        Some(Self {
            vendor: info.vendor.to_string(),
            device: info.name,
            driver_version: info.driver,
            api_version: info.backend.to_str().to_string(),
            max_texture_size: limits.max_texture_dimension_2d,
            max_compute_workgroups: limits.max_compute_workgroups_per_dimension,
            supports_surface_textures: false,  // Feature flag no longer exists in wgpu 23
            supports_bindless_textures: false,  // Feature flag no longer exists in wgpu 23
            dedicated_video_memory: None, // Would need platform-specific queries
            shared_system_memory: None,
        })
    }

    pub fn is_high_performance(&self) -> bool {
        // Check if this is a dedicated GPU with good capabilities
        self.max_texture_size >= 8192 &&
        self.max_compute_workgroups >= 65535
    }

    pub fn recommended_settings(&self) -> GpuSettings {
        if self.is_high_performance() {
            GpuSettings {
                enable_msaa: true,
                msaa_samples: 4,
                enable_vsync: true,
                texture_atlas_size: 4096,
                max_glyph_cache_size: 1024,
                enable_subpixel_rendering: true,
            }
        } else {
            GpuSettings {
                enable_msaa: false,
                msaa_samples: 1,
                enable_vsync: true,
                texture_atlas_size: 2048,
                max_glyph_cache_size: 512,
                enable_subpixel_rendering: false,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct GpuSettings {
    pub enable_msaa: bool,
    pub msaa_samples: u32,
    pub enable_vsync: bool,
    pub texture_atlas_size: u32,
    pub max_glyph_cache_size: usize,
    pub enable_subpixel_rendering: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_renderer_selection() {
        let mut config = Config::default();
        config.advanced.gpu_acceleration = false;

        let renderer_type = RendererSelector::select_renderer(&config).await;
        match renderer_type {
            RendererType::Terminal => assert!(true),
            RendererType::Gpu => assert!(false, "Should not select GPU when disabled"),
        }
    }

    #[test]
    fn test_profiler() {
        let mut profiler = RendererProfiler::new();

        // Simulate 60 FPS
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_millis(16));
            profiler.record_frame();
        }

        let fps = profiler.average_fps();
        assert!(fps > 50.0 && fps < 70.0);
    }
}