//! First playable exploration loop: survival, crafting, deployable equipment and biomes.
//! This module intentionally owns gameplay state separately from voxel storage.

use bevy::prelude::*;

use crate::{
    player::{Inventory, PlanetPlayer},
    terrain::TerrainWorld,
};

const OXYGEN_MAX: f32 = 100.0;
const ENERGY_MAX: f32 = 100.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Biome {
    Verdant,
    Arid,
    Rocky,
    Polar,
}

impl Biome {
    fn label(self) -> &'static str {
        match self {
            Self::Verdant => "Verdant",
            Self::Arid => "Arid",
            Self::Rocky => "Rocky",
            Self::Polar => "Polar",
        }
    }
}

/// Stable biome assignment based on the planet seed and a coarse surface cell.
pub fn biome_at(seed: u64, surface_normal: Vec3) -> Biome {
    let n = surface_normal.normalize_or_zero();
    if n.length_squared() < 0.5 {
        return Biome::Rocky;
    }
    if n.y.abs() > 0.78 {
        return Biome::Polar;
    }

    let cell = IVec3::new(
        (n.x * 32.0).floor() as i32,
        (n.y * 32.0).floor() as i32,
        (n.z * 32.0).floor() as i32,
    );
    let mut h = seed
        ^ (cell.x as i64 as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
        ^ (cell.y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (cell.z as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    match h % 100 {
        0..=27 => Biome::Verdant,
        28..=52 => Biome::Arid,
        _ => Biome::Rocky,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructureKind {
    Habitat,
    SolarArray,
}

#[derive(Component)]
struct DeployedStructure {
    kind: StructureKind,
}

#[derive(Resource, Debug)]
pub struct GameState {
    pub oxygen: f32,
    pub energy: f32,
    pub health: f32,
    pub oxygen_canisters: u16,
    pub habitat_count: u16,
    pub solar_count: u16,
    pub biome: Biome,
    pub elapsed_seconds: f64,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            oxygen: OXYGEN_MAX,
            energy: ENERGY_MAX,
            health: 100.0,
            oxygen_canisters: 0,
            habitat_count: 0,
            solar_count: 0,
            biome: Biome::Rocky,
            elapsed_seconds: 0.0,
        }
    }
}

#[derive(Component, Clone, Copy)]
enum Action {
    CraftOxygen,
    UseOxygen,
    BuildHabitat,
    BuildSolar,
}

#[derive(Component)]
struct StatusHud;

pub fn setup_gameplay_ui(mut commands: Commands) {
    commands.spawn((
        StatusHud,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(16.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.015, 0.035, 0.065, 0.72)),
        Text::new("O2 100  ENERGY 100  HEALTH 100\nBIOME ROCKY"),
        TextFont { font_size: FontSize::Px(15.0), ..default() },
        TextColor(Color::srgb(0.78, 0.94, 1.0)),
        Name::new("Survival HUD"),
    ));

    let buttons = [
        (Action::CraftOxygen, "CRAFT O2", Color::srgb(0.12, 0.42, 0.58)),
        (Action::UseOxygen, "USE O2", Color::srgb(0.12, 0.52, 0.42)),
        (Action::BuildHabitat, "HABITAT", Color::srgb(0.48, 0.34, 0.18)),
        (Action::BuildSolar, "SOLAR", Color::srgb(0.42, 0.38, 0.12)),
    ];

    for (action, label, color) in buttons {
        commands.spawn((
            Button,
            action,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0 + (buttons_index(label) as f32) * 116.0),
                top: Val::Px(196.0),
                width: Val::Px(104.0),
                height: Val::Px(48.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(9.0)),
                ..default()
            },
            BackgroundColor(color.with_alpha(0.88)),
            BorderColor::all(Color::srgba(0.82, 0.92, 1.0, 0.75)),
            children![(
                Text::new(label),
                TextFont { font_size: FontSize::Px(13.0), ..default() },
                TextColor(Color::WHITE),
            )],
            Name::new(format!("Action {label}")),
        ));
    }
}

