# Agent and contributor rules

## Project identity
- Product name: ASTRO-RUST.
- Build an original planetary exploration/survival game. Do not copy Astroneer branding, assets, code, UI, or proprietary content.
- Primary target: Android ARM64. Desktop builds are useful for fast iteration.

## Repository safety
- Never push directly to main after the bootstrap commit unless the user explicitly approves it.
- Do development on a focused branch and keep changes reviewable.
- Do not start builds, APK generation, CI workflows, or device tests unless the user asks.
- Never claim code was compiled, tested, installed, or profiled unless that action actually happened.
- Report completed work, unverified assumptions, and blockers separately.

## Engineering priorities
1. Correctness and a small working prototype.
2. Mobile stability and predictable memory use.
3. Scalable quality settings: High is the visual target, not a promise that every phone can sustain it.
4. Asynchronous chunk streaming and bounded work per frame.
5. Tests for deterministic math and world-generation logic.

## Architecture direction
- Keep planet math independent of rendering where practical.
- Use spherical coordinates and local tangent frames for surface movement.
- Keep terrain data chunked; never allocate a whole planet as one dense voxel array.
- Separate world generation, terrain storage/meshing, player controls, survival, building, and persistence.
- Treat save files as versioned data with explicit migration rules.
- Avoid unnecessary dependencies and measure before optimizing.

## Validation
- Run formatting, tests, and builds only when explicitly requested.
- Before changing Bevy versions or major APIs, check official migration notes and mobile examples.
- Android GPU features such as MSAA and shadow maps must be enabled selectively after device testing.


## Large-world persistence requirements

- Persist player terrain changes as sparse deltas against deterministic generation; never retain a full dense chunk array solely because it was edited.
- Keep loaded chunk arrays dense only while they are needed for gameplay/meshing. Unloaded edits must be compact and have a bounded in-memory cache.
- The target persistence design is an index/manifest plus independently checksummed chunk records. A damaged chunk must be recoverable or regenerable without invalidating unrelated chunks.
- Chunk-file reads and writes must be bounded and scheduled; do not perform unbounded synchronous disk work in a frame-critical path.
- Preserve v1–v3 save migration and never silently discard legacy edits. Any future migration must be validated with explicit fixtures before replacing the old format.
- Before extending planet dimensions, design 64-bit world/chunk coordinates and a floating-origin or chunk-local rendering transform. Do not assume f32 world-space positions retain precision at arbitrarily large distances.
