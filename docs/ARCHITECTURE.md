# ASTRO-RUST architecture

## World model

The initial world model uses a spherical planet centered at the origin. Positions are measured in meters. The outward surface normal at a position is the normalized center-to-position vector; gravity points in the opposite direction.

Pure planet and player math stays independent of rendering where practical. Local tangent frames provide an orthonormal basis (up, forward, right) at each surface position. The player is integrated in world space under radial gravity and is constrained by the current spherical collision boundary; terrain-aware contact is the next integration step.

## Player and controls

The prototype player uses a capsule transform, world-space velocity, radial gravity, and a grounded state. Gravity accelerates toward the planet center. Jumping adds an impulse along the local outward normal. The physics timestep is capped to reduce instability after long mobile frames.

Movement input accelerates along camera-relative tangent directions. Ground friction damps horizontal velocity when no movement input is present; ground and air tangent speeds are bounded. A local heading is transported along the surface to avoid resetting orientation to a fixed world axis near the poles.

On touch devices, a fixed lower-left virtual joystick controls movement. Touches beginning on the right side rotate camera yaw and pitch. A separate lower-right touch target triggers jumping and is excluded from camera drag. Simultaneous movement, view control, and jump touches are supported. Mouse drag, WASD, and Space remain available for desktop iteration. The orbit camera uses the local surface normal as its up vector.

## Terrain data and rendering

Terrain storage is divided into 16×16×16 voxel chunks. Integer world-voxel coordinates map to chunks with Euclidean division, so negative coordinates address the correct neighboring chunks. A seeded integer hash produces deterministic radial surface variation; shallow surface blocks are regolith and deeper blocks are rock.

`TerrainWorld` owns loaded chunks and exposes block lookup, block placement/removal, spherical digging, radial surface queries, and a simple sphere-versus-ground query. Gameplay edits mark chunks dirty. The startup prototype preloads the eight chunks surrounding the planet origin.

Each loaded chunk is converted to a custom triangle mesh containing only faces adjacent to air or unloaded space. Per-vertex colors distinguish regolith from rock. The old smooth placeholder sphere has been removed; the voxel chunk meshes are now the planet's visible surface. Dirty chunks can be rebuilt after block edits without rebuilding every chunk.

The current renderer builds the initial chunk meshes synchronously during startup. Asynchronous generation, distance-based LOD, bounded streaming, and terrain-aware player collision remain pending. The current collision resolver still uses the ideal spherical boundary, so visible hills and holes do not yet affect player movement.

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

The renderer's High profile is a quality target, not a universal default. Runtime settings should consider GPU capabilities, available memory, thermal state where exposed, and measured frame time. Expensive features must have lower-cost fallbacks. Chunk meshes currently duplicate vertices per exposed face to preserve flat voxel shading; greedy meshing and LOD are future optimization targets.

## Current implementation

The app contains a prototype capsule player, local tangent-frame math, radial gravity, jump impulse, spherical-boundary collision response, virtual touch joystick, jump touch target, touch/mouse camera orbit, desktop WASD/Space controls, deterministic chunked voxel terrain, spherical digging and block placement APIs, exposed-face chunk mesh generation, per-chunk materials/visual entities, and dirty mesh rebuilding. Terrain-aware player collision, asynchronous chunk streaming, LOD, saves, and Android packaging are not implemented.
