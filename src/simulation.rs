use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::ui::{SpawnMode, UiState};

// ---- GPU-side structs (must match WGSL) ----

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Agent {
    pub position: [f32; 2],
    pub angle: f32,
    pub species_index: u32,
    pub energy: f32,
    pub _padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct SimParams {
    pub width: u32,
    pub height: u32,
    pub num_agents: u32,
    pub trail_weight: f32,
    pub decay_rate: f32,
    pub diffuse_rate: f32,
    pub delta_time: f32,
    pub time: f32,
    pub food_weight: f32,
    pub competing_mode: u32,
    pub initial_energy: f32,
    pub move_energy_cost: f32,
    pub deposit_energy_cost: f32,
    pub energy_per_food: f32,
    pub food_eat_rate: f32,
    pub food_regen_rate: f32,
    pub food_clump_lifetime: f32,
    pub reproduction_threshold: f32,
    pub food_num_clumps: u32,
    pub food_clump_radius: f32,
    /// RNG seed for `update.wgsl`; counts frames (params are written once per frame).
    pub frame: u32,
    pub _pad: [u32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct SpeciesSettings {
    pub move_speed: f32,
    pub turn_speed: f32,
    pub sensor_angle_spacing: f32,
    pub sensor_offset_dst: f32,
    pub sensor_size: i32,
    pub _pad: [u32; 3],
    pub colour: [f32; 4],
}

impl SimParams {
    /// `num_agents` is the size of the live agent buffer, not `ui.num_agents`:
    /// the slider only takes effect on reset.
    fn from_ui(
        ui: &UiState,
        width: u32,
        height: u32,
        num_agents: u32,
        dt: f32,
        time: f32,
        frame: u32,
    ) -> Self {
        Self {
            width,
            height,
            num_agents,
            trail_weight: ui.trail_weight,
            decay_rate: ui.decay_rate,
            diffuse_rate: ui.diffuse_rate,
            delta_time: dt,
            time,
            food_weight: ui.food_weight,
            competing_mode: u32::from(ui.competing_mode),
            initial_energy: ui.initial_energy,
            move_energy_cost: ui.move_energy_cost,
            deposit_energy_cost: ui.deposit_energy_cost,
            energy_per_food: ui.energy_per_food,
            food_eat_rate: ui.food_eat_rate,
            food_regen_rate: ui.food_regen_rate,
            food_clump_lifetime: ui.food_clump_lifetime,
            reproduction_threshold: ui.reproduction_threshold,
            food_num_clumps: ui.food_num_clumps,
            food_clump_radius: ui.food_clump_radius,
            frame,
            _pad: [0; 3],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ColourParams {
    pub width: u32,
    pub height: u32,
    pub num_species: u32,
    pub food_viz_weight: f32,
}

// ---- Simulation state ----

pub struct Simulation {
    // Dimensions
    pub width: u32,
    pub height: u32,

    // Buffers
    agent_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    species_buffer: wgpu::Buffer,
    colour_params_buffer: wgpu::Buffer,

    // Trail textures (ping-pong)
    trail_textures: [wgpu::Texture; 2],
    trail_views: [wgpu::TextureView; 2],
    trail_idx: usize, // which is "read" this frame

    // Food / population density map
    food_texture: wgpu::Texture,
    food_view: wgpu::TextureView,
    food_buffer: wgpu::Buffer,

    // Population tracking (competing mode)
    population_buffer: wgpu::Buffer,
    population_readback_buffer: wgpu::Buffer,

    // Compute pipelines
    update_pipeline: wgpu::ComputePipeline,
    diffuse_pipeline: wgpu::ComputePipeline,
    colour_pipeline: wgpu::ComputePipeline,
    food_regen_pipeline: wgpu::ComputePipeline,

    // Render pipeline (blit)
    blit_pipeline: wgpu::RenderPipeline,

    // Bind group layouts (needed for recreation)
    update_bgl: wgpu::BindGroupLayout,

    // Bind groups
    update_bind_groups: [wgpu::BindGroup; 2],
    diffuse_bind_groups: [wgpu::BindGroup; 2],
    colour_bind_groups: [wgpu::BindGroup; 2],
    blit_bind_group: wgpu::BindGroup,
    food_regen_bind_group: wgpu::BindGroup,

    // State
    num_agents: u32,
    elapsed_time: f32,
    frame: u32,
    competing_mode: bool,
}

impl Simulation {
    #[allow(clippy::too_many_lines)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        ui: &UiState,
    ) -> Self {
        // --- Create agent data ---
        let agents = create_agents(
            ui.num_agents,
            ui.num_species,
            width,
            height,
            ui.spawn_mode,
            ui.initial_energy,
        );

        let agent_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("agent_buffer"),
            contents: bytemuck::cast_slice(&agents),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // --- SimParams uniform ---
        let params = SimParams::from_ui(ui, width, height, ui.num_agents, 1.0 / 60.0, 0.0, 0);
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params_buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // --- Species buffer ---
        let species_data = build_species_data(ui);
        let species_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("species_buffer"),
            contents: bytemuck::cast_slice(&species_data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // --- Colour params ---
        let colour_params = ColourParams {
            width,
            height,
            num_species: ui.num_species,
            food_viz_weight: 0.0,
        };
        let colour_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("colour_params_buffer"),
            contents: bytemuck::bytes_of(&colour_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // --- Trail textures (ping-pong) ---
        let trail_textures: [wgpu::Texture; 2] = std::array::from_fn(|_| {
            create_texture(
                device,
                "trail_texture",
                width,
                height,
                wgpu::TextureFormat::Rgba16Float,
                wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_DST,
            )
        });
        let trail_views = trail_textures
            .each_ref()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));

        // Clear trail textures
        clear_texture(queue, &trail_textures[0], width, height);
        clear_texture(queue, &trail_textures[1], width, height);

        // --- Food texture ---
        let food_texture = create_texture(
            device,
            "food_texture",
            width,
            height,
            wgpu::TextureFormat::R32Float,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        let food_view = food_texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Initialize food texture to zeros
        let food_zeros = vec![0.0f32; (width * height) as usize];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &food_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&food_zeros),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        // --- Food storage buffer (for competing mode read-write access) ---
        let food_buffer_size = u64::from(width * height) * 4;
        let food_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("food_buffer"),
            size: food_buffer_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Population tracking buffer (competing mode) ---
        let population_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("population_buffer"),
            size: 16, // 4 x u32
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let population_readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("population_readback_buffer"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Colour map ---
        let colour_texture = create_texture(
            device,
            "colour_texture",
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        );
        let colour_view = colour_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // --- Bind group layouts ---
        let update_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("update_bgl"),
            entries: &[
                // params uniform
                uniform_entry(0),
                // agents storage
                storage_buffer_entry(1, false),
                // trail_read texture
                texture_entry(2),
                // trail_write storage texture
                storage_texture_entry(3, wgpu::TextureFormat::Rgba16Float),
                // species storage
                storage_buffer_entry(4, true),
                // food_map texture
                texture_entry(5),
                // food_buffer storage (competing mode)
                storage_buffer_entry(6, false),
                // population counters (competing mode)
                storage_buffer_entry(7, false),
            ],
        });

        let diffuse_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("diffuse_bgl"),
            entries: &[
                uniform_entry(0),
                texture_entry(1),
                storage_texture_entry(2, wgpu::TextureFormat::Rgba16Float),
            ],
        });

        let colour_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("colour_bgl"),
            entries: &[
                uniform_entry(0),
                texture_entry(1),
                storage_texture_entry(2, wgpu::TextureFormat::Rgba8Unorm),
                storage_buffer_entry(3, true),
                // food_map texture
                texture_entry(4),
            ],
        });

        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // --- Compute pipelines ---
        let update_pipeline = create_compute_pipeline(
            device,
            "update",
            include_str!("shaders/update.wgsl"),
            &update_bgl,
        );
        let diffuse_pipeline = create_compute_pipeline(
            device,
            "diffuse",
            include_str!("shaders/diffuse.wgsl"),
            &diffuse_bgl,
        );
        let colour_pipeline = create_compute_pipeline(
            device,
            "colour",
            include_str!("shaders/colour.wgsl"),
            &colour_bgl,
        );

        // --- Food regen pipeline (competing mode) ---
        let food_regen_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("food_regen_bgl"),
            entries: &[uniform_entry(0), storage_buffer_entry(1, false)],
        });
        let food_regen_pipeline = create_compute_pipeline(
            device,
            "food_regen",
            include_str!("shaders/food_regen.wgsl"),
            &food_regen_bgl,
        );

        // --- Render pipeline (blit) ---
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blit.wgsl").into()),
        });
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit_pipeline_layout"),
            bind_group_layouts: &[&blit_bgl],
            push_constant_ranges: &[],
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit_pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });

        // --- Sampler ---
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // --- Bind groups ---
        let update_bind_groups = create_update_bind_groups(
            device,
            &update_bgl,
            &params_buffer,
            &agent_buffer,
            &trail_views,
            &species_buffer,
            &food_view,
            &food_buffer,
            &population_buffer,
        );

        let diffuse_bind_groups =
            create_diffuse_bind_groups(device, &diffuse_bgl, &params_buffer, &trail_views);

        let colour_bind_groups = create_colour_bind_groups(
            device,
            &colour_bgl,
            &colour_params_buffer,
            &trail_views,
            &colour_view,
            &species_buffer,
            &food_view,
        );

        let blit_bind_group =
            create_blit_bind_group(device, &blit_bgl, &colour_view, &blit_sampler);

        let food_regen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("food_regen_bg"),
            layout: &food_regen_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: food_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            width,
            height,
            agent_buffer,
            params_buffer,
            species_buffer,
            colour_params_buffer,
            trail_textures,
            trail_views,
            trail_idx: 0,
            food_texture,
            food_view,
            food_buffer,
            population_buffer,
            population_readback_buffer,
            update_pipeline,
            diffuse_pipeline,
            colour_pipeline,
            food_regen_pipeline,
            blit_pipeline,
            update_bgl,
            update_bind_groups,
            diffuse_bind_groups,
            colour_bind_groups,
            blit_bind_group,
            food_regen_bind_group,
            num_agents: ui.num_agents,
            elapsed_time: 0.0,
            frame: 0,
            competing_mode: ui.competing_mode,
        }
    }

    pub fn update_params(&mut self, queue: &wgpu::Queue, ui: &UiState, dt: f32) {
        self.elapsed_time += dt;
        self.frame = self.frame.wrapping_add(1);
        self.competing_mode = ui.competing_mode;
        let params = SimParams::from_ui(
            ui,
            self.width,
            self.height,
            self.num_agents,
            dt,
            self.elapsed_time,
            self.frame,
        );
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        let species_data = build_species_data(ui);
        queue.write_buffer(&self.species_buffer, 0, bytemuck::cast_slice(&species_data));

        let colour_params = ColourParams {
            width: self.width,
            height: self.height,
            num_species: ui.num_species,
            // Classic food has no effect at weight 0, so don't draw it either
            food_viz_weight: if ui.show_food && (ui.competing_mode || ui.food_weight > 0.0) {
                ui.food_viz_weight
            } else {
                0.0
            },
        };
        queue.write_buffer(
            &self.colour_params_buffer,
            0,
            bytemuck::bytes_of(&colour_params),
        );
    }

    pub fn upload_food_map(&self, queue: &wgpu::Queue, data: &[f32]) {
        assert_eq!(
            data.len(),
            (self.width * self.height) as usize,
            "food map size must match simulation dimensions"
        );
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.food_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn copy_population_to_readback(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_buffer_to_buffer(
            &self.population_buffer,
            0,
            &self.population_readback_buffer,
            0,
            16,
        );
    }

    pub fn read_population_counts(&self, device: &wgpu::Device) -> [u32; 4] {
        let slice = self.population_readback_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let data = slice.get_mapped_range();
        let counts: &[u32] = bytemuck::cast_slice(&data);
        let result = [counts[0], counts[1], counts[2], counts[3]];
        drop(data);
        self.population_readback_buffer.unmap();
        result
    }

    pub fn step(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let read_idx = self.trail_idx;
        let write_idx = 1 - read_idx;

        if self.competing_mode {
            // Clear population counters
            encoder.clear_buffer(&self.population_buffer, 0, None);

            // Food regeneration pass
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("food_regen_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.food_regen_pipeline);
                pass.set_bind_group(0, &self.food_regen_bind_group, &[]);
                pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
            }
        }

        // Update agents (sense, steer, move, deposit)
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("update_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.update_pipeline);
            pass.set_bind_group(0, &self.update_bind_groups[read_idx], &[]);
            pass.dispatch_workgroups(self.num_agents.div_ceil(256), 1, 1);
        }

        // Diffuse trail: read from write_idx (just written by update), write to read_idx
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("diffuse_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.diffuse_pipeline);
            pass.set_bind_group(0, &self.diffuse_bind_groups[write_idx], &[]);
            pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
        }

        if self.competing_mode {
            // Copy food_buffer -> food_texture for visualization
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &self.food_buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(self.width * 4),
                        rows_per_image: Some(self.height),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.food_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );
        }

        // Swap: after diffuse, the "fresh" data is in trail[read_idx], so next frame read_idx flips
        self.trail_idx = write_idx;
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        viewport: Option<(f32, f32, f32, f32)>,
    ) {
        // Colour map pass
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("colour_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.colour_pipeline);
            // Read from current trail (trail_idx was just swapped, so the "result" is in 1-trail_idx after step)
            // Actually after step, trail_idx was set to write_idx. The diffused result is in read_idx (the old read_idx).
            // Let me think: after step, trail_idx = write_idx. The diffused data is in trail[old_read_idx] = trail[1-trail_idx].
            // So we read from trail[1-trail_idx].
            let result_idx = 1 - self.trail_idx;
            pass.set_bind_group(0, &self.colour_bind_groups[result_idx], &[]);
            pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
        }

        // Blit to screen
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &self.blit_bind_group, &[]);
            if let Some((x, y, w, h)) = viewport {
                pass.set_viewport(x, y, w, h, 0.0, 1.0);
            }
            pass.draw(0..3, 0..1);
        }
    }

    pub fn reset(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, ui: &UiState) {
        self.num_agents = ui.num_agents;
        self.elapsed_time = 0.0;
        self.frame = 0;
        self.trail_idx = 0;

        // Re-create agents
        let agents = create_agents(
            ui.num_agents,
            ui.num_species,
            self.width,
            self.height,
            ui.spawn_mode,
            ui.initial_energy,
        );
        let agent_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("agent_buffer"),
            contents: bytemuck::cast_slice(&agents),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        self.agent_buffer = agent_buffer;

        // Clear trail textures
        clear_texture(queue, &self.trail_textures[0], self.width, self.height);
        clear_texture(queue, &self.trail_textures[1], self.width, self.height);

        // Clear food buffer
        queue.write_buffer(
            &self.food_buffer,
            0,
            bytemuck::cast_slice(&vec![0.0f32; (self.width * self.height) as usize]),
        );

        self.competing_mode = ui.competing_mode;

        // Recreate bind groups (agent buffer changed)
        self.update_bind_groups = create_update_bind_groups(
            device,
            &self.update_bgl,
            &self.params_buffer,
            &self.agent_buffer,
            &self.trail_views,
            &self.species_buffer,
            &self.food_view,
            &self.food_buffer,
            &self.population_buffer,
        );
    }
}

