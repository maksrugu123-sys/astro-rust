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
- Keep one explicitly defined current save format; reject obsolete formats clearly unless the user specifically requests migration.
- Avoid unnecessary dependencies and measure before optimizing.

## Validation
- Run formatting, tests, and builds only when explicitly requested.
- Before changing Bevy versions or major APIs, check official migration notes and mobile examples.
- Android GPU features such as MSAA and shadow maps must be enabled selectively after device testing.


## Large-world persistence requirements

- Persist player terrain changes as sparse deltas against deterministic generation; never retain a full dense chunk array solely because it was edited.
- Keep loaded chunk arrays dense only while they are needed for gameplay/meshing. Unloaded edits must be compact and have a bounded in-memory cache.
- The persistence format uses an index/manifest plus independently checksummed, immutable chunk records. A damaged chunk must be recoverable or regenerable without invalidating unrelated chunks.
- Chunk-file reads and writes must be bounded and scheduled; do not perform unbounded synchronous disk work in a frame-critical path. The current synchronous startup/autosave implementation is transitional and must not be described as the completed disk cache.
- The current save format is v5 only. It consists of a checksummed manifest and immutable, independently checksummed sparse chunk files. Reject v1–v4 explicitly; do not reintroduce legacy decoders unless the user requests backward compatibility.
- Before extending planet dimensions, design 64-bit world/chunk coordinates and a floating-origin or chunk-local rendering transform. Do not assume f32 world-space positions retain precision at arbitrarily large distances.


## Disk-backed chunk cache implementation contract

The v5 runtime uses index-first lazy reads and a 128-entry cap for unloaded edit payloads. Unsaved edits are never dropped to enforce the cap. SaveStore now uses generation-tagged asynchronous chunk writes, bounded submissions, stale-snapshot rejection, and a manifest commit gate: no new manifest is installed until every chunk write succeeds. Manifest replacement itself remains synchronous.

The next persistence implementation must preserve these invariants:

1. Startup reads and validates the manifest index without decoding every chunk record.
2. A chunk with a disk-backed edit record must not become visible or queryable as generated baseline terrain until its edit record has been loaded and validated.
3. Disk reads and writes run on bounded worker capacity. Main-thread systems only submit requests, consume completed results, and update Bevy assets.
4. The unloaded-edit cache has an explicit 128-chunk bound. Eviction is permitted only after the corresponding immutable chunk record is safely committed and referenced by the active manifest; if the cache is full, unsaved edited chunks must remain resident.
5. A dirty in-memory record always takes precedence over an older disk-index entry. Failed writes retain the dirty record and remain retryable.
6. Saving a new manifest must preserve untouched chunk references from the prior manifest; it must not require reading all untouched chunk payloads into RAM.
7. The manifest remains the commit point. New immutable records are flushed before manifest replacement; cleanup happens only after a successful commit.
8. Corruption or a missing chunk record is isolated to that coordinate and reported. Never silently replace a failed record with an empty edit list and then overwrite its manifest reference.
9. All queues, in-flight reads, pending writes, decoded payloads, and per-frame completions have explicit limits.
10. Keep world coordinates signed 32-bit only within their current documented representable range. A 64-bit coordinate/floating-origin design is a separate prerequisite before expanding that range.

Do not claim the disk cache is complete until the runtime uses this index-first, lazy-read, bounded-eviction flow and the persistence tests cover interrupted writes, failed reads/writes, eviction/reload, and manifest preservation. The current bounded cache and low-level write queue are groundwork; autosave still writes synchronously.


## Chunk I/O queue status

- `src/chunk_io.rs` is connected to manifest-indexed reads and generation-safe SaveStore transactions. Keep chunk writes immutable and bounded; do not commit a manifest until every write in its transaction has completed successfully.
- Do not expose a generated baseline chunk before any referenced disk edit record has been loaded and validated.
- Keep checksum/version/coordinate validation in the save subsystem; the I/O worker returns raw bytes only.
- Untouched manifest references must be preserved during incremental saves.
- Autosave submits bounded asynchronous chunk writes. Suspend and exit enqueue a final transaction, but completion is not guaranteed if the OS terminates the process before workers finish. Never claim otherwise.


### Asynchronous chunk writes

The bounded `ChunkIoQueue` now accepts immutable chunk-file write jobs as well as reads. It uses the same two workers and bounded request/completion channels. A write creates the parent directory, writes to a temporary path, flushes the file with `sync_all`, and renames it to its immutable final path. Completion is reported only after the rename succeeds. Queue saturation and duplicate in-flight writes are rejected without blocking the game thread; callers must retain dirty data and retry.

`SaveStore` now drives generation-tagged asynchronous save transactions. It submits at most four chunk writes per frame, tracks completions, rejects stale terrain/inventory snapshots, and atomically replaces the manifest only after all chunk writes succeed. The manifest commit is synchronous; suspend/exit completion remains subject to OS lifecycle behavior. Tests and compilation have not been run.

- The asynchronous chunk queue supports immutable writes. A successful chunk-write completion is required before its checksum reference may be committed into a new manifest. Failed writes must leave the old manifest valid and retain dirty data for retry.
