#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust/Cargo is missing. Install Rust first." >&2
    exit 1
fi
if ! command -v cargo-ndk >/dev/null 2>&1 && ! cargo ndk --version >/dev/null 2>&1; then
    echo "cargo-ndk is missing. Install it with: cargo install cargo-ndk" >&2
    exit 1
fi
if ! command -v gradle >/dev/null 2>&1; then
    echo "Gradle 8.6 is missing. Install Gradle 8.6 and ensure 'gradle' is on PATH." >&2
    exit 1
fi
if [[ -z "${ANDROID_HOME:-}${ANDROID_SDK_ROOT:-}" ]]; then
    echo "Set ANDROID_HOME or ANDROID_SDK_ROOT to the Android SDK directory." >&2
    exit 1
fi

cargo ndk -t arm64-v8a -P 31 -o android/app/src/main/jniLibs build --lib --release
(
    cd android
    gradle --no-daemon assembleDebug
)

APK="$ROOT_DIR/android/app/build/outputs/apk/debug/app-debug.apk"
if [[ -f "$APK" ]]; then
    echo "APK created: $APK"
else
    echo "Gradle completed but the expected APK was not found." >&2
    exit 1
fi