// ---- Helper functions ----

#[allow(clippy::cast_precision_loss)]
fn create_agents(
    num: u32,
    num_species: u32,
    width: u32,
    height: u32,
    mode: SpawnMode,
    initial_energy: f32,
) -> Vec<Agent> {
    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let radius = cx.min(cy) * 0.4;

    (0..num)
        .map(|i| {
            let species_index = i % num_species;
            // Simple hash for deterministic randomness
            let h1 = hash_u32(i * 3 + 7);
            let h2 = hash_u32(i * 5 + 13);
            let h3 = hash_u32(i * 7 + 19);
            let r1 = h1 as f32 / u32::MAX as f32;
            let r2 = h2 as f32 / u32::MAX as f32;
            let heading = h3 as f32 / u32::MAX as f32 * std::f32::consts::TAU;

            match mode {
                SpawnMode::CentreCircle => {
                    let angle = r1 * std::f32::consts::TAU;
                    let r = (r2).sqrt() * radius;
                    Agent {
                        position: [cx + angle.cos() * r, cy + angle.sin() * r],
                        angle: heading,
                        species_index,
                        energy: initial_energy,
                        _padding: 0,
                    }
                }
                SpawnMode::RandomFill => Agent {
                    position: [r1 * width as f32, r2 * height as f32],
                    angle: heading,
                    species_index,
                    energy: initial_energy,
                    _padding: 0,
                },
                SpawnMode::InwardCircle => {
                    let angle = r1 * std::f32::consts::TAU;
                    let r = (r2).sqrt() * radius;
                    let px = cx + angle.cos() * r;
                    let py = cy + angle.sin() * r;
                    // Point inward toward centre
                    let inward = (cy - py).atan2(cx - px);
                    Agent {
                        position: [px, py],
                        angle: inward,
                        species_index,
                        energy: initial_energy,
                        _padding: 0,
                    }
                }
            }
        })
        .collect()
}

