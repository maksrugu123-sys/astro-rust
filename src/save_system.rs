//! Versioned, checksummed persistence for edited terrain chunks.
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
};

use bevy::prelude::*;

use crate::terrain::{ChunkCoord, TerrainWorld, VoxelKind};

const SAVE_FILE: &str = "world.astrosave";
const TEMP_FILE: &str = "world.astrosave.tmp";
const MAGIC: &[u8; 8] = b"ASTRSAV1";
const FORMAT_VERSION: u32 = 1;
const CHUNK_BLOCKS: usize = 16 * 16 * 16;
const MAX_SAVED_CHUNKS: usize = 8_192;
const AUTOSAVE_SECONDS: f32 = 15.0;

#[derive(Resource)]
pub struct SaveStore {
    directory: Option<PathBuf>,
    last_saved_revision: Option<u64>,
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
            timer: Timer::from_seconds(AUTOSAVE_SECONDS, TimerMode::Repeating),
        }
    }

    fn save_path(&self) -> Option<PathBuf> {
        self.directory.as_ref().map(|dir| dir.join(SAVE_FILE))
    }

    fn load(&self) -> Result<Option<SavedWorld>, String> {
        let Some(path) = self.save_path() else {
            return Err("No writable application data directory is available".into());
        };
        if !path.exists() {
            return Ok(None);
        }
        let metadata = fs::metadata(&path).map_err(|e| format!("Cannot inspect save: {e}"))?;
        let max_size = 32 + MAX_SAVED_CHUNKS * (12 + CHUNK_BLOCKS) + 8;
        if metadata.len() as usize > max_size {
            return Err("Save file exceeds the supported size limit".into());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        File::open(path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|e| format!("Cannot read save: {e}"))?;
        decode_save(&bytes).map(Some)
    }

    fn save(&self, terrain: &TerrainWorld) -> Result<(), String> {
        let Some(directory) = self.directory.as_ref() else {
            return Err("No writable application data directory is available".into());
        };
        let chunks = terrain.export_edited_chunks();
        if chunks.len() > MAX_SAVED_CHUNKS {
            return Err(format!(
                "World contains {} edited chunks; save limit is {}",
                chunks.len(),
                MAX_SAVED_CHUNKS
            ));
        }

        let bytes = encode_save(terrain, &chunks)?;
        fs::create_dir_all(directory)
            .map_err(|e| format!("Cannot create save directory: {e}"))?;
        let temp_path = directory.join(TEMP_FILE);
        let save_path = directory.join(SAVE_FILE);
        let mut file = File::create(&temp_path)
            .map_err(|e| format!("Cannot create temporary save: {e}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Cannot flush temporary save: {e}"))?;
        fs::rename(&temp_path, &save_path)
            .map_err(|e| format!("Cannot atomically replace save: {e}"))?;
        Ok(())
    }
}

struct SavedWorld {
    seed: u64,
    voxel_size: f32,
    base_radius: f32,
    chunks: Vec<(ChunkCoord, Vec<VoxelKind>)>,
}

pub fn load_saved_world(mut terrain: ResMut<TerrainWorld>, store: Res<SaveStore>) {
    match store.load() {
        Ok(Some(save)) => {
            terrain.restore_saved_world(
                save.seed,
                save.voxel_size,
                save.base_radius,
                save.chunks,
            );
            info!("Loaded ASTRO-RUST world save");
        }
        Ok(None) => info!("No ASTRO-RUST save found; generating a new world"),
        Err(error) => warn!("Could not load ASTRO-RUST save: {error}"),
    }
}

pub fn autosave_world(
    time: Res<Time>,
    terrain: Res<TerrainWorld>,
    mut store: ResMut<SaveStore>,
) {
    store.timer.tick(time.delta());
    let revision = terrain.edit_revision();
    if !store.timer.just_finished() || store.last_saved_revision == Some(revision) {
        return;
    }
    match store.save(&terrain) {
        Ok(()) => {
            store.last_saved_revision = Some(revision);
            info!("ASTRO-RUST world autosaved");
        }
        Err(error) => warn!("ASTRO-RUST autosave failed: {error}"),
    }
}

