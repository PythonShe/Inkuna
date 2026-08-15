#!/usr/bin/env bash
# Builds the Rust core for Android (arm64-v8a only), generates Kotlin
# bindings, and drops .so files into the app's jniLibs.
# Changing the ABI list means changing abiFilters in build.gradle.kts too.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="$ROOT/core"
APP="$ROOT/apps/android/app"
JNI_LIBS="$APP/src/main/jniLibs"
GENERATED="$APP/src/generated/kotlin"

export CARGO_TARGET_DIR="$CORE/target"
# The Homebrew Rust lacks cross-compile targets; always go through rustup
# and pin RUSTC so cargo cannot pick the Homebrew rustc off PATH.
export RUSTC="$(rustup which --toolchain stable rustc)"
cargo() { rustup run stable cargo "$@"; }

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
  # Pick the newest installed NDK.
  ANDROID_NDK_HOME="$(ls -d "$HOME/Library/Android/sdk/ndk/"* | sort -V | tail -1)"
  export ANDROID_NDK_HOME
fi

cd "$CORE"
rm -rf "$JNI_LIBS" "$GENERATED"
cargo ndk -t arm64-v8a -o "$JNI_LIBS" build -p inkuna-ffi --release

cargo run -p inkuna-ffi --bin uniffi-bindgen --release -- generate \
  --library "$CARGO_TARGET_DIR/aarch64-linux-android/release/libinkuna_ffi.so" \
  --language kotlin \
  --out-dir "$GENERATED"

echo "OK: $JNI_LIBS + $GENERATED"
