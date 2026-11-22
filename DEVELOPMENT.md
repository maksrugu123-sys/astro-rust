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
- [x] Persist inventory counts and sparse terrain deltas in the current v4 save format; older save formats are intentionally unsupported.
- [ ] Inventory capacity, item stacks, and basic crafting.
- [ ] Modular base placement.

## Milestone 4 — Persistence and optimization
- [x] Versioned save/load with checksum, strict bounds, and atomic temporary-file replacement (not runtime verified).
- [x] Save on the Bevy `WillSuspend` lifecycle message (source implemented; device verification pending).
- [ ] Interrupted-save recovery after abrupt process termination.
- [ ] Performance and memory telemetry.
- [ ] Profiling on real Android hardware before raising visual defaults.

## Current validation status

No builds, tests, APK packaging, or device profiling have been run. Terrain, greedy-meshing, worker-generation, inventory, sparse-save, and old-version rejection tests are present but have not been executed. Chunk mesh generation, terrain-aware player contact, voxel ray queries, mining, block placement, bounded asynchronous chunk generation, ore collection, inventory persistence, and world save/load are implemented in source but remain uncompiled and unverified. Android packaging is not implemented.

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
- At most two new worker jobs are requested and two completed chunks are applied per frame; at most four stale chunk visuals are unloaded per frame.
- Active loaded terrain chunks are capped at 40 for this prototype.
- A shared terrain material is reused by all streamed chunk entities.
- Modified chunk block arrays are retained in an in-memory edit cache when unloaded and reapplied after deterministic regeneration.
- Chunk load/unload marks adjacent loaded chunk meshes dirty so exposed boundary faces are refreshed.
- Added a unit test for edit restoration after unload/reload; it has not been executed.
- The runtime edit cache survives chunk unload/reload while the process is alive; versioned world saves now persist edited chunks across launches. A hard bound for the in-memory edit cache and abrupt-kill recovery beyond the last autosave remain pending.


## Latest persistence batch — durable edited terrain

- Added a versioned binary world-save format containing the world seed, voxel scale, planet radius, and only player-edited chunks.
- The file includes a format magic/version, FNV-1a 64-bit integrity checksum, duplicate-coordinate and voxel-kind validation, strict size limits, and rejection of truncated or trailing data.
- Saves are written to a temporary file, flushed, then renamed over the previous save.
- Android resolves the app-private internal files directory through Bevy's Android application handle; desktop iteration uses a project-local `saves/` directory.
- Edited chunk data is loaded into the lazy runtime edit cache, so distant saved chunks do not all need to become active terrain meshes at startup.
- Autosave runs every 15 seconds when either the terrain edit revision or the four-slot inventory snapshot has changed; suspend and app-exit saves use the same combined dirty check.
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


### Persistence and coordinate hardening follow-up

- Save dirty-state tracking now includes inventory contents as well as terrain edits, so future inventory-only operations (such as crafting or transfers) cannot be missed by autosave, suspend, or exit persistence.
- Inventory snapshots are centralized in the inventory resource and covered by a focused source-level test.
- Chunk-neighbor invalidation and DDA stepping use saturating coordinate arithmetic at integer limits.
- Chunk coordinates restored from saves are restricted to ranges that can be represented by signed 32-bit voxel coordinates; invalid external chunk coordinates are ignored by the runtime loader.
- Added boundary-focused tests. They have not been run; no build, APK, CI, or device validation was performed.


### Placement and streaming ownership fix

- Block placement now requires the target voxel to be present and empty in a loaded chunk. An unloaded target is no longer mistaken for air, so the input handler cannot synchronously generate a chunk that bypasses the streaming system's mesh/entity registration.


### Streaming and save-boundary hardening

- The streaming neighborhood now filters chunk coordinates outside the representable signed 32-bit voxel range before queuing worker generation. This prevents edge-neighborhood jobs from overflowing when building voxel coordinates.
- Save size metadata is compared as u64 before conversion to usize.
- The edited-chunk count is checked against the save cap before full chunk arrays are cloned into the save snapshot, avoiding a large unnecessary allocation when a world already exceeds the supported save limit.
- Save encoding also rejects invalid chunk coordinates and inventory stacks above the supported 99-item cap.


### Worker failure recovery

- Chunk-generation threads now use named fallible thread creation instead of an infallible spawn call.
- A worker panic is caught at the generation boundary; failed jobs release their pending coordinate so the streaming scheduler can retry rather than permanently stalling that chunk.
- These paths are source-level hardening only and have not been exercised by runtime tests.


## Sparse terrain edits and independent chunk persistence v5

- Loaded chunks now retain only the original values for voxel indices changed by gameplay; reverting a voxel to its generated value removes that delta.
- Unloaded edited chunks are retained as sparse `(u16 voxel_index, voxel_kind)` records instead of complete 4096-voxel arrays.
- Save format v5 is the only supported world format. A checksummed manifest stores world metadata, inventory, chunk coordinates, and each chunk file's checksum. Every edited chunk is stored in its own compact file containing only changed voxel indices and values.
- Chunk filenames include their content checksum. New chunk files are written and flushed first; the manifest is atomically replaced last as the save commit point. Old/orphaned chunk files are cleaned only after the manifest commit, so an interrupted save leaves the previous manifest's immutable chunk files available.
- The loader validates the manifest and each chunk independently. A missing, oversized, malformed, coordinate-mismatched, or checksum-invalid chunk is skipped with a warning; other valid chunks still load.
- Versions 1–4 are explicitly rejected. There is no legacy decoder or migration path.
- This removes the single giant world-save file and whole-world checksum failure domain. However, startup still reads all indexed chunk deltas into the in-memory edit cache, and saving still snapshots all deltas before writing. A bounded disk-backed lazy cache, asynchronous scheduled disk I/O, and 64-bit world indexing remain future work.
- Source changes and new persistence tests are uncompiled and untested; no build, test, APK, CI, or device profiling was run.


### Bounded asynchronous chunk-read foundation

- Added `src/chunk_io.rs`, a bounded worker queue for reading individual chunk files away from the frame thread. It uses two named workers, a 16-request bounded queue, a 16-completion backpressure queue, per-file size limits, duplicate-coordinate suppression, and a per-frame completion limit at the API boundary.
- Registered the queue as an application resource. The worker returns raw bytes only; the save subsystem must still validate the chunk checksum, format version, and coordinate before applying any edit.
- This is infrastructure only: terrain streaming does not yet submit requests to this queue, and startup still eagerly decodes every indexed chunk. No claim of lazy loading, cache eviction, or non-blocking autosave is made.
- Added source-level tests for empty queue state and missing-file handling. Tests, formatting, and compilation have not been run.


### Asynchronous chunk writes

The bounded `ChunkIoQueue` now accepts immutable chunk-file write jobs as well as reads. It uses the same two workers and bounded request/completion channels. A write creates the parent directory, writes to a temporary path, flushes the file with `sync_all`, and renames it to its immutable final path. Completion is reported only after the rename succeeds. Queue saturation and duplicate in-flight writes are rejected without blocking the game thread; callers must retain dirty data and retry.

This is still a low-level I/O primitive, not an active autosave pipeline. The current `SaveStore` still performs synchronous chunk writes and manifest commits, and startup still decodes all chunk payloads. No claim of disk-backed lazy loading or bounded edit-cache memory is made. The new write-path test has been added but not executed.
