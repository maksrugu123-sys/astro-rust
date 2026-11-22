# ASTRO-RUST architecture

## World model

The initial world model uses a spherical planet centered at the origin. Positions are measured in meters. The outward surface normal at a position is the normalized center-to-position vector; gravity points in the opposite direction.

The first math module deliberately contains only small pure functions. It must remain easy to test without starting the renderer.

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

The repository currently contains a Bevy 0.20 app entry point, a simple icosphere scene, and tested spherical math helpers. Character movement, voxel terrain, streaming, saves, and Android packaging are planned but not implemented.