fn buttons_index(label: &str) -> usize {
    match label {
        "CRAFT O2" => 3,
        "USE O2" => 2,
        "HABITAT" => 1,
        _ => 0,
    }
}

fn can_pay(inventory: &Inventory, regolith: u16, rock: u16, copper: u16, iron: u16) -> bool {
    inventory.regolith >= regolith
        && inventory.rock >= rock
        && inventory.copper >= copper
        && inventory.iron >= iron
}

fn pay(inventory: &mut Inventory, regolith: u16, rock: u16, copper: u16, iron: u16) {
    inventory.regolith -= regolith;
    inventory.rock -= rock;
    inventory.copper -= copper;
    inventory.iron -= iron;
}

fn deploy_structure(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    player_position: Vec3,
    kind: StructureKind,
) {
    let up = player_position.normalize_or_zero();
    let forward = Vec3::Z - up * Vec3::Z.dot(up);
    let forward = forward.try_normalize().unwrap_or(Vec3::X);
    let right = up.cross(forward).normalize_or_zero();
    let position = player_position + forward * 2.8 + up * 0.25;

    match kind {
        StructureKind::Habitat => {
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(2.4, 1.8, 2.4))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.72, 0.78, 0.82),
                    metallic: 0.48,
                    perceptual_roughness: 0.36,
                    ..default()
                })),
                Transform::from_translation(position).with_rotation(Quat::from_rotation_arc(Vec3::Y, up)),
                DeployedStructure { kind },
                Name::new("Deployable Habitat"),
            ));
        }
        StructureKind::SolarArray => {
            let rotation = Quat::from_rotation_arc(Vec3::Y, up);
            let base = position + right * 1.6;
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(2.2, 0.12, 1.5))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.08, 0.22, 0.62),
                    metallic: 0.72,
                    perceptual_roughness: 0.24,
                    ..default()
                })),
                Transform::from_translation(base + up * 0.9)
                    .with_rotation(rotation * Quat::from_rotation_x(-0.28)),
                DeployedStructure { kind },
                Name::new("Solar Array"),
            ));
        }
    }
}

pub fn handle_actions(
    mut commands: Commands,
    mut interactions: Query<(&Interaction, &Action), (Changed<Interaction>, With<Button>)>,
    mut inventory: ResMut<Inventory>,
    mut state: ResMut<GameState>,
    player: Query<&Transform, With<PlanetPlayer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (interaction, action) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            Action::CraftOxygen => {
                // 2 copper + 1 iron -> one refill canister.
                if can_pay(&inventory, 0, 0, 2, 1) && state.oxygen_canisters < 20 {
                    pay(&mut inventory, 0, 0, 2, 1);
                    state.oxygen_canisters += 1;
                }
            }
            Action::UseOxygen => {
                if state.oxygen_canisters > 0 && state.oxygen < OXYGEN_MAX {
                    state.oxygen_canisters -= 1;
                    state.oxygen = (state.oxygen + 65.0).min(OXYGEN_MAX);
                }
            }
            Action::BuildHabitat => {
                // Habitat is an oxygen shelter; the player must gather materials first.
                if can_pay(&inventory, 12, 0, 8, 4) && state.habitat_count < 16 {
                    if let Ok(transform) = player.single() {
                        pay(&mut inventory, 12, 0, 8, 4);
                        deploy_structure(&mut commands, &mut meshes, &mut materials,
                            transform.translation, StructureKind::Habitat);
                        state.habitat_count += 1;
                    }
                }
            }
            Action::BuildSolar => {
                // Solar arrays recharge the suit when the player is nearby.
                if can_pay(&inventory, 0, 4, 8, 6) && state.solar_count < 24 {
                    if let Ok(transform) = player.single() {
                        pay(&mut inventory, 0, 4, 8, 6);
                        deploy_structure(&mut commands, &mut meshes, &mut materials,
                            transform.translation, StructureKind::SolarArray);
                        state.solar_count += 1;
                    }
                }
            }
        }
    }
}

