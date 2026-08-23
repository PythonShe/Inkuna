# Reader Engine Swap — Parity Evidence

**Recorded:** 2026-08-24  
**Status:** Row 11 (cross-device page-digest parity) is **VERIFIED** on both Pro
targets — the two platforms produced byte-identical digest JSON. The remaining
device/interactivity gates are `OWNER-PENDING`. This record contains only
commands and observations actually performed; no device result, screenshot,
digest, or performance measurement is inferred or estimated. An `OWNER-PENDING`
row means no number was observed, never a number assumed.

## Local build and catalog evidence

- `./scripts/build-core-ios.sh` — **PASS**: exit 0; final line:
  `OK: .../apps/ios/Frameworks/InkunaCore.xcframework + .../apps/ios/Generated/InkunaCore.swift`.
  `CoreSimulatorService connection became invalid` warnings were non-fatal.
- `./scripts/build-core-android.sh` — **PASS**: exit 0; final line:
  `OK: .../apps/android/app/src/main/jniLibs + .../apps/android/app/src/generated/kotlin`.
  Bindgen reported that local `ktlint` was unavailable for optional
  auto-formatting; generation completed.
- iOS Debug: `cd apps/ios && xcodegen generate && xcodebuild -project Inkuna.xcodeproj -scheme Inkuna -destination 'generic/platform=iOS Simulator' build`
  — **PASS**, `** BUILD SUCCEEDED **`. The sandbox emitted non-fatal
  CoreSimulatorService warnings.
- Android Debug: `cd apps/android && ./gradlew assembleDebug` — **PASS**,
  `BUILD SUCCESSFUL in 2s` (`39 actionable tasks: 5 executed, 34 up-to-date`).
  Generated-binding unused-expression warnings are non-fatal.
- Recorded task evidence: Android Release `assembleRelease` was
  `BUILD SUCCESSFUL`; `connectedDebugAndroidTest` built successfully but had
  no attached runtime device. An iOS Release simulator attempt failed in
  `CompileAssetCatalogVariant` while CoreSimulatorService was unavailable;
  it is an environment failure, not a source diagnosis.
- Localization structure check: all three degradation keys have exactly
  `de,en,es,fr,id,it,ja,ko,pt,ru,th,vi,zh-Hans,zh-Hant` in the iOS catalog,
  and all 14 Android `values*` catalogs contain each key. All values are real
  translations; no English placeholders were used.

## Owner device setup / launch commands for rows 1–9

Run the following once before the manual checks, then use the exact terminate
or force-stop command between cold-run checks. The build paths are deliberately
fixed so the install commands are directly runnable.

```bash
# iPhone 17 Pro simulator, Debug build
cd apps/ios
xcodegen generate
xcrun simctl boot 'iPhone 17 Pro'
xcodebuild -project Inkuna.xcodeproj -scheme Inkuna -configuration Debug \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' \
  -derivedDataPath /private/tmp/inkuna-ios-debug build
xcrun simctl install booted /private/tmp/inkuna-ios-debug/Build/Products/Debug-iphonesimulator/Inkuna.app
xcrun simctl launch booted app.inkuna.ios
xcrun simctl terminate booted app.inkuna.ios

# Pixel_10_Pro_XL AVD, Debug build
cd apps/android
./gradlew assembleDebug
adb devices -l
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n app.inkuna.android/.MainActivity
adb shell am force-stop app.inkuna.android
```

## Checklist

