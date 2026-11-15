// Trail diffusion shader: 3x3 blur + decay

@group(0) @binding(0) var<uniform> params: SimParams;
@group(0) @binding(1) var trail_read: texture_2d<f32>;
@group(0) @binding(2) var trail_write: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }

    let coord = vec2<i32>(i32(id.x), i32(id.y));
    let original = textureLoad(trail_read, coord, 0);

    // 3x3 box blur
    var sum = vec4<f32>(0.0);
    for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
        for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
            let sample_x = min(i32(params.width) - 1, max(0, coord.x + offset_x));
            let sample_y = min(i32(params.height) - 1, max(0, coord.y + offset_y));
            sum += textureLoad(trail_read, vec2<i32>(sample_x, sample_y), 0);
        }
    }
    let blurred = sum / 9.0;

    // Blend between original and blurred based on diffuse rate
    let diffused = mix(original, blurred, clamp(params.diffuse_rate * params.delta_time, 0.0, 1.0));

    // Decay
    let decayed = max(vec4<f32>(0.0), diffused - params.decay_rate * params.delta_time);

    textureStore(trail_write, coord, decayed);
}
