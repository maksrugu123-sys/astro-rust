//! Chunked voxel data and spherical terrain rendering for ASTRO-RUST.

use std::collections::{HashMap, HashSet};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

pub const CHUNK_SIZE: i32 = 16;
const CHUNK_VOLUME: usize = (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize;
const SURFACE_NOISE_VOXELS: f32 = 1.35;
const STREAM_RADIUS: i32 = 1;
const MAX_LOADED_CHUNKS: usize = 40;
const MAX_CHUNK_LOADS_PER_FRAME: usize = 2;
const MAX_CHUNK_UNLOADS_PER_FRAME: usize = 4;

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
    mesh_dirty: bool,
}

impl Default for VoxelChunk {
    fn default() -> Self {
        Self {
            blocks: vec![VoxelKind::Air; CHUNK_VOLUME],
            edited: false,
            mesh_dirty: true,
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
        self.mesh_dirty = true;
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
    edited_cache: HashMap<ChunkCoord, Vec<VoxelKind>>,
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
            edited_cache: HashMap::new(),
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

    pub fn contains_chunk(&self, coord: ChunkCoord) -> bool {
        self.chunks.contains_key(&coord)
    }

    /// Unloads a chunk while retaining player edits for a later reload.
    pub fn unload_chunk(&mut self, coord: ChunkCoord) -> bool {
        let Some(chunk) = self.chunks.remove(&coord) else {
            return false;
        };
        if chunk.edited {
            self.edited_cache.insert(coord, chunk.blocks);
        }
        for neighbor in [
            ChunkCoord { x: coord.x - 1, y: coord.y, z: coord.z },
            ChunkCoord { x: coord.x + 1, y: coord.y, z: coord.z },
            ChunkCoord { x: coord.x, y: coord.y - 1, z: coord.z },
            ChunkCoord { x: coord.x, y: coord.y + 1, z: coord.z },
            ChunkCoord { x: coord.x, y: coord.y, z: coord.z - 1 },
            ChunkCoord { x: coord.x, y: coord.y, z: coord.z + 1 },
        ] {
            if let Some(chunk) = self.chunks.get_mut(&neighbor) {
                chunk.mesh_dirty = true;
            }
        }
        true
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
        // Reapply player edits after deterministic regeneration.
        if let Some(saved_blocks) = self.edited_cache.remove(&coord) {
            if saved_blocks.len() == CHUNK_VOLUME {
                chunk.blocks = saved_blocks;
                chunk.edited = true;
            }
        }
        // Initial generation is not a gameplay edit.
        chunk.mesh_dirty = true;
        self.chunks.insert(coord, chunk);

        for neighbor in [
            ChunkCoord { x: coord.x - 1, y: coord.y, z: coord.z },
            ChunkCoord { x: coord.x + 1, y: coord.y, z: coord.z },
            ChunkCoord { x: coord.x, y: coord.y - 1, z: coord.z },
            ChunkCoord { x: coord.x, y: coord.y + 1, z: coord.z },
            ChunkCoord { x: coord.x, y: coord.y, z: coord.z - 1 },
            ChunkCoord { x: coord.x, y: coord.y, z: coord.z + 1 },
        ] {
            if let Some(chunk) = self.chunks.get_mut(&neighbor) {
                chunk.mesh_dirty = true;
            }
        }
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
        let changed = self.chunks
            .get_mut(&coord)
            .is_some_and(|chunk| chunk.set(local, kind));

        if changed {
            for (axis, value) in [(0, local.x), (1, local.y), (2, local.z)] {
                if value == 0 || value == CHUNK_SIZE - 1 {
                    let mut neighbor = coord;
                    let delta = if value == 0 { -1 } else { 1 };
                    match axis {
                        0 => neighbor.x += delta,
                        1 => neighbor.y += delta,
                        _ => neighbor.z += delta,
                    }
                    if let Some(chunk) = self.chunks.get_mut(&neighbor) {
                        chunk.mesh_dirty = true;
                    }
                }
            }
        }

        changed
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

    /// Finds the first solid voxel along a world-space ray using 3D DDA traversal.
    /// Returns the voxel coordinate and the exact axis-aligned entry-face normal.
    pub fn raycast_solid(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(IVec3, IVec3)> {
        if !origin.is_finite() || !max_distance.is_finite() || max_distance <= 0.0 {
            return None;
        }
        let direction = direction.try_normalize()?;
        let size = self.voxel_size;
        let start = (origin / size).floor().as_ivec3();
        let mut cell = [start.x, start.y, start.z];
        let dir = [direction.x, direction.y, direction.z];
        let origin_axis = [origin.x, origin.y, origin.z];
        let mut step = [0_i32; 3];
        let mut t_max = [f32::INFINITY; 3];
        let mut t_delta = [f32::INFINITY; 3];

        for axis in 0..3 {
            if dir[axis] > 0.0 {
                step[axis] = 1;
                let boundary = (cell[axis] + 1) as f32 * size;
                t_max[axis] = (boundary - origin_axis[axis]) / dir[axis];
                t_delta[axis] = size / dir[axis];
            } else if dir[axis] < 0.0 {
                step[axis] = -1;
                let boundary = cell[axis] as f32 * size;
                t_max[axis] = (boundary - origin_axis[axis]) / dir[axis];
                t_delta[axis] = -size / dir[axis];
            }
        }

        let mut distance = 0.0;
        let mut entry_normal = IVec3::ZERO;
        loop {
            let voxel = IVec3::new(cell[0], cell[1], cell[2]);
            if self.get_block(voxel).is_some_and(VoxelKind::is_solid) {
                if entry_normal == IVec3::ZERO {
                    let axis = if direction.x.abs() >= direction.y.abs()
                        && direction.x.abs() >= direction.z.abs()
                    {
                        0
                    } else if direction.y.abs() >= direction.z.abs() {
                        1
                    } else {
                        2
                    };
                    let mut fallback = IVec3::ZERO;
                    match axis {
                        0 => fallback.x = -direction.x.signum() as i32,
                        1 => fallback.y = -direction.y.signum() as i32,
                        _ => fallback.z = -direction.z.signum() as i32,
                    }
                    entry_normal = fallback;
                }
                return Some((voxel, entry_normal));
            }

            let axis = if t_max[0] <= t_max[1] && t_max[0] <= t_max[2] {
                0
            } else if t_max[1] <= t_max[2] {
                1
            } else {
                2
            };
            distance = t_max[axis];
            if !distance.is_finite() || distance > max_distance {
                return None;
            }

            cell[axis] += step[axis];
            entry_normal = IVec3::ZERO;
            match axis {
                0 => entry_normal.x = -step[axis],
                1 => entry_normal.y = -step[axis],
                _ => entry_normal.z = -step[axis],
            }
            t_max[axis] += t_delta[axis];
        }
    }

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

#[derive(Component)]
struct TerrainChunkVisual {
    coord: ChunkCoord,
    mesh: Handle<Mesh>,
}

#[derive(Resource)]
pub struct TerrainMaterial(Handle<StandardMaterial>);

/// Creates one shared material for all streamed terrain chunks.
pub fn setup_terrain_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.96,
        metallic: 0.0,
        ..default()
    });
    commands.insert_resource(TerrainMaterial(material));
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

/// Creates one exposed-face mesh per loaded chunk. Internal faces are omitted.
pub fn spawn_chunk_meshes(
    mut commands: Commands,
    mut terrain: ResMut<TerrainWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<TerrainMaterial>,
) {
    let coords: Vec<ChunkCoord> = terrain.chunks.keys().copied().collect();
    for coord in coords {
        let mesh = build_chunk_mesh(&terrain, coord);
        let handle = meshes.add(mesh);
        if let Some(chunk) = terrain.chunks.get_mut(&coord) {
            chunk.mesh_dirty = false;
        }

        commands.spawn((
            Mesh3d(handle.clone()),
            MeshMaterial3d(material.0.clone()),
            Transform::IDENTITY,
            TerrainChunkVisual { coord, mesh: handle },
            Name::new(format!("Terrain Chunk {},{},{}", coord.x, coord.y, coord.z)),
        ));
    }
}

/// Maintains a 3x3x3 chunk neighborhood around the player. Loading and unloading
/// are budgeted per frame to avoid a large one-frame generation/mesh spike.
pub fn stream_terrain_chunks(
    mut commands: Commands,
    player_query: Query<&Transform, With<crate::player::PlanetPlayer>>,
    mut terrain: ResMut<TerrainWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<TerrainMaterial>,
    visuals: Query<(Entity, &TerrainChunkVisual)>,
) {
    let Ok(player) = player_query.single() else {
        return;
    };
    let player_voxel = (player.translation / terrain.voxel_size).floor().as_ivec3();
    let center = ChunkCoord::from_voxel(player_voxel);
    let mut desired = HashSet::new();
    for z in -STREAM_RADIUS..=STREAM_RADIUS {
        for y in -STREAM_RADIUS..=STREAM_RADIUS {
            for x in -STREAM_RADIUS..=STREAM_RADIUS {
                desired.insert(ChunkCoord {
                    x: center.x + x,
                    y: center.y + y,
                    z: center.z + z,
                });
            }
        }
    }

    let stale: Vec<(Entity, ChunkCoord)> = visuals
        .iter()
        .filter(|(_, visual)| !desired.contains(&visual.coord))
        .map(|(entity, visual)| (entity, visual.coord))
        .take(MAX_CHUNK_UNLOADS_PER_FRAME)
        .collect();
    for (entity, coord) in stale {
        commands.entity(entity).despawn();
        terrain.unload_chunk(coord);
    }

    let mut load_order: Vec<ChunkCoord> = desired.iter().copied().collect();
    load_order.sort_by_key(|coord| {
        let dx = (coord.x - center.x) as i64;
        let dy = (coord.y - center.y) as i64;
        let dz = (coord.z - center.z) as i64;
        dx * dx + dy * dy + dz * dz
    });

    let mut loaded_this_frame = 0;
    for coord in load_order {
        if loaded_this_frame >= MAX_CHUNK_LOADS_PER_FRAME
            || terrain.loaded_chunk_count() >= MAX_LOADED_CHUNKS
        {
            break;
        }
        if terrain.contains_chunk(coord) {
            continue;
        }

        terrain.ensure_chunk(coord);
        let mesh = build_chunk_mesh(&terrain, coord);
        let handle = meshes.add(mesh);
        if let Some(chunk) = terrain.chunks.get_mut(&coord) {
            chunk.mesh_dirty = false;
        }
        commands.spawn((
            Mesh3d(handle.clone()),
            MeshMaterial3d(material.0.clone()),
            Transform::IDENTITY,
            TerrainChunkVisual { coord, mesh: handle },
            Name::new(format!("Terrain Chunk {},{},{}", coord.x, coord.y, coord.z)),
        ));
        loaded_this_frame += 1;
    }
}

/// Rebuilds only meshes affected by block edits.
pub fn refresh_dirty_chunk_meshes(
    mut terrain: ResMut<TerrainWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    query: Query<&TerrainChunkVisual>,
) {
    let dirty: Vec<(ChunkCoord, Handle<Mesh>)> = query
        .iter()
        .filter_map(|visual| {
            terrain
                .chunks
                .get(&visual.coord)
                .filter(|chunk| chunk.mesh_dirty)
                .map(|_| (visual.coord, visual.mesh.clone()))
        })
        .collect();

    for (coord, handle) in dirty {
        let rebuilt = build_chunk_mesh(&terrain, coord);
        if let Some(mesh) = meshes.get_mut(&handle) {
            *mesh = rebuilt;
        }
        if let Some(chunk) = terrain.chunks.get_mut(&coord) {
            chunk.mesh_dirty = false;
        }
    }
}

fn build_chunk_mesh(world: &TerrainWorld, coord: ChunkCoord) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let origin = coord.origin();

    let faces: [(IVec3, [[i32; 3]; 4]); 6] = [
        (IVec3::X, [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]]),
        (IVec3::NEG_X, [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]]),
        (IVec3::Y, [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]]),
        (IVec3::NEG_Y, [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]),
        (IVec3::Z, [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]),
        (IVec3::NEG_Z, [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]]),
    ];

    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let local = IVec3::new(x, y, z);
                let voxel = origin + local;
                let Some(kind) = world.get_block(voxel) else {
                    continue;
                };
                if !kind.is_solid() {
                    continue;
                }

                let color = match kind {
                    VoxelKind::Regolith => [0.66, 0.48, 0.30, 1.0],
                    VoxelKind::Rock => [0.31, 0.34, 0.38, 1.0],
                    VoxelKind::Air => [0.0, 0.0, 0.0, 0.0],
                };

                for (normal, corners) in faces {
                    let neighbor = voxel + normal;
                    if world.get_block(neighbor).is_some_and(VoxelKind::is_solid) {
                        continue;
                    }

                    let base = positions.len() as u32;
                    for corner in corners {
                        let point = (voxel.as_vec3() + Vec3::from_array([
                            corner[0] as f32,
                            corner[1] as f32,
                            corner[2] as f32,
                        ])) * world.voxel_size;
                        positions.push(point.to_array());
                        normals.push(normal.as_vec3().to_array());
                        colors.push(color);
                    }
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
        }
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_indices(Indices::U32(indices))
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
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
    fn edited_chunk_restores_blocks_after_unload_and_reload() {
        let mut world = TerrainWorld::new(7, 0.25, 3.0);
        let coord = ChunkCoord { x: 0, y: 0, z: 0 };
        world.ensure_chunk(coord);
        let voxel = IVec3::new(4, 4, 4);
        assert!(world.set_block(voxel, VoxelKind::Air));
        assert!(world.unload_chunk(coord));
        assert!(!world.contains_chunk(coord));
        world.ensure_chunk(coord);
        assert_eq!(world.get_block(voxel), Some(VoxelKind::Air));
    }

    #[test]
    fn raycast_finds_a_solid_voxel_and_face() {
        let mut world = TerrainWorld::new(7, 0.25, 3.0);
        world.ensure_chunk(ChunkCoord { x: 0, y: 0, z: 0 });
        let hit = world.raycast_solid(Vec3::new(0.0, 4.0, 0.0), -Vec3::Y, 2.0);
        assert!(hit.is_some());
        let (voxel, normal) = hit.unwrap();
        assert!(world.get_block(voxel).is_some_and(VoxelKind::is_solid));
        assert_eq!(normal, IVec3::Y);
    }

    #[test]
    fn placing_a_block_loads_its_chunk() {
        let mut world = TerrainWorld::new(7, 0.25, 3.0);
        let position = IVec3::new(-17, 0, 2);
        assert!(world.set_block(position, VoxelKind::Rock));
        assert_eq!(world.get_block(position), Some(VoxelKind::Rock));
    }
}
