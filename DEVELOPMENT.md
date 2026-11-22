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
- [x] Move deterministic voxel chunk generation to bounded background worker tasks (source implemented; runtime verification pending).
- [ ] Move mesh building to background tasks with safe asset handoff.
- [ ] Add cancellation tokens and worker-failure recovery for interrupted generation jobs.
- [ ] Replace per-job OS thread creation with Bevy's shared compute pool after profiling thread scheduling on Android.
- [ ] Add a bounded durable edit-cache policy and device-aware memory budgets.
- [x] Add stable distance-based chunk request priorities.
- [x] Greedily merge coplanar faces of the same voxel material to reduce chunk mesh geometry (source implemented; visual verification pending).
- [ ] Add distance-based LOD for far terrain.
- [x] Add voxel ray queries for mining and block placement.
- [x] Add touch HUD actions for MINE and PLACE, plus desktop E/Q controls.

## Milestone 3 — Exploration systems
- [ ] Seeded biomes and points of interest.
- [x] Deterministic copper and iron ore veins in underground rock (source implemented; runtime distribution pending).
- [ ] Oxygen and energy loops.
- [x] Basic material inventory with mining collection and on-screen counts (source implemented; UI verification pending).
- [x] Persist inventory counts in save format v3 while retaining v1/v2 terrain-save decoding.
- [ ] Inventory capacity, item stacks, and basic crafting.
- [ ] Modular base placement.

## Milestone 4 — Persistence and optimization
- [x] Versioned save/load with checksum, strict bounds, and atomic temporary-file replacement (not runtime verified).
- [x] Save on the Bevy `WillSuspend` lifecycle message (source implemented; device verification pending).
- [ ] Interrupted-save recovery after abrupt process termination.
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
- A save is also attempted on Bevy's `WillSuspend` lifecycle message, giving the app one final active frame to flush edits before background suspension. A forced process kill without a lifecycle callback can still lose edits made since the last successful save. The in-memory edit cache can still grow with the number of edited chunks during a very long session; a disk-backed per-chunk cache is a future memory optimization.
- Save codec unit tests were added but not run. No build, test, APK, CI, or device validation was performed.


## Latest asynchronous streaming batch

- Extracted deterministic voxel generation into a pure worker-safe function.
- Added a bounded generation queue with at most four pending chunk jobs and at most two new requests per frame.
- Worker threads generate only CPU-side voxel arrays; they never access Bevy ECS or render assets.
- The main thread collects completed results, reapplies edited chunk data, builds meshes, and creates render entities with a two-chunk application budget per frame.
- Chunk requests are ordered by distance with coordinate tie-breakers for deterministic priority.
- Results that are no longer in the player's desired neighborhood are discarded instead of entering the active world.
- The active loaded-chunk cap remains 40; stale visuals continue to unload in bounded batches.
- Worker generation and queue integration have not been compiled or runtime-tested. Mesh generation remains on the main thread, and startup's initial eight chunks are still generated synchronously.


## Latest terrain mesh optimization

- Replaced per-voxel exposed-face emission with a 16×16 greedy face-mask pass.
- Adjacent coplanar faces merge only when they share the same voxel material, preserving regolith/rock color boundaries.
- Face bases are selected so triangle winding follows each face normal for all six directions.
- Added focused tests for merging uniform regions and keeping different materials separate; tests have not been run.
- This optimization reduces mesh vertex/index count for broad flat surfaces, but actual savings and GPU impact still require a real build and device profiling.


## Latest exploration gameplay batch — ore and inventory

- Added deterministic copper and iron ore clusters to deeper rock using seed-derived 4×4×4 vein cells.
- Added distinct ore colors to voxel meshing; greedy meshing keeps unlike materials as separate faces.
- Mining now collects the selected block material into a small inventory, capped at 99 per material.
- Added a compact HUD counter for regolith, rock, copper, and iron.
- Bumped the save format to version 2 while keeping the decoder compatible with version 1 files; new voxel IDs are validated and restored.
- No biome-specific surface generation, crafting, inventory persistence beyond the existing terrain edits, or runtime balancing is included yet.
- Tests/builds/device checks remain unrun.


### Inventory persistence follow-up

- Save format v3 now stores regolith, rock, copper, and iron counts with the world snapshot.
- Decoder compatibility remains for v1 terrain saves and v2 ore-aware terrain saves; those older formats restore an empty inventory.
- Inventory counts are clamped to the prototype stack cap of 99 when loading.
- Save round-trip coverage now includes inventory counters; the test has not been executed.


### Resource collection safety

- Mining now refuses to destroy a block when its corresponding inventory stack is already at 99, preventing silent resource loss.
- Surface noise and ore-vein placement share one deterministic integer hash implementation.
- Added source-level coverage for version-1 save migration and inventory stack capacity; neither test has been run.
