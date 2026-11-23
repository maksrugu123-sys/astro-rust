//! Radial-gravity player physics and mobile controls for spherical planets.

use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*, window::PrimaryWindow};

pub mod math;

use crate::{
    planet::math::surface_normal,
    terrain::{TerrainWorld, VoxelKind},
};
use math::{radial_gravity, resolve_spherical_contact, tangent_frame};

const PLAYER_SURFACE_OFFSET: f32 = 0.62;
const MOVE_ACCELERATION: f32 = 18.0;
const MAX_GROUND_SPEED: f32 = 3.0;
const MAX_AIR_SPEED: f32 = 3.4;
const GROUND_FRICTION: f32 = 7.0;
const GRAVITY_STRENGTH: f32 = 9.8;
const JUMP_SPEED: f32 = 5.2;
const JOYSTICK_RADIUS: f32 = 58.0;
const LOOK_SENSITIVITY: f32 = 0.005;
const CAMERA_DISTANCE: f32 = 8.5;
const MAX_PHYSICS_STEP: f32 = 0.05;

#[derive(Component)]
pub(crate) struct PlanetPlayer;

#[derive(Component)]
pub(crate) struct PlayerHeading(Vec3);

#[derive(Component)]
pub(crate) struct PlayerMotion {
    velocity: Vec3,
    grounded: bool,
}

#[derive(Component)]
pub(crate) struct JoystickThumb;

/// Basic carried materials collected by mining solid resource blocks.
#[derive(Resource, Default)]
pub struct Inventory {
    pub regolith: u16,
    pub rock: u16,
    pub copper: u16,
    pub iron: u16,
}

impl Inventory {
    fn can_collect(&self, kind: VoxelKind) -> bool {
        match kind {
            VoxelKind::Regolith => self.regolith < 99,
            VoxelKind::Rock => self.rock < 99,
            VoxelKind::CopperOre => self.copper < 99,
            VoxelKind::IronOre => self.iron < 99,
            VoxelKind::Air => false,
        }
    }

    pub(crate) fn snapshot(&self) -> [u16; 4] {
        [self.regolith, self.rock, self.copper, self.iron]
    }

    fn collect(&mut self, kind: VoxelKind) {
        let slot = match kind {
            VoxelKind::Regolith => &mut self.regolith,
            VoxelKind::Rock => &mut self.rock,
            VoxelKind::CopperOre => &mut self.copper,
            VoxelKind::IronOre => &mut self.iron,
            VoxelKind::Air => return,
        };
        *slot = (*slot).saturating_add(1).min(99);
    }
}

#[derive(Component)]
pub(crate) struct InventoryHud;

/// Normalized movement input, camera drag, and held jump state from touch.
#[derive(Resource, Default)]
pub struct TouchControls {
    movement: Vec2,
    look_delta: Vec2,
    jump_pressed: bool,
    jump_was_pressed: bool,
    mine_pressed: bool,
    mine_was_pressed: bool,
    place_pressed: bool,
    place_was_pressed: bool,
}

/// Camera orbit is expressed relative to the player's local surface frame.
#[derive(Resource)]
pub struct CameraOrbit {
    yaw: f32,
    pitch: f32,
}

impl Default for CameraOrbit {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.38,
        }
    }
}

pub fn spawn_player(
    mut commands: Commands,
    terrain: Res<TerrainWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let ground_radius = terrain.surface_radius(Vec3::Y).unwrap_or(terrain.base_radius);
    let start_position = Vec3::Y * (ground_radius + PLAYER_SURFACE_OFFSET);

    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(0.22, 0.8))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.92, 0.58, 0.20),
            perceptual_roughness: 0.72,
            ..default()
        })),
        Transform {
            translation: start_position,
            rotation: Quat::IDENTITY,
            ..default()
        },
        PlanetPlayer,
        PlayerHeading(Vec3::Z),
        PlayerMotion {
            velocity: Vec3::ZERO,
            grounded: true,
        },
        Name::new("Player"),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 7.5).looking_at(start_position, Vec3::Y),
        #[cfg(target_os = "android")]
        Msaa::Off,
        Name::new("Orbit Camera"),
    ));
}