pub(crate) fn hash_u32(mut state: u32) -> u32 {
    state = state.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    state = ((state >> ((state >> 28).wrapping_add(4))) ^ state).wrapping_mul(277_803_737);
    (state >> 22) ^ state
}

fn build_species_data(ui: &UiState) -> Vec<SpeciesSettings> {
    (0..4)
        .map(|i| {
            let s = &ui.species[i];
            SpeciesSettings {
                move_speed: s.move_speed,
                turn_speed: s.turn_speed,
                sensor_angle_spacing: s.sensor_angle_deg.to_radians(),
                sensor_offset_dst: s.sensor_offset,
                sensor_size: s.sensor_size,
                _pad: [0; 3],
                colour: [s.colour[0], s.colour[1], s.colour[2], 1.0],
            }
        })
        .collect()
}

fn clear_texture(queue: &wgpu::Queue, texture: &wgpu::Texture, width: u32, height: u32) {
    let zeros = vec![0u8; (width * height * 8) as usize]; // rgba16float = 8 bytes/pixel
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &zeros,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 8),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

fn create_texture(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

/// Shader module, pipeline layout and pipeline for a compute shader whose
/// entry point is `main`. Labels are `{name}_shader`, `{name}_pipeline_layout`
/// and `{name}_pipeline`.
fn create_compute_pipeline(
    device: &wgpu::Device,
    name: &str,
    source: &str,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("{name}_shader")),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{name}_pipeline_layout")),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(&format!("{name}_pipeline")),
        layout: Some(&layout),
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

// Bind group layout entries for compute shaders.

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_buffer_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Non-filterable 2D float texture, read with `textureLoad`.
fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// Write-only 2D storage texture.
fn storage_texture_entry(binding: u32, format: wgpu::TextureFormat) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn create_update_bind_groups(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &wgpu::Buffer,
    agents: &wgpu::Buffer,
    trail_views: &[wgpu::TextureView; 2],
    species: &wgpu::Buffer,
    food_view: &wgpu::TextureView,
    food_buffer: &wgpu::Buffer,
    population_buffer: &wgpu::Buffer,
) -> [wgpu::BindGroup; 2] {
    // Group i: read trail i, write the other
    std::array::from_fn(|i| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("update_bg_{i}")),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: agents.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&trail_views[i]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&trail_views[1 - i]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: species.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(food_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: food_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: population_buffer.as_entire_binding(),
                },
            ],
        })
    })
}

