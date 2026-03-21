use egui::Context;

use crate::config_io;

/// Categorical palettes — distinct colors, best for telling species apart.
const PALETTES_CATEGORICAL: &[(&str, [[f32; 3]; 4])] = &[
    (
        "Tableau 10",
        [
            [0.306, 0.475, 0.655], // #4e79a7 steel blue
            [0.949, 0.557, 0.169], // #f28e2b tangerine
            [0.882, 0.341, 0.349], // #e15759 brick red
            [0.463, 0.718, 0.698], // #76b7b2 teal
        ],
    ),
    (
        "Okabe-Ito",
        [
            [0.902, 0.624, 0.000], // #e69f00 orange
            [0.337, 0.706, 0.914], // #56b4e9 sky blue
            [0.000, 0.620, 0.451], // #009e73 bluish green
            [0.800, 0.475, 0.655], // #cc79a7 reddish purple
        ],
    ),
    (
        "Set 1",
        [
            [0.894, 0.102, 0.110], // #e41a1c red
            [0.216, 0.494, 0.722], // #377eb8 blue
            [0.302, 0.686, 0.290], // #4daf4a green
            [0.596, 0.306, 0.639], // #984ea3 purple
        ],
    ),
    (
        "Dark 2",
        [
            [0.106, 0.620, 0.467], // #1b9e77 teal
            [0.851, 0.373, 0.008], // #d95f02 orange
            [0.459, 0.439, 0.702], // #7570b3 lilac
            [0.906, 0.161, 0.541], // #e7298a magenta pink
        ],
    ),
    (
        "Neon",
        [
            [0.000, 1.000, 1.000], // cyan
            [1.000, 0.200, 0.800], // hot pink
            [0.200, 1.000, 0.200], // lime green
            [1.000, 0.800, 0.000], // amber
        ],
    ),
];

/// Sequential palettes — colors sampled from perceptually-uniform gradients.
const PALETTES_SEQUENTIAL: &[(&str, [[f32; 3]; 4])] = &[
    (
        "Viridis",
        [
            [0.267, 0.004, 0.329], // #440154 purple
            [0.192, 0.400, 0.553], // #31688e teal-blue
            [0.208, 0.718, 0.475], // #35b779 green
            [0.992, 0.906, 0.141], // #fde724 yellow
        ],
    ),
    (
        "Plasma",
        [
            [0.416, 0.000, 0.659], // #6a00a8 violet
            [0.800, 0.306, 0.471], // #cc4e78 hot pink
            [0.973, 0.584, 0.251], // #f89540 orange
            [0.941, 0.976, 0.129], // #f0f921 yellow
        ],
    ),
    (
        "Fire",
        [
            [0.600, 0.000, 0.000], // deep red
            [1.000, 0.100, 0.000], // bright red
            [1.000, 0.500, 0.000], // orange
            [1.000, 1.000, 0.000], // yellow
        ],
    ),
];

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[allow(clippy::struct_excessive_bools)]
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
    pub load_dialog_open: bool,
    pub available_configs: Vec<config_io::ConfigEntry>,
    pub selected_config_index: Option<usize>,
    pub load_status: Option<String>,
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
            load_dialog_open: false,
            available_configs: Vec::new(),
            selected_config_index: None,
            load_status: None,
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn palette_row(
    ui: &mut egui::Ui,
    name: &str,
    colors: &[[f32; 3]; 4],
    species: &mut [SpeciesUi; 4],
) {
    ui.horizontal(|ui| {
        let mut apply = ui.small_button(name).clicked();
        for &color in colors {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
            if ui.is_rect_visible(rect) {
                ui.painter().rect_filled(
                    rect,
                    2.0,
                    egui::Color32::from_rgb(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                    ),
                );
            }
            apply |= response.clicked();
        }
        if apply {
            for (i, &color) in colors.iter().enumerate() {
                species[i].colour = color;
            }
        }
    });
}

#[allow(clippy::too_many_lines)]
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
                #[allow(clippy::cast_possible_truncation)]
                {
                    state.num_species = ns as u32;
                }

                egui::CollapsingHeader::new("Palette Presets")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label("Categorical");
                        for &(name, colors) in PALETTES_CATEGORICAL {
                            palette_row(ui, name, &colors, &mut state.species);
                        }
                        ui.add_space(4.0);
                        ui.label("Sequential");
                        for &(name, colors) in PALETTES_SEQUENTIAL {
                            palette_row(ui, name, &colors, &mut state.species);
                        }
                    });

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

                if ui.button("Load Configuration").clicked() {
                    state.available_configs = config_io::list_config_files();
                    state.selected_config_index = None;
                    state.load_dialog_open = true;
                }

                if let Some(status) = &state.save_status {
                    if status.starts_with("Error:") {
                        ui.colored_label(egui::Color32::RED, status);
                    } else {
                        ui.colored_label(egui::Color32::GREEN, status);
                    }
                }

                if let Some(status) = &state.load_status {
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

    if state.load_dialog_open {
        let mut load_dialog_open = state.load_dialog_open;
        let mut should_close = false;

        egui::Window::new("Load Configuration")
            .open(&mut load_dialog_open)
            .resizable(true)
            .show(ctx, |ui| {
                if state.available_configs.is_empty() {
                    ui.label("No saved configurations found.");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for (i, entry) in state.available_configs.iter().enumerate() {
                                let selected = state.selected_config_index == Some(i);
                                if ui.selectable_label(selected, &entry.display_name).clicked() {
                                    state.selected_config_index = Some(i);
                                }
                            }
                        });
                }

                ui.separator();
                let can_load = state.selected_config_index.is_some();
                if ui
                    .add_enabled(can_load, egui::Button::new("Load"))
                    .clicked()
                {
                    // Safe: button is disabled when selected_config_index is None
                    let idx = state.selected_config_index.unwrap();
                    let path = state.available_configs[idx].path.clone();
                    let name = state.available_configs[idx].display_name.clone();
                    match config_io::load_ui_state_from_xml(&path) {
                        Ok(loaded) => {
                            let panel_open = state.panel_open;
                            let panel_width_points = state.panel_width_points;
                            let fps = state.fps;
                            let paused = state.paused;

                            *state = loaded;

                            state.panel_open = panel_open;
                            state.panel_width_points = panel_width_points;
                            state.fps = fps;
                            state.paused = paused;
                            state.reset_requested = true;
                            state.load_status = Some(format!("Loaded: {name}"));
                            should_close = true;
                        }
                        Err(err) => {
                            state.load_status = Some(format!("Error: {err}"));
                        }
                    }
                }

                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });

        if should_close {
            load_dialog_open = false;
        }
        state.load_dialog_open = load_dialog_open;
    }

    // Report panel width for viewport calculation
    state.panel_width_points = ctx.available_rect().left();
}