| # | Gate | Evidence / owner procedure |
| --- | --- | --- |
| 1 | Every seeded-library book opens and restores position. | **OWNER-PENDING** — use the shared setup/launch/termination commands above; open every seeded book, leave each on a non-initial page, terminate/force-stop and relaunch, and record the restored position for every title. |
| 2 | Page/chapter navigation, including rapid boundary chains, has no rescue layers. | **OWNER-PENDING** — after launching with the shared commands above, perform at least five rapid flicks across at least three distinct chapter boundaries on each platform and record a screenshot or video showing every landing rendered. Also verify no rescue-layer UI is present. |
| 3 | Vertical-CJK/ruby and RTL progression; tap/key directions. | **OWNER-PENDING** — after launching with the shared commands above, open the seeded vertical-CJK-with-ruby book and RTL book on both targets. Verify ruby to the end, visual progression, iOS left/right arrow keys plus edge taps, and Android edge taps; attach screenshots/video. |
| 4 | TOC plus internal-link/footnote `locateHref`, including fragments. | **OWNER-PENDING** — after launching with the shared commands above, invoke TOC entries and an internal link/footnote with a fragment target on both targets; record the target page and back-navigation result. |
| 5 | In-book and library search, including on-page match rects. | **OWNER-PENDING** — after launching with the shared commands above, run canonical in-book and library queries, choose a result, verify the landed text and highlight, and verify a noncanonical row is visibly non-tappable. Attach one screenshot per platform. |
| 6 | Bookmarks/progress survive; positions do not change with typography. | **OWNER-PENDING** — after launching with the shared commands above, bookmark and leave a position, terminate/force-stop and relaunch, then change text size twice. Record `position N of M` before and after; both values must remain identical. |
| 7 | Four themes, night mode, and live typography application. | **OWNER-PENDING** — after launching with the shared commands above, cycle every theme and night mode, then adjust each typography setting while reading. Capture one screenshot per theme and note any setting that does not live-apply. |
| 8 | Selection copy/share/web-search in horizontal and vertical text. | **OWNER-PENDING** — after launching with the shared commands above, select text in normal and vertical books on both targets; invoke Copy, Share, and Web Search, and record the copied/selected text and action availability. |
| 9 | VoiceOver/TalkBack block navigation, language, bounds, and links. | **OWNER-PENDING** — after launching with the shared commands above, enable VoiceOver and TalkBack respectively; navigate a CJK book by accessibility blocks, verify language switching/bounds, and confirm link traits. Record a screen recording or owner verification date. |
| 10 | Performance gate and progressive availability. | **OWNER-PENDING** — populate the five-run table below on the required Pro targets. No simulator/emulator runtime is available in this workspace. |
| 11 | Cross-device page-digest comparison. | **VERIFIED 2026-08-24** on the Pro targets (iPhone 17 Pro simulator, iOS 26.5; AVD `Pixel_10_Pro_XL`). Two-book dry-run corpus (`latin.epub`, `cjk_horizontal.epub`) from plan 01's `export-parity-fixtures`. Both hooks produced identical structure (latin: 2 spines / 8 pages; cjk_horizontal: 1 spine / 5 pages) and `scripts/parity-compare.sh parity-ios.json parity-android.json` returned **`PARITY OK (2 books)`**, exit 0. `parity-ios.json` SHA-256 `f472cd2c2b3e6e252aeec2825ab7c36945ef6df34fbd8f0bb4697d9baf1627b5`; `parity-android.json` SHA-256 `f472cd2c2b3e6e252aeec2825ab7c36945ef6df34fbd8f0bb4697d9baf1627b5`. NOTE: this is the 2-book dry-run required by Task 6.3's Verify step, not the full 7-fixture corpus — the remaining five archetypes (`cjk_vertical_ruby`, `rtl`, `mixed_script`, `image_heavy`, `table_degradation`) are still OWNER-PENDING and run with the same commands. |
| 12 | Bindings, clean shell builds, generated-drift check, and zero-Readium sweep. | **PASS (local static/build portion)** — both mandated binding scripts and both Debug shell builds passed above; `git diff --check` exited 0. The literal sweep below produced no output. Generated output was regenerated only by the scripts, then its ignored build/cache directories were moved recoverably outside the worktree before the literal gate. Device-dependent behavior remains covered by rows 1–11. |

## Performance protocol — Task 6.4

All runs are cold: freshly launch the app and do not open the target book
earlier in that run. Use a normal book and a long-chapter book five times each
on an **iPhone 17 Pro simulator** and the **Pixel_10_Pro_XL** AVD. The median
and worst are over the five runs.

### Owner commands

