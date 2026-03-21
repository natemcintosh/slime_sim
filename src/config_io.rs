use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ui::{SpawnMode, SpeciesUi, UiState};

#[derive(Serialize, Deserialize)]
#[serde(rename = "slime_config")]
struct ConfigFile {
    #[serde(rename = "@version")]
    version: u32,
    metadata: Metadata,
    simulation: SimulationConfig,
}

#[derive(Serialize, Deserialize)]
struct Metadata {
    title: String,
    notes: String,
    saved_at: String,
}

#[derive(Serialize, Deserialize)]
struct SimulationConfig {
    trail_weight: f32,
    decay_rate: f32,
    diffuse_rate: f32,
    steps_per_frame: u32,
    spawn_mode: String,
    num_agents: u32,
    num_species: u32,
    #[serde(rename = "species")]
    species_list: Vec<SpeciesConfig>,
    #[serde(default)]
    food_weight: f32,
    #[serde(default = "default_food_num_clumps")]
    food_num_clumps: u32,
    #[serde(default = "default_food_clump_radius")]
    food_clump_radius: f32,
    #[serde(default = "default_food_viz_weight")]
    food_viz_weight: f32,
    #[serde(default = "default_true")]
    show_food: bool,
}

fn default_food_num_clumps() -> u32 {
    5
}
fn default_food_clump_radius() -> f32 {
    30.0
}
fn default_food_viz_weight() -> f32 {
    0.3
}
fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct SpeciesConfig {
    move_speed: f32,
    turn_speed: f32,
    sensor_angle_deg: f32,
    sensor_offset: f32,
    sensor_size: i32,
    colour: ColourConfig,
}

#[derive(Serialize, Deserialize)]
struct ColourConfig {
    r: f32,
    g: f32,
    b: f32,
}

pub fn save_ui_state_to_xml(ui: &UiState, title: &str, notes: &str) -> Result<PathBuf, String> {
    let trimmed_title = title.trim();
    if trimmed_title.is_empty() {
        return Err("Title is required.".to_string());
    }

    let timestamp = current_unix_seconds()?;
    let config = build_config(trimmed_title, notes, ui, timestamp);
    let mut xml = String::new();
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    config
        .serialize(serializer)
        .map_err(|e| format!("XML serialization failed: {e}"))?;

    let dir = PathBuf::from("configs");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create configs directory: {e}"))?;

    let sanitized = sanitize_title_for_filename(trimmed_title);
    let filename = format!("{sanitized}-{timestamp}.xml");
    let path = dir.join(filename);

    fs::write(&path, xml).map_err(|e| format!("Failed to write XML file: {e}"))?;
    Ok(path)
}

#[allow(clippy::cast_possible_truncation)]
fn build_config(title: &str, notes: &str, ui: &UiState, timestamp: u64) -> ConfigFile {
    let species_count = ui.num_species.min(ui.species.len() as u32) as usize;
    let species_list = ui.species[..species_count]
        .iter()
        .map(|s| SpeciesConfig {
            move_speed: s.move_speed,
            turn_speed: s.turn_speed,
            sensor_angle_deg: s.sensor_angle_deg,
            sensor_offset: s.sensor_offset,
            sensor_size: s.sensor_size,
            colour: ColourConfig {
                r: s.colour[0],
                g: s.colour[1],
                b: s.colour[2],
            },
        })
        .collect::<Vec<_>>();

    ConfigFile {
        version: 1,
        metadata: Metadata {
            title: title.to_string(),
            notes: notes.to_string(),
            saved_at: timestamp.to_string(),
        },
        simulation: SimulationConfig {
            trail_weight: ui.trail_weight,
            decay_rate: ui.decay_rate,
            diffuse_rate: ui.diffuse_rate,
            steps_per_frame: ui.steps_per_frame,
            spawn_mode: spawn_mode_to_string(ui.spawn_mode).to_string(),
            num_agents: ui.num_agents,
            num_species: species_count as u32,
            species_list,
            food_weight: ui.food_weight,
            food_num_clumps: ui.food_num_clumps,
            food_clump_radius: ui.food_clump_radius,
            food_viz_weight: ui.food_viz_weight,
            show_food: ui.show_food,
        },
    }
}

fn spawn_mode_to_string(mode: SpawnMode) -> &'static str {
    match mode {
        SpawnMode::CentreCircle => "centre_circle",
        SpawnMode::RandomFill => "random_fill",
        SpawnMode::InwardCircle => "inward_circle",
    }
}

fn current_unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| format!("Clock error: {e}"))
}

