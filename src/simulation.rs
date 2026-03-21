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

    // Colour map
    #[allow(dead_code)]
    colour_texture: wgpu::Texture,
    colour_view: wgpu::TextureView,

    // Food / population density map
    #[allow(dead_code)]
    food_texture: wgpu::Texture,
    food_view: wgpu::TextureView,

    // Compute pipelines
    update_pipeline: wgpu::ComputePipeline,
    diffuse_pipeline: wgpu::ComputePipeline,
    colour_pipeline: wgpu::ComputePipeline,

    // Render pipeline (blit)
    blit_pipeline: wgpu::RenderPipeline,

    // Bind group layouts (needed for recreation)
    update_bgl: wgpu::BindGroupLayout,
    diffuse_bgl: wgpu::BindGroupLayout,
    colour_bgl: wgpu::BindGroupLayout,
    blit_bgl: wgpu::BindGroupLayout,

    // Bind groups
    update_bind_groups: [wgpu::BindGroup; 2],
    diffuse_bind_groups: [wgpu::BindGroup; 2],
    colour_bind_groups: [wgpu::BindGroup; 2],
    blit_bind_group: wgpu::BindGroup,

    // Sampler for blit
    blit_sampler: wgpu::Sampler,

    // State
    num_agents: u32,
    elapsed_time: f32,
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
        let agents = create_agents(ui.num_agents, ui.num_species, width, height, ui.spawn_mode);

        let agent_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("agent_buffer"),
            contents: bytemuck::cast_slice(&agents),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // --- SimParams uniform ---
        let params = SimParams {
            width,
            height,
            num_agents: ui.num_agents,
            trail_weight: ui.trail_weight,
            decay_rate: ui.decay_rate,
            diffuse_rate: ui.diffuse_rate,
            delta_time: 1.0 / 60.0,
            time: 0.0,
            food_weight: ui.food_weight,
            _pad: [0; 3],
        };
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
        let trail_desc = wgpu::TextureDescriptor {
            label: Some("trail_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        let trail_a = device.create_texture(&trail_desc);
        let trail_b = device.create_texture(&trail_desc);
        let trail_view_a = trail_a.create_view(&wgpu::TextureViewDescriptor::default());
        let trail_view_b = trail_b.create_view(&wgpu::TextureViewDescriptor::default());

        // Clear trail textures
        clear_texture(queue, &trail_a, width, height);
        clear_texture(queue, &trail_b, width, height);

        // --- Food texture ---
        let food_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("food_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
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

        // --- Colour map ---
        let colour_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("colour_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let colour_view = colour_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // --- Shader modules ---
        let update_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("update_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/update.wgsl").into()),
        });
        let diffuse_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("diffuse_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/diffuse.wgsl").into()),
        });
        let colour_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("colour_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/colour.wgsl").into()),
        });
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blit.wgsl").into()),
        });

        // --- Bind group layouts ---
        let update_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("update_bgl"),
            entries: &[
                // params uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // agents storage
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // trail_read texture
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // trail_write storage texture
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                // species storage
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let diffuse_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("diffuse_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                // food_map texture
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let colour_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("colour_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // food_map texture
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
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
        let update_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("update_pipeline_layout"),
                bind_group_layouts: &[&update_bgl],
                push_constant_ranges: &[],
            });
        let update_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("update_pipeline"),
            layout: Some(&update_pipeline_layout),
            module: &update_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let diffuse_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("diffuse_pipeline_layout"),
                bind_group_layouts: &[&diffuse_bgl],
                push_constant_ranges: &[],
            });
        let diffuse_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("diffuse_pipeline"),
            layout: Some(&diffuse_pipeline_layout),
            module: &diffuse_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let colour_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("colour_pipeline_layout"),
                bind_group_layouts: &[&colour_bgl],
                push_constant_ranges: &[],
            });
        let colour_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("colour_pipeline"),
            layout: Some(&colour_pipeline_layout),
            module: &colour_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // --- Render pipeline (blit) ---
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
            &trail_view_a,
            &trail_view_b,
            &species_buffer,
        );

        let diffuse_bind_groups = create_diffuse_bind_groups(
            device,
            &diffuse_bgl,
            &params_buffer,
            &trail_view_a,
            &trail_view_b,
            &food_view,
        );

        let colour_bind_groups = create_colour_bind_groups(
            device,
            &colour_bgl,
            &colour_params_buffer,
            &trail_view_a,
            &trail_view_b,
            &colour_view,
            &species_buffer,
            &food_view,
        );

        let blit_bind_group =
            create_blit_bind_group(device, &blit_bgl, &colour_view, &blit_sampler);

        Self {
            width,
            height,
            agent_buffer,
            params_buffer,
            species_buffer,
            colour_params_buffer,
            trail_textures: [trail_a, trail_b],
            trail_views: [trail_view_a, trail_view_b],
            trail_idx: 0,
            colour_texture,
            colour_view,
            food_texture,
            food_view,
            update_pipeline,
            diffuse_pipeline,
            colour_pipeline,
            blit_pipeline,
            update_bgl,
            diffuse_bgl,
            colour_bgl,
            blit_bgl,
            update_bind_groups,
            diffuse_bind_groups,
            colour_bind_groups,
            blit_bind_group,
            blit_sampler,
            num_agents: ui.num_agents,
            elapsed_time: 0.0,
        }
    }

    pub fn update_params(&mut self, queue: &wgpu::Queue, ui: &UiState, dt: f32) {
        self.elapsed_time += dt;
        let params = SimParams {
            width: self.width,
            height: self.height,
            num_agents: self.num_agents,
            trail_weight: ui.trail_weight,
            decay_rate: ui.decay_rate,
            diffuse_rate: ui.diffuse_rate,
            delta_time: dt,
            time: self.elapsed_time,
            food_weight: ui.food_weight,
            _pad: [0; 3],
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        let species_data = build_species_data(ui);
        queue.write_buffer(&self.species_buffer, 0, bytemuck::cast_slice(&species_data));

        let colour_params = ColourParams {
            width: self.width,
            height: self.height,
            num_species: ui.num_species,
            food_viz_weight: if ui.show_food {
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

    pub fn step(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let read_idx = self.trail_idx;
        let write_idx = 1 - read_idx;

        // Update agents (sense, steer, move, deposit)
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("update_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.update_pipeline);
            // Bind group index: read_idx reads from trail[read_idx], writes to trail[write_idx]
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
            // After update: trail[write_idx] has the deposits. Diffuse reads write_idx, writes read_idx
            pass.set_bind_group(0, &self.diffuse_bind_groups[write_idx], &[]);
            pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
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
        self.trail_idx = 0;

        // Re-create agents
        let agents = create_agents(
            ui.num_agents,
            ui.num_species,
            self.width,
            self.height,
            ui.spawn_mode,
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

        // Recreate bind groups (agent buffer changed)
        self.update_bind_groups = create_update_bind_groups(
            device,
            &self.update_bgl,
            &self.params_buffer,
            &self.agent_buffer,
            &self.trail_views[0],
            &self.trail_views[1],
            &self.species_buffer,
        );
        self.diffuse_bind_groups = create_diffuse_bind_groups(
            device,
            &self.diffuse_bgl,
            &self.params_buffer,
            &self.trail_views[0],
            &self.trail_views[1],
            &self.food_view,
        );
        self.colour_bind_groups = create_colour_bind_groups(
            device,
            &self.colour_bgl,
            &self.colour_params_buffer,
            &self.trail_views[0],
            &self.trail_views[1],
            &self.colour_view,
            &self.species_buffer,
            &self.food_view,
        );
        self.blit_bind_group = create_blit_bind_group(
            device,
            &self.blit_bgl,
            &self.colour_view,
            &self.blit_sampler,
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
            let r1 = h1 as f32 / u32::MAX as f32;
            let r2 = h2 as f32 / u32::MAX as f32;

            match mode {
                SpawnMode::CentreCircle => {
                    let angle = r1 * std::f32::consts::TAU;
                    let r = (r2).sqrt() * radius;
                    Agent {
                        position: [cx + angle.cos() * r, cy + angle.sin() * r],
                        angle: r1 * std::f32::consts::TAU,
                        species_index,
                    }
                }
                SpawnMode::RandomFill => Agent {
                    position: [r1 * width as f32, r2 * height as f32],
                    angle: r1 * std::f32::consts::TAU,
                    species_index,
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

fn create_update_bind_groups(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &wgpu::Buffer,
    agents: &wgpu::Buffer,
    trail_view_a: &wgpu::TextureView,
    trail_view_b: &wgpu::TextureView,
    species: &wgpu::Buffer,
) -> [wgpu::BindGroup; 2] {
    // Group 0: read A, write B
    let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("update_bg_0"),
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
                resource: wgpu::BindingResource::TextureView(trail_view_a),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(trail_view_b),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: species.as_entire_binding(),
            },
        ],
    });
    // Group 1: read B, write A
    let bg1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("update_bg_1"),
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
                resource: wgpu::BindingResource::TextureView(trail_view_b),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(trail_view_a),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: species.as_entire_binding(),
            },
        ],
    });
    [bg0, bg1]
}

fn create_diffuse_bind_groups(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &wgpu::Buffer,
    trail_view_a: &wgpu::TextureView,
    trail_view_b: &wgpu::TextureView,
    food_view: &wgpu::TextureView,
) -> [wgpu::BindGroup; 2] {
    let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("diffuse_bg_0"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(trail_view_a),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(trail_view_b),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(food_view),
            },
        ],
    });
    let bg1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("diffuse_bg_1"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(trail_view_b),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(trail_view_a),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(food_view),
            },
        ],
    });
    [bg0, bg1]
}