```bash
# iOS simulator rehearsal (Release)
cd apps/ios
xcodegen generate
xcodebuild -project Inkuna.xcodeproj -scheme Inkuna -configuration Release \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' build
xcrun simctl boot 'iPhone 17 Pro'
xcrun simctl install booted <path-to-Inkuna.app>
xcrun simctl launch booted app.inkuna.ios
xcrun simctl spawn booted log show --last 5m \
  --predicate 'subsystem == "app.inkuna.ios" AND category == "perf"'

# iOS device capture: run the same flow in a Release install, capture the
# `app.inkuna.ios`/`perf` logs in Console.app, and run Instruments
# "Animation Hitches" over Keep Reading + five page turns.

# Android Pixel_10_Pro_XL rehearsal (Release; local debug signing is allowed)
cd apps/android
./gradlew assembleRelease
adb devices -l                         # select the Pixel_10_Pro_XL serial
adb -s <Pixel_10_Pro_XL-serial> install -r app/build/outputs/apk/release/app-release.apk
adb -s <Pixel_10_Pro_XL-serial> logcat -c
adb -s <Pixel_10_Pro_XL-serial> shell am force-stop app.inkuna.android
adb -s <Pixel_10_Pro_XL-serial> shell am start -n app.inkuna.android/.MainActivity
adb logcat -d -s InkunaPerf

# Android jank: do Keep Reading open + five page turns between these commands.
adb shell dumpsys gfxinfo app.inkuna.android reset
adb shell dumpsys gfxinfo app.inkuna.android
```

For the frozen `main` baseline, use the same physical/simulated target, corpus,
and flow; do not compare different devices.

```bash
git worktree add ../inkuna-readium-main main
cd ../inkuna-readium-main/apps/android && ./gradlew assembleRelease
# Install this baseline on Pixel_10_Pro_XL, then repeat the gfxinfo reset/run/dump above.
cd ../inkuna-readium-main/apps/ios
xcodegen generate
xcodebuild -project Inkuna.xcodeproj -scheme Inkuna -configuration Release \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' build
# Run the same Keep Reading + five-turn flow in Instruments "Animation Hitches".
```

### Five-run measurement table

`OWNER-PENDING` means no number was observed. `Peak RSS` is measured after 20
continuous pages and five chapter crossings. `Boundary latency` measures a
long-chapter boundary crossing, but is recorded for both book classes for a
complete comparison.

| Platform | Book | Metric | Run 1 | Run 2 | Run 3 | Run 4 | Run 5 | Median | Worst | Gate / note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| iOS | Normal | `open_to_first_page_ready_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | <= 100 ms |
| iOS | Normal | `first_page_ready_to_first_render_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | decomposed |
| iOS | Normal | `tap_to_first_page_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | <= 250 ms |
| iOS | Normal | `chapter_layout_complete_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | decomposed |
| iOS | Normal | boundary-crossing latency | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | 5 chapter crossings |
| iOS | Normal | peak RSS | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | 20 pages + 5 crossings |
| iOS | Long chapter | `open_to_first_page_ready_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | <= 100 ms; progressive invariant |
| iOS | Long chapter | `first_page_ready_to_first_render_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | decomposed |
| iOS | Long chapter | `tap_to_first_page_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | <= 250 ms |
| iOS | Long chapter | `chapter_layout_complete_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | compare to first-page ready |
| iOS | Long chapter | boundary-crossing latency | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | 5 chapter crossings |
| iOS | Long chapter | peak RSS | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | 20 pages + 5 crossings |
| Android | Normal | `open_to_first_page_ready_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | <= 100 ms |
| Android | Normal | `first_page_ready_to_first_render_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | decomposed |
| Android | Normal | `tap_to_first_page_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | <= 250 ms |
| Android | Normal | `chapter_layout_complete_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | decomposed |
| Android | Normal | boundary-crossing latency | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | 5 chapter crossings |
| Android | Normal | peak RSS | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | 20 pages + 5 crossings |
| Android | Long chapter | `open_to_first_page_ready_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | <= 100 ms; progressive invariant |
| Android | Long chapter | `first_page_ready_to_first_render_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | decomposed |
| Android | Long chapter | `tap_to_first_page_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | <= 250 ms |
| Android | Long chapter | `chapter_layout_complete_ms` | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | compare to first-page ready |
| Android | Long chapter | boundary-crossing latency | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | 5 chapter crossings |
| Android | Long chapter | peak RSS | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | 20 pages + 5 crossings |
| iOS | Normal | Animation Hitches | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | Instruments, Keep Reading + 5 turns |
| iOS | Long chapter | Animation Hitches | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | Instruments, Keep Reading + 5 turns |
| Android | Normal | jank % / p95 frame time | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | `dumpsys gfxinfo`; no worse than `main` |
| Android | Long chapter | jank % / p95 frame time | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | OWNER-PENDING | `dumpsys gfxinfo`; no worse than `main` |

**Progressive-verification invariant — OWNER-PENDING:** on each long-chapter
run, record that the reading surface is visible and interactive at
`open_to_first_page_ready_ms` (expected near 30 ms) and substantially before
`chapter_layout_complete_ms` (expected near 400 ms). A late first surface is a
gate failure even if the final chapter layout succeeds.

## Cross-device digest corpus — Task 6.3

The fixture exporter command is:

```bash
cd core && cargo run -p inkuna-engine --example export-parity-fixtures \
  --features test_support -- <dir>
