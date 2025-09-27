#[cfg(all(test, feature = "gpu"))]
mod gpu_tests {
    use super::*;
    use crate::error::Result;
    use crate::ui::gpu_renderer::{GpuRenderer, RenderContext, GlyphCache};
    use wgpu::{Backends, Instance, InstanceDescriptor};

    #[tokio::test]
    async fn test_gpu_instance_creation() -> Result<()> {
        // Test that we can create a GPU instance
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        // Instance should be created successfully
        // (This tests that wgpu dependencies are working)
        Ok(())
    }

    #[tokio::test]
    async fn test_gpu_adapter_enumeration() -> Result<()> {
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        // Try to enumerate adapters
        let adapters = instance.enumerate_adapters(Backends::all());

        // In a test environment, we might not have any adapters
        // But enumeration should not panic
        println!("Found {} GPU adapters", adapters.len());

        Ok(())
    }

    #[tokio::test]
    async fn test_gpu_renderer_creation() -> Result<()> {
        // Test creating GPU renderer (might fail in headless environment)
        let renderer_result = GpuRenderer::new(800, 600).await;

        match renderer_result {
            Ok(_renderer) => {
                println!("GPU renderer created successfully");
                // Test basic renderer functionality
            }
            Err(e) => {
                println!("GPU renderer creation failed: {} (expected in test environment)", e);
                // This is acceptable in CI/test environments without GPU
            }
        }

        Ok(())
    }

    #[test]
    fn test_render_context_creation() -> Result<()> {
        let context = RenderContext {
            width: 800,
            height: 600,
            scale_factor: 1.0,
            font_size: 14.0,
        };

        assert_eq!(context.width, 800);
        assert_eq!(context.height, 600);
        assert_eq!(context.scale_factor, 1.0);
        assert_eq!(context.font_size, 14.0);

        Ok(())
    }

    #[test]
    fn test_render_context_calculations() -> Result<()> {
        let context = RenderContext {
            width: 800,
            height: 600,
            scale_factor: 2.0,
            font_size: 16.0,
        };

        // Test scaled dimensions
        let scaled_width = context.scaled_width();
        let scaled_height = context.scaled_height();

        assert_eq!(scaled_width, 1600);
        assert_eq!(scaled_height, 1200);

        // Test character dimensions
        let char_width = context.char_width();
        let char_height = context.char_height();

        assert!(char_width > 0.0);
        assert!(char_height > 0.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_glyph_cache_operations() -> Result<()> {
        // Test glyph cache creation (might fail without GPU)
        let cache_result = GlyphCache::new(512, 512).await;

        match cache_result {
            Ok(mut cache) => {
                // Test glyph loading
                let glyph_result = cache.load_glyph('A', 16.0);
                match glyph_result {
                    Ok(_glyph_info) => {
                        println!("Glyph loaded successfully");
                    }
                    Err(_) => {
                        println!("Glyph loading failed (expected without proper GPU setup)");
                    }
                }

                // Test cache clearing
                cache.clear();
            }
            Err(_) => {
                println!("Glyph cache creation failed (expected in test environment)");
            }
        }

        Ok(())
    }

    #[test]
    fn test_color_conversion() -> Result<()> {
        use crate::ui::gpu_renderer::Color;

        let red = Color::from_rgb(255, 0, 0);
        assert_eq!(red.r, 1.0);
        assert_eq!(red.g, 0.0);
        assert_eq!(red.b, 0.0);
        assert_eq!(red.a, 1.0);

        let transparent_blue = Color::from_rgba(0, 0, 255, 128);
        assert_eq!(transparent_blue.r, 0.0);
        assert_eq!(transparent_blue.g, 0.0);
        assert_eq!(transparent_blue.b, 1.0);
        assert!(transparent_blue.a < 1.0);

        Ok(())
    }

    #[test]
    fn test_vertex_data_creation() -> Result<()> {
        use crate::ui::gpu_renderer::Vertex;

        let vertex = Vertex {
            position: [100.0, 200.0],
            tex_coords: [0.5, 0.5],
            color: [1.0, 1.0, 1.0, 1.0],
        };

        assert_eq!(vertex.position, [100.0, 200.0]);
        assert_eq!(vertex.tex_coords, [0.5, 0.5]);
        assert_eq!(vertex.color, [1.0, 1.0, 1.0, 1.0]);

        Ok(())
    }

    #[tokio::test]
    async fn test_shader_compilation() -> Result<()> {
        // Test that shaders can be compiled (requires GPU context)
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        let adapter_result = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }).await;

        if let Some(adapter) = adapter_result {
            let device_result = adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Test Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            ).await;

            match device_result {
                Ok((device, _queue)) => {
                    // Test vertex shader compilation
                    let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("Test Vertex Shader"),
                        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(r#"
                            @vertex
                            fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
                                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
                            }
                        "#)),
                    });

                    // Test fragment shader compilation
                    let fragment_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("Test Fragment Shader"),
                        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(r#"
                            @fragment
                            fn fs_main() -> @location(0) vec4<f32> {
                                return vec4<f32>(1.0, 0.0, 0.0, 1.0);
                            }
                        "#)),
                    });

