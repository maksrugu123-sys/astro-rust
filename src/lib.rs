use bevy::prelude::*;

mod planet;

#[bevy_main]
pub fn main() {
    run_app();
}

pub fn run_app() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "ASTRO-RUST".into(),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.008, 0.012, 0.035)))
    .add_systems(Startup, planet::spawn_test_planet);

    #[cfg(target_os = "android")]
    {
        app.insert_resource(Msaa::Off);
    }

    app.run();
}
