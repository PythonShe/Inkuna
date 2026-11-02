# Inkuna

Minimalist book reader (EPUB now; CBZ/CBR comics planned) for iOS and Android.
Apple Books-level feel is the quality bar. CJK support (vertical writing,
CJK-aware search) is a core product goal. License AGPL-3.0. Site `inkuna.app`.

| Directory | Stack | Role |
|---|---|---|
| `core/` | Rust workspace + UniFFI | All non-UI logic: library DB, import, formats, metadata, progress, search, reader layout engine |
| `apps/ios/` | UIKit + XcodeGen | iOS shell (`app.inkuna.ios`), min iOS 18 |
| `apps/android/` | Kotlin + Jetpack Compose | Android shell (`app.inkuna.android`), minSdk 33 |
| `website/` | Astro (static) + pnpm | Marketing site, Cloudflare Pages |
| `scripts/` | bash | Core cross-builds, UniFFI bindings, version bumps, icons |

- Reader boundary: the Rust core owns layout (XHTML parse, style, shaping with
  bundled fonts, line breaking, pagination) and emits per-page glyph-run
  display lists; the shells own drawing (Core Text / `Canvas.drawGlyphs`) and
  all interaction.
- Each component has its own `CLAUDE.md`; follow it too. Cross-component
  changes follow this file, then each affected component's.
- `docs/dev/architecture.md` is the architecture decision record; read it
  before structural changes.

## Stack policy

- Newest stable everything (Rust, Swift, Kotlin, Gradle, AGP, JDK, SDKs,
  deps). A dependency that caps another below latest loses (refinery is
  deferred because it pins rusqlite below current).
- Designated crates for upcoming needs: `deadpool-sqlite`, `notify`
  (watch-folder import), `argon2` (if auth lands), `lofty` (if audio lands).
- Reflowable formats (EPUB, MOBI, AZW3, TXT) normalize to EPUB at import;
  fixed-layout (PDF, CBZ/CBR) get dedicated navigators. MOBI/AZW3 are
  DRM-free only; never implement or integrate DRM circumvention.

## Build gotchas

- After any `core/crates/inkuna-ffi` change, regenerate bindings with BOTH
  `scripts/build-core-ios.sh` and `scripts/build-core-android.sh` before
  building the shells.
- Cross-compiles need rustup's stable, not Homebrew's rustc: the scripts pin
  `RUSTC` to `rustup which --toolchain stable rustc` and wrap
  `rustup run stable cargo`; do the same (plus `RUSTDOC` for doctests) for
  any new cross-compiling cargo call, or it fails with E0463. The scripts
  also set `CARGO_TARGET_DIR=core/target`.
- Generated, never committed: `apps/ios/Generated/`, `apps/ios/Frameworks/`,
  `apps/ios/Inkuna.xcodeproj/`, `apps/android/app/src/generated/`,
  `apps/android/app/src/main/jniLibs/`, `core/target/`.
- No component-specific source or config at the repo root.

## Release and CI

- `deploy-website.yml`: builds `website/` and deploys to Cloudflare Pages
  (project `inkuna`) on `main` pushes touching `website/` and on release
  publish.
- `release-ios.yml` / `release-android.yml`: tag-driven (`ios-vX.Y.Z+N` /
  `android-vX.Y.Z+N`); core tests gate the build; iOS uploads to TestFlight,
  Android attaches a signed APK to a GitHub release.
- `release-notes.yml`: reusable Claude-written release notes (override model
  via `LLM_MODEL` / `LLM_EFFORT` repo variables).
- Versions live in `core/Cargo.toml`, `apps/ios/project.yml`,
  `apps/android/app/build.gradle.kts`. Bump only via
  `scripts/bump-version.sh <core|ios|android> <major|minor|patch|X.Y.Z> [--tag]`;
  release workflows fail on tag/file mismatch.
- CI secrets: `ANDROID_*` (signing; local copies in `~/Documents/Dev/inkuna/`),
  `APPSTORE_*` / `IOS_*` / `APPLE_TEAM_ID`, `CLOUDFLARE_*`,
  `ANTHROPIC_API_KEY`. iOS ExportOptions.plist is generated in CI.

## Commit scopes

`core`, `ios`, `android`, `website`, `docs`, `workspace` (root files,
`scripts/`). Combine for cross-component work (`core,ios`); refine with a
sub-scope (`core/epub`, `ios/reader`).
