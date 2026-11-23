use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
};

use tracing_subscriber::fmt;

static DEBUG_DIRECTORY: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

pub fn set_directory(path: PathBuf) {
    let _ = DEBUG_DIRECTORY.set(path);
}

fn directory() -> PathBuf {
    DEBUG_DIRECTORY.get().cloned().unwrap_or_else(|| {
        PathBuf::from(
            "/storage/emulated/0/Android/data/net.astrorust.game/files/ASTRO-RUST/debug",
        )
    })
}

pub fn initialize() {
    let directory = directory();
    if let Err(error) = fs::create_dir_all(&directory) {
        eprintln!("ASTRO-RUST could not create diagnostics directory: {error}");
        return;
    }

    let log_path = directory.join("session.log");
    rotate_if_needed(&log_path);

    let writer_path = log_path.clone();
    let subscriber = fmt()
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_max_level(tracing::Level::INFO)
        .with_writer(move || {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&writer_path)
                .unwrap_or_else(|_| {
                    File::create("/dev/null").expect("Android /dev/null must exist")
                })
        });

    if let Err(error) = subscriber.try_init() {
        eprintln!("ASTRO-RUST diagnostics subscriber was not installed: {error}");
    }

    install_panic_hook(&directory);
    tracing::info!(
        "Diagnostics initialized; directory={}",
        directory.display()
    );
}

fn rotate_if_needed(path: &PathBuf) {
    let too_large = fs::metadata(path)
        .map(|metadata| metadata.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    if !too_large {
        return;
    }

    let previous = path.with_extension("log.1");
    let _ = fs::remove_file(&previous);
    let _ = fs::rename(path, previous);
}

fn install_panic_hook(directory: &PathBuf) {
    let crash_path = directory.join("panic.log");
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&crash_path) {
            let _ = writeln!(file, "{panic_info}");
            let _ = file.flush();
        }
        tracing::error!("Rust panic: {panic_info}");
        previous_hook(panic_info);
    }));
}
