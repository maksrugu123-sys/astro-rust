# ASTRO-RUST architecture

## World model

The initial world model uses a spherical planet centered at the origin. Positions are measured in meters. The outward surface normal at a position is the normalized center-to-position vector; gravity points in the opposite direction.

Pure planet and player math stays independent of rendering where practical. Local tangent frames provide an orthonormal basis (up, forward, right) at each surface position. The player is integrated in world space under radial gravity and is constrained by the first solid voxel boundary sampled along the current radial direction. The capsule is represented by a radial surface offset for contact; this is a lightweight planetary controller, not a general capsule-versus-voxel solver.

## Player and controls

The prototype player uses a capsule transform, world-space velocity, radial gravity, and a grounded state. Gravity accelerates toward the planet center. Jumping adds an impulse along the local outward normal. The physics timestep is capped to reduce instability after long mobile frames.

Movement input accelerates along camera-relative tangent directions. Ground friction damps horizontal velocity when no movement input is present; ground and air tangent speeds are bounded. A local heading is transported along the surface to avoid resetting orientation to a fixed world axis near the poles.

On touch devices, a fixed lower-left virtual joystick controls movement. Touches beginning on the right side rotate camera yaw and pitch. A separate lower-right touch target triggers jumping and is excluded from camera drag. Simultaneous movement, view control, and jump touches are supported. Mouse drag, WASD, and Space remain available for desktop iteration. The orbit camera uses the local surface normal as its up vector.

## Terrain data and rendering

Terrain storage is divided into 16×16×16 voxel chunks. Integer world-voxel coordinates map to chunks with Euclidean division, so negative coordinates address the correct neighboring chunks. A seeded integer hash produces deterministic radial surface variation; shallow surface blocks are regolith and deeper blocks are rock. Player spawn height and ground contact sample the actual loaded voxel surface along the radial direction, so terrain edits can change the local support height. If the required terrain sample is unavailable, the controller does not substitute the old ideal sphere as an invisible collider.

`TerrainWorld` owns loaded chunks and exposes block lookup, block placement/removal, spherical digging, radial surface queries, and a simple sphere-versus-ground query. Gameplay edits mark chunks dirty. The startup prototype preloads the eight chunks surrounding the planet origin, then maintains a player-centered 3×3×3 chunk neighborhood. Generation and mesh creation are limited to two new chunks per frame, stale visuals are unloaded in small batches, and active chunks are capped at 40. A 3D DDA ray query returns the first solid voxel and its axis-aligned entry face; mining removes that voxel and placement inserts regolith into the adjacent cell after a player-distance check. Touch HUD actions provide MINE and PLACE, with E/Q keyboard bindings for desktop iteration.

Each loaded chunk is converted to a custom triangle mesh containing only faces adjacent to air or unloaded space. A greedy 16×16 face-mask pass merges adjacent coplanar faces only when their voxel materials match; per-vertex colors distinguish regolith from rock. Face basis vectors preserve winding for all six directions. The old smooth placeholder sphere has been removed; the voxel chunk meshes are now the planet's visible surface. Dirty chunks can be rebuilt after block edits without rebuilding every chunk.

The initial eight chunks and their meshes are built synchronously during startup. Subsequent streamed chunks use a bounded worker queue: worker threads generate pure CPU-side voxel arrays, while the main thread applies edits, builds render meshes, and creates entities under a per-frame budget. At most four chunk jobs are pending, and at most two are requested and applied per frame. Mesh generation remains on the main thread; distance-based LOD remains pending. Radial player contact now uses the voxel surface height, so hills, edits, and radial holes can affect support height. This approximation does not yet resolve side walls, overhangs, caves, or arbitrary capsule/voxel intersections.

## Planned subsystem boundaries

- planet_core: spherical coordinates, local tangent frames, radial gravity, planet scale.
- terrain: sparse voxel chunks, edits, mesh generation, collision data.
- worldgen: deterministic seeds, biome rules, resource distribution, cave rules.
- player: camera, movement, touch controls, radial orientation.
- renderer: materials, lighting, LOD, scalable quality profiles.
- survival: oxygen, energy, inventory, crafting.
- building: base modules, snapping, power and resource links.
- save_system: current-format persistence, independent chunk records, integrity checks, and recovery.

## Terrain constraints

