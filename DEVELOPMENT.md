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
- [x] Replace ideal spherical collision with radial contact against the generated voxel surface.
- [x] Spawn the player using the sampled terrain height at the starting location.
- [ ] Basic terrain material and horizon/sky presentation.
- [ ] Device-aware graphics settings (High / Medium / Low).

## Milestone 2 — Chunked voxel terrain
- [x] Define a 16³ chunk data structure and negative-coordinate mapping.
- [x] Add seeded spherical surface voxel generation.
- [x] Add block lookup, placement, and spherical digging operations.
- [x] Add radial surface and sphere-ground query primitives.
- [x] Build exposed-face chunk meshes and render voxel terrain.
- [x] Replace the smooth placeholder planet mesh with voxel chunk visuals.
- [x] Add per-chunk dirty flags and targeted mesh rebuild support.
- [x] Connect player grounding and movement to voxel terrain collision.
- [x] Preserve deterministic terrain edits across chunk unload/reload in the runtime edit cache.
- [x] Stream a bounded 3×3×3 chunk neighborhood around the player.
- [x] Budget chunk generation/mesh creation to at most two chunks per frame.
- [x] Bound active chunk count and unload stale chunk visuals.
- [x] Persist edited chunks to a versioned, checksummed save in app-private storage (source implemented; runtime verification pending).
- [ ] Move chunk generation and mesh building to background tasks.
- [ ] Add a bounded durable edit-cache policy and device-aware memory budgets.
- [ ] Add distance-based LOD and streaming priorities.
- [x] Add voxel ray queries for mining and block placement.
- [x] Add touch HUD actions for MINE and PLACE, plus desktop E/Q controls.

## Milestone 3 — Exploration systems
- [ ] Seeded biomes, resources, and points of interest.
- [ ] Oxygen and energy loops.
- [ ] Inventory and basic crafting.
- [ ] Modular base placement.

## Milestone 4 — Persistence and optimization
- [x] Versioned save/load with checksum, strict bounds, and atomic temporary-file replacement (not runtime verified).
- [ ] Background lifecycle save on Android suspend and interrupted-save recovery.
- [ ] Performance and memory telemetry.
- [ ] Profiling on real Android hardware before raising visual defaults.

## Current validation status

No builds, tests, APK packaging, or device profiling have been run. Terrain unit tests are present but have not been executed. Chunk mesh generation, terrain-aware player contact, voxel ray queries, mining, block placement, bounded chunk streaming, and runtime edit restoration are implemented in source but remain uncompiled and unverified. Android packaging is not implemented.

## Latest gameplay batch — voxel interaction

- Player spawn height is sampled from the generated terrain along the initial radial direction.
- Ground contact now uses the first solid radial terrain boundary plus the capsule surface offset; the old ideal sphere is no longer used as the sole collision surface.
- Missing terrain samples do not silently become an invisible spherical collider.
- Added a 3D voxel DDA ray query for selecting the first solid block and its exact axis-aligned entry face.
- Mining removes the selected voxel; placement adds a regolith block in the adjacent face cell and rejects placements too close to the player.
- Added mobile MINE and PLACE touch targets. Desktop iteration uses E to mine and Q to place; the existing jump control remains separate.
- Terrain edits flow through existing dirty-chunk invalidation and mesh refresh.
- Known limits: contact is radial rather than a full capsule-vs-voxel solver; interaction and support queries only work where the relevant chunks are loaded.

## Latest streaming batch — bounded terrain neighborhood

- Added a player-centered 3×3×3 desired chunk neighborhood.
- At most two missing chunks are generated and meshed per frame; at most four stale chunk visuals are unloaded per frame.
- Active loaded terrain chunks are capped at 40 for this prototype.
- A shared terrain material is reused by all streamed chunk entities.
- Modified chunk block arrays are retained in an in-memory edit cache when unloaded and reapplied after deterministic regeneration.
- Chunk load/unload marks adjacent loaded chunk meshes dirty so exposed boundary faces are refreshed.
- Added a unit test for edit restoration after unload/reload; it has not been executed.
- This cache survives chunk unload/reload only while the app process remains alive. Durable save files, crash recovery, asynchronous workers, and a hard bound for the accumulated edit cache are still pending.


## Latest persistence batch — durable edited terrain

- Added a versioned binary world-save format containing the world seed, voxel scale, planet radius, and only player-edited chunks.
- The file includes a format magic/version, FNV-1a 64-bit integrity checksum, duplicate-coordinate and voxel-kind validation, strict size limits, and rejection of truncated or trailing data.
- Saves are written to a temporary file, flushed, then renamed over the previous save.
- Android resolves the app-private internal files directory through Bevy's Android application handle; desktop iteration uses a project-local `saves/` directory.
- Edited chunk data is loaded into the lazy runtime edit cache, so distant saved chunks do not all need to become active terrain meshes at startup.
- Autosave runs every 15 seconds when the terrain edit revision has changed; an app-exit save is also registered.
- Current limitation: Android suspend/process-kill recovery still needs a lifecycle hook, so the periodic autosave is the primary protection against abrupt termination. The in-memory edit cache can still grow with the number of edited chunks during a very long session; a disk-backed per-chunk cache is a future memory optimization.
- Save codec unit tests were added but not run. No build, test, APK, CI, or device validation was performed.
