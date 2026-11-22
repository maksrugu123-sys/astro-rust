use bevy::prelude::*;

mod planet;
mod player;

#[bevy_main]
pub fn main() {
    run_app();
}

pub fn run_app() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "ASTRO-RUST".into(),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.008, 0.012, 0.035)))
    .insert_resource(WinitSettings::mobile())
    .init_resource::<player::TouchControls>()
    .init_resource::<player::CameraOrbit>()
    .add_systems(
        Startup,
        (planet::spawn_test_planet, player::spawn_player, player::spawn_touch_hud),
    )
    .add_systems(
        Update,
        (
            player::read_touch_controls,
            player::update_orbit_state,
            player::move_player_on_planet,
            player::update_follow_camera,
        )
            .chain(),
    );

    app.run();
}
