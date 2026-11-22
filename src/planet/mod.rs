use bevy::prelude::*;

pub mod math;

pub fn spawn_test_planet(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(3.0).mesh().ico(5).expect("valid icosphere"))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.32, 0.48),
            perceptual_roughness: 0.92,
            metallic: 0.0,
            ..default()
        })),
        Transform::IDENTITY,
        Name::new("Prototype Planet"),
    ));

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
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 2.0, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
        #[cfg(target_os = "android")]
        Msaa::Off,
    ));
}