/// Save on the last active frame before Android/iOS backgrounds the app.
pub fn save_world_on_suspend(
    mut lifecycle: MessageReader<bevy::window::AppLifecycle>,
    terrain: Res<TerrainWorld>,
    mut store: ResMut<SaveStore>,
) {
    let should_save = lifecycle
        .read()
        .any(|state| *state == bevy::window::AppLifecycle::WillSuspend);
    if !should_save || store.last_saved_revision == Some(terrain.edit_revision()) {
        return;
    }
    match store.save(&terrain) {
        Ok(()) => {
            store.last_saved_revision = Some(terrain.edit_revision());
            info!("ASTRO-RUST world saved before suspend");
        }
        Err(error) => warn!("ASTRO-RUST suspend save failed: {error}"),
    }
}

pub fn save_world_on_exit(
    mut exits: MessageReader<AppExit>,
    terrain: Res<TerrainWorld>,
    mut store: ResMut<SaveStore>,
) {
    if exits.read().next().is_none() {
        return;
    }
    if store.last_saved_revision == Some(terrain.edit_revision()) {
        return;
    }
    match store.save(&terrain) {
        Ok(()) => {
            store.last_saved_revision = Some(terrain.edit_revision());
            info!("ASTRO-RUST world saved on exit");
        }
        Err(error) => warn!("ASTRO-RUST final save failed: {error}"),
    }
}

fn encode_save(
    terrain: &TerrainWorld,
    chunks: &[(ChunkCoord, Vec<VoxelKind>)],
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(32 + chunks.len() * (12 + CHUNK_BLOCKS) + 8);
    bytes.extend_from_slice(MAGIC);
    push_u32(&mut bytes, FORMAT_VERSION);
    push_u64(&mut bytes, terrain.seed);
    push_u32(&mut bytes, terrain.voxel_size.to_bits());
    push_u32(&mut bytes, terrain.base_radius.to_bits());
    push_u32(&mut bytes, chunks.len() as u32);

    let mut unique = HashSet::with_capacity(chunks.len());
    for (coord, blocks) in chunks {
        if blocks.len() != CHUNK_BLOCKS || !unique.insert(*coord) {
            return Err("Invalid or duplicate edited chunk in save snapshot".into());
        }
        push_i32(&mut bytes, coord.x);
        push_i32(&mut bytes, coord.y);
        push_i32(&mut bytes, coord.z);
        for block in blocks {
            bytes.push(*block as u8);
        }
    }
    let checksum = checksum64(&bytes);
    push_u64(&mut bytes, checksum);
    Ok(bytes)
}

