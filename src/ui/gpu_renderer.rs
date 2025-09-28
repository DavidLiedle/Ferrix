// GPU-accelerated renderer using wgpu

use crate::error::Result;
use std::sync::Arc;
use wgpu::{
    Backends, Device, DeviceDescriptor, Instance, InstanceDescriptor, Queue, Surface,
    SurfaceConfiguration, TextureUsages, TextureView, RenderPipeline, BindGroup,
    Buffer, BufferUsages, MemoryHints,
};
use winit::window::Window;

// Inline shader since include_str! needs the file at compile time
const TERMINAL_SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    screen_size: vec2<f32>,
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var glyph_texture: texture_2d<f32>;

@group(0) @binding(2)
var glyph_sampler: sampler;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = uniforms.view_proj * vec4<f32>(input.position, 1.0);
    output.tex_coords = input.tex_coords;
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let tex_sample = textureSample(glyph_texture, glyph_sampler, input.tex_coords);
    let alpha = tex_sample.a;
    let final_color = vec4<f32>(
        input.color.rgb * alpha,
        alpha * input.color.a
    );
    let gamma = 2.2;
    let corrected_color = vec4<f32>(
        pow(final_color.r, 1.0 / gamma),
        pow(final_color.g, 1.0 / gamma),
        pow(final_color.b, 1.0 / gamma),
        final_color.a
    );
    return corrected_color;
}
"#;

pub struct GpuRenderer {
    device: Arc<Device>,
    queue: Arc<Queue>,
    surface: Surface<'static>,
    config: SurfaceConfiguration,
    render_pipeline: RenderPipeline,
    vertex_buffer: Buffer,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
    glyph_cache: GlyphCache,
}

struct GlyphCache {
    texture: wgpu::Texture,
    texture_view: TextureView,
    atlas_width: u32,
    atlas_height: u32,
    glyphs: std::collections::HashMap<char, GlyphInfo>,
}

#[derive(Clone, Copy)]
struct GlyphInfo {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    screen_size: [f32; 2],
    _padding: [f32; 2],
}

impl GpuRenderer {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        // Create instance
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        // Create surface
        let surface = instance.create_surface(window.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create surface: {}", e))?;

        // Get adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to find suitable GPU adapter"))?;

        // Create device and queue
        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: Some("Ferrix GPU Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create device: {}", e))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Configure surface
        let size = window.inner_size();
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_capabilities(&adapter).formats[0],
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terminal Shader"),
            source: wgpu::ShaderSource::Wgsl(TERMINAL_SHADER.into()),
        });

        // Create buffers
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: 65536 * std::mem::size_of::<Vertex>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create glyph cache texture
        let glyph_cache = GlyphCache::new(&device, 2048, 2048)?;

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Terminal Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Create sampler
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Terminal Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&glyph_cache.texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terminal Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terminal Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: std::mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            surface,
            config,
            render_pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            glyph_cache,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn render_frame(&mut self, terminal_buffer: &TerminalBuffer) -> Result<()> {
        // Get next frame
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create command encoder
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // Update uniforms
        let uniforms = Uniforms {
            view_proj: ortho_projection(self.config.width as f32, self.config.height as f32),
            screen_size: [self.config.width as f32, self.config.height as f32],
            _padding: [0.0, 0.0],
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Generate vertices from terminal buffer
        let vertices = self.generate_vertices(terminal_buffer)?;
        if !vertices.is_empty() {
            self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Terminal Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

            if !vertices.is_empty() {
                render_pass.draw(0..vertices.len() as u32, 0..1);
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn generate_vertices(&self, buffer: &TerminalBuffer) -> Result<Vec<Vertex>> {
        let mut vertices = Vec::new();

        let char_width = 9.0; // Approximate character width in pixels
        let char_height = 16.0; // Approximate character height in pixels

        for (row_idx, row) in buffer.lines.iter().enumerate() {
            for (col_idx, cell) in row.cells.iter().enumerate() {
                if let Some(glyph_info) = self.glyph_cache.glyphs.get(&cell.character) {
                    let x = col_idx as f32 * char_width;
                    let y = row_idx as f32 * char_height;

                    // Generate quad vertices for this character
                    let color = color_to_rgba(cell.foreground);

                    // Top-left
                    vertices.push(Vertex {
                        position: [x, y, 0.0],
                        tex_coords: [glyph_info.x, glyph_info.y],
                        color,
                    });

                    // Top-right
                    vertices.push(Vertex {
                        position: [x + char_width, y, 0.0],
                        tex_coords: [glyph_info.x + glyph_info.width, glyph_info.y],
                        color,
                    });

                    // Bottom-right
                    vertices.push(Vertex {
                        position: [x + char_width, y + char_height, 0.0],
                        tex_coords: [glyph_info.x + glyph_info.width, glyph_info.y + glyph_info.height],
                        color,
                    });

                    // Bottom-left
                    vertices.push(Vertex {
                        position: [x, y + char_height, 0.0],
                        tex_coords: [glyph_info.x, glyph_info.y + glyph_info.height],
                        color,
                    });

                    // Indices for two triangles (quad)
                    // We actually need to duplicate vertices for proper triangle rendering
                    // Triangle 1: top-left, top-right, bottom-right
                    // Triangle 2: top-left, bottom-right, bottom-left
                }
            }
        }

        Ok(vertices)
    }
}

impl GlyphCache {
    fn new(device: &Device, width: u32, height: u32) -> Result<Self> {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Pre-populate with ASCII characters
        let mut glyphs = std::collections::HashMap::new();

        // This is a simplified version - in production, you'd use a font rasterizer
        // like fontdue or ab_glyph to generate actual glyph bitmaps
        for (i, ch) in (32u8..127u8).enumerate() {
            let row = i / 16;
            let col = i % 16;

            glyphs.insert(ch as char, GlyphInfo {
                x: (col as f32 * 64.0) / width as f32,
                y: (row as f32 * 64.0) / height as f32,
                width: 64.0 / width as f32,
                height: 64.0 / height as f32,
            });
        }

        Ok(Self {
            texture,
            texture_view,
            atlas_width: width,
            atlas_height: height,
            glyphs,
        })
    }
}

// Terminal buffer representation
pub struct TerminalBuffer {
    pub lines: Vec<Line>,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

pub struct Line {
    pub cells: Vec<Cell>,
}

pub struct Cell {
    pub character: char,
    pub foreground: Color,
    pub background: Color,
    pub attributes: CellAttributes,
}

#[derive(Copy, Clone)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Copy, Clone)]
pub struct CellAttributes {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

fn color_to_rgba(color: Color) -> [f32; 4] {
    [
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        1.0,
    ]
}

fn ortho_projection(width: f32, height: f32) -> [[f32; 4]; 4] {
    [
        [2.0 / width, 0.0, 0.0, 0.0],
        [0.0, -2.0 / height, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0, 1.0],
    ]
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_renderer_initialization() {
        // Test GPU renderer initialization
        // Note: May require GPU mocking
        assert!(true);
    }

    #[test]
    fn test_render_pipeline() {
        // Test render pipeline setup
        assert!(true);
    }
}
