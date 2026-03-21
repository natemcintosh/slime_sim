use egui::Context;

use crate::config_io;

#[derive(Clone, Copy, PartialEq)]
pub enum SpawnMode {
    CentreCircle,
    RandomFill,
    InwardCircle,
}

pub struct SpeciesUi {
    pub move_speed: f32,
    pub turn_speed: f32,
    pub sensor_angle_deg: f32,
    pub sensor_offset: f32,
    pub sensor_size: i32,
    pub colour: [f32; 3],
}

pub struct UiState {
    pub num_species: u32,
    pub species: [SpeciesUi; 4],
    pub trail_weight: f32,
    pub decay_rate: f32,
    pub diffuse_rate: f32,
    pub steps_per_frame: u32,
    pub spawn_mode: SpawnMode,
    pub reset_requested: bool,
    pub num_agents: u32,
    pub paused: bool,
    pub fps: f32,
    pub panel_open: bool,
    pub panel_width_points: f32,
    pub save_dialog_open: bool,
    pub save_title: String,
    pub save_notes: String,
    pub save_status: Option<String>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            num_species: 1,
            species: [
                SpeciesUi {
                    move_speed: 100.0,
                    turn_speed: 2.0,
                    sensor_angle_deg: 30.0,
                    sensor_offset: 35.0,
                    sensor_size: 1,
                    colour: [1.0, 1.0, 1.0],
                },
                SpeciesUi {
                    move_speed: 100.0,
                    turn_speed: 2.0,
                    sensor_angle_deg: 30.0,
                    sensor_offset: 35.0,
                    sensor_size: 1,
                    colour: [0.306, 0.475, 0.655],
                },
                SpeciesUi {
                    move_speed: 100.0,
                    turn_speed: 2.0,
                    sensor_angle_deg: 30.0,
                    sensor_offset: 35.0,
                    sensor_size: 1,
                    colour: [0.949, 0.557, 0.169],
                },
                SpeciesUi {
                    move_speed: 100.0,
                    turn_speed: 2.0,
                    sensor_angle_deg: 30.0,
                    sensor_offset: 35.0,
                    sensor_size: 1,
                    colour: [0.882, 0.341, 0.349],
                },
            ],
            trail_weight: 5.0,
            decay_rate: 0.3,
            diffuse_rate: 3.0,
            steps_per_frame: 1,
            spawn_mode: SpawnMode::CentreCircle,
            reset_requested: false,
            num_agents: 250_000,
            paused: false,
            fps: 0.0,
            panel_open: true,
            panel_width_points: 0.0,
            save_dialog_open: false,
            save_title: String::new(),
            save_notes: String::new(),
            save_status: None,
        }
    }
}

pub fn draw_ui(ctx: &Context, state: &mut UiState) {
    // Floating toggle button (top-left corner, always visible)
    egui::Area::new(egui::Id::new("panel_toggle"))
        .fixed_pos(egui::pos2(4.0, 4.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            if ui
                .button(if state.panel_open { "⏴" } else { "⏵" })
                .clicked()
            {
                state.panel_open = !state.panel_open;
            }
        });

    // Tab key toggles panel
    if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
        state.panel_open = !state.panel_open;
    }

    // Animated collapsible panel
    egui::SidePanel::left("controls")
        .resizable(true)
        .show_animated(ctx, state.panel_open, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Slime Simulation");
                ui.separator();

                ui.label(format!("FPS: {:.0}", state.fps));
                if ui
                    .button(if state.paused { "Resume" } else { "Pause" })
                    .clicked()
                {
                    state.paused = !state.paused;
                }
                ui.separator();

                ui.label("Global Settings");
                ui.add(egui::Slider::new(&mut state.trail_weight, 0.1..=20.0).text("Trail Weight"));
                ui.add(egui::Slider::new(&mut state.decay_rate, 0.01..=2.0).text("Decay Rate"));
                ui.add(egui::Slider::new(&mut state.diffuse_rate, 0.1..=10.0).text("Diffuse Rate"));
                ui.add(egui::Slider::new(&mut state.steps_per_frame, 1..=10).text("Steps / Frame"));
                ui.separator();

                let mut ns = state.num_species as usize;
                ui.add(egui::Slider::new(&mut ns, 1..=4).text("Species Count"));
                state.num_species = ns as u32;

                for i in 0..state.num_species as usize {
                    ui.separator();
                    ui.label(format!("Species {}", i + 1));
                    let s = &mut state.species[i];
                    ui.add(egui::Slider::new(&mut s.move_speed, 10.0..=300.0).text("Move Speed"));
                    ui.add(egui::Slider::new(&mut s.turn_speed, 0.1..=10.0).text("Turn Speed"));
                    ui.add(
                        egui::Slider::new(&mut s.sensor_angle_deg, 5.0..=90.0).text("Sensor Angle"),
                    );
                    ui.add(
                        egui::Slider::new(&mut s.sensor_offset, 5.0..=100.0)
                            .text("Sensor Distance"),
                    );
                    ui.add(egui::Slider::new(&mut s.sensor_size, 1..=5).text("Sensor Size"));
                    ui.color_edit_button_rgb(&mut s.colour);
                }

                ui.separator();
                ui.label("Spawn Mode");
                ui.radio_value(
                    &mut state.spawn_mode,
                    SpawnMode::CentreCircle,
                    "Centre Circle",
                );
                ui.radio_value(&mut state.spawn_mode, SpawnMode::RandomFill, "Random Fill");
                ui.radio_value(
                    &mut state.spawn_mode,
                    SpawnMode::InwardCircle,
                    "Inward Circle",
                );

                ui.separator();
                let mut na = state.num_agents;
                ui.add(
                    egui::Slider::new(&mut na, 1000..=500_000)
                        .text("Agents")
                        .logarithmic(true),
                );
                state.num_agents = na;

                if ui.button("Reset Simulation").clicked() {
                    state.reset_requested = true;
                }

                if ui.button("Save Configuration").clicked() {
                    state.save_dialog_open = true;
                }

                if let Some(status) = &state.save_status {
                    if status.starts_with("Error:") {
                        ui.colored_label(egui::Color32::RED, status);
                    } else {
                        ui.colored_label(egui::Color32::GREEN, status);
                    }
                }
            });
        });

    if state.save_dialog_open {
        let mut save_dialog_open = state.save_dialog_open;
        let mut should_close = false;

        egui::Window::new("Save Configuration")
            .open(&mut save_dialog_open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("Title");
                ui.text_edit_singleline(&mut state.save_title);

                ui.separator();
                ui.label("Notes");
                ui.add(
                    egui::TextEdit::multiline(&mut state.save_notes)
                        .desired_rows(6)
                        .desired_width(380.0),
                );

                ui.separator();
                if ui.button("Save").clicked() {
                    let title = state.save_title.clone();
                    let notes = state.save_notes.clone();
                    match config_io::save_ui_state_to_xml(state, &title, &notes) {
                        Ok(path) => {
                            state.save_status =
                                Some(format!("Saved configuration: {}", path.display()));
                            should_close = true;
                        }
                        Err(err) => {
                            state.save_status = Some(format!("Error: {err}"));
                        }
                    }
                }

                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });

        if should_close {
            save_dialog_open = false;
        }
        state.save_dialog_open = save_dialog_open;
    }

    // Report panel width for viewport calculation
    state.panel_width_points = ctx.available_rect().left();
}