/// Draws the movement joystick and a separate jump touch target.
pub fn spawn_touch_hud(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(16.0),
            top: Val::Px(16.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        Text::new("REG 0   ROCK 0   CU 0   FE 0"),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(Color::WHITE),
        InventoryHud,
        Name::new("Inventory HUD"),
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(24.0),
            bottom: Val::Px(24.0),
            width: Val::Px(148.0),
            height: Val::Px(148.0),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgba(0.08, 0.14, 0.22, 0.48)),
        BorderColor::all(Color::srgba(0.72, 0.84, 0.96, 0.55)),
        Name::new("Movement Joystick"),
    ));

    commands.spawn((
        JoystickThumb,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(73.0),
            bottom: Val::Px(73.0),
            width: Val::Px(50.0),
            height: Val::Px(50.0),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgba(0.76, 0.86, 0.96, 0.78)),
        Name::new("Movement Joystick Thumb"),
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(28.0),
            top: Val::Px(28.0),
            width: Val::Px(112.0),
            height: Val::Px(64.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgba(0.20, 0.58, 0.34, 0.78)),
        BorderColor::all(Color::srgba(0.72, 1.0, 0.80, 0.9)),
        children![(
            Text::new("MINE"),
            TextFont { font_size: FontSize::Px(17.0), ..default() },
            TextColor(Color::WHITE),
        )],
        Name::new("Mine Button"),
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(28.0),
            top: Val::Px(108.0),
            width: Val::Px(112.0),
            height: Val::Px(64.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgba(0.28, 0.42, 0.72, 0.78)),
        BorderColor::all(Color::srgba(0.72, 0.84, 1.0, 0.9)),
        children![(
            Text::new("PLACE"),
            TextFont { font_size: FontSize::Px(17.0), ..default() },
            TextColor(Color::WHITE),
        )],
        Name::new("Place Button"),
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(28.0),
            bottom: Val::Px(32.0),
            width: Val::Px(112.0),
            height: Val::Px(76.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgba(0.92, 0.42, 0.16, 0.72)),
        BorderColor::all(Color::srgba(1.0, 0.82, 0.62, 0.9)),
        children![(
            Text::new("JUMP"),
            TextFont {
                font_size: FontSize::Px(19.0),
                ..default()
            },
            TextColor(Color::WHITE),
        )],
        Name::new("Jump Button"),
    ));
}

/// Left touch zone moves; right touch zone rotates the camera. The lower-right
/// jump target is excluded from camera drag, so movement, look, and jump coexist.
pub fn update_inventory_hud(
    inventory: Res<Inventory>,
    mut query: Query<&mut Text, With<InventoryHud>>,
) {
    if !inventory.is_changed() {
        return;
    }
    for mut text in &mut query {
        *text = Text::new(format!(
            "REG {}   ROCK {}   CU {}   FE {}",
            inventory.regolith, inventory.rock, inventory.copper, inventory.iron
        ));
    }
}

pub fn read_touch_controls(
    touches: Res<Touches>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut controls: ResMut<TouchControls>,
    mut thumb_query: Query<&mut Node, With<JoystickThumb>>,
) {
    controls.movement = Vec2::ZERO;
    controls.look_delta = Vec2::ZERO;
    controls.jump_pressed = false;
    controls.mine_pressed = false;
    controls.place_pressed = false;

    let Ok(window) = windows.single() else {
        return;
    };

    let size = Vec2::new(window.width(), window.height());
    let joystick_center = Vec2::new(98.0, size.y - 98.0);
    let mut joystick_active = false;

    for touch in touches.iter() {
        let start = touch.start_position();
        let position = touch.position();
        let jump_zone = start.x >= size.x - 160.0 && start.y >= size.y - 130.0;
        let mine_zone = start.x >= size.x - 160.0 && start.y <= 100.0;
        let place_zone = start.x >= size.x - 160.0 && (100.0..=190.0).contains(&start.y);

        if jump_zone {
            controls.jump_pressed = true;
        } else if mine_zone {
            controls.mine_pressed = true;
        } else if place_zone {
            controls.place_pressed = true;
        } else if start.x <= 210.0 && start.y >= size.y - 210.0 && !joystick_active {
            let displacement = Vec2::new(
                position.x - joystick_center.x,
                joystick_center.y - position.y,
            );
            controls.movement = (displacement / JOYSTICK_RADIUS).clamp_length_max(1.0);
            joystick_active = true;
        } else if start.x >= size.x * 0.42 {
            controls.look_delta += touch.delta();
        }
    }

    if let Ok(mut thumb) = thumb_query.single_mut() {
        let offset = controls.movement * 34.0;
        thumb.left = Val::Px(73.0 + offset.x);
        thumb.bottom = Val::Px(73.0 + offset.y);
    }
}

