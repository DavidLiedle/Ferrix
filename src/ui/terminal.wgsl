// WGSL shader for terminal rendering

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

    // Use texture alpha for glyph shape, multiply by vertex color
    let alpha = tex_sample.a;

    // Apply subpixel antialiasing if enabled
    let final_color = vec4<f32>(
        input.color.rgb * alpha,
        alpha * input.color.a
    );

    // Gamma correction for better text rendering
    let gamma = 2.2;
    let corrected_color = vec4<f32>(
        pow(final_color.r, 1.0 / gamma),
        pow(final_color.g, 1.0 / gamma),
        pow(final_color.b, 1.0 / gamma),
        final_color.a
    );

    return corrected_color;
}

// Additional shader for background rendering
@vertex
fn vs_background(
    @builtin(vertex_index) vertex_index: u32
) -> VertexOutput {
    var output: VertexOutput;

    // Generate full-screen quad
    let x = f32(vertex_index & 1u) * 2.0 - 1.0;
    let y = f32((vertex_index >> 1u) & 1u) * 2.0 - 1.0;

    output.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    output.tex_coords = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    output.color = vec4<f32>(0.0, 0.0, 0.0, 1.0); // Default background color

    return output;
}

@fragment
fn fs_background(input: VertexOutput) -> @location(0) vec4<f32> {
    // Simple gradient background
    let gradient = mix(
        vec4<f32>(0.05, 0.05, 0.1, 1.0), // Dark blue
        vec4<f32>(0.0, 0.0, 0.0, 1.0),    // Black
        input.tex_coords.y
    );

    return gradient;
}

// Shader for cursor rendering with blinking animation
@vertex
fn vs_cursor(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32
) -> VertexOutput {
    var output: VertexOutput;

    // Cursor position and size would be passed as instance data
    // For now, using a simple quad
    let x = f32(vertex_index & 1u);
    let y = f32((vertex_index >> 1u) & 1u);

    output.clip_position = uniforms.view_proj * vec4<f32>(x * 10.0, y * 20.0, 0.0, 1.0);
    output.tex_coords = vec2<f32>(x, y);
    output.color = vec4<f32>(1.0, 1.0, 1.0, 0.7); // White with transparency

    return output;
}

@fragment
fn fs_cursor(input: VertexOutput) -> @location(0) vec4<f32> {
    // Smooth cursor with rounded corners
    let dist_from_center = length(input.tex_coords - vec2<f32>(0.5, 0.5));
    let smooth_edge = 1.0 - smoothstep(0.4, 0.5, dist_from_center);

    return vec4<f32>(input.color.rgb, input.color.a * smooth_edge);
}

// Effects shader for CRT-like retro terminal effect
@fragment
fn fs_crt_effect(input: VertexOutput) -> @location(0) vec4<f32> {
    let base_color = textureSample(glyph_texture, glyph_sampler, input.tex_coords);

    // Scanlines
    let scanline = sin(input.tex_coords.y * uniforms.screen_size.y * 3.14159) * 0.04;

    // Chromatic aberration
    let r = textureSample(glyph_texture, glyph_sampler, input.tex_coords + vec2<f32>(0.001, 0.0)).r;
    let g = base_color.g;
    let b = textureSample(glyph_texture, glyph_sampler, input.tex_coords - vec2<f32>(0.001, 0.0)).b;

    // Vignette
    let vignette = 1.0 - length(input.tex_coords - vec2<f32>(0.5, 0.5)) * 0.5;

    // Combine effects
    var final_color = vec4<f32>(r, g, b, base_color.a);
    final_color = final_color * (1.0 - scanline) * vignette;

    // Add slight green phosphor glow
    final_color.g = final_color.g * 1.1;

    return final_color;
}