#[allow(clippy::too_many_arguments)]
fn create_colour_bind_groups(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    colour_params: &wgpu::Buffer,
    trail_view_a: &wgpu::TextureView,
    trail_view_b: &wgpu::TextureView,
    colour_view: &wgpu::TextureView,
    species: &wgpu::Buffer,
    food_view: &wgpu::TextureView,
) -> [wgpu::BindGroup; 2] {
    let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("colour_bg_0"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: colour_params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(trail_view_a),
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
    });
    let bg1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("colour_bg_1"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: colour_params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(trail_view_b),
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
    });
    [bg0, bg1]
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

    #[rstest]
    #[case(0, 129708002)]
    #[case(1, 2831084092)]
    #[case(7, 2120684060)]
    #[case(42, 1223963391)]
    #[case(123456789, 4272394698)]
    #[case(u32::MAX, 3861530882)]
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

        let first = create_agents(num_agents, num_species, width, height, mode);
        let second = create_agents(num_agents, num_species, width, height, mode);

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
        let agents = create_agents(1000, 4, width, height, SpawnMode::RandomFill);

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
        let agents = create_agents(1000, 4, width, height, mode);

        for agent in agents {
            let dx = agent.position[0] - cx;
            let dy = agent.position[1] - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(dist <= radius + 1e-4);
        }
    }

    #[test]
    fn inward_circle_agents_point_toward_centre() {
        let width = 600;
        let height = 600;
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let agents = create_agents(1000, 2, width, height, SpawnMode::InwardCircle);

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
    #[ignore = "Requires compatible local GPU/driver stack"]
    fn gpu_smoke_can_create_simulation_and_step_once() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });

            let adapter = match instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
            {
                Ok(adapter) => adapter,
                Err(_) => {
                    eprintln!("Skipping GPU smoke test: no adapter available");
                    return;
                }
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
