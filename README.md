<p align="center">
  <img src="assets/brand/appicon-glass-256.png" alt="Inkuna app icon" width="128">
</p>

# Inkuna

<!-- Dynamic badges: versions are read from the files on main, so they update
     with every scripts/bump-version.sh commit — no README edit needed. -->
[![core](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2FPythonShe%2FInkuna%2Fmain%2Fcore%2FCargo.toml&query=%24.workspace.package.version&prefix=v&label=core&logo=rust&color=b45309)](core/)
[![apps](https://img.shields.io/badge/dynamic/yaml?url=https%3A%2F%2Fraw.githubusercontent.com%2FPythonShe%2FInkuna%2Fmain%2Fapps%2Fios%2Fproject.yml&query=%24.settings.base.MARKETING_VERSION&prefix=v&label=ios%20%C2%B7%20android&color=b45309)](apps/)
[![license](https://img.shields.io/badge/license-AGPL--3.0-333333)](LICENSE)

A minimalist book reader where ink meets moonlight. Crafted, quiet, literary.

Website: [inkuna.app](https://inkuna.app)

## Architecture

One Rust core, two native shells. All non-UI logic (library database, import,
format detection, metadata, reading progress — later search, annotations, sync)
lives in Rust and is exposed to both platforms through
[UniFFI](https://mozilla.github.io/uniffi-rs/). Each shell is thin and fully
native so the reading experience feels at home on its platform. EPUB/CBZ
rendering is planned on the [Readium](https://readium.org) native toolkits.

```
core/           Rust workspace
  crates/inkuna-core/   pure-Rust domain: library, import, formats
  crates/inkuna-ffi/    UniFFI surface -> Swift + Kotlin bindings
apps/ios/       UIKit shell (min iOS 18, Liquid Glass gated on iOS 26)
apps/android/   Jetpack Compose shell (minSdk 33, targetSdk 37)
assets/         brand assets (icon sources + Icon Composer layers)
scripts/        core cross-build + bindings generation + icon rasters
website/        Astro static site for inkuna.app (Cloudflare Pages)
docs/           project documentation
```

Formats: EPUB, MOBI, AZW3 (DRM-free), TXT, PDF, with CBZ/CBR comics planned —
reflowable formats normalize to EPUB at import; fixed-layout formats get
dedicated navigators. CJK typography — including vertical writing — is a
first-class goal.

## Building

Requires the latest stable rustup toolchain (with iOS/Android targets),
cargo-ndk, Xcode, XcodeGen, Android SDK + NDK, and a current JDK.

```sh
# Core (tests)
cd core && cargo test

# iOS
./scripts/build-core-ios.sh
cd apps/ios && xcodegen generate && open Inkuna.xcodeproj

# Android
./scripts/build-core-android.sh
cd apps/android && ./gradlew assembleDebug
```

## Website

[inkuna.app](https://inkuna.app) is an Astro site in `website/` (fully static
output, zero client JS) on Cloudflare Pages. Pushes to `main` that touch
`website/` build and deploy automatically via GitHub Actions; locally, run
`pnpm install`, then `pnpm dev` / `pnpm build` inside `website/`. pnpm is the
only supported package manager here — the pinned version comes from the
`packageManager` field in `website/package.json` (`corepack enable pnpm` once,
and the right version is fetched for you), and `pnpm-lock.yaml` is the
committed lockfile.

## Releases

Releases are tag-driven, one tag per platform: `ios-vX.Y.Z+N` and
`android-vX.Y.Z+N` (X.Y.Z marketing version, N monotonic build number).
`scripts/bump-version.sh <component> <major|minor|patch> [--tag]` bumps a
component's version and mints the tag; pushing the tag builds the Rust core
and shell in CI, generates release notes from the commit log, and publishes
(TestFlight for iOS, a GitHub release with the APK for Android).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Pull requests require a signed
[Contributor License Agreement](https://gist.github.com/PythonShe/3c97ab17f679a42d675ffbebf62f42a2).

## License

[AGPL-3.0](LICENSE)
