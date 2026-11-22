use bevy::prelude::*;

mod planet;
mod player;
mod save_system;
mod terrain;

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
    .init_resource::<terrain::TerrainWorld>()
    .insert_resource(save_system::SaveStore::discover())
    .add_systems(
        Startup,
        (
            save_system::load_saved_world,
            planet::spawn_test_planet,
            terrain::setup_terrain_material,
            terrain::generate_starting_chunks,
            terrain::spawn_chunk_meshes,
            player::spawn_player,
            player::spawn_touch_hud,
        )
            .chain(),
    )
    .add_systems(
        Update,
        (
            player::read_touch_controls,
            player::update_orbit_state,
            player::move_player_on_planet,
            player::update_follow_camera,
            player::apply_terrain_actions,
            terrain::stream_terrain_chunks,
            terrain::refresh_dirty_chunk_meshes,
            save_system::autosave_world,
        )
            .chain(),
    )
    .add_systems(Last, save_system::save_world_on_exit);

    app.run();
}
