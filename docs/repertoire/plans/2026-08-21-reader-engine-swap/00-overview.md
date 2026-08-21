# Reader Engine Swap — Plan Set Overview

> **For the conductor:** this goal ships as 2 coordinated plans, listed below
> in execution order. Conduct one plan per `/repertoire:maestro` run, in
> dependency order, and read this overview before any plan. This file defines
> the cross-plan contracts — it contains no tasks itself.

**Spec:** `docs/repertoire/specs/2026-08-21-reader-engine-swap-spec.md`
**Goal:** Remove Readium entirely and ship Inkuna's own reader engine — Rust
core owns layout and emits per-page glyph-run display lists; the shells draw
them natively and keep the custom pager.
**Architecture:** The core (5-crate workspace after restructure) runs a
deterministic fixed-point pipeline — XHTML parse → opinionated style →
rustybuzz shaping with bundled fonts → UAX-14 breaking → progressive
pagination — behind a new `ReaderSession` FFI object whose interaction path is
synchronous and cache-only. The work splits at the FFI boundary: plan 01 is
everything Rust (engine + FFI + data rebaseline), plan 02 is both shells plus
the parity gate. The docs/ADR/CLAUDE.md sweep is already done (commit
`7a11273`) — no plan re-does it.
**Build:** core `cd core && cargo build --workspace` · iOS
`cd apps/ios && xcodegen generate && xcodebuild -project Inkuna.xcodeproj
-scheme Inkuna -destination 'generic/platform=iOS Simulator' build` · Android
`cd apps/android && ./gradlew assembleDebug`
**Test:** `cd core && cargo test --workspace` (single test:
`cargo test -p <crate> <module::path>`). Neither shell has a test target;
shell verification is compile + the plan-02 parity movement.
**Bindings:** after ANY `inkuna-ffi` change run BOTH
`./scripts/build-core-ios.sh` and `./scripts/build-core-android.sh` (repo
root) before touching shell code. Never strip before bindgen; never hand-edit
generated files.

## The plans

| # | Plan | Delivers | Depends on |
|---|------|----------|------------|
| 01 | `plan-01-core-engine.md` | 5-crate restructure, layout engine, `ReaderSession` FFI, V8 + reconcile, search unification — engine complete and tested behind regenerated bindings | — |
| 02 | `plan-02-shells.md` | Both shells reading on the engine (PageView, pager surfaces, selection, a11y), Readium fully removed, parity-gate evidence | 01 |

**Execution shape:** stacked chain — both conducted on `dev/core`, plan 02
directly on top of plan 01's landed work. `main` stays the frozen Readium beta
until the parity movement at the end of plan 02 passes; only then does
`dev/core` merge.

## Shared contracts

Everything below is **defined and implemented in plan 01** and **consumed in
plan 02** through the generated bindings (Swift: types visible directly, no
`import InkunaCore`; Kotlin: package `app.inkuna.core`, methods `suspend`
where async). Shapes are written language-neutrally; UniFFI maps `u32`/`u64`
→ UInt32/ULong etc. All geometry is in **layout points at 1× scale** — shells
apply screen scale when drawing. All `char_offset`/`CharRange` values index
the **canonical text projection** (spec §2) and are Unicode scalar counts.

### Root object & facades

- `Bookshelf.open(data_dir: String, font_dir: String) -> Bookshelf` —
  **signature changes** (breaking; shells pass their bundled fonts directory).
