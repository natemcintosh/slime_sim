// Food regeneration shader: procedurally grows food at shifting clump locations

struct SimParams {
    width: u32,
    height: u32,
    num_agents: u32,
    trail_weight: f32,
    decay_rate: f32,
    diffuse_rate: f32,
    delta_time: f32,
    time: f32,
    food_weight: f32,
    competing_mode: u32,
    initial_energy: f32,
    move_energy_cost: f32,
    deposit_energy_cost: f32,
    energy_per_food: f32,
    food_eat_rate: f32,
    food_regen_rate: f32,
    food_clump_lifetime: f32,
    reproduction_threshold: f32,
    food_num_clumps: u32,
    food_clump_radius: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
}

@group(0) @binding(0) var<uniform> params: SimParams;
@group(0) @binding(1) var<storage, read_write> food_buffer: array<f32>;

fn hash(state_in: u32) -> u32 {
    var state = state_in;
    state = state ^ 2747636419u;
    state = state * 2654435769u;
    state = state ^ (state >> 16u);
    state = state * 2654435769u;
    state = state ^ (state >> 16u);
    state = state * 2654435769u;
    return state;
}

fn scale_to_01(state: u32) -> f32 {
    return f32(state) / 4294967295.0;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }

    let pixel_idx = id.y * params.width + id.x;
    let px = f32(id.x) + 0.5;
    let py = f32(id.y) + 0.5;

    let lifetime = max(params.food_clump_lifetime, 0.1);
    let phase = params.time / lifetime;
    let current_gen = u32(floor(phase));
    let next_gen = current_gen + 1u;
    let blend = fract(phase);

    var regen_value = 0.0;
    let num_clumps = params.food_num_clumps;
    let sigma = max(params.food_clump_radius / 2.0, 1.0);
    let inv_2sigma2 = 1.0 / (2.0 * sigma * sigma);

    // Current generation clumps (fading out)
    for (var i = 0u; i < num_clumps; i++) {
        let seed = hash(i + current_gen * num_clumps + 12345u);
        let cx = scale_to_01(hash(seed)) * f32(params.width);
        let cy = scale_to_01(hash(seed + 1u)) * f32(params.height);
        let dx = px - cx;
        let dy = py - cy;
        regen_value += (1.0 - blend) * exp(-(dx * dx + dy * dy) * inv_2sigma2);
    }

    // Next generation clumps (fading in)
    for (var i = 0u; i < num_clumps; i++) {
        let seed = hash(i + next_gen * num_clumps + 12345u);
        let cx = scale_to_01(hash(seed)) * f32(params.width);
        let cy = scale_to_01(hash(seed + 1u)) * f32(params.height);
        let dx = px - cx;
        let dy = py - cy;
        regen_value += blend * exp(-(dx * dx + dy * dy) * inv_2sigma2);
    }

    let current = food_buffer[pixel_idx];
    food_buffer[pixel_idx] = clamp(current + regen_value * params.food_regen_rate * params.delta_time, 0.0, 1.0);
}
