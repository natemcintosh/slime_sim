use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ui::{SpawnMode, UiState};

#[derive(Serialize)]
#[serde(rename = "slime_config")]
struct ConfigFile {
    #[serde(rename = "@version")]
    version: u32,
    metadata: Metadata,
    simulation: SimulationConfig,
}

#[derive(Serialize)]
struct Metadata {
    title: String,
    notes: String,
    saved_at: String,
}

#[derive(Serialize)]
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
}

#[derive(Serialize)]
struct SpeciesConfig {
    move_speed: f32,
    turn_speed: f32,
    sensor_angle_deg: f32,
    sensor_offset: f32,
    sensor_size: i32,
    colour: ColourConfig,
}

#[derive(Serialize)]
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
}
