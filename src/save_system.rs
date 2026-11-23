//! Versioned, checksummed persistence for sparse terrain edits.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use bevy::prelude::*;

use crate::{
    chunk_io::{ChunkIoOperation, ChunkIoQueue},
    player::Inventory,
    terrain::{ChunkCoord, TerrainWorld, VoxelKind},
};

const MANIFEST_FILE: &str = "world.manifest";
const OBSOLETE_MONOLITHIC_SAVE: &str = "world.astrosave";
const CHUNK_DIRECTORY: &str = "chunks";
const MANIFEST_TEMP: &str = "world.manifest.tmp";
const MAGIC: &[u8; 8] = b"ASTRMNF1";
const CHUNK_MAGIC: &[u8; 8] = b"ASTRCHK1";
const FORMAT_VERSION: u32 = 5;
const CHUNK_FORMAT_VERSION: u32 = 1;
const CHUNK_BLOCKS: usize = 16 * 16 * 16;
const MAX_SAVED_CHUNKS: usize = 8_192;
const MAX_CHUNK_FILE_BYTES: u64 = (8 + 4 + 12 + 2 + CHUNK_BLOCKS * 3 + 8) as u64;
const MAX_MANIFEST_BYTES: u64 = (8 + 4 + 8 + 4 + 4 + 8 + 4 + MAX_SAVED_CHUNKS * 20 + 8) as u64;
const AUTOSAVE_SECONDS: f32 = 15.0;
const MAX_ASYNC_WRITES_PER_FRAME: usize = 4;

struct PendingChunkWrite { coord: ChunkCoord, temp_path: PathBuf, final_path: PathBuf, bytes: Vec<u8> }
struct SaveTransaction {
 generation: u64, revision: u64, inventory: [u16; 4], manifest_bytes: Vec<u8>,
 manifest_temp: PathBuf, manifest_path: PathBuf, chunk_directory: PathBuf,
 records: HashMap<ChunkCoord, u64>, previous_catalog: HashMap<ChunkCoord, u64>,
 writes: VecDeque<PendingChunkWrite>, in_flight: HashSet<ChunkCoord>, saved_coords: Vec<ChunkCoord>,
}

#[derive(Resource)]
pub struct SaveStore {
    directory: Option<PathBuf>,
    last_saved_revision: Option<u64>,
    last_saved_inventory: Option<[u16; 4]>,
    timer: Timer,
    catalog: HashMap<ChunkCoord, u64>,
    loaded_records: HashSet<ChunkCoord>,
    failed_reads: HashSet<ChunkCoord>,
    cache_deferred_reads: HashSet<ChunkCoord>,
    load_failed: bool,
    pending_save: Option<SaveTransaction>,
    next_generation: u64,
}

impl SaveStore {
    /// Android uses the app-private internal files directory; desktop uses a
    /// project-local saves directory. No public-storage permission is needed.
    pub fn discover() -> Self {
        #[cfg(target_os = "android")]
        let base = bevy::android::ANDROID_APP
            .get()
            .and_then(|app| app.internal_data_path());

        #[cfg(not(target_os = "android"))]
        let base = std::env::current_dir().ok();

        Self {
            directory: base.map(|path| {
                if cfg!(target_os = "android") {
                    path.join("astro-rust")
                } else {
                    path.join("saves")
                }
            }),
            last_saved_revision: None,
            last_saved_inventory: None,
            timer: Timer::from_seconds(AUTOSAVE_SECONDS, TimerMode::Repeating),
            catalog: HashMap::new(),
            loaded_records: HashSet::new(),
            failed_reads: HashSet::new(),
            cache_deferred_reads: HashSet::new(),
            load_failed: false,
            pending_save: None,
            next_generation: 1,
        }
    }

    fn manifest_path(&self) -> Option<PathBuf> {
        self.directory.as_ref().map(|dir| dir.join(MANIFEST_FILE))
    }

    fn chunk_directory(&self) -> Option<PathBuf> {
        self.directory.as_ref().map(|dir| dir.join(CHUNK_DIRECTORY))
    }

    fn chunk_filename(coord: ChunkCoord, checksum: u64) -> String {
        format!("{}_{}_{}_{checksum:016x}.chunk", coord.x, coord.y, coord.z)
    }

    fn chunk_path(directory: &Path, coord: ChunkCoord, checksum: u64) -> PathBuf {
        directory.join(Self::chunk_filename(coord, checksum))
    }

