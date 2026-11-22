# ASTRO-RUST architecture

## World model

The initial world model uses a spherical planet centered at the origin. Positions are measured in meters. The outward surface normal at a position is the normalized center-to-position vector; gravity points in the opposite direction.

Pure planet and player math stays independent of rendering where practical. Local tangent frames provide an orthonormal basis (up, forward, right) at each surface position. Movement is projected onto this tangent plane, then the player is re-projected to the configured surface radius.

## Player and controls

The player is kinematic in the current prototype. A local heading is transported along the surface to avoid resetting orientation to a fixed world axis near the poles. Movement input is resolved against the camera-relative tangent basis.

On touch devices, a fixed lower-left virtual joystick controls movement. Touches beginning on the right side of the screen rotate camera yaw and pitch; simultaneous touches are supported so movement and view control can be used together. Mouse drag and WASD remain available for desktop iteration. The orbit camera uses the local surface normal as its up vector.

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

A planet must never be represented as one dense in-memory voxel volume. Store terrain in bounded chunks and load only the neighborhood required by the player and current LOD. Generation and mesh building should be scheduled asynchronously with explicit per-frame budgets.

## Mobile performance

The renderer's High profile is a quality target, not a universal default. Runtime settings should consider GPU capabilities, available memory, thermal state where exposed, and measured frame time. Expensive features must have lower-cost fallbacks.

## Current implementation

The app contains a prototype spherical planet, capsule player, local tangent-frame math, radial surface alignment, virtual touch joystick, touch/mouse camera orbit, and desktop WASD movement. Movement is still kinematic: physical acceleration, collision, jumping, terrain, streaming, saves, and Android packaging are not implemented.
