# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Commands

```bash
just fmt          # Format code with cargo fmt
just lint         # Lint with Clippy (pedantic)
just test         # Run tests (cargo nextest if available, else cargo test)
just run          # Run release build

cargo test <test_name>          # Run a single test
cargo build                     # Debug build
cargo build --release           # Optimized build
cargo install --path .          # Install locally
```

## Architecture

A GPU-accelerated slime mold simulation. Thousands of agents sense, turn, move, and deposit pheromones, producing emergent organic patterns. Up to 4 independent species with unique parameters and colors, all adjustable in real-time via an egui side panel.

### Frame Loop (`main.rs`)

Each frame:
1. winit event loop handles input
2. egui builds UI
3. Simulation runs 1–10 compute steps (configurable)
4. Render: blit color texture to screen
5. egui rendered on top, frame presented
6. Competing mode only: per-species population counts read back from the GPU (a blocking wait)

### GPU Pipeline (`simulation.rs` — core logic)

GPU stages, in order. `food_regen`, `update` and `diffuse` run once per step (`step()`); `colour` and `blit` run once per frame (`render()`):
- **`food_regen`** — competing mode only: grows food back into `food_buffer`
- **`update`** — agents sense pheromone channels, steer, move, deposit trail
- **`diffuse`** — 3×3 box blur + exponential decay on trail textures
- **`colour`** — maps per-species trail intensities to RGB, over a faint food layer
- **`blit`** — fullscreen triangle renders color texture to screen

**Ping-pong buffering:** Two trail textures (RGBA16Float, one channel per species) swap read/write roles each step to avoid read-after-write conflicts.

### Food and Competing Mode

Food is a per-pixel value in 0..1. Each sensor sample adds `food * food_weight` to the agent's score, the same for every species. There are two modes, chosen by `UiState::competing_mode` (toggling it resets the sim):

- **Classic (default):** a static map. `food.rs` builds it on the CPU from Gaussian clumps (`food_num_clumps`, `food_clump_radius`, `food_seed`), and `upload_classic_food_map` in `main.rs` writes it to `food_texture` on startup, resize, reset and "Regenerate Food". At the default Food Weight of 0 it has no effect and is not drawn.
- **Competing:** turning it on calls `UiState::apply_competing_defaults` (at least 2 species, non-zero Food Weight, Territories spawn). Agents carry `energy`. Moving and depositing trail cost energy; eating from `food_buffer` (a read-write storage buffer, zeroed on reset) gives energy. An agent with energy ≤ 0 is dead and skipped. An agent above `reproduction_threshold` gives half its energy to a dead slot at `(idx + 7919) % num_agents`, so the agent buffer is a fixed-size pool and the population never exceeds `num_agents`. Only about 10% of the pool starts alive (`COMPETING_START_ALIVE` in `simulation.rs`), which leaves room to grow. `food_regen.wgsl` grows food back at clumps that move every `food_clump_lifetime` seconds (it crossfades between the old and new positions). `colour.wgsl` draws food straight from `food_buffer` in this mode (and from `food_texture` in classic mode). `update.wgsl` counts live agents per species into `population_buffer` (cleared each step) for the panel, which keeps a history of the counts (`UiState::record_population`) and draws it with `egui_plot`. Food is drawn bright green in this mode (`food_colour` in `simulation.rs`).

### Key Data Structures

- `Agent` — position (vec2), angle (f32), species_index (u32), energy (f32, competing mode)
- `SimParams` — texture dimensions, agent count, trail physics, delta_time, food and competing-mode settings, `frame` (RNG seed; counts frames, reset to 0 on reset)
- `SpeciesSettings` — per-species movement, sensor config, RGBA color
- `UiState` (`ui.rs`) — all runtime-adjustable parameters
- `Simulation` — owns all GPU buffers, textures, pipelines, and bind groups

### Shaders (`src/shaders/`, WGSL)

| File | Purpose |
|------|---------|
| `common.wgsl` | Shared declarations: `SimParams`, `SpeciesSettings`, `hash`, `scale_to_01` |
| `update.wgsl` | Agent behavior: sense with per-species attraction/repulsion and food, stochastic turning, movement; in competing mode also energy, eating and reproduction |
| `diffuse.wgsl` | Trail processing: 3×3 blur + decay per channel |
| `colour.wgsl` | Visualization: blend species colors by trail channel intensities over the food map |
| `food_regen.wgsl` | Competing mode: regrow food at moving clump positions |
| `blit.wgsl` | Fullscreen triangle to screen |

`common.wgsl` is prepended (`concat!` in `simulation.rs`) to every compute shader, but not to `blit.wgsl`. Its `SimParams` and `SpeciesSettings` must match the Rust structs. Shader error line numbers count from the start of `common.wgsl`, so subtract its line count to get the line in the shader's own file.

### Other Modules

- `gpu.rs` — wgpu device/queue/surface setup; selects high-performance adapter (Vulkan/Metal/DX12)
- `food.rs` — CPU generation of the classic-mode food map (Gaussian clumps, normalised to 0..1)
- `config_io.rs` — serializes/deserializes `UiState` to/from XML (quick-xml + serde), saves to and loads from `configs/`

## Testing

Tests live in `src/simulation.rs`, `src/config_io.rs`, `src/food.rs` and `src/ui.rs`. GPU smoke tests exist but are `#[ignore]`d by default (require a GPU). Use `cargo test -- --ignored` to run them explicitly.