    fn load(&self) -> Result<Option<SavedWorld>, String> {
        let Some(manifest_path) = self.manifest_path() else {
            return Err("No writable application data directory is available".into());
        };
        if !manifest_path.exists() {
            let obsolete_path = self.directory.as_ref()
                .map(|directory| directory.join(OBSOLETE_MONOLITHIC_SAVE));
            if obsolete_path.as_ref().is_some_and(|path| path.exists()) {
                return Err("An obsolete monolithic v4 save was found. This build requires v5 chunk files; the old file was left untouched.".into());
            }
            return Ok(None);
        }
        let manifest_metadata = fs::metadata(&manifest_path)
            .map_err(|e| format!("Cannot inspect world manifest: {e}"))?;
        if manifest_metadata.len() > MAX_MANIFEST_BYTES {
            return Err("World manifest exceeds the supported size limit".into());
        }
        let mut manifest_bytes = Vec::with_capacity(manifest_metadata.len() as usize);
        File::open(&manifest_path)
            .and_then(|mut file| file.read_to_end(&mut manifest_bytes))
            .map_err(|e| format!("Cannot read world manifest: {e}"))?;
        let manifest = decode_manifest(&manifest_bytes)?;
        Ok(Some(SavedWorld {
            seed: manifest.seed,
            voxel_size: manifest.voxel_size,
            base_radius: manifest.base_radius,
            chunk_records: manifest.chunk_records.into_iter().collect(),
            inventory: manifest.inventory,
        }))
    }

    pub(crate) fn forget_loaded_chunk(&mut self, coord: ChunkCoord) {
        self.loaded_records.remove(&coord);
    }

    pub(crate) fn request_saved_chunk(
        &mut self,
        coord: ChunkCoord,
        io: &mut ChunkIoQueue,
        terrain: &TerrainWorld,
    ) -> bool {
        let Some(checksum) = self.catalog.get(&coord).copied() else {
            return false;
        };
        if self.loaded_records.contains(&coord) {
            return false;
        }
        if self.cache_deferred_reads.contains(&coord) {
            if !terrain.has_edit_cache_capacity() {
                return true;
            }
            self.cache_deferred_reads.remove(&coord);
        }
        if self.failed_reads.contains(&coord) {
            return true;
        }
        let Some(directory) = self.chunk_directory() else {
            self.failed_reads.insert(coord);
            warn!("Cannot load saved chunk {},{},{}: no save directory", coord.x, coord.y, coord.z);
            return true;
        };
        let path = Self::chunk_path(&directory, coord, checksum);
        if !io.is_read_pending(coord) {
            let _ = io.request_read(coord, path);
        }
        true
    }

