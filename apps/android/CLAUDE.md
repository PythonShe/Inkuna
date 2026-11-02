# Inkuna Android

| Layer | Choice | Notes |
|---|---|---|
| UI | Jetpack Compose + Material 3 | single activity |
| Language | Kotlin built into AGP 9+ | no standalone `kotlin.android` plugin |
| Core | UniFFI Kotlin bindings + JNA | generated into `app/src/generated/kotlin` by `../../scripts/build-core-android.sh`; core methods are `suspend` |
| Rendering | `ui/reader/engine/PageView.kt` | draws core display-list glyph runs via `Canvas.drawGlyphs` (API 31+) |
| ABIs | local `arm64-v8a` + `x86_64`; CI release `arm64-v8a` | never armeabi-v7a; `ANDROID_ABIS` (core script) and `-PinkunaAbis` (Gradle) must agree |

```bash
../../scripts/build-core-android.sh   # after any core/FFI change
./gradlew assembleDebug               # must pass
```

- `gradle.properties` pins `org.gradle.java.home` to Homebrew OpenJDK (CI
  strips that line); if Gradle lags the newest JDK, use Android Studio's JBR.
  `local.properties` is machine-local.
- Release signing reads `key.properties` + `app/upload-keystore.jks`
  (gitignored; CI writes them from `ANDROID_*` secrets). Without them
  `assembleRelease` falls back to debug signing.

## Architecture

- Thin shell: all domain logic goes through the Rust core
  (`app.inkuna.core.Bookshelf`). Never duplicate parsing, DB access, or
  progress math in Kotlin.
- Package root `app.inkuna.android`; screens under `ui/<feature>/`.
- State flows down, events flow up. Core calls belong in ViewModels or
  stores, never in composables or `remember` blocks.

## Conventions

- Android idiom (predictive back, edge-to-edge); this is not an iOS port.
- Correct in light and dark.
- User-visible strings go in `strings.xml` with translations in
  `values-<locale>/` (16 languages, per-app language via
  `res/xml/locales_config.xml`); never a hardcoded literal in a composable.