fn string_to_spawn_mode(s: &str) -> Result<SpawnMode, String> {
    match s {
        "centre_circle" => Ok(SpawnMode::CentreCircle),
        "random_fill" => Ok(SpawnMode::RandomFill),
        "inward_circle" => Ok(SpawnMode::InwardCircle),
        other => Err(format!("Unknown spawn mode: '{other}'")),
    }
}

pub struct ConfigEntry {
    pub path: PathBuf,
    pub display_name: String,
}

pub fn list_config_files() -> Vec<ConfigEntry> {
    let dir = PathBuf::from("configs");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut configs: Vec<ConfigEntry> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "xml"))
        .map(|e| {
            let path = e.path();
            let display_name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            ConfigEntry { path, display_name }
        })
        .collect();

    configs.sort_by(|a, b| b.display_name.cmp(&a.display_name));
    configs
}

#[allow(clippy::field_reassign_with_default)]
pub fn load_ui_state_from_xml(path: &Path) -> Result<UiState, String> {
    let xml = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))?;
    let config: ConfigFile =
        quick_xml::de::from_str(&xml).map_err(|e| format!("Failed to parse XML: {e}"))?;

    let sim = &config.simulation;
    let spawn_mode = string_to_spawn_mode(&sim.spawn_mode)?;

    let mut ui = UiState::default();
    ui.trail_weight = sim.trail_weight;
    ui.decay_rate = sim.decay_rate;
    ui.diffuse_rate = sim.diffuse_rate;
    ui.steps_per_frame = sim.steps_per_frame;
    ui.spawn_mode = spawn_mode;
    ui.num_agents = sim.num_agents;
    ui.num_species = sim.num_species.min(4);

    ui.food_weight = sim.food_weight;
    ui.food_num_clumps = sim.food_num_clumps;
    ui.food_clump_radius = sim.food_clump_radius;
    ui.food_viz_weight = sim.food_viz_weight;
    ui.show_food = sim.show_food;

    for (i, sc) in sim.species_list.iter().enumerate().take(4) {
        ui.species[i] = SpeciesUi {
            move_speed: sc.move_speed,
            turn_speed: sc.turn_speed,
            sensor_angle_deg: sc.sensor_angle_deg,
            sensor_offset: sc.sensor_offset,
            sensor_size: sc.sensor_size,
            colour: [sc.colour.r, sc.colour.g, sc.colour.b],
        };
    }

    Ok(ui)
}