    /// Processes bounded I/O completions and advances the active save transaction.
    pub fn process_chunk_io(mut io: ResMut<ChunkIoQueue>, mut store: ResMut<SaveStore>, mut terrain: ResMut<TerrainWorld>, inventory: Res<Inventory>) {
        for completion in io.collect_completed(8) {
            let coord = completion.coord;
            match completion.operation {
                ChunkIoOperation::Read => {
                    let Some(expected) = store.catalog.get(&coord).copied() else { continue; };
                    match completion.result.and_then(|bytes| decode_chunk(&bytes, coord, expected)) {
                        Ok(edits) if terrain.cache_saved_chunk_edits(coord, edits) => {
                            store.loaded_records.insert(coord);
                            store.failed_reads.remove(&coord);
                            store.cache_deferred_reads.remove(&coord);
                        }
                        Ok(_) => { store.cache_deferred_reads.insert(coord); }
                        Err(error) => {
                            store.failed_reads.insert(coord);
                            warn!("Saved chunk {},{},{} remains hidden: {error}", coord.x, coord.y, coord.z);
                        }
                    }
                }
                ChunkIoOperation::Write => {
                    let Some(tx) = store.pending_save.as_mut() else { continue; };
                    if !tx.in_flight.remove(&coord) { continue; }
                    if let Err(error) = completion.result {
                        warn!("Save generation {} failed on chunk {},{},{}: {error}", tx.generation, coord.x, coord.y, coord.z);
                        store.pending_save = None;
                    }
                }
            }
        }

        // A changed world invalidates the captured snapshot. Its content-addressed
        // chunk files are harmless orphans; the old manifest remains authoritative.
        if store.pending_save.as_ref().is_some_and(|tx| tx.revision != terrain.edit_revision() || tx.inventory != inventory.snapshot()) {
            store.pending_save = None;
        }

        if let Some(tx) = store.pending_save.as_mut() {
            let mut submitted = 0;
            while submitted < MAX_ASYNC_WRITES_PER_FRAME {
                let Some(job) = tx.writes.pop_front() else { break; };
                if io.request_write(job.coord, job.temp_path.clone(), job.final_path.clone(), job.bytes.clone()) {
                    tx.in_flight.insert(job.coord);
                    submitted += 1;
                } else {
                    tx.writes.push_front(job);
                    break;
                }
            }
        }

        let ready = store.pending_save.as_ref().is_some_and(|tx| tx.writes.is_empty() && tx.in_flight.is_empty());
        if ready {
            let tx = store.pending_save.take().expect("save transaction checked");
            match write_atomic(&tx.manifest_temp, &tx.manifest_path, &tx.manifest_bytes) {
                Ok(()) => {
                    let retained: HashSet<String> = tx.records.iter().map(|(c, h)| SaveStore::chunk_filename(*c, *h)).collect();
                    store.catalog = tx.records;
                    store.loaded_records.retain(|c| store.catalog.contains_key(c) && terrain.is_chunk_loaded(*c));
                    for coord in tx.previous_catalog.keys() {
                        if !store.catalog.contains_key(coord) {
                            terrain.clear_saved_chunk(*coord);
                            store.loaded_records.remove(coord);
                            store.failed_reads.remove(coord);
                            store.cache_deferred_reads.remove(coord);
                        }
                    }
                    for coord in tx.saved_coords {
                        terrain.mark_saved_chunk(coord);
                        if terrain.is_chunk_loaded(coord) { store.loaded_records.insert(coord); }
                        else { store.loaded_records.remove(&coord); }
                        store.failed_reads.remove(&coord);
                        store.cache_deferred_reads.remove(&coord);
                    }
                    if let Ok(entries) = fs::read_dir(&tx.chunk_directory) {
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().into_owned();
                            if name.ends_with(".chunk") && !retained.contains(&name) { let _ = fs::remove_file(entry.path()); }
                        }
                    }
                    store.last_saved_revision = Some(tx.revision);
                    store.last_saved_inventory = Some(tx.inventory);
                    info!("ASTRO-RUST save generation {} committed", tx.generation);
                }
                Err(error) => warn!("Manifest commit failed; previous save remains active: {error}"),
            }
        }
    }

    fn begin_save(&mut self, terrain: &TerrainWorld, inventory: &Inventory) -> Result<(), String> {
        if self.load_failed { return Err("Existing save failed validation; refusing overwrite".into()); }
        if self.pending_save.is_some() { return Ok(()); }
        let directory = self.directory.as_ref().ok_or("No writable application data directory is available")?;
        let chunks = terrain.export_edited_chunks();
        let previous_catalog = self.catalog.clone();
        let mut records = self.catalog.clone();
        for coord in terrain.cleared_chunk_coords() { records.remove(&coord); }
        let old: Vec<_> = records.keys().copied().collect();
        for coord in old {
            if terrain.is_chunk_loaded(coord) && !terrain.is_chunk_edited(coord) { records.remove(&coord); }
        }
        let chunk_directory = directory.join(CHUNK_DIRECTORY);
        let mut writes = VecDeque::with_capacity(chunks.len());
        let mut saved_coords = Vec::with_capacity(chunks.len());
        for (coord, edits) in &chunks {
            let bytes = encode_chunk(*coord, edits)?;
            let checksum = read_u64(&bytes, bytes.len() - 8)?;
            let final_path = Self::chunk_path(&chunk_directory, *coord, checksum);
            let temp_path = final_path.with_extension(format!("{checksum:016x}.{}.tmp", self.next_generation));
            records.insert(*coord, checksum);
            writes.push_back(PendingChunkWrite { coord: *coord, temp_path, final_path, bytes });
            saved_coords.push(*coord);
        }
        if records.len() > MAX_SAVED_CHUNKS {
            return Err(format!("World contains {} edited chunks; save limit is {}", records.len(), MAX_SAVED_CHUNKS));
        }
        let manifest_bytes = encode_manifest_records(terrain, &records, inventory)?;
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        self.pending_save = Some(SaveTransaction {
            generation, revision: terrain.edit_revision(), inventory: inventory.snapshot(),
            manifest_bytes, manifest_temp: directory.join(format!("{MANIFEST_TEMP}.{generation}")),
            manifest_path: directory.join(MANIFEST_FILE), chunk_directory, records,
            previous_catalog, writes, in_flight: HashSet::new(), saved_coords,
        });
        Ok(())
    }
}

