# Android development

ASTRO-RUST targets Android ARM64 first. The Bevy application entry point is prepared for Bevy's Android GameActivity integration, but the native Android Gradle host project and APK packaging are not part of this bootstrap yet.

## Toolchain

The official Bevy mobile example currently documents:
- A current stable Rust toolchain.
- Android SDK and Android NDK (side by side).
- JDK 21.
- The Rust target aarch64-linux-android.
- cargo-ndk.

Install the Rust target and cargo-ndk:

    rustup target add aarch64-linux-android
    cargo install cargo-ndk

Set ANDROID_HOME to the Android SDK directory and ANDROID_NDK_ROOT to the selected NDK directory.

## Native library build

Once the Android Gradle host project exists, build the ARM64 native library into its JNI library directory:

    cargo ndk -t arm64-v8a -P 26 -o android/app/src/main/jniLibs build --release

This command is documented for the Bevy mobile workflow; it has not been run for this repository.

## Device considerations

- Start with ARM64; add 32-bit ARM only if there is a concrete device-support requirement.
- Keep MSAA and shadow maps disabled in the first mobile scene.
- Measure GPU limits, memory use, frame time, and thermal behavior on real devices before enabling expensive effects.
- The High visual target must have Medium and Low fallbacks.

## Current limitation

There is not yet an Android Gradle wrapper project in this repository, so an APK cannot be produced from this bootstrap alone. The next Android milestone is to add the minimal GameActivity host project and verify a real ARM64 launch. No build or APK task has been run.
