# Gameplay Loop V1

## Design target

Create a compact but connected first-playable loop for Android: explore a spherical planet, mine materials, keep the suit running, craft oxygen, and deploy equipment. This is an original game design and uses no Astroneer assets or code.

## Current source implementation

### Exploration and biomes
- Planet biome assignment is deterministic from the world seed and a quantized surface-normal cell.
- Four surface regions are currently classified: Verdant, Arid, Rocky and Polar.
- Polar regions are selected at high absolute latitude; the other regions are seed-distributed.
- Current biome output is shown in the survival HUD. Biome-specific foliage, materials, weather and resource tables are future work.

### Survival
- Oxygen starts at 100 and drains at 1.8 units per second outside a habitat.
- A nearby habitat restores oxygen at 12 units per second.
- Energy starts at 100 and passively drains at 0.12 units per second.
- A nearby solar array restores energy at 8 units per second.
- Health loses 5 units per second at zero oxygen and slowly recovers while oxygen is available.
- These are prototype balancing values, not final design values.

### Crafting
- One oxygen canister costs 2 copper and 1 iron.
- A canister restores up to 65 oxygen, with a maximum carried count of 20.
- Crafting and use are exposed as on-screen buttons.

### Deployable equipment
- Habitat: 12 regolith, 8 copper and 4 iron. Restores oxygen within an 8-unit radius.
- Solar array: 4 rock, 8 copper and 6 iron. Restores energy within a 10-unit radius.
- Deployable counts are capped to keep prototype entity counts bounded.

## Tool costs

- Mining costs 0.35 suit energy per successfully removed block.
- Placing a regolith block costs 0.20 suit energy and consumes one regolith from inventory.
- Failed actions do not consume energy or materials.

## Current controls

- Existing left virtual joystick: movement.
- Existing right-side MINE / PLACE / JUMP controls: terrain interaction and movement.
- New top-right action row: CRAFT O2, USE O2, HABITAT, SOLAR.
- Desktop keyboard terrain controls remain E (mine), Q (place), WASD (move), Space (jump).

## Persistence now present in source

A separate checksummed gameplay sidecar stores oxygen, energy, health, oxygen canisters, elapsed time, current biome, and the type/position of each deployed habitat or solar array. It attempts autosave every 10 seconds and additional saves on app suspend/exit. The file is bounded to 40 structures and validated on load. The sidecar is separate from the terrain/inventory manifest. This is not yet device-verified.

## Known gaps before calling this a first playable build

1. Compile the project and repair any Bevy API or Rust errors.
2. Run unit tests and add integration tests for survival and crafting transitions.
3. Verify the new versioned gameplay sidecar and terrain/inventory save coexist, survive process termination, and recover safely from interrupted writes.
4. Make biome classification affect terrain materials, vegetation and resource distribution.
5. Add tool energy costs and meaningful energy use.
6. Add a real inventory screen, item stacks, recipes and equipment upgrades.
7. Verify touch layout, frame pacing, memory and save recovery on Android hardware.
8. Produce and install an APK only after a successful build and validation pass.

## Validation status

This document describes source-level behavior only. The gameplay batch has not been compiled, unit-tested, packaged or run on a device. No claim of a working APK is made.
