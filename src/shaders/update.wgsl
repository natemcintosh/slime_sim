// Agent update shader: sense, steer, move, deposit

struct Agent {
    position: vec2<f32>,
    angle: f32,
    species_index: u32,
    energy: f32,
    _padding: u32,
}

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

@group(0) @binding(0) var<uniform> params: SimParams;
@group(0) @binding(1) var<storage, read_write> agents: array<Agent>;
@group(0) @binding(2) var trail_read: texture_2d<f32>;
@group(0) @binding(3) var trail_write: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var<storage, read> species: array<SpeciesSettings>;
@group(0) @binding(5) var food_map: texture_2d<f32>;
@group(0) @binding(6) var<storage, read_write> food_buffer: array<f32>;
@group(0) @binding(7) var<storage, read_write> population: array<atomic<u32>, 4>;

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

fn sense(agent: Agent, spec: SpeciesSettings, sensor_angle_offset: f32) -> f32 {
    let sensor_angle = agent.angle + sensor_angle_offset;
    let sensor_dir = vec2<f32>(cos(sensor_angle), sin(sensor_angle));
    let sensor_pos = agent.position + sensor_dir * spec.sensor_offset_dst;
    let sensor_centre_x = i32(sensor_pos.x);
    let sensor_centre_y = i32(sensor_pos.y);

    var sum: f32 = 0.0;
    let sense_weight = vec4<f32>(
        select(-1.0, 1.0, agent.species_index == 0u),
        select(-1.0, 1.0, agent.species_index == 1u),
        select(-1.0, 1.0, agent.species_index == 2u),
        select(-1.0, 1.0, agent.species_index == 3u),
    );

    let half = spec.sensor_size / 2;
    for (var offset_x = -half; offset_x <= half; offset_x++) {
        for (var offset_y = -half; offset_y <= half; offset_y++) {
            let sample_x = min(i32(params.width) - 1, max(0, sensor_centre_x + offset_x));
            let sample_y = min(i32(params.height) - 1, max(0, sensor_centre_y + offset_y));
            let sample = textureLoad(trail_read, vec2<i32>(sample_x, sample_y), 0);
            sum += dot(sense_weight, sample);

            // Sense food — always positive, bypasses inter-species repulsion
            if (params.competing_mode != 0u) {
                let food_idx = u32(sample_y) * params.width + u32(sample_x);
                let food_value = food_buffer[food_idx];
                sum += food_value * params.food_weight;
            } else {
                let food_value = textureLoad(food_map, vec2<i32>(sample_x, sample_y), 0).r;
                sum += food_value * params.food_weight;
            }
        }
    }
    return sum;
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let idx = id.x;
    if (idx >= params.num_agents) {
        return;
    }

    var agent = agents[idx];

    // In competing mode, skip dead agents
    if (params.competing_mode != 0u) {
        if (agent.energy <= 0.0) {
            return;
        }
        atomicAdd(&population[agent.species_index], 1u);
    }

    let spec = species[agent.species_index];

    // Random number generation
    let random_seed = hash(u32(agent.position.y * f32(params.width) + agent.position.x) + hash(idx + u32(params.time * 100000.0)));
    let random_steer = scale_to_01(random_seed);

    // Sense
    let weight_forward = sense(agent, spec, 0.0);
    let weight_left = sense(agent, spec, spec.sensor_angle_spacing);
    let weight_right = sense(agent, spec, -spec.sensor_angle_spacing);

    let turn_speed = spec.turn_speed * 2.0 * 3.14159265;

    // Steer
    if (weight_forward > weight_left && weight_forward > weight_right) {
        // Continue straight
        agent.angle += 0.0;
    } else if (weight_forward < weight_left && weight_forward < weight_right) {
        // Random turn
        agent.angle += (random_steer - 0.5) * 2.0 * turn_speed * params.delta_time;
    } else if (weight_right > weight_left) {
        // Turn right
        agent.angle -= random_steer * turn_speed * params.delta_time;
    } else if (weight_left > weight_right) {
        // Turn left
        agent.angle += random_steer * turn_speed * params.delta_time;
    }

    // Move
    let direction = vec2<f32>(cos(agent.angle), sin(agent.angle));
    var new_pos = agent.position + direction * spec.move_speed * params.delta_time;

    // Boundary wrapping / bouncing
    let w = f32(params.width);
    let h = f32(params.height);
    if (new_pos.x < 0.0 || new_pos.x >= w || new_pos.y < 0.0 || new_pos.y >= h) {
        // Bounce off walls
        let random2 = hash(random_seed);
        new_pos = clamp(new_pos, vec2<f32>(0.0), vec2<f32>(w - 1.0, h - 1.0));
        agent.angle = scale_to_01(random2) * 2.0 * 3.14159265;
    }
    agent.position = new_pos;

    // Energy cost for movement (competing mode)
    if (params.competing_mode != 0u) {
        agent.energy -= params.move_energy_cost * params.delta_time;
    }

    // Deposit pheromone
    let coord = vec2<i32>(i32(agent.position.x), i32(agent.position.y));
    let old_trail = textureLoad(trail_read, coord, 0);

    var deposit = vec4<f32>(0.0);
    if (agent.species_index == 0u) {
        deposit.x = params.trail_weight * params.delta_time;
    } else if (agent.species_index == 1u) {
        deposit.y = params.trail_weight * params.delta_time;
    } else if (agent.species_index == 2u) {
        deposit.z = params.trail_weight * params.delta_time;
    } else {
        deposit.w = params.trail_weight * params.delta_time;
    }

    let new_trail = min(old_trail + deposit, vec4<f32>(1.0));
    textureStore(trail_write, coord, new_trail);

    // Energy cost for trail deposition (competing mode)
    if (params.competing_mode != 0u) {
        agent.energy -= params.deposit_energy_cost * params.delta_time;

        // Eat food at current position
        let food_idx = u32(agent.position.y) * params.width + u32(agent.position.x);
        let current_food = food_buffer[food_idx];
        let eat_amount = min(current_food, params.food_eat_rate * params.delta_time);
        food_buffer[food_idx] = current_food - eat_amount;
        agent.energy += eat_amount * params.energy_per_food;

        // Death check
        if (agent.energy <= 0.0) {
            agent.energy = 0.0;
        }

        // Reproduction
        if (agent.energy > params.reproduction_threshold) {
            let candidate_idx = (idx + 7919u) % params.num_agents;
            var candidate = agents[candidate_idx];
            if (candidate.energy <= 0.0) {
                candidate.position = agent.position;
                candidate.angle = agent.angle + 3.14159265;
                candidate.species_index = agent.species_index;
                candidate.energy = agent.energy * 0.5;
                agent.energy = agent.energy * 0.5;
                agents[candidate_idx] = candidate;
            }
        }
    }

    agents[idx] = agent;
}