A planet must never be represented as one dense in-memory voxel volume. Store terrain in bounded chunks and load only the neighborhood required by the player and current LOD. Generation and mesh building should be scheduled asynchronously with explicit per-frame budgets. Edited chunks must not be overwritten by procedural regeneration. Chunk-boundary edits must invalidate the neighboring chunk mesh when exposed faces change.

## Mobile performance

The renderer's High profile is a quality target, not a universal default. Runtime settings should consider GPU capabilities, available memory, thermal state where exposed, and measured frame time. Expensive features must have lower-cost fallbacks. Greedy meshing reduces duplicated face geometry on broad coplanar surfaces; distance-based LOD and device profiling remain future optimization targets.

## Current implementation

The app contains a prototype capsule player, local tangent-frame math, radial gravity, jump impulse, voxel-surface radial contact, virtual touch joystick, jump/mine/place touch targets, touch/mouse camera orbit, desktop WASD/Space/E/Q controls, deterministic chunked voxel terrain, voxel DDA ray selection, mining and block placement actions, exposed-face chunk mesh generation, per-chunk materials/visual entities, and dirty mesh rebuilding. Full capsule/voxel collision, asynchronous mesh building, distance-based LOD, and Android packaging are not implemented. Deterministic chunk data generation runs on bounded worker threads, and versioned terrain saves are implemented in source but not runtime-verified.

## Terrain interaction and collision limits

The current controller samples the outer solid boundary from the planet center along the player's radial direction and adds a fixed capsule support offset. This makes the support height respond to the currently loaded voxel surface and to mining edits without requiring a full physics engine. It is intentionally not treated as a complete collision solution: lateral wall contact, overhangs, tunnels that curve away from the radial line, and capsule shape contacts still require a local voxel collision solver.

Mining and placement use a 3D DDA ray from just in front of the player along the camera's view direction. The first solid voxel is selected; mining changes it to air, while placement targets the neighboring cell indicated by the exact axis-aligned entry-face normal. Existing chunk dirty flags rebuild only affected loaded chunk meshes. A 3D DDA traversal steps from voxel boundary to voxel boundary and returns the exact axis-aligned entry face without fixed-step oversampling. Modified chunk data is held in an in-memory edit cache during unload and restored after deterministic regeneration; this does not survive app process termination.

## Chunk streaming and edit retention

The runtime requests the 27 chunks in a one-chunk radius around the player's current chunk coordinate. Up to two generation jobs are requested per update, with no more than four jobs in flight; up to two completed chunks are applied and meshed per update. Stale visuals are removed in batches of four. The current active-chunk cap is 40. One shared material is used across streamed terrain entities.

When an edited chunk is unloaded, only its sparse voxel deltas are retained in the runtime edit cache. On reload, deterministic terrain generation runs first and those deltas are overlaid. Neighboring loaded chunks are marked dirty on both load and unload so shared boundaries are remeshed. The cache is still process-local, unbounded, and populated from all saved edits at startup; disk-backed lazy eviction is not implemented.


## Durable terrain persistence

The save subsystem stores the world seed, terrain parameters, inventory, and an index of edited chunk coordinates. Each edited chunk is stored as its own checksummed sparse-delta file; unmodified terrain is regenerated deterministically. The manifest references each chunk by coordinate and content checksum, and chunk filenames include that checksum so records remain immutable while a new save generation is being written.

A save writes and flushes all chunk records before atomically replacing the manifest. The manifest replacement is the commit point; obsolete chunk files are removed only afterward. The loader validates the manifest and each chunk independently. Missing or damaged chunk files are reported and skipped without rejecting unrelated valid chunks. The only supported world format is v5; earlier formats are rejected. Android uses the app-private internal files directory, while desktop development uses `./saves/`.

Autosave checks the terrain edit revision and inventory snapshot every 15 seconds and writes only when either has changed. A final save is also registered for Bevy app exit, and the `WillSuspend` lifecycle message triggers a save on the last active frame before the app is backgrounded. A forced process kill that bypasses lifecycle delivery can still lose edits made since the last successful save.

This is an intermediate persistence stage, not the finished disk cache. Startup currently reads every manifest-listed chunk delta into RAM, save creation clones all sparse deltas into one snapshot, and file I/O is synchronous in startup/autosave/suspend/exit systems. A bounded asynchronous disk-backed cache with lazy chunk reads and eviction remains required before very large edited worlds are memory-scalable. Chunk coordinates also remain signed 32-bit values; larger-world coordinate precision needs a separate 64-bit/floating-origin design.