pub fn survival_tick(
    time: Res<Time>,
    terrain: Res<TerrainWorld>,
    mut state: ResMut<GameState>,
    player: Query<&Transform, With<PlanetPlayer>>,
    structures: Query<(&Transform, &DeployedStructure)>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let dt = time.delta_secs().clamp(0.0, 0.1);
    if dt <= 0.0 {
        return;
    }

    state.elapsed_seconds += dt as f64;
    let up = player.translation.normalize_or_zero();
    state.biome = biome_at(terrain.seed, up);

    let mut in_habitat = false;
    let mut near_solar = false;
    for (transform, structure) in &structures {
        let distance = transform.translation.distance(player.translation);
        match structure.kind {
            StructureKind::Habitat if distance <= 8.0 => in_habitat = true,
            StructureKind::SolarArray if distance <= 10.0 => near_solar = true,
            _ => {}
        }
    }

    if in_habitat {
        state.oxygen = (state.oxygen + 12.0 * dt).min(OXYGEN_MAX);
    } else {
        state.oxygen = (state.oxygen - 1.8 * dt).max(0.0);
    }

    if near_solar {
        state.energy = (state.energy + 8.0 * dt).min(ENERGY_MAX);
    } else {
        state.energy = (state.energy - 0.12 * dt).max(0.0);
    }

    if state.oxygen <= 0.0 {
        state.health = (state.health - 5.0 * dt).max(0.0);
    } else {
        state.health = (state.health + 0.15 * dt).min(100.0);
    }
}

