# Architecture decision record — 2026-08-14

## Decision

Rust core (UniFFI) + two fully native shells: UIKit on iOS, Jetpack Compose
on Android. Framework options evaluated: Flutter, React Native,
fully-native ×2, KMP core + native shells, Rust core + native shells.

Rendering: originally via the Readium native toolkits — **superseded by the
2026-08-21 amendment below**.

## Amendment — 2026-08-21: core-owned reader engine

The "Rust core never renders" boundary is retired. Readium (navigator and
streamer, both shells) is being removed and replaced by our own engine:

- **The core owns layout**: XHTML parse → opinionated style → text shaping
  (rustybuzz, bundled fonts) → line breaking → fixed-point pagination →
  per-page glyph-run display lists.
- **The shells own drawing and interaction**: they rasterize glyph runs
  natively (Core Text / `Canvas.drawGlyphs`), map semantic color roles
  through their theme tokens, and keep the custom pager, selection UI, and
  chrome.

Why: public WebView APIs are out-of-process and asynchronous, so the shells
could never synchronously know layout state at interaction time (the root of
the pager's rescue-layer bug class and the open-jank on Android); Android's
native text stack cannot lay out vertical CJK at all; and identical
cross-platform pagination plus core-computed positions (search, bookmarks,
future annotations/TTS) require shaping to happen once, in core, against one
set of font bytes. Positions are content coordinates
`(spine_idx, char_offset)` into a canonical text projection.

The `core/` workspace grows to five crates — `inkuna-content` (EPUB
container), `inkuna-format` (import conversion), `inkuna-engine` (layout),
`inkuna-core` (services), `inkuna-ffi` (bindings, facade objects) — and the
reader FFI gains a `ReaderSession` object with a synchronous cache-only
interaction path. Full design:
`docs/repertoire/specs/2026-08-21-reader-engine-swap-spec.md`.

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

## Near-term vertical writing (superseded 2026-08-21)

Original position: Readium renders EPUB in a WebView; WebKit/Chromium support
`writing-mode: vertical-rl`, so vertical books work from day one and the
custom Rust engine is a long-term option. The amendment above makes the
engine the plan of record: vertical CJK, ruby, and RTL progression are
first-class engine features, not WebView behaviors.

## Version targets

- iOS: min 18, built against iOS 26 SDK; Liquid Glass behind
  `if #available(iOS 26, *)`.
- Android: minSdk 33 (per-app language preferences for CJK users, single
  permission model, ~78% coverage), compile/target 35.
- IDs: `app.inkuna.ios`, `app.inkuna.android` (registered by owner).

## Format strategy (2026-08-14)

| Format | Import path | Rendering path |
|--------|------------|----------------|
| EPUB | native | core engine (`inkuna-engine` display lists → native drawing) |
| MOBI / AZW3 | convert to EPUB in core (DRM-free only) | core engine |
| TXT | charset detection (GB18030/Big5/Shift-JIS) + CJK chapter splitting → synthesized EPUB | core engine |
| Fixed-layout EPUB | detected at open | "not yet supported" state (v1) |
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

1. Readium Swift + Kotlin toolkits: shelf → open EPUB → paginated reading —
   shipped, now being replaced by the core-owned engine (2026-08-21
   amendment); Readium is removed entirely at the engine-swap parity gate.
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
