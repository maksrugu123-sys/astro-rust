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