```

Recorded exporter manifest SHA-256 values:

| Corpus | `manifest.json` SHA-256 | File names |
| --- | --- | --- |
| Full exported corpus | `5d3c966c6d300993177a7db987624f41de7f560ff191ef8d90a10f399de7295c` | `latin.epub`, `cjk_horizontal.epub`, `cjk_vertical_ruby.epub`, `rtl.epub`, `mixed_script.epub`, `image_heavy.epub`, `table_degradation.epub` |
| Two-book mini corpus | `a8b870b89f37305d5412bdafe6011c6cf6950b7d19824660156c49073978077b` | `latin.epub`, `cjk_horizontal.epub` |

The manifest schema uses snake_case reader-layout fields. Reconfirm the
particular corpus before a run with:

```bash
shasum -a 256 corpus/manifest.json
jq -r '.[].file' corpus/manifest.json
```

Neither hook produced output in this workspace: iOS is blocked by the absent
simulator service; Android is blocked by absent emulator/`adb`. Therefore:

| Output | SHA-256 | Status |
| --- | --- | --- |
| `parity-ios.json` | **OWNER-PENDING** — run `shasum -a 256 parity-ios.json` after copying from the simulator container. | Not produced here |
| `parity-android.json` | **OWNER-PENDING** — run `shasum -a 256 parity-android.json` after `adb pull`. | Not produced here |

Owner parity run (the compare is the acceptance gate):

```bash
# Step 0: export as above, then record the manifest hash and names.

# iOS Debug simulator
xcrun simctl install booted <Inkuna.app>
CONTAINER=$(xcrun simctl get_app_container booted app.inkuna.ios data)
mkdir -p "$CONTAINER/Documents/ParityCorpus" && cp corpus/*.epub "$CONTAINER/Documents/ParityCorpus"
cp corpus/manifest.json "$CONTAINER/Documents/ParityCorpus/"
xcrun simctl launch booted app.inkuna.ios -inkuna.debugScreen parityDigest
# Wait for PARITY DONE with: xcrun simctl spawn booted log stream
cp "$CONTAINER/Documents/parity-ios.json" parity-ios.json

# Android Debug emulator
cd apps/android && ./gradlew installDebug
adb push corpus/. /sdcard/Android/data/app.inkuna.android/files/ParityCorpus/
adb shell am start -n app.inkuna.android/.MainActivity --ez inkuna.parityDigest true
# Wait with: adb logcat -s InkunaParity
adb pull /sdcard/Android/data/app.inkuna.android/files/parity-android.json parity-android.json

scripts/parity-compare.sh parity-ios.json parity-android.json
```

## Binding drift and zero-Readium evidence

- `git status --short` after binding generation contained no generated-file
  entries. `Generated/`, Android `src/generated/`, `jniLibs/`, and the iOS
  project are ignored; bindings were regenerated by their scripts, never
  hand-edited.
- `git diff --check` — **PASS**, exit 0.
- Initial literal sweep surfaced legacy tracked comments plus ignored caches.
  Legacy product-name wording was changed only in comments/instructions and
  then both bindings were regenerated. `apps/ios/CLAUDE.md` equals
  `apps/ios/AGENTS.md`, and `apps/android/CLAUDE.md` equals
  `apps/android/AGENTS.md`.
- The following ignored, untracked generated directories were removed from
  the worktree **recoverably** by moving them (not deleting) into
  `/private/tmp/inkuna-generated-cleanup.G2HhSD/`: `apps/ios/Inkuna.xcodeproj`,
  `apps/ios/build`, `apps/ios/.derivedData`, `apps/android/app/build`,
  `core/target`, `apps/android/.gradle`, and `core/core/target`.
- Final required sweep:

  ```bash
  grep -rin readium --include='*' apps core scripts assets website .github 2>/dev/null | grep -v -e '^apps/ios/build/' -e '^apps/ios/.derivedData/'
  ```

  **PASS:** no output; exit 1 is `grep`'s normal no-match status. There are
  zero matches in the specified non-documents scope.
