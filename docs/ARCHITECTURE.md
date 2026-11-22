# ASTRO-RUST architecture

## World model

The initial world model uses a spherical planet centered at the origin. Positions are measured in meters. The outward surface normal at a position is the normalized center-to-position vector; gravity points in the opposite direction.

Pure planet and player math stays independent of rendering where practical. Local tangent frames provide an orthonormal basis (up, forward, right) at each surface position. The player is integrated in world space under radial gravity and is constrained by terrain collision data as that subsystem becomes available.

## Player and controls

The prototype player uses a capsule transform, world-space velocity, radial gravity, and a grounded state. Gravity accelerates toward the planet center. Jumping adds an impulse along the local outward normal. The physics timestep is capped to reduce instability after long mobile frames.

Movement input accelerates along camera-relative tangent directions. Ground friction damps horizontal velocity when no movement input is present; ground and air tangent speeds are bounded. A local heading is transported along the surface to avoid resetting orientation to a fixed world axis near the poles.

On touch devices, a fixed lower-left virtual joystick controls movement. Touches beginning on the right side rotate camera yaw and pitch. A separate lower-right touch target triggers jumping and is excluded from camera drag. Simultaneous movement, view control, and jump touches are supported. Mouse drag, WASD, and Space remain available for desktop iteration. The orbit camera uses the local surface normal as its up vector.

## Terrain data core

Terrain storage is divided into 16×16×16 voxel chunks. Integer world-voxel coordinates map to chunks with Euclidean division, so negative coordinates address the correct neighboring chunks. A seeded integer hash produces deterministic radial surface variation; shallow surface blocks are regolith and deeper blocks are rock.

`TerrainWorld` owns loaded chunks and exposes block lookup, block placement/removal, spherical digging, radial surface queries, and a simple sphere-versus-ground query. Gameplay edits mark chunks dirty for later mesh rebuild and persistence. The startup prototype preloads the eight chunks surrounding the planet origin.

The current terrain milestone is data-only: voxel chunk meshing, visible terrain replacement, asynchronous streaming, and player collision integration are still pending. The existing planet sphere remains the visible render mesh and the player still uses its spherical collision boundary until those pieces are connected.

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

A planet must never be represented as one dense in-memory voxel volume. Store terrain in bounded chunks and load only the neighborhood required by the player and current LOD. Generation and mesh building should be scheduled asynchronously with explicit per-frame budgets. Edited chunks must not be overwritten by procedural regeneration.

## Mobile performance

The renderer's High profile is a quality target, not a universal default. Runtime settings should consider GPU capabilities, available memory, thermal state where exposed, and measured frame time. Expensive features must have lower-cost fallbacks.

## Current implementation

The app contains a prototype spherical planet, capsule player, local tangent-frame math, radial gravity, jump impulse, spherical-boundary collision response, virtual touch joystick, jump touch target, touch/mouse camera orbit, desktop WASD/Space controls, and a chunked voxel terrain data core with deterministic generation and digging APIs. Terrain rendering, terrain-aware player collision, chunk streaming, saves, and Android packaging are not implemented.
