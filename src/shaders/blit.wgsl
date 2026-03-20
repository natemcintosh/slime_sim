// Fullscreen triangle blit shader: samples colour map to screen

@group(0) @binding(0) var colour_tex: texture_2d<f32>;
@group(0) @binding(1) var colour_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle trick: 3 vertices cover the entire screen
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vertex_index >> 1u)) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    // UV: map from clip space to [0,1], flip Y for texture coords
    out.uv = vec2<f32>((x + 1.0) / 2.0, 1.0 - (y + 1.0) / 2.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(colour_tex, colour_sampler, in.uv);
}