pub fn update_status_hud(
    state: Res<GameState>,
    mut hud: Query<&mut Text, With<StatusHud>>,
) {
    if !state.is_changed() {
        return;
    }
    for mut text in &mut hud {
        *text = Text::new(format!(
            "O2 {:03.0}  ENERGY {:03.0}  HEALTH {:03.0}\nBIOME {}  O2 CANISTERS {}\nHABITATS {}  SOLAR {}",
            state.oxygen,
            state.energy,
            state.health,
            state.biome.label(),
            state.oxygen_canisters,
            state.habitat_count,
            state.solar_count,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biome_assignment_is_seeded_and_polar_caps_are_stable() {
        assert_eq!(biome_at(7, Vec3::Y), Biome::Polar);
        assert_eq!(biome_at(7, Vec3::Y), Biome::Polar);
        assert_eq!(biome_at(7, Vec3::ZERO), Biome::Rocky);
    }

    #[test]
    fn crafting_payment_is_exact_and_does_not_overdraw() {
        let mut inventory = Inventory { copper: 2, iron: 1, ..default() };
        assert!(can_pay(&inventory, 0, 0, 2, 1));
        pay(&mut inventory, 0, 0, 2, 1);
        assert_eq!((inventory.copper, inventory.iron), (0, 0));
        assert!(!can_pay(&inventory, 0, 0, 2, 1));
    }

    #[test]
    fn survival_values_are_clamped_to_valid_ranges() {
        let oxygen = (98.0_f32 + 12.0).min(OXYGEN_MAX);
        let energy = (2.0_f32 - 8.0).max(0.0);
        assert_eq!(oxygen, 100.0);
        assert_eq!(energy, 0.0);
    }
}


// ---- Durable gameplay state -------------------------------------------------
// Kept in a small sidecar so the sparse voxel-save format remains independent.
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
};

const GAMEPLAY_SAVE_MAGIC: &[u8; 8] = b"ASTROGP1";
const GAMEPLAY_SAVE_VERSION: u32 = 1;
const MAX_SAVED_STRUCTURES: usize = 40;
const GAMEPLAY_AUTOSAVE_SECONDS: f32 = 10.0;

#[derive(Resource)]
pub struct GameplaySaveStore {
    path: Option<PathBuf>,
    timer: Timer,
    last_bytes: Option<Vec<u8>>,
    load_failed: bool,
}

impl GameplaySaveStore {
    pub fn discover() -> Self {
        #[cfg(target_os = "android")]
        let base = bevy::android::ANDROID_APP
            .get()
            .and_then(|app| app.internal_data_path());

        #[cfg(not(target_os = "android"))]
        let base = std::env::current_dir().ok();

        Self {
            path: base.map(|dir| {
                let directory = if cfg!(target_os = "android") {
                    dir.join("astro-rust")
                } else {
                    dir.join("saves")
                };
                directory.join("gameplay.dat")
            }),
            timer: Timer::from_seconds(GAMEPLAY_AUTOSAVE_SECONDS, TimerMode::Repeating),
            last_bytes: None,
            load_failed: false,
        }
    }
}

fn gameplay_checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn encode_gameplay_save(
    state: &GameState,
    structures: &[(StructureKind, Vec3)],
) -> Result<Vec<u8>, String> {
    if structures.len() > MAX_SAVED_STRUCTURES {
        return Err("Too many deployed structures to save".into());
    }
    let values = [state.oxygen, state.energy, state.health];
    if values.iter().any(|v| !v.is_finite() || !(0.0..=100.0).contains(v))
        || !state.elapsed_seconds.is_finite()
        || state.elapsed_seconds < 0.0
    {
        return Err("Gameplay state contains invalid survival values".into());
    }

    let mut bytes = Vec::with_capacity(64 + structures.len() * 13);
    bytes.extend_from_slice(GAMEPLAY_SAVE_MAGIC);
    bytes.extend_from_slice(&GAMEPLAY_SAVE_VERSION.to_le_bytes());
    for value in values {
        bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    bytes.extend_from_slice(&state.oxygen_canisters.to_le_bytes());
    bytes.push(match state.biome {
        Biome::Verdant => 0,
        Biome::Arid => 1,
        Biome::Rocky => 2,
        Biome::Polar => 3,
    });
    bytes.extend_from_slice(&state.elapsed_seconds.to_bits().to_le_bytes());
    bytes.extend_from_slice(&(structures.len() as u16).to_le_bytes());

    for (kind, position) in structures {
        if !position.is_finite() || position.length_squared() > 1.0e12 {
            return Err("A deployed structure has an invalid position".into());
        }
        bytes.push(match kind {
            StructureKind::Habitat => 0,
            StructureKind::SolarArray => 1,
        });
        for component in position.to_array() {
            bytes.extend_from_slice(&component.to_bits().to_le_bytes());
        }
    }

    let checksum = gameplay_checksum(&bytes);
    bytes.extend_from_slice(&checksum.to_le_bytes());
    Ok(bytes)
}

fn take_gameplay<const N: usize>(bytes: &[u8], cursor: &mut usize) -> Result<[u8; N], String> {
    let end = cursor.checked_add(N).ok_or("Gameplay save offset overflow")?;
    let slice = bytes.get(*cursor..end).ok_or("Gameplay save is truncated")?;
    let mut result = [0_u8; N];
    result.copy_from_slice(slice);
    *cursor = end;
    Ok(result)
}

fn decode_gameplay_save(
    bytes: &[u8],
) -> Result<(GameState, Vec<(StructureKind, Vec3)>), String> {
    if bytes.len() < 8 + 4 + 12 + 2 + 1 + 8 + 2 + 8
        || bytes.len() > 8 + 4 + 12 + 2 + 1 + 8 + 2 + MAX_SAVED_STRUCTURES * 13 + 8
        || &bytes[..8] != GAMEPLAY_SAVE_MAGIC
    {
        return Err("Gameplay save header or size is invalid".into());
    }
    let payload_end = bytes.len() - 8;
    let mut checksum_bytes = [0_u8; 8];
    checksum_bytes.copy_from_slice(&bytes[payload_end..]);
    if gameplay_checksum(&bytes[..payload_end]) != u64::from_le_bytes(checksum_bytes) {
        return Err("Gameplay save checksum mismatch".into());
    }

    let mut cursor = 8;
    let version = u32::from_le_bytes(take_gameplay::<4>(bytes, &mut cursor)?);
    if version != GAMEPLAY_SAVE_VERSION {
        return Err(format!("Unsupported gameplay save version: {version}"));
    }
    let oxygen = f32::from_bits(u32::from_le_bytes(take_gameplay::<4>(bytes, &mut cursor)?));
    let energy = f32::from_bits(u32::from_le_bytes(take_gameplay::<4>(bytes, &mut cursor)?));
    let health = f32::from_bits(u32::from_le_bytes(take_gameplay::<4>(bytes, &mut cursor)?));
    if [oxygen, energy, health].iter().any(|v| !v.is_finite() || !(0.0..=100.0).contains(v)) {
        return Err("Gameplay save survival values are invalid".into());
    }
    let oxygen_canisters = u16::from_le_bytes(take_gameplay::<2>(bytes, &mut cursor)?);
    if oxygen_canisters > 20 {
        return Err("Gameplay save canister count exceeds its limit".into());
    }
    let biome = match take_gameplay::<1>(bytes, &mut cursor)?[0] {
        0 => Biome::Verdant,
        1 => Biome::Arid,
        2 => Biome::Rocky,
        3 => Biome::Polar,
        _ => return Err("Gameplay save biome identifier is invalid".into()),
    };
    let elapsed_seconds = f64::from_bits(u64::from_le_bytes(take_gameplay::<8>(bytes, &mut cursor)?));
    if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
        return Err("Gameplay save elapsed time is invalid".into());
    }
    let count = u16::from_le_bytes(take_gameplay::<2>(bytes, &mut cursor)?) as usize;
    if count > MAX_SAVED_STRUCTURES || cursor.checked_add(count * 13) != Some(payload_end) {
        return Err("Gameplay save structure table is invalid".into());
    }

    let mut structures = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = match take_gameplay::<1>(bytes, &mut cursor)?[0] {
            0 => StructureKind::Habitat,
            1 => StructureKind::SolarArray,
            _ => return Err("Gameplay save structure type is invalid".into()),
        };
        let x = f32::from_bits(u32::from_le_bytes(take_gameplay::<4>(bytes, &mut cursor)?));
        let y = f32::from_bits(u32::from_le_bytes(take_gameplay::<4>(bytes, &mut cursor)?));
        let z = f32::from_bits(u32::from_le_bytes(take_gameplay::<4>(bytes, &mut cursor)?));
        let position = Vec3::new(x, y, z);
        if !position.is_finite() || position.length_squared() > 1.0e12 {
            return Err("Gameplay save structure position is invalid".into());
        }
        structures.push((kind, position));
    }

    let mut state = GameState::default();
    state.oxygen = oxygen;
    state.energy = energy;
    state.health = health;
    state.oxygen_canisters = oxygen_canisters;
    state.biome = biome;
    state.elapsed_seconds = elapsed_seconds;
    state.habitat_count = structures.iter().filter(|(kind, _)| *kind == StructureKind::Habitat).count() as u16;
    state.solar_count = structures.iter().filter(|(kind, _)| *kind == StructureKind::SolarArray).count() as u16;
    Ok((state, structures))
}

fn spawn_saved_structure(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    position: Vec3,
    kind: StructureKind,
) {
    let up = position.normalize_or_zero();
    let rotation = Quat::from_rotation_arc(Vec3::Y, up);
    match kind {
        StructureKind::Habitat => {
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(2.4, 1.8, 2.4))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.72, 0.78, 0.82),
                    metallic: 0.48,
                    perceptual_roughness: 0.36,
                    ..default()
                })),
                Transform::from_translation(position).with_rotation(rotation),
                DeployedStructure { kind },
                Name::new("Restored Habitat"),
            ));
        }
        StructureKind::SolarArray => {
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(2.2, 0.12, 1.5))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.08, 0.22, 0.62),
                    metallic: 0.72,
                    perceptual_roughness: 0.24,
                    ..default()
                })),
                Transform::from_translation(position)
                    .with_rotation(rotation * Quat::from_rotation_x(-0.28)),
                DeployedStructure { kind },
                Name::new("Restored Solar Array"),
            ));
        }
    }
}

