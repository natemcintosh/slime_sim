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

## Examples
<img width="1920" height="1013" alt="Screenshot From 2026-03-22 11-37-09" src="https://github.com/user-attachments/assets/5172d661-cb77-415f-9ddf-59a18702d73c" />

https://github.com/user-attachments/assets/48091bcf-8f7f-4caf-8abb-91dfaaea4a78



https://github.com/user-attachments/assets/b5109721-1b5e-41ce-84bb-8cb4d23a4a67



## Acknowledgments

Inspired by [Sebastian Lague's Slime Simulation](https://github.com/SebLague/Slime-Simulation), which is itself based on the paper:

> Jeff Jones, "Characteristics of Pattern Formation and Evolution in Approximations of *Physarum* Transport Networks," *Artificial Life*, 16(2), 127–153, 2010.

The paper describes a multi-agent, chemotaxis-based model inspired by the slime mold *Physarum polycephalum*. Simple particle-like agents sense, turn, move, and deposit chemoattractant trails, producing emergent transport networks and complex patterns through diffusion and decay — no explicit global coordination required.

## License

MIT
