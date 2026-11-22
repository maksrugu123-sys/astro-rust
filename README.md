# ASTRO-RUST

An original mobile-first 3D planetary exploration and survival game built with Rust.

> Inspired by the planetary exploration and creative-survival genre. ASTRO-RUST is an independent project and does not use Astroneer names, assets, code, or branding.

## Project direction

- **Platform:** Android first; desktop development support where practical.
- **Language:** Rust.
- **Engine:** Bevy.
- **World:** Procedurally generated spherical planets with biome variation.
- **Terrain:** Chunked voxel terrain designed for digging and deformation.
- **Rendering target:** High-quality visual direction with scalable quality settings and device-aware fallbacks.
- **Performance:** Asynchronous world streaming, level of detail, bounded memory use, and adaptive rendering settings.

## Planned systems

| System | Responsibility |
| --- | --- |
| planet_core | Planet coordinates, spherical surface math, gravity frames |
| terrain | Voxel chunks, meshing, terrain edits |
| worldgen | Deterministic seeds, biomes, resource placement |
| player | Character controller, camera, touch input |
| renderer | Materials, lighting, shadows, quality profiles |
| survival | Oxygen, energy, inventory |
| building | Modular bases and deployable equipment |
| save_system | Versioned world and player persistence |

The initial milestone is a small, measurable Android-capable prototype—not a complete game. The first prototype will validate the engine startup path, input, camera, and a simple spherical test world before large-scale voxel features are added.

## Development status

This repository is at project bootstrap. No Android build or device performance test has been run yet.

See [DEVELOPMENT.md](DEVELOPMENT.md) for milestones and [AGENTS.md](AGENTS.md) for contributor/agent rules.

## Build status

Builds are intentionally not run during the initial repository setup. Android toolchain and device compatibility must be verified in a later, explicitly requested build/test pass.