- Facade accessors (methods relocate, signatures otherwise unchanged from
  today's `Bookshelf` methods): `library() -> ShelfLibrary`,
  `importer() -> ShelfImport`, `search() -> ShelfSearch`,
  `settings() -> ShelfSettings`, `progress() -> ShelfProgress`,
  `stats() -> ShelfStats`. Shell call-site migration is mechanical:
  `bookshelf.foo(...)` → `bookshelf.<facade>().foo(...)`.
- `open_reader(id: String, viewport: Viewport, settings: ReaderLayoutSettings,
  listener: LayoutListener) -> ReaderSession` — async. Opening an id with a
  live session closes and replaces it (last-open-wins).

### Records & enums

- `Viewport { width: f64, height: f64 }` — the page content area available to
  the engine: excludes shell chrome insets, **includes** reading margins
  (margins come from settings and are applied inside core).
- `ReaderLayoutSettings { reading_font: String, reading_bold: bool,
  text_size_step: u8, line_spacing: f64, letter_spacing: f64,
  word_spacing: f64, reading_margins: u32 }` — same stored values as today's
  settings feature (V7 fields); shells populate the record from wherever those
  six values are read today and pass it verbatim; core interprets units
  (spec §9).
- `Coordinate { spine_idx: u32, char_offset: u64 }`
- `CharRange { start: u64, end: u64 }` — end exclusive.
- `Rect { x: f64, y: f64, width: f64, height: f64 }`
- `WritingMode { HorizontalTb, VerticalRl }`
- `SelectionRect { rect: Rect, writing_mode: WritingMode }`
- `ChapterGeometry { generation: u64, page_count: u32, char_range: CharRange,
  writing_mode: WritingMode, rtl_progression: bool, truncated: bool }` —
  `truncated` is true when the resource exceeded a layout/parse budget and
  renders only its laid-out prefix; shells show a localized truncation notice.
- `PageLocation { generation: u64, spine_idx: u32, page_idx: u32 }`
- `HitResult { coordinate: Coordinate, link_target: Option<String> }`
- `ColorRole { Text, Secondary, Link }`
- `RunOrientation { Upright, SidewaysRotated }`
- `GlyphRun { font_id: u32, size: f64, color_role: ColorRole,
  glyph_ids: Vec<u16>, positions: Vec<f32> /* x,y interleaved, len = 2×glyphs */,
  orientation: RunOrientation }`
- `ImagePlacement { href: String, rect: Rect }`
- `DecorationKind { Rule, Underline }` ·
  `Decoration { kind: DecorationKind, rect: Rect, color_role: ColorRole }` —
  core assigns the role (Rule → Secondary; Underline → Text, or Link inside a
  link region); shells never infer decoration colors.
- `LinkRegion { rect: Rect, target: String }`
- `A11yRole { Body, Heading, Link }` · `A11yBlock { text: String, rect: Rect,
  lang: Option<String>, is_link: bool, role: A11yRole }`
- `PageDisplayList { generation: u64, glyph_runs: Vec<GlyphRun>,
  images: Vec<ImagePlacement>, decorations: Vec<Decoration>,
  links: Vec<LinkRegion>, a11y: Vec<A11yBlock> }`
- `FontAxis { tag: String, value: f64 }` · `FontEntry { id: u32,
  file_path: String, collection_index: u32, axes: Vec<FontAxis> }`

### `ReaderSession` methods

Synchronous, non-blocking, cache-only (safe on the UI thread; throw
`InkunaError.NotReady` instead of blocking; calling `chapter`/`page` on an
un-laid-out chapter schedules its layout):
`chapter(spine_idx: u32) -> ChapterGeometry` ·
`page(spine_idx: u32, page_idx: u32) -> PageDisplayList` ·
`is_ready(spine_idx: u32) -> bool` ·
`locate(coordinate: Coordinate) -> PageLocation` ·
`locate_href(href: String, fragment: Option<String>) -> Coordinate` (throws
`AnchorNotFound`) ·
`hit_test(spine_idx: u32, page_idx: u32, x: f64, y: f64) -> HitResult` ·
`selection_rects(spine_idx: u32, range: CharRange) -> Vec<SelectionRect>` ·
`word_at(coordinate: Coordinate) -> CharRange` ·
`text_range(spine_idx: u32, range: CharRange) -> String` ·
`match_rects(spine_idx: u32, char_offset: u64, len: u64) -> Vec<SelectionRect>` ·
`accessibility_blocks(spine_idx: u32, page_idx: u32) -> Vec<A11yBlock>` ·
`font_registry() -> Vec<FontEntry>` ·
`spine_count() -> u32` ·
`page_char_range(spine_idx: u32, page_idx: u32) -> CharRange` — the exact
projection range a laid-out page covers (page anchors, progress reporting,
accessibility) ·
`position_of(coordinate: Coordinate) -> u32` and `position_count() -> u32` —
1-based synthetic position lookup over the publication's position table,
snapshotted into the session at open (sync-safe); shells never mirror the
1024-char constant. Outside a session (Home/Detail screens),
`ShelfProgress` gains the async twins
`position_of(id: String, coordinate: Coordinate) -> u32` and
`position_count(id: String) -> u32` backed by the same `resource_positions`
rows ·
`page_digest(spine_idx: u32, page_idx: u32) -> String` — blake3 hex of the
canonical display-list serialization; the parity harness compares it across
platforms.

Async: `update_layout(viewport: Viewport, settings: ReaderLayoutSettings)`
(bumps generation; listener re-fires) · `resource(href: String) -> Vec<u8>`
(image bytes, budget-capped).

### Callback interface

`LayoutListener` (foreign-implemented, like `ImportProgressListener`):
`on_first_page_ready(generation: u64, spine_idx: u32)` ·
`on_chapter_ready(generation: u64, spine_idx: u32, page_count: u32)`.
Callbacks may arrive on any thread; shells hop to their main thread.

### Errors & degradation signaling

New `InkunaError` variants (all with `detail: String`): `NotReady`,
`UnsupportedContent`, `LayoutBudgetExceeded`, `AnchorNotFound`. Existing
variants unchanged; `InvalidPositionRanges` is deleted in plan 01 (along
with `report_position_ranges` and `report_position_count`).

Degradation contract: a **fixed-layout book** fails at `open_reader` with
`UnsupportedContent { detail: "fixed-layout" }` — shells show the localized
"not yet supported" state. A **single unparseable resource** throws
`UnsupportedContent` from `chapter()`/`page()` for that spine index only —
shells render a localized placeholder page there; the rest of the book works.
A **budget-truncated resource** lays out its prefix and sets
`ChapterGeometry.truncated` — shells show a truncation notice.

### Fonts

`assets/fonts/` at repo root is the single source of truth (plan 01 creates
it and moves/adds files; plan 02 wires bundle copying). Contents: the four
existing Noto Latin TTFs (NotoSans, NotoSans-Italic, NotoSerif,
NotoSerif-Italic), Latin Bold/BoldItalic statics, a symbols face (fallback
chain), OFL.txt, plus the Noto CJK faces chosen in plan 01 (exact files
recorded in plan 01's commit; plan 02 copies the directory wholesale — it
never names individual font files). Shells pass the bundled directory path
as `font_dir` and build platform fonts from `FontEntry.file_path` +
`collection_index` + `axes`.

### Data

Plan 01 owns all DB changes (V8 + reconcile). Plan 02 never touches SQL; it
reads/writes positions only through existing progress/bookmark FFI methods,
which plan 01 migrates to `Coordinate` in/out (same method names, `locator:
String` parameters/fields become `coordinate: Coordinate`).

### Parity fixture export

Plan 01 ships `cargo run -p inkuna-engine --example export-parity-fixtures --
<dir>` (repo: run from `core/`): writes the golden fixture EPUBs
deterministically plus `manifest.json` — an array of
`{ file, viewport: {width, height}, settings: <ReaderLayoutSettings values> }`
cases. Plan 02's digest harness imports exactly these files and runs exactly
these cases on both platforms; it never invents its own corpus.

### Parity numbers (plan 02's gate, fixed here so neither plan re-derives)

Tap-to-first-rendered-page ≤ 250 ms and session-open-to-first-page-ready
≤ 100 ms, cold open, reference devices (owner's iOS dev device; Pixel-class
emulator), seeded benchmark library incl. one long-chapter book;
`page_digest` equality across iOS and Android for the whole fixture corpus;
zero `readium` references outside docs/git history (ignore `apps/ios/build/`
and `apps/ios/.derivedData/` — gitignored artifacts).

### Readium-era workaround inventory (plan 02 deletes — never ports)

Every fix that exists only because layout lived in an async WebView dies with
it (owner directive). iOS `ReaderPager.swift`/`ReadiumPagerSurface.swift`:
`seedInnerMax` reseeding, edge-flick rescue routing, boundary-commit
verification/retries, widened arrival-commit thresholds — the pager keeps its
physics, springs, and chained-turn velocity (feel features, not workarounds).
Android: the same rescue family in `ReaderPagerLayout.kt`, the
`transitionSettled` navigator-mount latch in `ReaderScreen.kt` (commit
`0c899c1`), `ReaderWebViewTuner.kt`, `ReaderPageTurnListener.kt`,
`ReaderStyleInjector.kt`, ViewPager fake-drag glue, and the four
Readium-forced build constraints (`androidx.webkit` strict pin, core-library
desugaring, `viewpager`, `fragment-ktx` — each removed after verifying no
other consumer). Both shells: `ReaderUserStyle.swift`/`ReaderUserCss.kt` (CSS
generators; §9 constants transcribed to core first), `ChapterHref.swift`,
`ReadingFontDeclarations.swift`. When in doubt whether something is a
workaround or a feel feature: rescue/verify/latch/timing-compensation logic is
a workaround; gesture physics and animation curves are feel.

## Shared conventions & constraints

- All UniFFI exports stay in `inkuna-ffi` (bindgen is `--library`-mode over
  one artifact); `blocking()` is crate-private — facades live in `inkuna-ffi`
  modules, never sibling crates. Everything `inkuna-ffi` consumes must be
  `pub use`d from `inkuna_core`'s (or the new crates') roots per the
  crate-root re-export rule.