struct SavedWorld {
    seed: u64,
    voxel_size: f32,
    base_radius: f32,
    chunk_records: HashMap<ChunkCoord, u64>,
    inventory: [u16; 4],
}

pub fn load_saved_world(
    mut terrain: ResMut<TerrainWorld>,
    mut inventory: ResMut<Inventory>,
    mut store: ResMut<SaveStore>,
) {
    match store.load() {
        Ok(Some(save)) => {
            let saved_coords: Vec<ChunkCoord> = save.chunk_records.keys().copied().collect();
            terrain.restore_saved_world_index(
                save.seed,
                save.voxel_size,
                save.base_radius,
                saved_coords,
            );
            store.catalog = save.chunk_records;
            store.loaded_records.clear();
            store.failed_reads.clear();
            store.cache_deferred_reads.clear();
            store.load_failed = false;
            inventory.regolith = save.inventory[0].min(99);
            inventory.rock = save.inventory[1].min(99);
            inventory.copper = save.inventory[2].min(99);
            inventory.iron = save.inventory[3].min(99);
            info!("Loaded ASTRO-RUST world save");
        }
        Ok(None) => {
            store.catalog.clear();
            store.loaded_records.clear();
            store.failed_reads.clear();
            store.cache_deferred_reads.clear();
            store.load_failed = false;
            info!("No ASTRO-RUST save found; generating a new world");
        }
        Err(error) => {
            store.load_failed = true;
            warn!("Could not load ASTRO-RUST save: {error}");
        }
    }
}

pub fn autosave_world(
    time: Res<Time>,
    mut terrain: ResMut<TerrainWorld>,
    inventory: Res<Inventory>,
    mut store: ResMut<SaveStore>,
) {
    store.timer.tick(time.delta());
    let revision = terrain.edit_revision();
    let inventory_snapshot = inventory.snapshot();
    let world_is_current = store.last_saved_revision == Some(revision)
        && store.last_saved_inventory == Some(inventory_snapshot);
    if !store.timer.just_finished() || world_is_current {
        return;
    }
    if store.pending_save.is_none() {
        if let Err(error) = store.begin_save(&terrain, &inventory) {
            warn!("ASTRO-RUST autosave could not start: {error}");
        }
    }
}

/// Save on the last active frame before Android/iOS backgrounds the app.
pub fn save_world_on_suspend(
    mut lifecycle: MessageReader<bevy::window::AppLifecycle>,
    mut terrain: ResMut<TerrainWorld>,
    inventory: Res<Inventory>,
    mut store: ResMut<SaveStore>,
) {
    let should_save = lifecycle
        .read()
        .any(|state| matches!(state, bevy::window::AppLifecycle::WillSuspend));
    let inventory_snapshot = inventory.snapshot();
    if !should_save
        || (store.last_saved_revision == Some(terrain.edit_revision())
            && store.last_saved_inventory == Some(inventory_snapshot))
    {
        return;
    }
    if store.pending_save.is_none() {
        if let Err(error) = store.begin_save(&terrain, &inventory) {
            warn!("ASTRO-RUST suspend save could not be queued: {error}");
        } else {
            warn!("Suspend save queued asynchronously; OS background execution may be limited");
        }
    }
}

pub fn save_world_on_exit(
    mut exits: MessageReader<AppExit>,
    terrain: Res<TerrainWorld>,
    inventory: Res<Inventory>,
    store: Res<SaveStore>,
) {
    if exits.read().next().is_none() { return; }
    if store.last_saved_revision == Some(terrain.edit_revision())
        && store.last_saved_inventory == Some(inventory.snapshot())
    {
        return;
    }
    // Last runs immediately before app teardown. It cannot pump an asynchronous
    // transaction to completion, so never pretend that merely enqueueing one is durable.
    warn!("ASTRO-RUST has unsaved changes at AppExit; rely on the periodic autosave or WillSuspend transaction for durability");
}

struct ManifestData {
    seed: u64,
    voxel_size: f32,
    base_radius: f32,
    inventory: [u16; 4],
    chunk_records: Vec<(ChunkCoord, u64)>,
}

fn validate_inventory(inventory: &Inventory) -> Result<(), String> {
    if inventory.regolith > 99 || inventory.rock > 99
        || inventory.copper > 99 || inventory.iron > 99
    {
        return Err("Inventory contains a stack above its supported capacity".into());
    }
    Ok(())
}

