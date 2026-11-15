// Shared declarations, prepended to each compute shader in simulation.rs

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
    frame: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
}

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

// Hash function for pseudo-random numbers
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
