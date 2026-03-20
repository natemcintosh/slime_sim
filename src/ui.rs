use egui::Context;

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
                    colour: [0.2, 0.5, 1.0],
                },
                SpeciesUi {
                    move_speed: 100.0,
                    turn_speed: 2.0,
                    sensor_angle_deg: 30.0,
                    sensor_offset: 35.0,
                    sensor_size: 1,
                    colour: [1.0, 0.3, 0.2],
                },
                SpeciesUi {
                    move_speed: 100.0,
                    turn_speed: 2.0,
                    sensor_angle_deg: 30.0,
                    sensor_offset: 35.0,
                    sensor_size: 1,
                    colour: [0.2, 1.0, 0.3],
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
        }
    }
}

pub fn draw_ui(ctx: &Context, state: &mut UiState) {
    egui::SidePanel::left("controls").show(ctx, |ui| {
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
        ui.add(
            egui::Slider::new(&mut state.steps_per_frame, 1..=10).text("Steps / Frame"),
        );
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
                egui::Slider::new(&mut s.sensor_offset, 5.0..=100.0).text("Sensor Distance"),
            );
            ui.add(egui::Slider::new(&mut s.sensor_size, 1..=5).text("Sensor Size"));
            ui.color_edit_button_rgb(&mut s.colour);
        }

        ui.separator();
        ui.label("Spawn Mode");
        ui.radio_value(&mut state.spawn_mode, SpawnMode::CentreCircle, "Centre Circle");
        ui.radio_value(&mut state.spawn_mode, SpawnMode::RandomFill, "Random Fill");
        ui.radio_value(&mut state.spawn_mode, SpawnMode::InwardCircle, "Inward Circle");

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
    });
}