fn encode_manifest(
    terrain: &TerrainWorld,
    chunks: &[(ChunkCoord, Vec<(u16, VoxelKind)>)],
    inventory: &Inventory,
) -> Result<Vec<u8>, String> {
    let mut records = HashMap::with_capacity(chunks.len());
    for (coord, edits) in chunks {
        let chunk_bytes = encode_chunk(*coord, edits)?;
        let checksum = read_u64(&chunk_bytes, chunk_bytes.len() - 8)?;
        records.insert(*coord, checksum);
    }
    encode_manifest_records(terrain, &records, inventory)
}

fn encode_manifest_records(
    terrain: &TerrainWorld,
    records: &HashMap<ChunkCoord, u64>,
    inventory: &Inventory,
) -> Result<Vec<u8>, String> {
    validate_inventory(inventory)?;
    if records.len() > MAX_SAVED_CHUNKS {
        return Err("Save contains too many edited chunks".into());
    }
    let mut ordered: Vec<_> = records.iter().collect();
    ordered.sort_by_key(|(coord, _)| (coord.x, coord.y, coord.z));
    let mut bytes = Vec::with_capacity(40 + ordered.len() * 20 + 8);
    bytes.extend_from_slice(MAGIC);
    push_u32(&mut bytes, FORMAT_VERSION);
    push_u64(&mut bytes, terrain.seed);
    push_u32(&mut bytes, terrain.voxel_size.to_bits());
    push_u32(&mut bytes, terrain.base_radius.to_bits());
    push_u16(&mut bytes, inventory.regolith);
    push_u16(&mut bytes, inventory.rock);
    push_u16(&mut bytes, inventory.copper);
    push_u16(&mut bytes, inventory.iron);
    push_u32(&mut bytes, ordered.len() as u32);
    let mut unique = HashSet::with_capacity(ordered.len());
    for (coord, checksum) in ordered {
        if !coord.is_representable() || !unique.insert(*coord) {
            return Err("Manifest contains an invalid or duplicate chunk coordinate".into());
        }
        push_i32(&mut bytes, coord.x);
        push_i32(&mut bytes, coord.y);
        push_i32(&mut bytes, coord.z);
        push_u64(&mut bytes, *checksum);
    }
    let checksum = checksum64(&bytes);
    push_u64(&mut bytes, checksum);
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("Encoded manifest exceeds the supported size limit".into());
    }
    Ok(bytes)
}

fn encode_chunk(coord: ChunkCoord, edits: &[(u16, VoxelKind)]) -> Result<Vec<u8>, String> {
    if !coord.is_representable() || edits.is_empty() || edits.len() > CHUNK_BLOCKS {
        return Err("Chunk record metadata is outside supported bounds".into());
    }
    let mut sorted = edits.to_vec();
    sorted.sort_by_key(|(index, _)| *index);
    if sorted.iter().any(|(index, _)| *index as usize >= CHUNK_BLOCKS)
        || sorted.windows(2).any(|pair| pair[0].0 == pair[1].0)
    {
        return Err("Chunk record contains invalid or duplicate voxel indices".into());
    }
    let mut bytes = Vec::with_capacity(26 + sorted.len() * 3);
    bytes.extend_from_slice(CHUNK_MAGIC);
    push_u32(&mut bytes, CHUNK_FORMAT_VERSION);
    push_i32(&mut bytes, coord.x);
    push_i32(&mut bytes, coord.y);
    push_i32(&mut bytes, coord.z);
    push_u16(&mut bytes, sorted.len() as u16);
    for (index, kind) in sorted {
        bytes.extend_from_slice(&index.to_le_bytes());
        bytes.push(kind as u8);
    }
    let checksum = checksum64(&bytes);
    push_u64(&mut bytes, checksum);
    if bytes.len() as u64 > MAX_CHUNK_FILE_BYTES {
        return Err("Encoded chunk exceeds the supported size limit".into());
    }
    Ok(bytes)
}