fn decode_save(bytes: &[u8]) -> Result<SavedWorld, String> {
    if bytes.len() < 40 || &bytes[..8] != MAGIC {
        return Err("Save header is missing or invalid".into());
    }
    let payload_end = bytes.len() - 8;
    let stored_checksum = read_u64(bytes, payload_end)?;
    if checksum64(&bytes[..payload_end]) != stored_checksum {
        return Err("Save checksum mismatch; file may be damaged".into());
    }

    let mut cursor = 8;
    let version = take_u32(bytes, &mut cursor)?;
    if version != FORMAT_VERSION {
        return Err(format!("Unsupported save format version: {version}"));
    }
    let seed = take_u64(bytes, &mut cursor)?;
    let voxel_size = f32::from_bits(take_u32(bytes, &mut cursor)?);
    let base_radius = f32::from_bits(take_u32(bytes, &mut cursor)?);
    let count = take_u32(bytes, &mut cursor)? as usize;
    if !voxel_size.is_finite()
        || voxel_size <= 0.0
        || !base_radius.is_finite()
        || base_radius <= 0.0
        || count > MAX_SAVED_CHUNKS
    {
        return Err("Save metadata is outside supported bounds".into());
    }
    let expected = 32usize
        .checked_add(count.checked_mul(12 + CHUNK_BLOCKS).ok_or("Save size overflow")?)
        .and_then(|size| size.checked_add(8))
        .ok_or("Save size overflow")?;
    if bytes.len() != expected || cursor > payload_end {
        return Err("Save length does not match its chunk index".into());
    }

    let mut chunks = Vec::with_capacity(count);
    let mut unique = HashSet::with_capacity(count);
    for _ in 0..count {
        let coord = ChunkCoord {
            x: take_i32(bytes, &mut cursor)?,
            y: take_i32(bytes, &mut cursor)?,
            z: take_i32(bytes, &mut cursor)?,
        };
        if !unique.insert(coord) {
            return Err("Save contains a duplicate chunk coordinate".into());
        }
        let end = cursor.checked_add(CHUNK_BLOCKS).ok_or("Save size overflow")?;
        let raw = bytes.get(cursor..end).ok_or("Save chunk is truncated")?;
        let mut blocks = Vec::with_capacity(CHUNK_BLOCKS);
        for value in raw {
            let kind = match value {
                0 => VoxelKind::Air,
                1 => VoxelKind::Regolith,
                2 => VoxelKind::Rock,
                _ => return Err("Save contains an unknown voxel kind".into()),
            };
            blocks.push(kind);
        }
        cursor = end;
        chunks.push((coord, blocks));
    }
    if cursor != payload_end {
        return Err("Save contains unexpected trailing data".into());
    }
    Ok(SavedWorld {
        seed,
        voxel_size,
        base_radius,
        chunks,
    })
}

fn checksum64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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
fn take_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let end = cursor.checked_add(4).ok_or("Save offset overflow")?;
    let raw: [u8; 4] = bytes.get(*cursor..end).ok_or("Save is truncated")?
        .try_into().map_err(|_| "Invalid u32 field")?;
    *cursor = end;
    Ok(u32::from_le_bytes(raw))
}
fn take_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let end = cursor.checked_add(8).ok_or("Save offset overflow")?;
    let raw: [u8; 8] = bytes.get(*cursor..end).ok_or("Save is truncated")?
        .try_into().map_err(|_| "Invalid u64 field")?;
    *cursor = end;
    Ok(u64::from_le_bytes(raw))
}
fn take_i32(bytes: &[u8], cursor: &mut usize) -> Result<i32, String> {
    let end = cursor.checked_add(4).ok_or("Save offset overflow")?;
    let raw: [u8; 4] = bytes.get(*cursor..end).ok_or("Save is truncated")?
        .try_into().map_err(|_| "Invalid i32 field")?;
    *cursor = end;
    Ok(i32::from_le_bytes(raw))
}
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let raw: [u8; 8] = bytes.get(offset..offset + 8).ok_or("Save checksum is missing")?
        .try_into().map_err(|_| "Invalid checksum field")?;
    Ok(u64::from_le_bytes(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_round_trip_preserves_seed_and_voxel_edits() {
        let mut world = TerrainWorld::new(99, 0.25, 3.0);
        let coord = ChunkCoord { x: 0, y: 0, z: 0 };
        world.ensure_chunk(coord);
        world.set_block(IVec3::new(4, 4, 4), VoxelKind::Air);
        let snapshot = world.export_edited_chunks();
        let bytes = encode_save(&world, &snapshot).unwrap();
        let decoded = decode_save(&bytes).unwrap();
        assert_eq!(decoded.seed, 99);
        assert_eq!(decoded.voxel_size, 0.25);
        assert_eq!(decoded.chunks.len(), 1);
        assert_eq!(decoded.chunks[0].1[4 + 16 * (4 + 16 * 4)], VoxelKind::Air);
    }

    #[test]
    fn damaged_save_is_rejected_by_checksum() {
        let world = TerrainWorld::new(99, 0.25, 3.0);
        let mut bytes = encode_save(&world, &[]).unwrap();
        bytes[12] ^= 0x01;
        assert!(decode_save(&bytes).is_err());
    }
}