## Asynchronous chunk generation

Procedural chunk generation is isolated from Bevy resources and can run on worker threads. A bounded queue limits in-flight jobs, prioritizes chunks by distance from the player with stable coordinate tie-breakers, and drops results that are no longer in the desired neighborhood. The main thread remains responsible for edit-cache overlay, mesh creation, and ECS entity changes. This separation avoids touching renderer assets from workers and preserves a clear ownership boundary. The current implementation uses at most four in-flight standard-library worker threads and drops completed results outside the current desired neighborhood; it does not yet cancel running jobs or recover a panicked worker's pending key. The first eight startup chunks remain synchronous for the initial scene.


## Greedy voxel meshing

For each of six face directions and each chunk slice, the mesher creates a 16×16 mask of exposed voxel faces. A greedy rectangle pass merges adjacent entries of the same material into larger quads. Face-local axes are chosen so their cross product matches the outward normal, keeping triangle winding consistent. Different materials are never merged, preserving the existing regolith/rock vertex colors. This reduces geometry for large flat regions without changing the voxel data or mining coordinates.


## Resources and inventory prototype

Underground rock generation includes deterministic copper and iron vein cells. Vein placement is derived from the world seed and integer voxel coordinates, so chunks can be generated independently without seams or runtime random-state dependencies. Mining collects the actual removed voxel material into a bounded prototype inventory (99 per material), and the HUD reports regolith, rock, copper, and iron counts.


Inventory is stored as four bounded material counters in the current save format. On load, counts are clamped to the current per-material stack cap.


Mining checks stack capacity before removing a voxel. A full material stack therefore leaves the selected block intact instead of silently discarding the resource. The same seed-derived integer hash underpins both surface variation and ore-vein placement.


Inventory is part of the save dirty-state key independently of terrain revision. This keeps later crafting, dropping, or transfer operations durable even when they do not directly edit a voxel. Terrain edits are represented as sparse deltas; the current save writer still assembles the full sparse snapshot and encoded file in memory.


## Sparse edit representation and persistence v5

A generated chunk remains a normal dense 16³ array while it is loaded for fast block access and meshing. Gameplay changes are tracked separately as sparse deltas: each changed voxel index stores its original generated value while the current chunk array stores the live value. If gameplay restores the original value, the delta is removed. On unload, only the changed index/value pairs are retained, and they are overlaid on a freshly generated baseline when the chunk is loaded again.

Save format v5 uses a checksummed world manifest plus independently checksummed chunk files. The manifest stores seed, voxel scale, planet radius, inventory, and a sorted index of chunk coordinates paired with the checksum of each chunk file. Each chunk file contains only sparse `(u16 voxel_index, voxel_kind)` edits. Filenames include the chunk checksum, making records immutable across save generations.

Saving writes and flushes all new chunk records first, then atomically replaces the manifest as the commit point. Only after that commit are unreferenced chunk files removed. This means a crash during chunk writes leaves the previous manifest and its immutable records intact. On load, a bad or missing chunk is skipped and reported without rejecting other valid chunk records. Versions 1–4 are rejected; no legacy migration code remains.

This is not yet a fully disk-backed lazy cache: startup currently reads all manifest-listed sparse deltas into `TerrainWorld`, and saving still creates a full sparse snapshot in memory. Chunk-file reads are currently synchronous during startup, and chunk writes are synchronous during autosave/suspend/exit. The next stage must add a bounded asynchronous disk cache/eviction queue so distant edits do not accumulate in RAM and file I/O does not block frame-critical work. Large-planet indexing and coordinate precision still need separate design before expanding world bounds.


## Bounded chunk I/O groundwork

A dedicated `ChunkIoQueue` now provides two named worker threads, a bounded request channel, a bounded completion channel that applies backpressure, duplicate-coordinate suppression, and bounded per-frame completion collection. Reads are capped before allocation and checked again after reading to handle files that grow during the operation. Worker panics are converted to failed read results. The queue returns raw bytes and does not bypass the v5 decoder's checksum, version, or coordinate validation.

This queue is registered but not yet connected to terrain streaming or the v5 manifest index. The current runtime therefore still loads all edited chunk records synchronously at startup and writes saves synchronously. The next integration step must make the manifest index authoritative, request records only when their chunks enter the streaming neighborhood, keep generated chunks hidden until their disk edits have been validated, and preserve untouched manifest entries during incremental saves.