fn decode_manifest(bytes: &[u8]) -> Result<ManifestData, String> {
    if bytes.len() < 44 || &bytes[..8] != MAGIC {
        return Err("World manifest header is missing or invalid".into());
    }
    let payload_end = bytes.len() - 8;
    if checksum64(&bytes[..payload_end]) != read_u64(bytes, payload_end)? {
        return Err("World manifest checksum mismatch".into());
    }
    let mut cursor = 8;
    let version = take_u32(bytes, &mut cursor)?;
    if version != FORMAT_VERSION {
        return Err(format!("Unsupported save format version: {version}; this build requires v{FORMAT_VERSION}"));
    }
    let seed = take_u64(bytes, &mut cursor)?;
    let voxel_size = f32::from_bits(take_u32(bytes, &mut cursor)?);
    let base_radius = f32::from_bits(take_u32(bytes, &mut cursor)?);
    let inventory = [
        take_u16(bytes, &mut cursor)?,
        take_u16(bytes, &mut cursor)?,
        take_u16(bytes, &mut cursor)?,
        take_u16(bytes, &mut cursor)?,
    ];
    let count = take_u32(bytes, &mut cursor)? as usize;
    if !voxel_size.is_finite() || voxel_size <= 0.0
        || !base_radius.is_finite() || base_radius <= 0.0
        || inventory.iter().any(|count| *count > 99)
        || count > MAX_SAVED_CHUNKS
    {
        return Err("World manifest metadata is outside supported bounds".into());
    }
    let expected = cursor.checked_add(count.checked_mul(20).ok_or("Manifest size overflow")?)
        .ok_or("Manifest size overflow")?;
    if expected != payload_end {
        return Err("World manifest length does not match its chunk index".into());
    }
    let mut chunk_records = Vec::with_capacity(count);
    let mut unique = HashSet::with_capacity(count);
    for _ in 0..count {
        let coord = ChunkCoord {
            x: take_i32(bytes, &mut cursor)?,
            y: take_i32(bytes, &mut cursor)?,
            z: take_i32(bytes, &mut cursor)?,
        };
        if !coord.is_representable() || !unique.insert(coord) {
            return Err("World manifest contains an invalid or duplicate chunk coordinate".into());
        }
        let chunk_checksum = take_u64(bytes, &mut cursor)?;
        chunk_records.push((coord, chunk_checksum));
    }
    Ok(ManifestData { seed, voxel_size, base_radius, inventory, chunk_records })
}

fn decode_chunk(bytes: &[u8], expected_coord: ChunkCoord, expected_checksum: u64) -> Result<Vec<(u16, VoxelKind)>, String> {
    if bytes.len() < 37 || bytes.len() as u64 > MAX_CHUNK_FILE_BYTES
        || &bytes[..8] != CHUNK_MAGIC
    {
        return Err("Chunk file header is missing or invalid".into());
    }
    let payload_end = bytes.len() - 8;
    let stored_checksum = read_u64(bytes, payload_end)?;
    if stored_checksum != expected_checksum || checksum64(&bytes[..payload_end]) != stored_checksum {
        return Err("Chunk checksum mismatch".into());
    }
    let mut cursor = 8;
    let version = take_u32(bytes, &mut cursor)?;
    if version != CHUNK_FORMAT_VERSION {
        return Err(format!("Unsupported chunk file version: {version}"));
    }
    let coord = ChunkCoord {
        x: take_i32(bytes, &mut cursor)?,
        y: take_i32(bytes, &mut cursor)?,
        z: take_i32(bytes, &mut cursor)?,
    };
    if coord != expected_coord || !coord.is_representable() {
        return Err("Chunk file coordinate does not match its manifest entry".into());
    }
    let edit_count = take_u16(bytes, &mut cursor)? as usize;
    if edit_count == 0 || edit_count > CHUNK_BLOCKS {
        return Err("Chunk edit count is outside supported bounds".into());
    }
    let expected_end = cursor.checked_add(edit_count.checked_mul(3).ok_or("Chunk size overflow")?)
        .ok_or("Chunk size overflow")?;
    if expected_end != payload_end {
        return Err("Chunk file length does not match its edit count".into());
    }
    let mut edits = Vec::with_capacity(edit_count);
    let mut indices = HashSet::with_capacity(edit_count);
    for _ in 0..edit_count {
        let index = take_u16(bytes, &mut cursor)?;
        if index as usize >= CHUNK_BLOCKS || !indices.insert(index) {
            return Err("Chunk contains an invalid or duplicate voxel index".into());
        }
        let kind = decode_voxel(*bytes.get(cursor).ok_or("Chunk file is truncated")?)?;
        cursor += 1;
        edits.push((index, kind));
    }
    Ok(edits)
}

