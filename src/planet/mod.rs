use bevy::prelude::*;

pub mod math;

/// Spawns the sun light; the planet surface itself is rendered from voxel chunks.
pub fn spawn_test_planet(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 18_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.7,
            -0.5,
            0.0,
        )),
        Name::new("Planet Key Light"),
    ));
}
