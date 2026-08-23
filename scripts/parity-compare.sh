#!/usr/bin/env bash
#
# Cross-device reader-engine parity run recipe:
#
# Step 0: `cd core && cargo run -p inkuna-engine --example export-parity-fixtures --features test_support -- <dir>` and record manifest SHA-256.
#
# iOS — build Debug for simulator, `xcrun simctl install booted …`, `CONTAINER=$(xcrun simctl get_app_container booted app.inkuna.ios data)`, `mkdir -p "$CONTAINER/Documents/ParityCorpus" && cp corpus/*.epub` there, `xcrun simctl launch booted app.inkuna.ios -inkuna.debugScreen parityDigest`, wait for PARITY DONE via `xcrun simctl spawn booted log stream`, copy `parity-ios.json` out of container.
# Also copy the required manifest: `cp corpus/manifest.json "$CONTAINER/Documents/ParityCorpus/"`.
#
# Android — `./gradlew installDebug`, `adb push corpus/. /sdcard/Android/data/app.inkuna.android/files/ParityCorpus/`,
#   then `adb shell chmod 777 /sdcard/Android/data/app.inkuna.android/files/ParityCorpus` — REQUIRED: adb creates that
#   directory owned by `shell` with no other-exec bit, so without it the app cannot traverse its own corpus directory
#   and the run dies with `PARITY ERROR … open failed: EACCES` on manifest.json. Verified on Pixel_10_Pro_XL.
#   Then `adb shell am start -n app.inkuna.android/.MainActivity --ez inkuna.parityDigest true`, wait via
#   `adb logcat -s InkunaParity`, `adb pull …/parity-android.json`.

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 a.json b.json" >&2
  exit 2
fi

parity_temp=$(mktemp -d)
trap 'rm -f "$parity_temp/a.json" "$parity_temp/b.json"; rmdir "$parity_temp"' EXIT

jq -S . "$1" > "$parity_temp/a.json"
jq -S . "$2" > "$parity_temp/b.json"

if diff -u "$parity_temp/a.json" "$parity_temp/b.json"; then
  printf 'PARITY OK (%s books)\n' "$(jq 'length' "$parity_temp/a.json")"
else
  exit 1
fi