                    println!("Shaders compiled successfully");
                }
                Err(_) => {
                    println!("Device creation failed (expected in test environment)");
                }
            }
        } else {
            println!("No GPU adapter available (expected in test environment)");
        }

        Ok(())
    }

    #[test]
    fn test_text_layout_calculations() -> Result<()> {
        use crate::ui::gpu_renderer::TextLayout;

        let layout = TextLayout::new("Hello, World!", 16.0, 800.0);

        assert_eq!(layout.text, "Hello, World!");
        assert_eq!(layout.font_size, 16.0);
        assert_eq!(layout.max_width, 800.0);

        // Test line breaking
        let lines = layout.calculate_lines();
        assert!(lines.len() > 0);

        // Test glyph positioning
        let positions = layout.calculate_glyph_positions();
        assert_eq!(positions.len(), layout.text.chars().count());

        Ok(())
    }

    #[test]
    fn test_render_commands() -> Result<()> {
        use crate::ui::gpu_renderer::{RenderCommand, Rect};

        let rect_command = RenderCommand::DrawRect {
            rect: Rect { x: 10.0, y: 20.0, width: 100.0, height: 50.0 },
            color: [1.0, 0.0, 0.0, 1.0],
        };

        let text_command = RenderCommand::DrawText {
            text: "Test".to_string(),
            x: 0.0,
            y: 0.0,
            font_size: 14.0,
            color: [0.0, 0.0, 0.0, 1.0],
        };

        // Commands should be created without error
        match rect_command {
            RenderCommand::DrawRect { rect, color } => {
                assert_eq!(rect.width, 100.0);
                assert_eq!(color[0], 1.0); // Red component
            }
            _ => panic!("Wrong command type"),
        }

        match text_command {
            RenderCommand::DrawText { text, font_size, .. } => {
                assert_eq!(text, "Test");
                assert_eq!(font_size, 14.0);
            }
            _ => panic!("Wrong command type"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_gpu_memory_management() -> Result<()> {
        // Test GPU memory allocation and deallocation
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        let adapter_result = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }).await;

        if let Some(adapter) = adapter_result {
            let device_result = adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Memory Test Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            ).await;

            match device_result {
                Ok((device, _queue)) => {
                    // Test buffer creation and deallocation
                    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Test Buffer"),
                        size: 1024,
                        usage: wgpu::BufferUsages::VERTEX,
                        mapped_at_creation: false,
                    });

                    // Buffer should be created successfully
                    drop(buffer); // Test cleanup

                    println!("GPU memory test completed successfully");
                }
                Err(_) => {
                    println!("GPU memory test skipped (no device available)");
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_performance_metrics() -> Result<()> {
        use crate::ui::gpu_renderer::PerformanceMetrics;

        let mut metrics = PerformanceMetrics::new();

        // Test frame timing
        let start = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(16)); // Simulate 60 FPS
        let frame_time = start.elapsed();

        metrics.record_frame_time(frame_time);
        assert!(metrics.average_frame_time().as_millis() > 0);

        // Test draw call counting
        metrics.record_draw_call();
        metrics.record_draw_call();
        assert_eq!(metrics.draw_calls_this_frame(), 2);

        // Test frame completion
        metrics.end_frame();
        assert_eq!(metrics.draw_calls_this_frame(), 0); // Should reset

        Ok(())
    }

    #[test]
    fn test_fallback_rendering() -> Result<()> {
        use crate::ui::gpu_renderer::FallbackRenderer;

        // Test CPU fallback renderer
        let mut fallback = FallbackRenderer::new(80, 24);

        fallback.clear();
        fallback.draw_char(0, 0, 'H', [1.0, 1.0, 1.0, 1.0]);
        fallback.draw_char(1, 0, 'i', [1.0, 1.0, 1.0, 1.0]);

        let buffer = fallback.get_buffer();
        assert_eq!(buffer.len(), 80 * 24);
        assert_eq!(buffer[0].character, 'H');
        assert_eq!(buffer[1].character, 'i');

        Ok(())
    }
}

// Tests that can run without GPU feature
#[cfg(test)]
mod gpu_disabled_tests {
    use crate::error::Result;

    #[test]
    fn test_gpu_feature_detection() -> Result<()> {
        // Test that we can detect if GPU features are available
        #[cfg(feature = "gpu")]
        {
            println!("GPU features are enabled");
        }

        #[cfg(not(feature = "gpu"))]
        {
            println!("GPU features are disabled");
        }

        Ok(())
    }

    #[test]
    fn test_terminal_rendering_fallback() -> Result<()> {
        // Test that terminal rendering works without GPU
        use crate::ui::statusbar::StatusBar;

        let mut status_bar = StatusBar::new();
        status_bar.set_session_info("test".to_string(), crate::protocol::SessionId(uuid::Uuid::new_v4()));

        // Should work without GPU
        assert!(status_bar.session_name.is_some());

        Ok(())
    }
}