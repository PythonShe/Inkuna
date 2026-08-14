# Inkuna Android Rules

You are working inside `apps/android/`, the native Android shell. Follow the
rules below; they capture project-specific conventions and override common
defaults.

## 0. Tech Stack & Dev Commands

| Layer | Technology | Notes |
|------|------|------|
| UI | Jetpack Compose + Material 3, latest BOM | single-activity |
| Language | Kotlin (built into AGP 9+ — no standalone kotlin.android plugin) | latest stable AGP + Gradle wrapper |
| Core | UniFFI Kotlin bindings + JNA | generated into `app/src/generated/kotlin` by `../../scripts/build-core-android.sh`; core methods are `suspend` |
| Rendering (planned) | Readium Kotlin Toolkit | |
| Targets | minSdk 33, compile/target = latest SDK | application id `app.inkuna.android` |

```bash
../../scripts/build-core-android.sh   # after any core/FFI change
./gradlew assembleDebug               # must pass
```

`gradle.properties` pins `org.gradle.java.home` to the latest Homebrew
OpenJDK (CI strips that line and uses its own JDK); `local.properties` is
machine-local and gitignored.

Release signing reads `key.properties` + `app/upload-keystore.jks` (both
gitignored; CI materializes them from `ANDROID_*` secrets, local copies live
in `~/Keys/inkuna/`). Without them `assembleRelease` falls back to debug
signing.

## 1. Architecture

- The shell is thin: all domain logic goes through the Rust core
  (`app.inkuna.core.Bookshelf`). Never duplicate core logic (parsing, DB
  access, progress math) in Kotlin.
- Never edit generated bindings (`app/src/generated/`); regenerate via the
  script instead.
- Package root `app.inkuna.android`; feature packages per screen (`library`,
  later `reader`, `settings`).

## 2. Compose Conventions

- Material 3 with dynamic color eventually; the reader's ink/moonlight themes
  come later — system dark mode correctness comes now.
- State flows down, events flow up; no business logic in composables. Core
  calls belong in a store/repository layer, not inside `remember` blocks
  (the current `MainActivity` scaffold is a placeholder to be replaced by a
  ViewModel when the shelf becomes real).
- Design for Android idiom (predictive back, edge-to-edge) — this shell is a
  first-class Android app, not an iOS port.

## 3. Quality Bar

- CJK audiences are primary: per-app language (minSdk 33 `LocaleManager`) is
  a planned feature; all user-visible strings go to `strings.xml` resources —
  no hardcoded literals in composables once real UI lands.
