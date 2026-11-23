# Android build — ASTRO-RUST

The Android shell uses the Bevy 0.19 GameActivity integration. The Android app loads the Rust shared library named `astro_rust`, produced from the root Rust library crate.

## Project settings

- Application ID: `net.astrorust.game`
- Minimum Android API: 31 (Bevy 0.19 GameActivity path)
- Compile/target API: 34
- ABI: `arm64-v8a`
- Android Gradle Plugin: 8.4.0
- Gradle: 8.6
- JDK: 17
- Rust target: `aarch64-linux-android`

## Build prerequisites

Install:
1. Rust stable and the `aarch64-linux-android` target.
2. Android SDK and NDK (side-by-side).
3. JDK 17.
4. Gradle 8.6.
5. `cargo-ndk` (`cargo install cargo-ndk`).

Set `ANDROID_HOME` or `ANDROID_SDK_ROOT`; if the NDK is not auto-detected, set `ANDROID_NDK_HOME` or `ANDROID_NDK_ROOT` to its installed directory.

## Build

From the repository root:

```sh
bash scripts/build-android.sh
```

The script compiles the Rust library for `arm64-v8a`, puts `libastro_rust.so` in the Android module's `jniLibs` folder, and then runs `gradle assembleDebug`.

Expected output:

```
android/app/build/outputs/apk/debug/app-debug.apk
```

To install with ADB:

```sh
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

## Current limitations

- The Android project is a first integration scaffold, not a verified APK.
- This repository does not yet include the Gradle wrapper JAR or wrapper scripts; use an installed Gradle 8.6 executable for now.
- The Rust crate, NDK link, GameActivity dependency, and Android packaging have not yet been compiled or tested together.
- Only 64-bit ARM is configured. This matches the current Android-first target device class; 32-bit ABI support is not enabled.
- No assets are copied into the APK yet because the current game scene is generated procedurally in code.