fn sanitize_title_for_filename(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut last_was_sep = false;

    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('-');
            last_was_sep = true;
        }
    }

    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "config".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::UiState;
    use rstest::rstest;

    #[rstest]
    #[case("My Great Config!", "my-great-config")]
    #[case("___", "config")]
    #[case("  alpha---beta  ", "alpha-beta")]
    #[case("Version 2.1", "version-2-1")]
    #[case("___A___B___", "a-b")]
    fn sanitize_title_generates_stable_filename_part(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(sanitize_title_for_filename(input), expected);
    }

    #[test]
    fn save_rejects_empty_title() {
        let ui = UiState::default();
        let result = save_ui_state_to_xml(&ui, "   ", "notes");
        assert!(result.is_err());
    }

    #[test]
    fn serialized_config_uses_active_species_count() {
        let mut ui = UiState::default();
        ui.num_species = 2;
        ui.species[0].colour = [0.9, 0.1, 0.2];
        ui.species[1].colour = [0.2, 0.8, 0.4];

        let cfg = build_config("title", "notes", &ui, 42);

        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.metadata.title, "title");
        assert_eq!(cfg.metadata.notes, "notes");
        assert_eq!(cfg.metadata.saved_at, "42");
        assert_eq!(cfg.simulation.num_species, 2);
        assert_eq!(cfg.simulation.species_list.len(), 2);
        assert_eq!(cfg.simulation.species_list[0].colour.r, 0.9);
        assert_eq!(cfg.simulation.species_list[1].colour.g, 0.8);
    }

    #[test]
    fn build_config_clamps_species_count_to_available_slots() {
        let mut ui = UiState::default();
        ui.num_species = 99;

        let cfg = build_config("title", "notes", &ui, 99);

        assert_eq!(cfg.simulation.num_species, 4);
        assert_eq!(cfg.simulation.species_list.len(), 4);
    }

    #[rstest]
    #[case(SpawnMode::CentreCircle, "centre_circle")]
    #[case(SpawnMode::RandomFill, "random_fill")]
    #[case(SpawnMode::InwardCircle, "inward_circle")]
    fn spawn_mode_to_string_maps_all_variants(#[case] mode: SpawnMode, #[case] expected: &str) {
        assert_eq!(spawn_mode_to_string(mode), expected);
    }

    #[rstest]
    #[case("centre_circle", SpawnMode::CentreCircle)]
    #[case("random_fill", SpawnMode::RandomFill)]
    #[case("inward_circle", SpawnMode::InwardCircle)]
    fn string_to_spawn_mode_maps_all_variants(#[case] input: &str, #[case] expected: SpawnMode) {
        assert_eq!(string_to_spawn_mode(input).unwrap(), expected);
    }

    #[test]
    fn string_to_spawn_mode_rejects_unknown() {
        assert!(string_to_spawn_mode("unknown").is_err());
    }

    #[test]
    fn round_trip_save_then_load_preserves_simulation_params() {
        let mut ui = UiState::default();
        ui.num_species = 2;
        ui.trail_weight = 7.5;
        ui.spawn_mode = SpawnMode::InwardCircle;
        ui.species[0].move_speed = 200.0;
        ui.species[1].colour = [0.5, 0.6, 0.7];

        let path = save_ui_state_to_xml(&ui, "round-trip-test", "").unwrap();
        let loaded = load_ui_state_from_xml(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(loaded.num_species, 2);
        assert!((loaded.trail_weight - 7.5).abs() < 0.01);
        assert_eq!(loaded.spawn_mode, SpawnMode::InwardCircle);
        assert!((loaded.species[0].move_speed - 200.0).abs() < 0.01);
        assert!((loaded.species[1].colour[1] - 0.6).abs() < 0.01);
    }

    #[test]
    fn round_trip_preserves_food_params() {
        let mut ui = UiState::default();
        ui.food_weight = 0.07;
        ui.food_num_clumps = 12;
        ui.food_clump_radius = 45.5;
        ui.food_viz_weight = 0.8;
        ui.show_food = false;

        let path = save_ui_state_to_xml(&ui, "food-round-trip", "").unwrap();
        let loaded = load_ui_state_from_xml(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert!((loaded.food_weight - 0.07).abs() < 0.01);
        assert_eq!(loaded.food_num_clumps, 12);
        assert!((loaded.food_clump_radius - 45.5).abs() < 0.1);
        assert!((loaded.food_viz_weight - 0.8).abs() < 0.01);
        assert!(!loaded.show_food);
    }

    #[test]
    fn load_old_config_without_food_fields_uses_defaults() {
        let dir = PathBuf::from("configs");
        fs::create_dir_all(&dir).ok();
        let path = dir.join("__test_no_food.xml");
        // XML config from before food support was added
        fs::write(
            &path,
            r#"<slime_config version="1">
  <metadata>
    <title>old config</title>
    <notes/>
    <saved_at>0</saved_at>
  </metadata>
  <simulation>
    <trail_weight>5</trail_weight>
    <decay_rate>0.3</decay_rate>
    <diffuse_rate>3</diffuse_rate>
    <steps_per_frame>1</steps_per_frame>
    <spawn_mode>centre_circle</spawn_mode>
    <num_agents>1000</num_agents>
    <num_species>1</num_species>
    <species>
      <move_speed>100</move_speed>
      <turn_speed>2</turn_speed>
      <sensor_angle_deg>30</sensor_angle_deg>
      <sensor_offset>35</sensor_offset>
      <sensor_size>1</sensor_size>
      <colour><r>1</r><g>1</g><b>1</b></colour>
    </species>
  </simulation>
</slime_config>"#,
        )
        .unwrap();

        let loaded = load_ui_state_from_xml(&path).unwrap();
        fs::remove_file(&path).ok();

        // Food fields should be their defaults
        assert!((loaded.food_weight - 0.0).abs() < 1e-6);
        assert_eq!(loaded.food_num_clumps, 5);
        assert!((loaded.food_clump_radius - 30.0).abs() < 0.1);
        assert!((loaded.food_viz_weight - 0.3).abs() < 0.01);
        assert!(loaded.show_food);
    }

    #[test]
    fn load_rejects_malformed_xml() {
        let dir = PathBuf::from("configs");
        fs::create_dir_all(&dir).ok();
        let path = dir.join("__test_malformed.xml");
        fs::write(&path, "<garbage>not valid</garbage>").ok();
        let result = load_ui_state_from_xml(&path);
        fs::remove_file(&path).ok();
        assert!(result.is_err());
    }

    #[test]
    fn list_config_files_does_not_panic() {
        let _ = list_config_files();
    }
}