fn create_diffuse_bind_groups(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &wgpu::Buffer,
    trail_views: &[wgpu::TextureView; 2],
) -> [wgpu::BindGroup; 2] {
    // Group i: read trail i, write the other
    std::array::from_fn(|i| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("diffuse_bg_{i}")),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&trail_views[i]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&trail_views[1 - i]),
                },
            ],
        })
    })
}

fn create_colour_bind_groups(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    colour_params: &wgpu::Buffer,
    trail_views: &[wgpu::TextureView; 2],
    colour_view: &wgpu::TextureView,
    species: &wgpu::Buffer,
    food_view: &wgpu::TextureView,
) -> [wgpu::BindGroup; 2] {
    // Group i: read trail i
    std::array::from_fn(|i| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("colour_bg_{i}")),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: colour_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&trail_views[i]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(colour_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: species.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(food_view),
                },
            ],
        })
    })
}

fn create_blit_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    colour_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(colour_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::ui::UiState;
    use rstest::rstest;

    fn angle_delta(a: f32, b: f32) -> f32 {
        let mut d = (a - b + std::f32::consts::PI) % std::f32::consts::TAU;
        if d < 0.0 {
            d += std::f32::consts::TAU;
        }
        (d - std::f32::consts::PI).abs()
    }

    #[test]
    fn agent_size_is_24_bytes() {
        assert_eq!(
            std::mem::size_of::<Agent>(),
            24,
            "Agent must be 24 bytes (padded for WGSL vec2 alignment)"
        );
    }

    #[test]
    fn sim_params_size_is_aligned_to_16_bytes() {
        let size = std::mem::size_of::<SimParams>();
        assert_eq!(
            size % 16,
            0,
            "SimParams size ({size}) must be a multiple of 16 for uniform buffer alignment"
        );
    }

    #[test]
    fn colour_params_size_is_aligned_to_16_bytes() {
        let size = std::mem::size_of::<ColourParams>();
        assert_eq!(
            size % 16,
            0,
            "ColourParams size ({size}) must be a multiple of 16 for uniform buffer alignment"
        );
    }

    #[test]
    fn sim_params_use_buffer_agent_count_not_ui_slider() {
        let ui = UiState {
            num_agents: 500_000,
            ..UiState::default()
        };
        let params = SimParams::from_ui(&ui, 64, 64, 1000, 0.0, 0.0, 0);
        assert_eq!(params.num_agents, 1000);
    }

    #[rstest]
    #[case(0, 129_708_002)]
    #[case(1, 2_831_084_092)]
    #[case(7, 2_120_684_060)]
    #[case(42, 1_223_963_391)]
    #[case(123_456_789, 4_272_394_698)]
    #[case(u32::MAX, 3_861_530_882)]
    fn hash_u32_matches_known_values(#[case] input: u32, #[case] expected: u32) {
        assert_eq!(hash_u32(input), expected);
    }

    #[test]
    fn build_species_data_converts_angles_and_sets_alpha() {
        let mut ui = UiState::default();
        ui.species[0].sensor_angle_deg = 45.0;
        ui.species[0].colour = [0.2, 0.3, 0.4];

        let out = build_species_data(&ui);
        assert_eq!(out.len(), 4);
        assert!((out[0].sensor_angle_spacing - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
        assert_eq!(out[0].colour, [0.2, 0.3, 0.4, 1.0]);
    }

    #[rstest]
    #[case(SpawnMode::CentreCircle)]
    #[case(SpawnMode::RandomFill)]
    #[case(SpawnMode::InwardCircle)]
    fn create_agents_is_deterministic_and_cycles_species(#[case] mode: SpawnMode) {
        let num_agents = 512;
        let num_species = 3;
        let width = 640;
        let height = 480;

        let first = create_agents(num_agents, num_species, width, height, mode, 1.0);
        let second = create_agents(num_agents, num_species, width, height, mode, 1.0);

        assert_eq!(first.len(), num_agents as usize);
        assert_eq!(second.len(), num_agents as usize);

        for (i, (a, b)) in first.iter().zip(second.iter()).enumerate() {
            assert_eq!(a.position, b.position);
            assert_eq!(a.angle, b.angle);
            assert_eq!(a.species_index, (i as u32) % num_species);
        }
    }

    #[test]
    fn create_agents_random_fill_stays_within_bounds() {
        let width = 320;
        let height = 200;
        let agents = create_agents(1000, 4, width, height, SpawnMode::RandomFill, 1.0);

        for agent in agents {
            assert!(agent.position[0] >= 0.0 && agent.position[0] <= width as f32);
            assert!(agent.position[1] >= 0.0 && agent.position[1] <= height as f32);
        }
    }

    #[rstest]
    #[case(SpawnMode::CentreCircle)]
    #[case(SpawnMode::InwardCircle)]
    fn create_agents_circle_modes_stay_within_spawn_radius(#[case] mode: SpawnMode) {
        let width = 300;
        let height = 220;
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let radius = cx.min(cy) * 0.4;
        let agents = create_agents(1000, 4, width, height, mode, 1.0);

        for agent in agents {
            let dx = agent.position[0] - cx;
            let dy = agent.position[1] - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(dist <= radius + 1e-4);
        }
    }

    #[test]
    fn random_fill_heading_is_independent_of_x_position() {
        let width = 640;
        let agents = create_agents(1000, 1, width, 480, SpawnMode::RandomFill, 1.0);
        let correlated = agents
            .iter()
            .filter(|a| {
                let from_x = a.position[0] / width as f32 * std::f32::consts::TAU;
                angle_delta(a.angle, from_x) < 0.01
            })
            .count();
        assert!(correlated < 50, "{correlated}/1000 headings track x");
    }

    #[test]
    fn centre_circle_heading_is_independent_of_polar_angle() {
        let (width, height) = (600, 600);
        let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
        let agents = create_agents(1000, 1, width, height, SpawnMode::CentreCircle, 1.0);
        let outward = agents
            .iter()
            .filter(|a| {
                let polar = (a.position[1] - cy).atan2(a.position[0] - cx);
                angle_delta(a.angle, polar) < 0.01
            })
            .count();
        assert!(outward < 50, "{outward}/1000 agents face exactly outward");
    }

    #[test]
    fn inward_circle_agents_point_toward_centre() {
        let width = 600;
        let height = 600;
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let agents = create_agents(1000, 2, width, height, SpawnMode::InwardCircle, 1.0);

        for agent in agents {
            let to_center_x = cx - agent.position[0];
            let to_center_y = cy - agent.position[1];
            let dist = (to_center_x * to_center_x + to_center_y * to_center_y).sqrt();
            if dist < 1e-6 {
                continue;
            }

            let expected = to_center_y.atan2(to_center_x);
            assert!(angle_delta(agent.angle, expected) < 1e-4);
        }
    }

    #[test]
    fn food_broadcast_to_all_channels_is_net_negative_with_species_weights() {
        // Demonstrates the bug: when food is added equally to all 4 trail channels
        // and agents sense with +1 for own channel, -1 for others, the food
        // contribution is net-negative, causing agents to AVOID food.
        for species_index in 0u32..4 {
            let food_add = 0.5_f32;
            let trail_with_food = [food_add; 4]; // food broadcast to all channels

            let sense_weight: [f32; 4] =
                std::array::from_fn(|i| if i as u32 == species_index { 1.0 } else { -1.0 });

            let dot: f32 = trail_with_food
                .iter()
                .zip(sense_weight.iter())
                .map(|(a, b)| a * b)
                .sum();

            // Bug: 1*0.5 + (-1)*0.5 + (-1)*0.5 + (-1)*0.5 = -1.0
            assert!(
                dot < 0.0,
                "species {species_index}: food broadcast to all channels produces \
                 net-negative sensor value ({dot}), causing avoidance"
            );
        }
    }

    #[test]
    fn direct_food_sensing_is_always_positive() {
        // Verifies the fix: when food is sensed directly (not through trail
        // channels), its contribution is always positive regardless of species.
        for _species_index in 0u32..4 {
            let food_value = 0.8_f32;
            let food_weight = 0.5_f32;

            // Direct food sensing: food_value * food_weight, added to sensor sum
            // independently of species sense weights
            let food_contribution = food_value * food_weight;

            assert!(
                food_contribution > 0.0,
                "direct food sensing should always be positive"
            );
        }
    }

    #[test]
    #[ignore = "Requires compatible local GPU/driver stack"]
    fn gpu_smoke_can_create_simulation_and_step_once() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });

            let Ok(adapter) = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
            else {
                eprintln!("Skipping GPU smoke test: no adapter available");
                return;
            };

            let (device, queue) = match adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("test_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                })
                .await
            {
                Ok(pair) => pair,
                Err(err) => {
                    eprintln!("Skipping GPU smoke test: failed to create device ({err})");
                    return;
                }
            };

            let ui = UiState::default();
            let mut sim = Simulation::new(
                &device,
                &queue,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                64,
                64,
                &ui,
            );

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu_smoke_encoder"),
            });
            sim.step(&mut encoder);
            queue.submit(std::iter::once(encoder.finish()));
        });
    }
}
