#!/usr/bin/env bash
# Builds the Rust core for Android, generates Kotlin bindings, and drops
# .so files into the app's jniLibs.
#
# ANDROID_ABIS defaults to device + x86_64 emulator so local builds run
# anywhere; CI sets ANDROID_ABIS=arm64-v8a (with a matching -PinkunaAbis)
# to keep the shipped APK minimal. arm64-v8a must stay in the list — the
# bindgen step below reads the aarch64 build.
set -euo pipefail

ANDROID_ABIS="${ANDROID_ABIS:-arm64-v8a x86_64}"

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

NDK_TARGETS=()
for abi in $ANDROID_ABIS; do
  NDK_TARGETS+=(-t "$abi")
done

cd "$CORE"
rm -rf "$JNI_LIBS" "$GENERATED"
cargo ndk "${NDK_TARGETS[@]}" -o "$JNI_LIBS" build -p inkuna-ffi --release

cargo run -p inkuna-ffi --bin uniffi-bindgen --release -- generate \
  --library "$CARGO_TARGET_DIR/aarch64-linux-android/release/libinkuna_ffi.so" \
  --language kotlin \
  --out-dir "$GENERATED"

# Strip AFTER bindgen, never before: uniffi-bindgen --library reads the UniFFI
# metadata out of the static symbol table, so building with strip = "symbols"
# makes it fail with "No UniFFI metadata found". Stripping the jniLibs copies
# here leaves the target/ artifacts (and therefore bindgen) untouched while
# taking ~2.2 MB off each shipped .so. JNA resolves the exports through .dynsym,
# which --strip-all preserves.
#
# Set INKUNA_STRIP=0 to keep symbols when symbolicating a native crash locally.
if [ "${INKUNA_STRIP:-1}" != "0" ]; then
  STRIP="$(ls "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-strip | head -1)"
  for so in "$JNI_LIBS"/*/libinkuna_ffi.so; do
    before=$(wc -c < "$so")
    "$STRIP" --strip-all "$so"
    echo "stripped $(basename "$(dirname "$so")"): $((before / 1024))K -> $(($(wc -c < "$so") / 1024))K"
  done
fi

echo "OK: $JNI_LIBS + $GENERATED"
