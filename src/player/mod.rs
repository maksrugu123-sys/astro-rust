//! Minimal kinematic player for a spherical planet prototype.

use bevy::prelude::*;

pub mod math;

use crate::planet::math::surface_normal;
use math::tangent_frame;

const PLANET_RADIUS: f32 = 3.0;
const PLAYER_SURFACE_OFFSET: f32 = 0.62;
const MOVE_SPEED: f32 = 2.8;

#[derive(Component)]
struct PlanetPlayer;

/// Spawns a simple capsule and a follow camera above the starting surface.
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
        Name::new("Player"),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 7.5).looking_at(start_position, Vec3::Y),
        #[cfg(target_os = "android")]
        Msaa::Off,
        Name::new("Player Camera"),
    ));
}

/// Keyboard input is a temporary desktop-prototype control scheme.
/// The same surface movement function will be reused by Android touch controls.
pub fn move_player_on_planet(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player_query: Query<&mut Transform, With<PlanetPlayer>>,
    mut camera_query: Query<&mut Transform, (With<Camera3d>, Without<PlanetPlayer>)>,
) {
    let Ok(mut player_transform) = player_query.single_mut() else {
        return;
    };

    let position = player_transform.translation;
    let Some(frame) = tangent_frame(position, Vec3::Z) else {
        return;
    };

    let horizontal = f32::from(keyboard.pressed(KeyCode::KeyD))
        - f32::from(keyboard.pressed(KeyCode::KeyA));
    let vertical = f32::from(keyboard.pressed(KeyCode::KeyW))
        - f32::from(keyboard.pressed(KeyCode::KeyS));

    let input = frame.right * horizontal + frame.forward * vertical;
    if input.length_squared() > 1.0e-6 {
        let direction = input.normalize();
        let next_position = (position + direction * MOVE_SPEED * time.delta_secs())
            .normalize()
            * (PLANET_RADIUS + PLAYER_SURFACE_OFFSET);
        player_transform.translation = next_position;
    }

    if let Some(up) = surface_normal(player_transform.translation) {
        player_transform.rotation = Quat::from_rotation_arc(Vec3::Y, up);
        if let Ok(mut camera_transform) = camera_query.single_mut() {
            let camera_position = player_transform.translation + up * 2.4 - frame.forward * 4.8;
            *camera_transform = Transform::from_translation(camera_position)
                .looking_at(player_transform.translation + up * 0.55, up);
        }
    }
}
