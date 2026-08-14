# Inkuna

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
scripts/        core cross-build + bindings generation
website/        static site for inkuna.app (Cloudflare Pages)
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

[inkuna.app](https://inkuna.app) is a hand-written static site in `website/`
(no framework, no build step) deployed via Cloudflare Pages: root directory
`website`, empty build command, output directory `.`. Pushes to `main` that
touch `website/` deploy automatically.

## License

[AGPL-3.0](LICENSE)
