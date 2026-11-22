//! Versioned, checksummed persistence for sparse terrain edits.
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use bevy::prelude::*;

use crate::{
    player::Inventory,
    terrain::{ChunkCoord, TerrainWorld, VoxelKind},
};

const MANIFEST_FILE: &str = "world.manifest";
const CHUNK_DIRECTORY: &str = "chunks";
const MANIFEST_TEMP: &str = "world.manifest.tmp";
const MAGIC: &[u8; 8] = b"ASTRMNF1";
const CHUNK_MAGIC: &[u8; 8] = b"ASTRCHK1";
const FORMAT_VERSION: u32 = 5;
const CHUNK_FORMAT_VERSION: u32 = 1;
const CHUNK_BLOCKS: usize = 16 * 16 * 16;
const MAX_SAVED_CHUNKS: usize = 8_192;
const MAX_CHUNK_FILE_BYTES: u64 = (8 + 4 + 12 + 2 + CHUNK_BLOCKS * 3 + 8) as u64;
const MAX_MANIFEST_BYTES: u64 = (8 + 4 + 8 + 4 + 4 + 8 + 4 + MAX_SAVED_CHUNKS * 12 + 8) as u64;
const AUTOSAVE_SECONDS: f32 = 15.0;

#[derive(Resource)]
pub struct SaveStore {
    directory: Option<PathBuf>,
    last_saved_revision: Option<u64>,
    last_saved_inventory: Option<[u16; 4]>,
    timer: Timer,
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
        }
    }

    fn manifest_path(&self) -> Option<PathBuf> {
        self.directory.as_ref().map(|dir| dir.join(MANIFEST_FILE))
    }

    fn chunk_directory(&self) -> Option<PathBuf> {
        self.directory.as_ref().map(|dir| dir.join(CHUNK_DIRECTORY))
    }

    fn chunk_path(directory: &Path, coord: ChunkCoord) -> PathBuf {
        directory.join(format!("{}_{}_{}.chunk", coord.x, coord.y, coord.z))
    }

    fn load(&self) -> Result<Option<SavedWorld>, String> {
        let Some(manifest_path) = self.manifest_path() else {
            return Err("No writable application data directory is available".into());
        };
        if !manifest_path.exists() {
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
        let chunk_directory = self.chunk_directory()
            .ok_or("No writable chunk directory is available")?;
        let mut chunks = Vec::with_capacity(manifest.chunk_coords.len());
        let mut damaged = 0usize;
        for coord in manifest.chunk_coords {
            let path = Self::chunk_path(&chunk_directory, coord);
            let loaded = (|| -> Result<Vec<(u16, VoxelKind)>, String> {
                let metadata = fs::metadata(&path)
                    .map_err(|e| format!("Cannot inspect chunk file: {e}"))?;
                if metadata.len() > MAX_CHUNK_FILE_BYTES {
                    return Err("Chunk file exceeds the supported size limit".into());
                }
                let mut bytes = Vec::with_capacity(metadata.len() as usize);
                File::open(&path)
                    .and_then(|mut file| file.read_to_end(&mut bytes))
                    .map_err(|e| format!("Cannot read chunk file: {e}"))?;
                decode_chunk(&bytes, coord)
            })();
            match loaded {
                Ok(edits) => chunks.push((coord, edits)),
                Err(error) => {
                    damaged += 1;
                    warn!("Skipping damaged ASTRO-RUST chunk {},{},{}: {error}", coord.x, coord.y, coord.z);
                }
            }
        }
        if damaged > 0 {
            warn!("Loaded world with {damaged} damaged or missing chunk file(s); unaffected chunks were preserved");
        }
        Ok(Some(SavedWorld {
            seed: manifest.seed,
            voxel_size: manifest.voxel_size,
            base_radius: manifest.base_radius,
            chunks,
            inventory: manifest.inventory,
        }))
    }

    fn save(&self, terrain: &TerrainWorld, inventory: &Inventory) -> Result<(), String> {
        let Some(directory) = self.directory.as_ref() else {
            return Err("No writable application data directory is available".into());
        };
        let edited_count = terrain.edited_chunk_count();
        if edited_count > MAX_SAVED_CHUNKS {
            return Err(format!(
                "World contains {} edited chunks; save limit is {}",
                edited_count, MAX_SAVED_CHUNKS
            ));
        }
        let chunks = terrain.export_edited_chunks();
        let manifest = encode_manifest(terrain, &chunks, inventory)?;
        let chunk_directory = directory.join(CHUNK_DIRECTORY);
        fs::create_dir_all(&chunk_directory)
            .map_err(|e| format!("Cannot create chunk directory: {e}"))?;

        let mut retained = HashSet::with_capacity(chunks.len());
        for (coord, edits) in &chunks {
            let filename = format!("{}_{}_{}.chunk", coord.x, coord.y, coord.z);
            retained.insert(filename);
            let bytes = encode_chunk(*coord, edits)?;
            let path = Self::chunk_path(&chunk_directory, *coord);
            let temp_path = path.with_extension("chunk.tmp");
            write_atomic(&temp_path, &path, &bytes)
                .map_err(|e| format!("Cannot persist chunk {},{},{}: {e}", coord.x, coord.y, coord.z))?;
        }

        let manifest_path = directory.join(MANIFEST_FILE);
        let temp_manifest = directory.join(MANIFEST_TEMP);
        write_atomic(&temp_manifest, &manifest_path, &manifest)
            .map_err(|e| format!("Cannot commit world manifest: {e}"))?;

        // The manifest is the commit point. Orphaned files are safe to remove
        // only after it has been replaced successfully.
        for entry in fs::read_dir(&chunk_directory)
            .map_err(|e| format!("Cannot inspect chunk directory: {e}"))?
        {
            let entry = entry.map_err(|e| format!("Cannot inspect chunk entry: {e}"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".chunk") && !retained.contains(&name) {
                let _ = fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
}

struct SavedWorld {
    seed: u64,
    voxel_size: f32,
    base_radius: f32,
    chunks: Vec<(ChunkCoord, Vec<(u16, VoxelKind)>)>,
    inventory: [u16; 4],
}

pub fn load_saved_world(
    mut terrain: ResMut<TerrainWorld>,
    mut inventory: ResMut<Inventory>,
    store: Res<SaveStore>,
) {
    match store.load() {
        Ok(Some(save)) => {
            terrain.restore_saved_world(
                save.seed,
                save.voxel_size,
                save.base_radius,
                save.chunks,
            );
            inventory.regolith = save.inventory[0].min(99);
            inventory.rock = save.inventory[1].min(99);
            inventory.copper = save.inventory[2].min(99);
            inventory.iron = save.inventory[3].min(99);
            info!("Loaded ASTRO-RUST world save");
        }
        Ok(None) => info!("No ASTRO-RUST save found; generating a new world"),
        Err(error) => warn!("Could not load ASTRO-RUST save: {error}"),
    }
}

pub fn autosave_world(
    time: Res<Time>,
    terrain: Res<TerrainWorld>,
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
    match store.save(&terrain, &inventory) {
        Ok(()) => {
            store.last_saved_revision = Some(revision);
            store.last_saved_inventory = Some(inventory_snapshot);
            info!("ASTRO-RUST world autosaved");
        }
        Err(error) => warn!("ASTRO-RUST autosave failed: {error}"),
    }
}

/// Save on the last active frame before Android/iOS backgrounds the app.
pub fn save_world_on_suspend(
    mut lifecycle: MessageReader<bevy::window::AppLifecycle>,
    terrain: Res<TerrainWorld>,
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
    match store.save(&terrain, &inventory) {
        Ok(()) => {
            store.last_saved_revision = Some(terrain.edit_revision());
            store.last_saved_inventory = Some(inventory_snapshot);
            info!("ASTRO-RUST world saved before suspend");
        }
        Err(error) => warn!("ASTRO-RUST suspend save failed: {error}"),
    }
}

pub fn save_world_on_exit(
    mut exits: MessageReader<AppExit>,
    terrain: Res<TerrainWorld>,
    inventory: Res<Inventory>,
    mut store: ResMut<SaveStore>,
) {
    if exits.read().next().is_none() {
        return;
    }
    let inventory_snapshot = inventory.snapshot();
    if store.last_saved_revision == Some(terrain.edit_revision())
        && store.last_saved_inventory == Some(inventory_snapshot)
    {
        return;
    }
    match store.save(&terrain, &inventory) {
        Ok(()) => {
            store.last_saved_revision = Some(terrain.edit_revision());
            store.last_saved_inventory = Some(inventory_snapshot);
            info!("ASTRO-RUST world saved on exit");
        }
        Err(error) => warn!("ASTRO-RUST final save failed: {error}"),
    }
}

struct ManifestData {
    seed: u64,
    voxel_size: f32,
    base_radius: f32,
    inventory: [u16; 4],
    chunk_coords: Vec<ChunkCoord>,
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
    validate_inventory(inventory)?;
    if chunks.len() > MAX_SAVED_CHUNKS {
        return Err("Save contains too many edited chunks".into());
    }
    let mut ordered: Vec<_> = chunks.iter().map(|(coord, _)| *coord).collect();
    ordered.sort_by_key(|coord| (coord.x, coord.y, coord.z));
    let mut bytes = Vec::with_capacity(40 + ordered.len() * 12 + 8);
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
    for coord in ordered {
        if !coord.is_representable() || !unique.insert(coord) {
            return Err("Manifest contains an invalid or duplicate chunk coordinate".into());
        }
        push_i32(&mut bytes, coord.x);
        push_i32(&mut bytes, coord.y);
        push_i32(&mut bytes, coord.z);
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
    let expected = cursor.checked_add(count.checked_mul(12).ok_or("Manifest size overflow")?)
        .ok_or("Manifest size overflow")?;
    if expected != payload_end {
        return Err("World manifest length does not match its chunk index".into());
    }
    let mut chunk_coords = Vec::with_capacity(count);
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
        chunk_coords.push(coord);
    }
    Ok(ManifestData { seed, voxel_size, base_radius, inventory, chunk_coords })
}

fn decode_chunk(bytes: &[u8], expected_coord: ChunkCoord) -> Result<Vec<(u16, VoxelKind)>, String> {
    if bytes.len() < 34 || bytes.len() as u64 > MAX_CHUNK_FILE_BYTES
        || &bytes[..8] != CHUNK_MAGIC
    {
        return Err("Chunk file header is missing or invalid".into());
    }
    let payload_end = bytes.len() - 8;
    if checksum64(&bytes[..payload_end]) != read_u64(bytes, payload_end)? {
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
        assert_eq!(decoded_manifest.chunk_coords, vec![coord]);

        let chunk_file = encode_chunk(coord, &edits).unwrap();
        assert_eq!(decode_chunk(&chunk_file, coord).unwrap(), edits);
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
        assert_eq!(decode_chunk(&first_bytes, first).unwrap(), vec![(3, VoxelKind::Air)]);
        assert!(decode_chunk(&second_bytes, second).is_err());
    }

    #[test]
    fn chunk_file_rejects_manifest_coordinate_mismatch() {
        let stored = ChunkCoord { x: 2, y: 0, z: 0 };
        let expected = ChunkCoord { x: 1, y: 0, z: 0 };
        let bytes = encode_chunk(stored, &[(0, VoxelKind::Rock)]).unwrap();
        assert!(decode_chunk(&bytes, expected).is_err());
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
