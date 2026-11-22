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
- save_system: versioned persistence, migrations, recovery.

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

The runtime requests the 27 chunks in a one-chunk radius around the player's current chunk coordinate. New chunk generation and mesh creation are limited to two chunks per update, and stale visuals are removed in batches of four. The current active-chunk cap is 40. One shared material is used across streamed terrain entities.

When an edited chunk is unloaded, its complete voxel array is retained in a runtime cache. On reload, deterministic terrain generation runs first and the cached edited array is restored over it. Neighboring loaded chunks are marked dirty on both load and unload so shared boundaries are remeshed. The edit cache is process-local and currently has no durable serialization or hard capacity; it is not a substitute for versioned save files.


## Durable terrain persistence

The save subsystem stores the world seed and terrain parameters plus complete voxel arrays for player-edited chunks only. Unmodified terrain is regenerated deterministically. Saved edited chunks are loaded into a lazy edit cache and reapplied when their chunk is streamed in; loading a save therefore does not force every edited chunk to be resident or meshed.

The binary format has a magic header, explicit format version, fixed little-endian fields, a 64-bit FNV-1a integrity checksum, strict chunk-count and voxel-kind validation, and exact-length validation. Writes go to a temporary file, are flushed with `sync_all`, then renamed into place. Android resolves the app-private internal files directory from Bevy's Android application handle; desktop development uses `./saves/`.

Autosave checks the terrain edit revision every 15 seconds and writes only when edits have changed. A final save is also registered for Bevy app exit, and the `WillSuspend` lifecycle message triggers a save on the last active frame before the app is backgrounded. A forced process kill that bypasses lifecycle delivery can still lose edits made since the last successful save. The runtime edit cache is still process memory and currently has no disk-backed eviction policy.


## Asynchronous chunk generation

Procedural chunk generation is isolated from Bevy resources and can run on worker threads. A bounded queue limits in-flight jobs, prioritizes chunks by distance from the player with stable coordinate tie-breakers, and drops results that are no longer in the desired neighborhood. The main thread remains responsible for edit-cache overlay, mesh creation, and ECS entity changes. This separation avoids touching renderer assets from workers and preserves a clear ownership boundary. The current implementation uses at most four in-flight standard-library worker threads and drops completed results outside the current desired neighborhood; it does not yet cancel running jobs or recover a panicked worker's pending key. The first eight startup chunks remain synchronous for the initial scene.


## Greedy voxel meshing

For each of six face directions and each chunk slice, the mesher creates a 16×16 mask of exposed voxel faces. A greedy rectangle pass merges adjacent entries of the same material into larger quads. Face-local axes are chosen so their cross product matches the outward normal, keeping triangle winding consistent. Different materials are never merged, preserving the existing regolith/rock vertex colors. This reduces geometry for large flat regions without changing the voxel data or mining coordinates.
