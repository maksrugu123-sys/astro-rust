# ASTRO-RUST

An original mobile-first 3D planetary exploration, crafting and survival game built with Rust and Bevy.

> An independent planetary-exploration game. ASTRO-RUST does not use Astroneer names, assets, code, or branding.

## Project direction

- **Platform:** Android first; desktop development support where practical.
- **Language / engine:** Rust + Bevy.
- **World:** Procedurally generated spherical planets with deterministic surface biomes.
- **Terrain:** Streamed voxel chunks, mining, block placement and sparse persistent terrain edits.
- **Gameplay:** Material collection, oxygen and energy management, crafting, deployable habitats and solar arrays.
- **Rendering:** High-quality visual direction with scalable quality settings and device-aware fallbacks.
- **Performance:** Bounded chunk streaming, background generation and bounded save I/O.

## Current gameplay loop

The current development branch contains source implementations for:
- spherical radial-gravity movement, camera orbit, jump and touch controls;
- procedural spherical voxel terrain, chunk streaming, mining and block placement;
- copper and iron ore veins plus a four-material inventory;
- seeded surface-biome classification (Verdant, Arid, Rocky and Polar);
- oxygen, energy and health survival meters;
- oxygen-canister crafting and use;
- deployable habitats that replenish oxygen nearby;
- solar arrays that recharge suit energy nearby;
- versioned, checksummed sparse terrain and material-inventory persistence; a separate checksummed gameplay sidecar now saves survival meters, oxygen canisters, elapsed time, biome and deployed equipment positions.

Terrain tools now consume suit energy (mining 0.35, placement 0.20) and block placement consumes one regolith. The gameplay additions are source-level work only. They have **not** yet been compiled, tested, packaged as an APK, or verified on a phone. The gameplay sidecar is implemented in source but has not been verified by a build or device run.

## Repository map

| Area | Responsibility |
| --- | --- |
| `planet` | Spherical planet math and scene |
| `terrain` | Voxel chunks, generation, meshing and edits |
| `player` | Radial movement, camera and touch controls |
| `gameplay.rs` | Biomes, survival, crafting and deployable structures |
| `save_system.rs` | Sparse world and inventory persistence |
| `chunk_io.rs` | Bounded asynchronous chunk file I/O |

See [DEVELOPMENT.md](DEVELOPMENT.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and [docs/GAMEPLAY_LOOP_V1.md](docs/GAMEPLAY_LOOP_V1.md).

## Validation

No build, unit-test run, APK packaging, CI run, or Android device profiling has been performed for the latest gameplay batch. Do not treat source implementation as a verified playable release.
