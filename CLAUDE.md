# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Commands

```bash
just fmt          # Format code with cargo fmt
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

### GPU Pipeline (`simulation.rs` — core logic)

Four GPU stages per simulation step:
- **`update`** — agents sense pheromone channels, steer, move, deposit trail
- **`diffuse`** — 3×3 box blur + exponential decay on trail textures
- **`colour`** — maps per-species trail intensities to RGB
- **`blit`** — fullscreen triangle renders color texture to screen

**Ping-pong buffering:** Two trail textures (RGBA16Float, one channel per species) swap read/write roles each step to avoid read-after-write conflicts.

### Key Data Structures

- `Agent` — position (vec2), angle (f32), species_index (u32)
- `SimParams` — texture dimensions, agent count, trail physics, delta_time
- `SpeciesSettings` — per-species movement, sensor config, RGBA color
- `UiState` (`ui.rs`) — all runtime-adjustable parameters
- `Simulation` — owns all GPU buffers, textures, pipelines, and bind groups

### Shaders (`src/shaders/`, WGSL)

| File | Purpose |
|------|---------|
| `update.wgsl` | Agent behavior: sense with per-species attraction/repulsion, stochastic turning, movement |
| `diffuse.wgsl` | Trail processing: 3×3 blur + decay per channel |
| `colour.wgsl` | Visualization: blend species colors by trail channel intensities |
| `blit.wgsl` | Fullscreen triangle to screen |

### Other Modules

- `gpu.rs` — wgpu device/queue/surface setup; selects high-performance adapter (Vulkan/Metal/DX12)
- `config_io.rs` — serializes/deserializes `UiState` to/from XML (quick-xml + serde), saves to and loads from `configs/`

## Testing

Tests live in `src/simulation.rs` and `src/config_io.rs`. GPU smoke tests exist but are `#[ignore]`d by default (require a GPU). Use `cargo test -- --ignored` to run them explicitly.
