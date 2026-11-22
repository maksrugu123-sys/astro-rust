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
    .add_systems(Startup, (planet::spawn_test_planet, player::spawn_player))
    .add_systems(Update, player::move_player_on_planet);

    app.run();
}
