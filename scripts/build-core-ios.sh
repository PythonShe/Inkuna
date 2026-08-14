#!/usr/bin/env bash
# Builds the Rust core for iOS (device + simulator), generates Swift
# bindings, and packages InkunaCore.xcframework for the UIKit app.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="$ROOT/core"
OUT="$ROOT/apps/ios"
GENERATED="$OUT/Generated"
FRAMEWORKS="$OUT/Frameworks"

# Keep build artifacts local to the repo regardless of any global
# CARGO_TARGET_DIR the developer machine may set.
export CARGO_TARGET_DIR="$CORE/target"

# The Homebrew Rust lacks cross-compile targets; always go through rustup.
# RUSTC must be pinned too: cargo resolves rustc via PATH, where Homebrew's
# rustc shadows the rustup one.
export RUSTC="$(rustup which --toolchain stable rustc)"
cargo() { rustup run stable cargo "$@"; }

cd "$CORE"
cargo build -p inkuna-ffi --release --target aarch64-apple-ios
cargo build -p inkuna-ffi --release --target aarch64-apple-ios-sim

rm -rf "$GENERATED" "$FRAMEWORKS/InkunaCore.xcframework"
mkdir -p "$GENERATED" "$FRAMEWORKS"

cargo run -p inkuna-ffi --bin uniffi-bindgen --release -- generate \
  --library "$CARGO_TARGET_DIR/aarch64-apple-ios/release/libinkuna_ffi.a" \
  --language swift \
  --out-dir "$GENERATED"

# xcodebuild expects a directory of headers with a module.modulemap.
HEADERS="$CARGO_TARGET_DIR/inkuna-headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
mv "$GENERATED"/*.h "$HEADERS/"
mv "$GENERATED"/*.modulemap "$HEADERS/module.modulemap"

xcodebuild -create-xcframework \
  -library "$CARGO_TARGET_DIR/aarch64-apple-ios/release/libinkuna_ffi.a" -headers "$HEADERS" \
  -library "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/release/libinkuna_ffi.a" -headers "$HEADERS" \
  -output "$FRAMEWORKS/InkunaCore.xcframework"

echo "OK: $FRAMEWORKS/InkunaCore.xcframework + $GENERATED/InkunaCore.swift"