pub fn load_gameplay_save(
    mut commands: Commands,
    mut store: ResMut<GameplaySaveStore>,
    mut state: ResMut<GameState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(path) = store.path.clone() else {
        warn!("Gameplay save path is unavailable");
        store.load_failed = true;
        return;
    };
    if !path.exists() {
        return;
    }
    let result = (|| -> Result<Vec<u8>, String> {
        let metadata = fs::metadata(&path).map_err(|e| format!("Cannot inspect gameplay save: {e}"))?;
        if metadata.len() > 2048 {
            return Err("Gameplay save exceeds its size limit".into());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        File::open(&path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|e| format!("Cannot read gameplay save: {e}"))?;
        Ok(bytes)
    })();

    match result.and_then(|bytes| {
        let (loaded_state, structures) = decode_gameplay_save(&bytes)?;
        Ok((bytes, loaded_state, structures))
    }) {
        Ok((bytes, loaded_state, structures)) => {
            *state = loaded_state;
            for (kind, position) in structures {
                spawn_saved_structure(&mut commands, &mut meshes, &mut materials, position, kind);
            }
            store.last_bytes = Some(bytes);
            info!("Restored survival state and deployed equipment");
        }
        Err(error) => {
            store.load_failed = true;
            warn!("Gameplay save was rejected and will not be overwritten: {error}");
        }
    }
}

fn collect_structure_snapshot(
    structures: &Query<(&Transform, &DeployedStructure)>,
) -> Vec<(StructureKind, Vec3)> {
    let mut snapshot: Vec<_> = structures
        .iter()
        .map(|(transform, structure)| (structure.kind, transform.translation))
        .collect();
    snapshot.sort_by(|a, b| {
        let ka = match a.0 { StructureKind::Habitat => 0_u8, StructureKind::SolarArray => 1_u8 };
        let kb = match b.0 { StructureKind::Habitat => 0_u8, StructureKind::SolarArray => 1_u8 };
        ka.cmp(&kb)
            .then_with(|| a.1.x.total_cmp(&b.1.x))
            .then_with(|| a.1.y.total_cmp(&b.1.y))
            .then_with(|| a.1.z.total_cmp(&b.1.z))
    });
    snapshot
}

fn persist_gameplay_snapshot(
    state: &GameState,
    structures: &Query<(&Transform, &DeployedStructure)>,
    store: &mut GameplaySaveStore,
) {
    if store.load_failed {
        return;
    }
    let bytes = match encode_gameplay_save(state, &collect_structure_snapshot(structures)) {
        Ok(bytes) => bytes,
        Err(error) => {
            warn!("Gameplay save snapshot rejected: {error}");
            return;
        }
    };
    if store.last_bytes.as_ref() == Some(&bytes) {
        return;
    }
    let Some(path) = store.path.as_ref() else {
        warn!("Gameplay save path is unavailable");
        return;
    };
    let Some(directory) = path.parent() else {
        warn!("Gameplay save directory is unavailable");
        return;
    };
    if let Err(error) = fs::create_dir_all(directory) {
        warn!("Cannot create gameplay save directory: {error}");
        return;
    }
    let temp_path = path.with_extension("dat.tmp");
    let write_result = (|| -> Result<(), String> {
        let mut file = File::create(&temp_path)
            .map_err(|e| format!("Cannot create temporary gameplay save: {e}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Cannot flush gameplay save: {e}"))?;
        fs::rename(&temp_path, path)
            .map_err(|e| format!("Cannot atomically replace gameplay save: {e}"))?;
        Ok(())
    })();
    match write_result {
        Ok(()) => store.last_bytes = Some(bytes),
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            warn!("Gameplay save failed; previous save was retained: {error}");
        }
    }
}

