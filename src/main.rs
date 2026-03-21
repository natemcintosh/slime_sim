mod config_io;
mod food;
mod gpu;
mod simulation;
mod ui;

use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

struct App {
    gpu: Option<gpu::GpuContext>,
    sim: Option<simulation::Simulation>,
    egui_ctx: egui::Context,
    egui_winit: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,
    window: Option<Arc<Window>>,
    ui_state: ui::UiState,
    last_frame: Instant,
    smoothed_fps: f32,
}

impl App {
    fn new() -> Self {
        Self {
            gpu: None,
            sim: None,
            egui_ctx: egui::Context::default(),
            egui_winit: None,
            egui_renderer: None,
            window: None,
            ui_state: ui::UiState::default(),
            last_frame: Instant::now(),
            smoothed_fps: 0.0,
        }
    }
}

impl ApplicationHandler for App {
    #[allow(clippy::cast_possible_truncation)]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("Slime Simulation")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

        let window = Arc::new(event_loop.create_window(attrs).unwrap());

        let gpu = pollster::block_on(gpu::GpuContext::new(window.clone()));

        let sim_width = gpu.surface_config.width;
        let sim_height = gpu.surface_config.height;

        let sim = simulation::Simulation::new(
            &gpu.device,
            &gpu.queue,
            gpu.surface_format(),
            sim_width,
            sim_height,
            &self.ui_state,
        );

        let egui_winit = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format(),
            egui_wgpu::RendererOptions::default(),
        );

        self.window = Some(window);
        self.egui_winit = Some(egui_winit);
        self.egui_renderer = Some(egui_renderer);
        self.sim = Some(sim);
        self.gpu = Some(gpu);
    }

    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss
    )]
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };

        // Let egui handle events first
        if let Some(egui_winit) = self.egui_winit.as_mut() {
            let response = egui_winit.on_window_event(window, &event);
            if response.consumed {
                return;
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                gpu.resize(new_size.width, new_size.height);
                // Recreate simulation with new dimensions
                if new_size.width > 0 && new_size.height > 0 {
                    let new_sim = simulation::Simulation::new(
                        &gpu.device,
                        &gpu.queue,
                        gpu.surface_format(),
                        new_size.width,
                        new_size.height,
                        &self.ui_state,
                    );
                    // Re-upload food map at new dimensions
                    if self.ui_state.food_weight > 0.0 {
                        let food_data = food::generate_food_map(
                            new_size.width,
                            new_size.height,
                            self.ui_state.food_num_clumps,
                            self.ui_state.food_clump_radius,
                            self.ui_state.food_seed,
                        );
                        new_sim.upload_food_map(&gpu.queue, &food_data);
                    }
                    self.sim = Some(new_sim);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(self.last_frame).as_secs_f32();
                self.last_frame = now;

                if dt > 0.0 {
                    self.smoothed_fps = self.smoothed_fps * 0.95 + (1.0 / dt) * 0.05;
                }
                self.ui_state.fps = self.smoothed_fps;

                let Some(sim) = self.sim.as_mut() else {
                    return;
                };

                // Handle reset
                if self.ui_state.reset_requested {
                    self.ui_state.reset_requested = false;
                    sim.reset(&gpu.device, &gpu.queue, &self.ui_state);
                    // Re-upload food map after reset
                    if self.ui_state.food_weight > 0.0 {
                        let food_data = food::generate_food_map(
                            sim.width,
                            sim.height,
                            self.ui_state.food_num_clumps,
                            self.ui_state.food_clump_radius,
                            self.ui_state.food_seed,
                        );
                        sim.upload_food_map(&gpu.queue, &food_data);
                    }
                }

                // Handle food regeneration
                if self.ui_state.food_regen_requested {
                    self.ui_state.food_regen_requested = false;
                    let food_data = food::generate_food_map(
                        sim.width,
                        sim.height,
                        self.ui_state.food_num_clumps,
                        self.ui_state.food_clump_radius,
                        self.ui_state.food_seed,
                    );
                    sim.upload_food_map(&gpu.queue, &food_data);
                }

                // Update params from UI
                let step_dt = if self.ui_state.paused {
                    0.0
                } else {
                    dt / self.ui_state.steps_per_frame as f32
                };
                sim.update_params(&gpu.queue, &self.ui_state, step_dt);

                // Build egui
                let egui_winit = self.egui_winit.as_mut().unwrap();
                let raw_input = egui_winit.take_egui_input(window);
                let full_output = self.egui_ctx.run(raw_input, |ctx| {
                    ui::draw_ui(ctx, &mut self.ui_state);
                });
                egui_winit.handle_platform_output(window, full_output.platform_output);
                let clipped_primitives = self
                    .egui_ctx
                    .tessellate(full_output.shapes, full_output.pixels_per_point);

                // Get surface texture
                let output = match gpu.surface.get_current_texture() {
                    Ok(t) => t,
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        gpu.resize(gpu.surface_config.width, gpu.surface_config.height);
                        return;
                    }
                    Err(e) => {
                        log::error!("Surface error: {e:?}");
                        return;
                    }
                };
                let view = output
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let mut encoder =
                    gpu.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("frame_encoder"),
                        });

                // Simulation steps
                if !self.ui_state.paused {
                    for _ in 0..self.ui_state.steps_per_frame {
                        sim.step(&mut encoder);
                    }
                }

                // Render simulation to screen (offset by panel width)
                let scale = window.scale_factor() as f32;
                let panel_px = self.ui_state.panel_width_points * scale;
                let surface_w = gpu.surface_config.width as f32;
                let surface_h = gpu.surface_config.height as f32;

                let viewport = if panel_px > 1.0 {
                    Some((panel_px, 0.0, (surface_w - panel_px).max(1.0), surface_h))
                } else {
                    None
                };

                sim.render(&mut encoder, &view, viewport);

                // Render egui on top
                let screen_descriptor = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [gpu.surface_config.width, gpu.surface_config.height],
                    pixels_per_point: window.scale_factor() as f32,
                };

                let egui_renderer = self.egui_renderer.as_mut().unwrap();
                for (id, image_delta) in &full_output.textures_delta.set {
                    egui_renderer.update_texture(&gpu.device, &gpu.queue, *id, image_delta);
                }
                egui_renderer.update_buffers(
                    &gpu.device,
                    &gpu.queue,
                    &mut encoder,
                    &clipped_primitives,
                    &screen_descriptor,
                );

                {
                    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("egui_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    let mut pass = pass.forget_lifetime();
                    egui_renderer.render(&mut pass, &clipped_primitives, &screen_descriptor);
                }

                gpu.queue.submit(std::iter::once(encoder.finish()));
                output.present();

                for id in &full_output.textures_delta.free {
                    egui_renderer.free_texture(id);
                }

                window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
