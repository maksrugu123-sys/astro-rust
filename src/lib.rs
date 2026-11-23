use bevy::{prelude::*, winit::WinitSettings};

mod chunk_io;
mod debug_log;
mod planet;
mod player;
mod save_system;
mod terrain;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_net_astrorust_game_MainActivity_nativeSetDebugDirectory(
    mut env: jni::JNIEnv,
    _activity: jni::objects::JObject,
    path: jni::objects::JString,
) {
    if let Ok(path) = env.get_string(&path) {
        let path: String = path.into();
        debug_log::set_directory(std::path::PathBuf::from(path));
    }
}

#[bevy_main]
pub fn main() {
    run_app();
}

pub fn run_app() {
    debug_log::initialize();
    info!("ASTRO-RUST startup: initializing Bevy application");

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
    .init_resource::<player::Inventory>()
    .init_resource::<terrain::TerrainWorld>()
    .init_resource::<terrain::TerrainGenerationQueue>()
    .init_resource::<chunk_io::ChunkIoQueue>()
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
            player::update_inventory_hud,
            save_system::SaveStore::process_chunk_io,
            terrain::stream_terrain_chunks,
            terrain::refresh_dirty_chunk_meshes,
            save_system::autosave_world,
            save_system::save_world_on_suspend,
        )
            .chain(),
    )
    .add_systems(Last, save_system::save_world_on_exit);

    info!("ASTRO-RUST startup: Bevy plugins and systems registered");
    app.run();
}
