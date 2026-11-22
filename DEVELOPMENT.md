# Development roadmap

## Milestone 0 — Repository bootstrap
- [x] Create project identity and architecture notes.
- [x] Add a Bevy 0.20 application entry point.
- [x] Add the first spherical test planet scene.
- [x] Add pure spherical math helpers and unit tests.
- [ ] Verify the first desktop build.
- [ ] Verify Android ARM64 packaging and launch.

## Milestone 1 — Playable planetary prototype
- [x] Add a prototype capsule aligned to radial surface-up.
- [x] Add local tangent-frame math and keyboard movement for desktop iteration.
- [x] Add a virtual movement joystick and two-finger-capable touch camera drag.
- [x] Add camera orbit relative to the local spherical surface frame.
- [x] Add radial gravity, jump impulse, and spherical-boundary contact response.
- [x] Add a separate mobile jump touch target.
- [ ] Verify physical movement and touch layout on desktop and Android.
- [ ] Replace spherical collision with terrain-aware collision after voxel meshing is connected.
- [ ] Basic terrain material and horizon/sky presentation.
- [ ] Device-aware graphics settings (High / Medium / Low).

## Milestone 2 — Chunked voxel terrain
- [x] Define a 16³ chunk data structure and negative-coordinate mapping.
- [x] Add seeded spherical surface voxel generation.
- [x] Add block lookup, placement, and spherical digging operations.
- [x] Add radial surface and sphere-ground query primitives.
- [ ] Build exposed-face chunk meshes and render voxel terrain.
- [ ] Connect player grounding and movement to voxel terrain collision.
- [ ] Add deterministic terrain edits with persistence across unload/reload.
- [ ] Generate chunk meshes asynchronously.
- [ ] Add chunk cache limits and memory budgets.
- [ ] Add distance-based LOD and streaming priorities.

## Milestone 3 — Exploration systems
- [ ] Seeded biomes, resources, and points of interest.
- [ ] Oxygen and energy loops.
- [ ] Inventory and basic crafting.
- [ ] Modular base placement.

## Milestone 4 — Persistence and optimization
- [ ] Versioned save/load with corruption handling.
- [ ] Background lifecycle and interrupted-save recovery.
- [ ] Performance and memory telemetry.
- [ ] Profiling on real Android hardware before raising visual defaults.

## Current validation status

No builds, tests, APK packaging, or device profiling have been run. The terrain module includes unit tests for chunk indexing, deterministic generation, digging, and block placement, but they have not been executed. Terrain data is not yet rendered; the visible planet is still the original sphere mesh and player collision still uses the ideal spherical boundary. Android packaging is not implemented.
