# Development roadmap

## Milestone 0 — Repository bootstrap
- [x] Create project identity and architecture notes.
- [x] Add a Bevy 0.20 application entry point.
- [x] Add the first spherical test planet scene.
- [x] Add pure spherical math helpers and unit tests.
- [ ] Verify the first desktop build.
- [ ] Verify Android ARM64 packaging and launch.

## Milestone 1 — Playable planetary prototype
- [ ] Orbit camera and Android touch controls.
- [x] Add a prototype capsule aligned to radial surface-up.
- [x] Add local tangent-frame math and keyboard movement for desktop iteration.
- [ ] Add physical gravity, jumping, and collision handling.
- [ ] Basic terrain material and horizon/sky presentation.
- [ ] Device-aware graphics settings (High / Medium / Low).

## Milestone 2 — Chunked voxel terrain
- [ ] Define a versioned voxel/chunk data format.
- [ ] Generate chunk meshes asynchronously.
- [ ] Add deterministic terrain edits (dig/place).
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
No builds, tests, APK packaging, or device profiling have been run. Source changes are committed, but compilation and runtime behavior remain unverified. Checklist items are not complete until verified.
