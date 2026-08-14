# Inkuna — agent notes

Minimalist book reader (EPUB now; CBZ/CBR comics planned), Apple Books-level
feel is the quality bar. CJK support — vertical writing, CJK-aware search —
is a core product goal, not an afterthought. AGPL-3.0. Website inkuna.app,
dev PythonShe (dev@zheng-she.com).

## Architecture

Rust core + UniFFI, consumed by two fully native shells:

- `core/crates/inkuna-core` — pure Rust domain logic (no FFI types here).
- `core/crates/inkuna-ffi` — UniFFI proc-macro surface; mirrors core types as
  Records/Enums/Objects and converts at the boundary. Keep this boundary
  coarse-grained.
- `apps/ios` — UIKit (no SwiftUI by owner preference), XcodeGen project
  (`project.yml` is the source of truth; `Inkuna.xcodeproj` is generated and
  gitignored). Min iOS 18; iOS 26 Liquid Glass behind `if #available`.
- `apps/android` — Jetpack Compose, minSdk 33 / target+compile 35.
- Rendering will be Readium's native toolkits (Swift + Kotlin) in each shell;
  the Rust core never renders.

## Build commands

- Core: `cd core && cargo test`
- iOS: `./scripts/build-core-ios.sh` then `cd apps/ios && xcodegen generate &&
  xcodebuild -project Inkuna.xcodeproj -scheme Inkuna -destination
  'generic/platform=iOS Simulator' build`
- Android: `./scripts/build-core-android.sh` then
  `cd apps/android && ./gradlew assembleDebug`
- After changing anything in `core/crates/inkuna-ffi`, rerun BOTH
  build-core scripts to regenerate bindings before building the shells.

## Machine/toolchain gotchas (this dev machine)

- Two Rust installs exist: Homebrew (`/opt/homebrew/bin`, no cross targets)
  shadows rustup. The scripts pin `RUSTC` + `rustup run stable`; do the same
  for any new cargo invocation that cross-compiles.
- System Java is too new for AGP; `apps/android/gradle.properties` pins
  `org.gradle.java.home` to Homebrew `openjdk@21`.
- A global cargo config redirects target-dir to `~/.sonelis/cargo-target`;
  the scripts override with `CARGO_TARGET_DIR=core/target`.

## Conventions

- The FFI library object is named `Bookshelf`, not `Library` — UniFFI's
  Kotlin output imports JNA's `com.sun.jna.Library`, and the names collide.
- Bundle/application IDs: `app.inkuna.ios` and `app.inkuna.android`.
- Generated artifacts are never committed: `apps/ios/Generated/`,
  `apps/ios/Frameworks/`, `apps/android/app/src/generated/`, `jniLibs/`.
- Follow the Sonelis repo conventions for structure: `apps/<app>`, root
  `docs/` by topic, per-area `.gitignore` files.
