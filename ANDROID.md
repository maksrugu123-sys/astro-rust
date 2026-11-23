# Android development

ASTRO-RUST targets Android ARM64 first. The Bevy application entry point is prepared for Bevy's Android GameActivity integration, but the native Android Gradle host project and APK packaging are not part of this bootstrap yet.

## Toolchain

The Android host project is in `android/` and uses Bevy's GameActivity integration. The current CI target is ARM64 (arm64-v8a), Android API 31+, JDK 21, Android SDK 36, and NDK 27.2.12479018.

## Build an APK with GitHub Actions

The workflow `.github/workflows/android-apk.yml` builds the Rust shared library with cargo-ndk, packages it with Gradle, and uploads `app-debug.apk` as the artifact `astro-rust-android-arm64-debug`.

To start a build manually:
1. Open the repository's **Actions** tab.
2. Select **Android APK**.
3. Choose **Run workflow** and branch `initial/android-prototype`.
4. Wait for the ARM64 debug build to finish.
5. Open the completed run and download the `astro-rust-android-arm64-debug` artifact. Extract the ZIP to get `app-debug.apk`.

Pushes to `initial/android-prototype` that change Rust or Android project files also start the workflow.

## Local native library build

Install a stable Rust toolchain, JDK 21, Android SDK + NDK, the Rust target, and cargo-ndk:

    rustup target add aarch64-linux-android
    cargo install cargo-ndk

Set `ANDROID_HOME` and `ANDROID_NDK_HOME`, then build the native library:

    cargo ndk -t arm64-v8a -P 31 -o android/app/src/main/jniLibs build --release --lib

From the `android/` directory, run Gradle task `:app:assembleDebug` to package the APK.

## Device considerations

- Start with ARM64; add 32-bit ARM only if there is a concrete device-support requirement.
- Keep MSAA and shadow maps disabled in the first mobile scene.
- Measure GPU limits, memory use, frame time, and thermal behavior on real devices before enabling expensive effects.
- The High visual target must have Medium and Low fallbacks.

## Current limitation

The GameActivity host project and ARM64 APK workflow are now present. The workflow has not yet completed successfully, and a real-device launch still needs to be verified.


## Save location

World saves use the application's private internal files directory, resolved from Bevy's Android `ANDROID_APP` handle via `internal_data_path()`. The game stores its save under the app-private `astro-rust/` subdirectory. This does not require broad external-storage permissions and is not visible as a normal public Downloads folder. Desktop development uses a repository-local `saves/` directory.
