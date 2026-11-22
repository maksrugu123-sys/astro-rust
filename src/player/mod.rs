//! Kinematic player and mobile controls for spherical planets.

use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};

pub mod math;

use crate::planet::math::surface_normal;
use math::tangent_frame;

const PLANET_RADIUS: f32 = 3.0;
const PLAYER_SURFACE_OFFSET: f32 = 0.62;
const MOVE_SPEED: f32 = 2.8;
const JOYSTICK_RADIUS: f32 = 58.0;
const LOOK_SENSITIVITY: f32 = 0.005;
const CAMERA_DISTANCE: f32 = 5.4;

#[derive(Component)]
struct PlanetPlayer;

#[derive(Component)]
struct PlayerHeading(Vec3);

#[derive(Component)]
struct JoystickThumb;

/// Normalized movement input and per-frame camera drag, populated from touch.
#[derive(Resource, Default)]
pub struct TouchControls {
    movement: Vec2,
    look_delta: Vec2,
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let start_position = Vec3::Y * (PLANET_RADIUS + PLAYER_SURFACE_OFFSET);

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

/// Draws a fixed virtual joystick in the lower-left corner.
pub fn spawn_touch_hud(mut commands: Commands) {
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
}

/// The left touch zone drives movement; touches on the right side rotate the view.
/// A second finger can control the camera while the first remains on the joystick.
pub fn read_touch_controls(
    touches: Res<Touches>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut controls: ResMut<TouchControls>,
    mut thumb_query: Query<&mut Node, With<JoystickThumb>>,
) {
    controls.movement = Vec2::ZERO;
    controls.look_delta = Vec2::ZERO;

    let Ok(window) = windows.single() else {
        return;
    };

    let size = Vec2::new(window.width(), window.height());
    let joystick_center = Vec2::new(98.0, size.y - 98.0);
    let mut joystick_active = false;

    for touch in touches.iter() {
        let start = touch.start_position();

        if start.x <= 210.0 && start.y >= size.y - 210.0 && !joystick_active {
            let displacement = Vec2::new(
                touch.position().x - joystick_center.x,
                joystick_center.y - touch.position().y,
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

/// Moves along the sphere using camera-relative tangent directions.
pub fn move_player_on_planet(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    controls: Res<TouchControls>,
    orbit: Res<CameraOrbit>,
    mut player_query: Query<(&mut Transform, &mut PlayerHeading), With<PlanetPlayer>>,
) {
    let Ok((mut player_transform, mut heading)) = player_query.single_mut() else {
        return;
    };

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

    if direction.length_squared() > 1.0e-6 {
        player_transform.translation = (player_transform.translation
            + direction.normalize() * MOVE_SPEED * time.delta_secs())
            .normalize()
            * (PLANET_RADIUS + PLAYER_SURFACE_OFFSET);
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
