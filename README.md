# slime_sim

GPU-accelerated slime mold simulation using [wgpu](https://wgpu.rs/). Thousands of agents follow simple rules — sense, turn, move, deposit — producing emergent, organic patterns on screen.

<!-- ![screenshot](screenshot.png) -->

## Install

```sh
cargo install slime_sim
```

Or from source:

```sh
cargo install --git https://github.com/natemcintosh/slime_sim
```

## Controls

All parameters are adjustable in real-time via the egui side panel:

- **Trail Weight / Decay Rate / Diffuse Rate** — control pheromone intensity, fading, and spreading
- **Steps / Frame** — computational steps per rendered frame (1–10)
- **Species (1–4)** — each with independent move speed, turn speed, sensor angle/distance/size, and color
- **Spawn Mode** — centre circle, random fill, or inward circle
- **Agent Count** — 1,000 to 500,000 (logarithmic slider)
- **Reset Simulation** — reinitialize with current settings

## Requirements

Requires a GPU with Vulkan, Metal, or DX12 support. Rust 1.85+ (edition 2024).

## License

MIT