/// Applies touch and mouse drag to the orbit camera's yaw and pitch.
pub fn update_orbit_state(
    controls: Res<TouchControls>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut orbit: ResMut<CameraOrbit>,
) {
    let mut delta = controls.look_delta;
    if mouse_buttons.pressed(MouseButton::Left) {
        delta += mouse_motion.delta;
    }

    orbit.yaw = (orbit.yaw - delta.x * LOOK_SENSITIVITY).rem_euclid(std::f32::consts::TAU);
    orbit.pitch = (orbit.pitch - delta.y * LOOK_SENSITIVITY).clamp(-0.18, 1.12);
}

/// Integrates movement, radial gravity, jumping, and spherical ground contact.
/// The timestep is capped to reduce instability after a long mobile frame.
pub fn move_player_on_planet(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut controls: ResMut<TouchControls>,
    orbit: Res<CameraOrbit>,
    terrain: Res<TerrainWorld>,
    mut player_query: Query<
        (&mut Transform, &mut PlayerHeading, &mut PlayerMotion),
        With<PlanetPlayer>,
    >,
) {
    let Ok((mut player_transform, mut heading, mut motion)) = player_query.single_mut() else {
        return;
    };

    let dt = time.delta_secs().clamp(0.0, MAX_PHYSICS_STEP);
    if dt <= 0.0 {
        return;
    }

    let Some(frame) = tangent_frame(player_transform.translation, heading.0) else {
        return;
    };

    let mut movement = controls.movement;
    if movement.length_squared() <= 1.0e-4 {
        movement = Vec2::new(
            (if keyboard.pressed(KeyCode::KeyD) { 1.0 } else { 0.0 })
                - (if keyboard.pressed(KeyCode::KeyA) { 1.0 } else { 0.0 }),
            (if keyboard.pressed(KeyCode::KeyW) { 1.0 } else { 0.0 })
                - (if keyboard.pressed(KeyCode::KeyS) { 1.0 } else { 0.0 }),
        )
        .clamp_length_max(1.0);
    }

    let yaw = Quat::from_axis_angle(frame.up, orbit.yaw);
    let view_forward = yaw * frame.forward;
    let view_right = yaw * frame.right;
    let direction = view_right * movement.x + view_forward * movement.y;
    let has_movement = direction.length_squared() > 1.0e-6;

    if motion.grounded {
        let radial_velocity = motion.velocity.dot(frame.up);
        motion.velocity -= frame.up * radial_velocity;
    }

    if has_movement {
        motion.velocity += direction.normalize() * MOVE_ACCELERATION * dt;
    } else if motion.grounded {
        let damping = (-GROUND_FRICTION * dt).exp();
        motion.velocity *= damping;
    }

    let touch_jump_started = controls.jump_pressed && !controls.jump_was_pressed;
    controls.jump_was_pressed = controls.jump_pressed;
    let jump_requested = touch_jump_started || keyboard.just_pressed(KeyCode::Space);
    if jump_requested && motion.grounded {
        motion.velocity += frame.up * JUMP_SPEED;
        motion.grounded = false;
    }

    if let Some(gravity) = radial_gravity(player_transform.translation, GRAVITY_STRENGTH) {
        motion.velocity += gravity * dt;
    }

    let speed_limit = if motion.grounded {
        MAX_GROUND_SPEED
    } else {
        MAX_AIR_SPEED
    };
    let radial_speed = motion.velocity.dot(frame.up);
    let tangent_velocity = motion.velocity - frame.up * radial_speed;
    let limited_tangent = tangent_velocity.clamp_length_max(speed_limit);
    motion.velocity = limited_tangent + frame.up * radial_speed;

    player_transform.translation += motion.velocity * dt;

    if let Some(surface_radius) = terrain.surface_radius(player_transform.translation) {
        let (position, velocity, grounded) = resolve_spherical_contact(
            player_transform.translation,
            motion.velocity,
            surface_radius + PLAYER_SURFACE_OFFSET,
        );
        player_transform.translation = position;
        motion.velocity = velocity;
        motion.grounded = grounded;
    } else {
        // Unknown terrain is not treated as an invisible solid sphere.
        motion.grounded = false;
    }

    if let Some(up) = surface_normal(player_transform.translation) {
        if let Some(updated_frame) = tangent_frame(player_transform.translation, heading.0) {
            heading.0 = updated_frame.forward;
            let basis = Mat3::from_cols(updated_frame.right, up, updated_frame.forward);
            player_transform.rotation = Quat::from_mat3(&basis);
        }
    }
}

