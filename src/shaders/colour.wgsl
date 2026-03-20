// Colour mapping shader: trail intensities -> species colours

struct SpeciesSettings {
    move_speed: f32,
    turn_speed: f32,
    sensor_angle_spacing: f32,
    sensor_offset_dst: f32,
    sensor_size: i32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    colour: vec4<f32>,
}

struct ColourParams {
    width: u32,
    height: u32,
    num_species: u32,
    _pad: u32,
}

@group(0) @binding(0) var<uniform> colour_params: ColourParams;
@group(0) @binding(1) var trail_map: texture_2d<f32>;
@group(0) @binding(2) var colour_map: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<storage, read> species: array<SpeciesSettings>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= colour_params.width || id.y >= colour_params.height) {
        return;
    }

    let coord = vec2<i32>(i32(id.x), i32(id.y));
    let trail = textureLoad(trail_map, coord, 0);

    var colour = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    // Blend species colours based on trail intensity per channel
    if (colour_params.num_species > 0u) {
        colour = vec4<f32>(colour.rgb + trail.r * species[0].colour.rgb, 1.0);
    }
    if (colour_params.num_species > 1u) {
        colour = vec4<f32>(colour.rgb + trail.g * species[1].colour.rgb, 1.0);
    }
    if (colour_params.num_species > 2u) {
        colour = vec4<f32>(colour.rgb + trail.b * species[2].colour.rgb, 1.0);
    }
    if (colour_params.num_species > 3u) {
        colour = vec4<f32>(colour.rgb + trail.a * species[3].colour.rgb, 1.0);
    }

    // Clamp to valid range
    colour = clamp(colour, vec4<f32>(0.0), vec4<f32>(1.0));

    textureStore(colour_map, coord, colour);
}