pub fn autosave_gameplay(
    time: Res<Time>,
    state: Res<GameState>,
    structures: Query<(&Transform, &DeployedStructure)>,
    mut store: ResMut<GameplaySaveStore>,
) {
    store.timer.tick(time.delta());
    if store.timer.just_finished() {
        persist_gameplay_snapshot(&state, &structures, &mut store);
    }
}

pub fn save_gameplay_on_suspend(
    mut lifecycle: MessageReader<bevy::window::AppLifecycle>,
    state: Res<GameState>,
    structures: Query<(&Transform, &DeployedStructure)>,
    mut store: ResMut<GameplaySaveStore>,
) {
    if lifecycle.read().any(|event| matches!(event, bevy::window::AppLifecycle::WillSuspend)) {
        persist_gameplay_snapshot(&state, &structures, &mut store);
    }
}

pub fn save_gameplay_on_exit(
    mut exits: MessageReader<AppExit>,
    state: Res<GameState>,
    structures: Query<(&Transform, &DeployedStructure)>,
    mut store: ResMut<GameplaySaveStore>,
) {
    if exits.read().next().is_some() {
        persist_gameplay_snapshot(&state, &structures, &mut store);
    }
}

#[cfg(test)]
mod gameplay_save_tests {
    use super::*;

    #[test]
    fn gameplay_save_roundtrip_preserves_survival_and_structures() {
        let mut state = GameState::default();
        state.oxygen = 42.5;
        state.energy = 73.0;
        state.health = 91.25;
        state.oxygen_canisters = 3;
        state.biome = Biome::Arid;
        state.elapsed_seconds = 1234.5;
        let structures = vec![
            (StructureKind::Habitat, Vec3::new(12.0, 4.0, -8.0)),
            (StructureKind::SolarArray, Vec3::new(-2.0, 9.0, 17.0)),
        ];

        let bytes = encode_gameplay_save(&state, &structures).unwrap();
        let (loaded, loaded_structures) = decode_gameplay_save(&bytes).unwrap();

        assert_eq!(loaded.oxygen, state.oxygen);
        assert_eq!(loaded.energy, state.energy);
        assert_eq!(loaded.health, state.health);
        assert_eq!(loaded.oxygen_canisters, state.oxygen_canisters);
        assert_eq!(loaded.biome, state.biome);
        assert_eq!(loaded.elapsed_seconds, state.elapsed_seconds);
        assert_eq!(loaded.habitat_count, 1);
        assert_eq!(loaded.solar_count, 1);
        assert_eq!(loaded_structures.len(), 2);
        assert_eq!(loaded_structures[0].0, StructureKind::Habitat);
        assert_eq!(loaded_structures[0].1, structures[0].1);
        assert_eq!(loaded_structures[1].0, StructureKind::SolarArray);
        assert_eq!(loaded_structures[1].1, structures[1].1);
    }

    #[test]
    fn gameplay_save_rejects_corruption_and_invalid_counts() {
        let state = GameState::default();
        let mut bytes = encode_gameplay_save(&state, &[]).unwrap();
        bytes[12] ^= 0x40;
        assert!(decode_gameplay_save(&bytes).is_err());

        let too_many = vec![(StructureKind::Habitat, Vec3::ZERO); MAX_SAVED_STRUCTURES + 1];
        assert!(encode_gameplay_save(&state, &too_many).is_err());
    }

    #[test]
    fn gameplay_save_rejects_nan_and_out_of_range_survival_values() {
        let mut state = GameState::default();
        state.oxygen = f32::NAN;
        assert!(encode_gameplay_save(&state, &[]).is_err());
        state.oxygen = 101.0;
        assert!(encode_gameplay_save(&state, &[]).is_err());
    }
}