fn write_atomic(temp_path: &Path, final_path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::create(temp_path).map_err(|e| format!("Cannot create temporary file: {e}"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("Cannot flush temporary file: {e}"))?;
    fs::rename(temp_path, final_path)
        .map_err(|e| format!("Cannot atomically replace file: {e}"))?;
    Ok(())
}

fn decode_voxel(value: u8) -> Result<VoxelKind, String> {
    match value {
        0 => Ok(VoxelKind::Air),
        1 => Ok(VoxelKind::Regolith),
        2 => Ok(VoxelKind::Rock),
        3 => Ok(VoxelKind::CopperOre),
        4 => Ok(VoxelKind::IronOre),
        _ => Err("Save contains an unknown voxel kind".into()),
    }
}

fn checksum64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn take_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, String> {
    let end = (*cursor).checked_add(2).ok_or("Save offset overflow")?;
    let raw: [u8; 2] = bytes.get(*cursor..end).ok_or("Save is truncated")?
        .try_into().map_err(|_| "Invalid u16 field")?;
    *cursor = end;
    Ok(u16::from_le_bytes(raw))
}
fn take_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let end = (*cursor).checked_add(4).ok_or("Save offset overflow")?;
    let raw: [u8; 4] = bytes.get(*cursor..end).ok_or("Save is truncated")?
        .try_into().map_err(|_| "Invalid u32 field")?;
    *cursor = end;
    Ok(u32::from_le_bytes(raw))
}
fn take_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let end = (*cursor).checked_add(8).ok_or("Save offset overflow")?;
    let raw: [u8; 8] = bytes.get(*cursor..end).ok_or("Save is truncated")?
        .try_into().map_err(|_| "Invalid u64 field")?;
    *cursor = end;
    Ok(u64::from_le_bytes(raw))
}
fn take_i32(bytes: &[u8], cursor: &mut usize) -> Result<i32, String> {
    let end = (*cursor).checked_add(4).ok_or("Save offset overflow")?;
    let raw: [u8; 4] = bytes.get(*cursor..end).ok_or("Save is truncated")?
        .try_into().map_err(|_| "Invalid i32 field")?;
    *cursor = end;
    Ok(i32::from_le_bytes(raw))
}
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let end = offset.checked_add(8).ok_or("Save checksum offset overflow")?;
    let raw: [u8; 8] = bytes.get(offset..end).ok_or("Save checksum is missing")?
        .try_into().map_err(|_| "Invalid checksum field")?;
    Ok(u64::from_le_bytes(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_manifest_and_independent_chunk_round_trip() {
        let world = TerrainWorld::new(99, 0.25, 3.0);
        let coord = ChunkCoord { x: -2, y: 1, z: 3 };
        let edits = vec![(7, VoxelKind::Rock), (4095, VoxelKind::Air)];
        let chunks = vec![(coord, edits.clone())];
        let inventory = Inventory { regolith: 4, rock: 5, copper: 6, iron: 7 };
        let manifest = encode_manifest(&world, &chunks, &inventory).unwrap();
        let decoded_manifest = decode_manifest(&manifest).unwrap();
        assert_eq!(decoded_manifest.seed, 99);
        assert_eq!(decoded_manifest.voxel_size, 0.25);
        assert_eq!(decoded_manifest.base_radius, 3.0);
        assert_eq!(decoded_manifest.inventory, [4, 5, 6, 7]);
        assert_eq!(decoded_manifest.chunk_records.len(), 1);
        assert_eq!(decoded_manifest.chunk_records[0].0, coord);

        let chunk_file = encode_chunk(coord, &edits).unwrap();
        let checksum = read_u64(&chunk_file, chunk_file.len() - 8).unwrap();
        assert_eq!(decoded_manifest.chunk_records[0].1, checksum);
        assert_eq!(decode_chunk(&chunk_file, coord, checksum).unwrap(), edits);
    }

    #[test]
    fn startup_loads_manifest_index_without_opening_chunk_payloads() {
        let root = std::env::temp_dir().join(format!(
            "astro-rust-lazy-index-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let world = TerrainWorld::new(123, 0.25, 3.0);
        let coord = ChunkCoord { x: 4, y: -2, z: 1 };
        let records = HashMap::from([(coord, 0x1234_5678_u64)]);
        let manifest = encode_manifest_records(&world, &records, &Inventory::default()).unwrap();
        fs::write(root.join(MANIFEST_FILE), manifest).unwrap();

        let store = SaveStore {
            directory: Some(root.clone()),
            last_saved_revision: None,
            last_saved_inventory: None,
            timer: Timer::from_seconds(AUTOSAVE_SECONDS, TimerMode::Repeating),
            catalog: HashMap::new(),
            loaded_records: HashSet::new(),
            failed_reads: HashSet::new(),
            cache_deferred_reads: HashSet::new(),
            load_failed: false,
            pending_save: None,
            next_generation: 1,
        };
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.chunk_records, records);
        assert!(!root.join(CHUNK_DIRECTORY).join(
            SaveStore::chunk_filename(coord, records[&coord])
        ).exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn incremental_save_preserves_unloaded_manifest_records() {
        let root = std::env::temp_dir().join(format!(
            "astro-rust-preserve-index-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let mut world = TerrainWorld::new(321, 0.25, 3.0);
        let coord = ChunkCoord { x: -7, y: 2, z: 5 };
        let records = HashMap::from([(coord, 0xCAFE_BABE_u64)]);
        let mut store = SaveStore {
            directory: Some(root.clone()),
            last_saved_revision: None,
            last_saved_inventory: None,
            timer: Timer::from_seconds(AUTOSAVE_SECONDS, TimerMode::Repeating),
            catalog: records.clone(),
            loaded_records: HashSet::new(),
            failed_reads: HashSet::new(),
            cache_deferred_reads: HashSet::new(),
            load_failed: false,
            pending_save: None,
            next_generation: 1,
        };

        store.begin_save(&world, &Inventory::default()).unwrap();
        let transaction = store.pending_save.take().unwrap();
        assert!(transaction.writes.is_empty());
        fs::write(root.join(MANIFEST_FILE), transaction.manifest_bytes).unwrap();
        let manifest = fs::read(root.join(MANIFEST_FILE)).unwrap();
        let decoded = decode_manifest(&manifest).unwrap();
        assert_eq!(decoded.chunk_records, vec![(coord, records[&coord])]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manifest_rejects_old_save_versions() {
        for version in 1..FORMAT_VERSION {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(MAGIC);
            push_u32(&mut bytes, version);
            push_u64(&mut bytes, 77);
            push_u32(&mut bytes, 0.25_f32.to_bits());
            push_u32(&mut bytes, 3.0_f32.to_bits());
            for value in [0_u16; 4] {
                push_u16(&mut bytes, value);
            }
            push_u32(&mut bytes, 0);
            let checksum = checksum64(&bytes);
            push_u64(&mut bytes, checksum);
            let error = decode_manifest(&bytes).err().unwrap();
            assert!(error.contains("requires v5"));
        }
    }

    #[test]
    fn one_damaged_chunk_does_not_change_other_chunk_records() {
        let first = ChunkCoord { x: 0, y: 0, z: 0 };
        let second = ChunkCoord { x: 1, y: 0, z: 0 };
        let first_bytes = encode_chunk(first, &[(3, VoxelKind::Air)]).unwrap();
        let mut second_bytes = encode_chunk(second, &[(4, VoxelKind::CopperOre)]).unwrap();
        second_bytes[20] ^= 0x20;
        let first_checksum = read_u64(&first_bytes, first_bytes.len() - 8).unwrap();
        assert_eq!(decode_chunk(&first_bytes, first, first_checksum).unwrap(), vec![(3, VoxelKind::Air)]);
        let second_checksum = read_u64(&second_bytes, second_bytes.len() - 8).unwrap();
        assert!(decode_chunk(&second_bytes, second, second_checksum).is_err());
    }

    #[test]
    fn chunk_file_rejects_manifest_coordinate_mismatch() {
        let stored = ChunkCoord { x: 2, y: 0, z: 0 };
        let expected = ChunkCoord { x: 1, y: 0, z: 0 };
        let bytes = encode_chunk(stored, &[(0, VoxelKind::Rock)]).unwrap();
        let checksum = read_u64(&bytes, bytes.len() - 8).unwrap();
        assert!(decode_chunk(&bytes, expected, checksum).is_err());
    }

    #[test]
    fn manifest_rejects_out_of_range_chunk_coordinates() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        push_u32(&mut bytes, FORMAT_VERSION);
        push_u64(&mut bytes, 88);
        push_u32(&mut bytes, 0.25_f32.to_bits());
        push_u32(&mut bytes, 3.0_f32.to_bits());
        for value in [0_u16; 4] {
            push_u16(&mut bytes, value);
        }
        push_u32(&mut bytes, 1);
        push_i32(&mut bytes, i32::MAX);
        push_i32(&mut bytes, 0);
        push_i32(&mut bytes, 0);
        let checksum = checksum64(&bytes);
        push_u64(&mut bytes, checksum);
        assert!(decode_manifest(&bytes).is_err());
    }

    #[test]
    fn damaged_manifest_is_rejected_by_checksum() {
        let world = TerrainWorld::new(99, 0.25, 3.0);
        let bytes = encode_manifest(&world, &[], &Inventory::default()).unwrap();
        let mut damaged = bytes;
        damaged[12] ^= 0x01;
        assert!(decode_manifest(&damaged).is_err());
    }
}