- Rust conventions (all crates): thiserror-only, no `unwrap`/`expect` outside
  tests, declaration-only `mod.rs`, ≤400-line target / 500 ceiling, sibling
  `*_tests.rs`, CJK fixtures mandatory, no binary fixtures in git.
- Cross-compiles must pin `RUSTC` (`rustup which --toolchain stable rustc`) —
  the build scripts already do; plain `cargo test` needs no pinning.
- iOS: Swift 6 strict concurrency, min iOS 18, XcodeGen globbed sources (new
  files under `Inkuna/` need no `project.yml` edit; new resource dirs do),
  strings via `Localizable.xcstrings`. Android: minSdk 33, no version
  catalog, strings in `res/values*/strings*.xml` — a new user-facing string
  lands in all 14 locale dirs (English text as placeholder in non-English
  locales is acceptable; translation is a follow-up).
- Every `CLAUDE.md` edit re-copies its sibling `AGENTS.md` in the same commit.
- Commit format `<type>(<scope>): <description>`; scopes
  `core`/`ios`/`android`/`workspace` (+ sub-dimensions like `core/engine`).
- No version bumps in either plan (owner runs `scripts/bump-version.sh` at
  release time).

## Out of scope (no plan absorbs these)

- Highlights/annotations, TTS, dictionary popovers, scrolled mode,
  system-font reading mode, hyphenation, fixed-layout rendering, per-element
  writing-mode mixing, table grid layout, publisher embedded fonts, MathML,
  SVG, media overlays, cross-page drag selection, sync (spec Non-goals /
  Out of scope).
- The docs/ADR/CLAUDE.md sweep (already landed as `7a11273`); only in-code
  doc comments that move with restructured files are touched, by plan 01.
- TestFlight/store releases, version bumps, `main` merges — owner actions
  after the gate.
