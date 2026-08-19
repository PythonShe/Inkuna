# Architecture decision record — 2026-08-14

## Decision

Rust core (UniFFI) + two fully native shells: UIKit on iOS, Jetpack Compose
on Android. Rendering via the Readium native toolkits (planned). Framework
options evaluated: Flutter, React Native, fully-native ×2, KMP core + native
shells, Rust core + native shells.

## Why not Flutter / React Native

- No production-grade EPUB stack; Readium has no Flutter/RN toolkit.
- The Apple Books feel (spring physics, interruptible gestures, haptics,
  materials) is approximated, never exact. For a craft-first reader the
  last 5% is the product.

## Why not fully native ×2

All non-UI logic (library DB, import, progress, annotations, search, sync)
would be written and maintained twice and drift apart.

## Why Rust over KMP for the shared core

- **CJK ceiling**: tantivy + jieba for real Chinese full-text search;
  rustybuzz/cosmic-text/hyphenation if we ever build our own typesetting
  engine (the only path to first-class vertical writing outside a WebView).
  Kotlin has no cross-platform equivalent.
- **Reuse**: the core compiles to WASM — the same annotation/progress/sync
  logic can run in a Cloudflare Worker or a future web/desktop reader.
  Kotlin/Native cannot credibly do this.
- **No GC runtime** embedded in the iOS binary; smaller and leaner than
  Kotlin/Native.
- KMP's remaining advantage (frictionless Android consumption) is small:
  UniFFI generates idiomatic-enough Kotlin, and the FFI plumbing cost is
  borne by tooling/agents, not the owner.

## Near-term vertical writing

Readium renders EPUB in a WebView; WebKit/Chromium support
`writing-mode: vertical-rl`, and Readium CSS has explicit CJK vertical
support. So vertical books work from day one; the custom Rust engine is a
long-term option, not a prerequisite.

## Version targets

- iOS: min 18, built against iOS 26 SDK; Liquid Glass behind
  `if #available(iOS 26, *)`.
- Android: minSdk 33 (per-app language preferences for CJK users, single
  permission model, ~78% coverage), compile/target 35.
- IDs: `app.inkuna.ios`, `app.inkuna.android` (registered by owner).

## Format strategy (2026-08-14)

| Format | Import path | Rendering path |
|--------|------------|----------------|
| EPUB | native | Readium EPUB navigator (WebView) |
| MOBI / AZW3 | convert to EPUB in core (DRM-free only) | Readium EPUB navigator |
| TXT | charset detection (GB18030/Big5/Shift-JIS) + CJK chapter splitting → synthesized EPUB | Readium EPUB navigator |
| PDF | metadata only | PDFKit (iOS) / Pdfium (Android) navigator |
| CBZ / CBR | zip / unrar | image navigator |

Everything reflowable becomes EPUB at import so there is exactly one text
rendering pipeline; fixed-layout formats bypass it. No DRM circumvention,
ever.

## Stack policy (2026-08-14)

Latest stable everything — Rust, Swift (Swift 6 language mode), Kotlin (AGP
built-in), Gradle, AGP, JDK, compile/target SDK, and all dependencies —
verified against the live registries at bump time. Prefer mainstream crates
over hand-rolling; a crate that pins another dependency below latest is
deferred (current example: refinery vs rusqlite — schema versioning is
hand-rolled via `user_version` until refinery catches up). Reserved crates
for upcoming needs: deadpool-sqlite, notify, rayon, argon2, lofty.

## Roadmap (next steps)

1. Readium Swift + Kotlin toolkits: shelf → open EPUB → paginated reading.
2. Import UI (document picker on both platforms; copy into app storage).
3. Reader chrome: themes (ink/moonlight), typography controls, scrubber.
4. Reflowable formats — shipped (2026-08): MOBI6 + KF8/AZW3 (hand-rolled
   clean-room Palm/KF8 readers, DRM-free only) and TXT (chardetng charset
   detection, Legado-style CJK chapter rules) all normalize to EPUB at
   import via the core's own EPUB 3 writer.
5. Comics: CBZ via Readium; CBR via unrar in the core.
6. Full-text search in core — shipped: tantivy + jieba library-wide index
   (plus a CJK-unigram field for single-char/substring queries) and an
   exact case-folded scan for in-book search, both over the
   `resource_text` corpus.
7. Sync: core logic compiled to WASM on Cloudflare Workers.