/// Keeps the camera orbiting the player while using the local radial up vector.
pub fn update_follow_camera(
    orbit: Res<CameraOrbit>,
    player_query: Query<(&Transform, &PlayerHeading), With<PlanetPlayer>>,
    mut camera_query: Query<&mut Transform, (With<Camera3d>, Without<PlanetPlayer>)>,
) {
    let Ok((player_transform, heading)) = player_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let Some(frame) = tangent_frame(player_transform.translation, heading.0) else {
        return;
    };

    let yaw = Quat::from_axis_angle(frame.up, orbit.yaw);
    let view_forward = yaw * frame.forward;
    let horizontal_distance = CAMERA_DISTANCE * orbit.pitch.cos();
    let vertical_distance = CAMERA_DISTANCE * orbit.pitch.sin();
    let camera_position = player_transform.translation
        - view_forward * horizontal_distance
        + frame.up * vertical_distance;

    *camera_transform = Transform::from_translation(camera_position)
        .looking_at(player_transform.translation + frame.up * 0.55, frame.up);
}

/// Executes edge-triggered mining and block placement from touch or keyboard input.
/// The ray starts just in front of the player to avoid selecting the camera/player.
pub fn apply_terrain_actions(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut controls: ResMut<TouchControls>,
    camera_query: Query<&Transform, (With<Camera3d>, Without<PlanetPlayer>)>,
    player_query: Query<&Transform, With<PlanetPlayer>>,
    mut terrain: ResMut<TerrainWorld>,
    mut inventory: ResMut<Inventory>,
) {
    let mine = (controls.mine_pressed && !controls.mine_was_pressed)
        || keyboard.just_pressed(KeyCode::KeyE);
    let place = (controls.place_pressed && !controls.place_was_pressed)
        || keyboard.just_pressed(KeyCode::KeyQ);
    controls.mine_was_pressed = controls.mine_pressed;
    controls.place_was_pressed = controls.place_pressed;

    if !mine && !place {
        return;
    }

    let (Ok(camera), Ok(player)) = (camera_query.single(), player_query.single()) else {
        return;
    };
    let direction = camera.forward().as_vec3();
    let origin = player.translation + direction * 0.18;
    let Some((voxel, face_normal)) = terrain.raycast_solid(origin, direction, 7.0) else {
        return;
    };

    if mine {
        if let Some(kind) = terrain.get_block(voxel) {
            if kind.is_solid()
                && inventory.can_collect(kind)
                && terrain.set_block(voxel, VoxelKind::Air)
            {
                inventory.collect(kind);
            }
        }
    } else if place {
        let target = voxel + face_normal;
        let target_center = (target.as_vec3() + Vec3::splat(0.5)) * terrain.voxel_size;
        if target_center.distance(player.translation) >= 0.95
            && terrain.get_block(target).is_some_and(|kind| !kind.is_solid())
        {
            // Do not let placement synchronously create an unloaded chunk:
            // streaming owns chunk creation and visual-entity registration.
            terrain.set_block(target, VoxelKind::Regolith);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_stops_mining_when_a_stack_is_full() {
        let mut inventory = Inventory { regolith: 99, ..default() };
        assert!(!inventory.can_collect(VoxelKind::Regolith));
        assert!(inventory.can_collect(VoxelKind::CopperOre));
        inventory.collect(VoxelKind::CopperOre);
        assert_eq!(inventory.copper, 1);
    }

    #[test]
    fn inventory_snapshot_tracks_every_material() {
        let inventory = Inventory { regolith: 1, rock: 2, copper: 3, iron: 4 };
        assert_eq!(inventory.snapshot(), [1, 2, 3, 4]);
    }
}
