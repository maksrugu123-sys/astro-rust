//! Chunked voxel data and spherical terrain queries for ASTRO-RUST.
//!
//! This module deliberately keeps world storage independent from rendering.
//! Chunk meshing and asynchronous streaming can consume this stable data API.

use std::collections::HashMap;

use bevy::prelude::*;

pub const CHUNK_SIZE: i32 = 16;
const CHUNK_VOLUME: usize = (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize;
const SURFACE_NOISE_VOXELS: f32 = 1.35;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum VoxelKind {
    #[default]
    Air = 0,
    Regolith = 1,
    Rock = 2,
}

impl VoxelKind {
    pub fn is_solid(self) -> bool {
        !matches!(self, Self::Air)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkCoord {
    pub fn from_voxel(position: IVec3) -> Self {
        Self {
            x: position.x.div_euclid(CHUNK_SIZE),
            y: position.y.div_euclid(CHUNK_SIZE),
            z: position.z.div_euclid(CHUNK_SIZE),
        }
    }

    fn origin(self) -> IVec3 {
        IVec3::new(self.x, self.y, self.z) * CHUNK_SIZE
    }
}

#[derive(Clone, Debug)]
pub struct VoxelChunk {
    blocks: Vec<VoxelKind>,
    /// True when at least one block has been changed by gameplay.
    pub edited: bool,
}

impl Default for VoxelChunk {
    fn default() -> Self {
        Self {
            blocks: vec![VoxelKind::Air; CHUNK_VOLUME],
            edited: false,
        }
    }
}

impl VoxelChunk {
    fn index(local: IVec3) -> usize {
        (local.x + CHUNK_SIZE * (local.y + CHUNK_SIZE * local.z)) as usize
    }

    pub fn get(&self, local: IVec3) -> Option<VoxelKind> {
        if !Self::contains(local) {
            return None;
        }
        self.blocks.get(Self::index(local)).copied()
    }

    pub fn set(&mut self, local: IVec3, kind: VoxelKind) -> bool {
        if !Self::contains(local) {
            return false;
        }
        let index = Self::index(local);
        if self.blocks[index] == kind {
            return false;
        }
        self.blocks[index] = kind;
        self.edited = true;
        true
    }

    fn contains(local: IVec3) -> bool {
        (0..CHUNK_SIZE).contains(&local.x)
            && (0..CHUNK_SIZE).contains(&local.y)
            && (0..CHUNK_SIZE).contains(&local.z)
    }

    pub fn solid_count(&self) -> usize {
        self.blocks.iter().filter(|block| block.is_solid()).count()
    }
}

#[derive(Resource, Debug)]
pub struct TerrainWorld {
    chunks: HashMap<ChunkCoord, VoxelChunk>,
    pub seed: u64,
    pub voxel_size: f32,
    pub base_radius: f32,
}

impl Default for TerrainWorld {
    fn default() -> Self {
        Self::new(0xA57_2026, 0.25, 3.0)
    }
}

impl TerrainWorld {
    pub fn new(seed: u64, voxel_size: f32, base_radius: f32) -> Self {
        Self {
            chunks: HashMap::new(),
            seed,
            voxel_size: voxel_size.max(0.01),
            base_radius: base_radius.max(0.1),
        }
    }

    pub fn loaded_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn chunk(&self, coord: ChunkCoord) -> Option<&VoxelChunk> {
        self.chunks.get(&coord)
    }

    pub fn ensure_chunk(&mut self, coord: ChunkCoord) {
        if self.chunks.contains_key(&coord) {
            return;
        }

        let mut chunk = VoxelChunk::default();
        let origin = coord.origin();

        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let voxel = origin + IVec3::new(x, y, z);
                    let center = (voxel.as_vec3() + Vec3::splat(0.5)) * self.voxel_size;
                    let distance = center.length();
                    let noise = self.surface_noise(voxel) * SURFACE_NOISE_VOXELS * self.voxel_size;
                    let surface_radius = self.base_radius + noise;

                    if distance <= surface_radius {
                        let depth = surface_radius - distance;
                        let kind = if depth > self.voxel_size * 2.5 {
                            VoxelKind::Rock
                        } else {
                            VoxelKind::Regolith
                        };
                        let _ = chunk.set(IVec3::new(x, y, z), kind);
                    }
                }
            }
        }

        // Procedural generation is not a gameplay edit.
        chunk.edited = false;
        self.chunks.insert(coord, chunk);
    }

    pub fn get_block(&self, world_voxel: IVec3) -> Option<VoxelKind> {
        let coord = ChunkCoord::from_voxel(world_voxel);
        let local = world_voxel - coord.origin();
        self.chunks.get(&coord)?.get(local)
    }

    pub fn set_block(&mut self, world_voxel: IVec3, kind: VoxelKind) -> bool {
        let coord = ChunkCoord::from_voxel(world_voxel);
        self.ensure_chunk(coord);
        let local = world_voxel - coord.origin();
        self.chunks
            .get_mut(&coord)
            .is_some_and(|chunk| chunk.set(local, kind))
    }

    /// Removes solid voxels whose centers fall inside the requested world-space sphere.
    /// Returns the number of blocks actually removed.
    pub fn dig_sphere(&mut self, center: Vec3, radius: f32) -> usize {
        if !center.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return 0;
        }

        let min = ((center - Vec3::splat(radius)) / self.voxel_size).floor().as_ivec3();
        let max = ((center + Vec3::splat(radius)) / self.voxel_size).floor().as_ivec3();
        let mut removed = 0;

        for z in min.z..=max.z {
            for y in min.y..=max.y {
                for x in min.x..=max.x {
                    let voxel = IVec3::new(x, y, z);
                    let voxel_center = (voxel.as_vec3() + Vec3::splat(0.5)) * self.voxel_size;
                    if voxel_center.distance_squared(center) > radius * radius {
                        continue;
                    }

                    if self.get_block(voxel).is_some_and(VoxelKind::is_solid)
                        && self.set_block(voxel, VoxelKind::Air)
                    {
                        removed += 1;
                    }
                }
            }
        }

        removed
    }

    /// Finds the outer solid boundary along a ray from the planet center.
    /// Returns None if the ray does not intersect loaded solid terrain.
    pub fn surface_radius(&self, direction: Vec3) -> Option<f32> {
        let direction = direction.try_normalize()?;
        let max_radius = self.base_radius + self.voxel_size * 8.0;
        let step = self.voxel_size * 0.5;
        let mut last_solid = None;
        let mut radius = 0.0;

        while radius <= max_radius {
            let world_position = direction * radius;
            let voxel = (world_position / self.voxel_size).floor().as_ivec3();
            match self.get_block(voxel) {
                Some(kind) if kind.is_solid() => last_solid = Some(radius),
                Some(_) if last_solid.is_some() => return last_solid,
                None => {}
                _ => {}
            }
            radius += step;
        }

        last_solid
    }

    /// Tests a spherical body against the generated voxel surface.
    /// The caller supplies the body's center and collision radius in world units.
    pub fn sphere_touches_ground(&self, center: Vec3, body_radius: f32) -> bool {
        if !center.is_finite() || !body_radius.is_finite() || body_radius < 0.0 {
            return false;
        }
        let Some(surface) = self.surface_radius(center) else {
            return false;
        };
        center.length() - body_radius <= surface
    }

    fn surface_noise(&self, voxel: IVec3) -> f32 {
        let mut value = self.seed
            ^ (voxel.x as i64 as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
            ^ (voxel.y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (voxel.z as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        value ^= value >> 30;
        value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value ^= value >> 27;
        value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^= value >> 31;
        ((value >> 40) as f32 / ((1_u32 << 24) - 1) as f32) * 2.0 - 1.0
    }
}

/// Preloads the eight chunks whose bounds surround the planet origin.
pub fn generate_starting_chunks(mut terrain: ResMut<TerrainWorld>) {
    for z in -1..=0 {
        for y in -1..=0 {
            for x in -1..=0 {
                terrain.ensure_chunk(ChunkCoord { x, y, z });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_voxel_coordinates_map_to_negative_chunks() {
        let coord = ChunkCoord::from_voxel(IVec3::new(-1, -16, -17));
        assert_eq!(coord, ChunkCoord { x: -1, y: -1, z: -2 });
    }

    #[test]
    fn chunk_indexing_keeps_voxels_independent() {
        let mut chunk = VoxelChunk::default();
        assert!(chunk.set(IVec3::new(1, 2, 3), VoxelKind::Rock));
        assert_eq!(chunk.get(IVec3::new(1, 2, 3)), Some(VoxelKind::Rock));
        assert_eq!(chunk.get(IVec3::new(1, 2, 4)), Some(VoxelKind::Air));
    }

    #[test]
    fn generated_chunks_are_deterministic_for_a_seed() {
        let mut first = TerrainWorld::new(42, 0.25, 3.0);
        let mut second = TerrainWorld::new(42, 0.25, 3.0);
        let coord = ChunkCoord { x: 0, y: 0, z: 0 };
        first.ensure_chunk(coord);
        second.ensure_chunk(coord);
        assert_eq!(
            first.chunk(coord).unwrap().solid_count(),
            second.chunk(coord).unwrap().solid_count()
        );
    }

    #[test]
    fn digging_removes_voxels_and_marks_chunk_edited() {
        let mut world = TerrainWorld::new(7, 0.25, 3.0);
        let removed = world.dig_sphere(Vec3::new(0.0, 2.8, 0.0), 0.65);
        assert!(removed > 0);
        assert!(world.chunks.values().any(|chunk| chunk.edited));
    }

    #[test]
    fn placing_a_block_loads_its_chunk() {
        let mut world = TerrainWorld::new(7, 0.25, 3.0);
        let position = IVec3::new(-17, 0, 2);
        assert!(world.set_block(position, VoxelKind::Rock));
        assert_eq!(world.get_block(position), Some(VoxelKind::Rock));
    }
}
