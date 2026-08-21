# Reader Engine Swap — Readium Removal and Core-Owned Layout

Status: draft for adversarial review · Branch: `dev/core` · Date: 2026-08-21

## Problem & context

Inkuna renders reflowable books through Readium's WebView navigators (Swift
toolkit 3.11.0, Kotlin toolkit 3.3.0). Every hard reader bug of the past
releases traces to one root: public WebView APIs are out-of-process and
asynchronous, so the shell can never synchronously know layout state at
interaction time. The custom pager (PR #9) already carries rescue layers for
this — `seedInnerMax` reseeding, flick-rescue routing, boundary commit
verification, a `transitionSettled` latch to hide Chromium process-spawn jank —
and each new reader feature built on the WebView path (highlights, TTS ranges,
search rendering) would deepen the debt before an already-planned custom
frontend replaces it.

Meanwhile the product's core goals — first-class vertical CJK writing, ruby,
identical cross-platform pagination, core-computed positions feeding search and
future annotations — are either impossible on Android's native text stack or
permanently hostage to WebView behavior.

This spec defines the full swap: Readium is removed from both shells (navigator
AND streamer/publication layers), and the Rust core takes ownership of the
entire layout pipeline — XHTML parsing, styling, text shaping, line breaking,
pagination, and positioned glyph runs. The shells become thin painters and
interaction owners. The app is in TestFlight/beta; **no backward-compatible
design is carried** (user directive): stored Readium locators are rebaselined,
the DB schema migrates destructively where needed, and the reader FFI surface is
redesigned clean.

## Terminology

- **Display list** — the per-page render description the core emits: positioned
  glyph runs, image placements, decoration geometry, link regions.
- **Content coordinate** — `(spine_idx, char_offset)`: a position in the
  publication independent of layout, fonts, or settings. Replaces Readium
  locator JSON everywhere.
- **Synthetic positions** — the existing `resource_positions` model (fixed-size
  character blocks per resource) used for "position N of M" and progress
  percentage; layout-independent by design.
- **Parity gate** — the checklist that must pass before `dev/core` merges to
  `main` (§14).
- **Reading face / font registry** — the bundled font binaries the core shapes
  with and the shells rasterize with; identified by stable font IDs.

## Goals / non-goals

**Goals**

- Remove Readium entirely from both shells; zero Readium dependencies remain.
- Rust core owns parse → style → shape → line-break → paginate; shells draw
  glyph runs and own interaction.
- Byte-identical pagination across platforms on the bundled-font default path.
- First-class vertical CJK (`vertical-rl`, vertical punctuation forms, sideways
  Latin), ruby annotations, RTL page progression.
- Positions, hit-testing, selection geometry, and search-highlight geometry
  computed in core; content coordinates as the single position model.
- Native text selection (select → copy/look-up/share) on both platforms —
  inside the v1 parity gate.
- The custom pager survives with its physics and feel intact, minus every
  WebView rescue layer.
- `core/` restructured into focused crates that keep the file-size and module
  conventions workable as the engine lands.
- `docs/dev/architecture.md` amended as an ADR-level change; every echo of the
  "core never renders" rule updated.

**Non-goals**

- Fixed-layout (pre-paginated) EPUB rendering — detected and surfaced as
  "not yet supported" in v1.
- High-fidelity publisher CSS. The engine is deliberately opinionated (§6).
- Scrolled (non-paginated) reading mode; the reader is paginated-only today and
  stays so.
- Highlights/annotations, TTS, dictionary popovers — designed-for (the position
  model serves them) but not built here.
- MathML, SVG rendering, audio/media overlays, embedded fonts from publishers.
- CBZ/CBR comics — separate dedicated-navigator track per the format strategy.
- Any Readium fallback path, feature flag, or dual-path plumbing.

## Approach

**Chosen: core-computed glyph-run display lists, natively rasterized.** The
core shapes and paginates with bundled font binaries and emits one owned
display-list record per page; shells register the same font files with Core
Text / `Typeface` and draw via `CTFontDrawGlyphs` (iOS) and `Canvas.drawGlyphs`
(Android, API 31+; minSdk is 33). Pagination is identical because shaping
happens once, in core, against one set of font bytes; rasterization stays
native so text keeps each platform's rendering quality and GPU glyph caching.

Rejected alternatives:

- *Core rasterizes pages to bitmaps; shells blit.* Identical pixels, but
  full-page bitmaps at 3× scale are memory/battery-expensive, theme toggles
  force re-rasterization, and shell-side geometry is still needed for selection
  and taps — it buys little over the chosen approach and costs a rasterizer.
- *Core stops at styled text; shells do line layout.* Per-platform line
  breaking kills identical pagination, and Android's text stack cannot lay out
  vertical CJK — the original reason this project exists.

Decisions locked during the design dialogue:

1. **Fonts: bundled default, optional system.** Bundled faces guarantee
   identical pagination; a user-selectable system-font mode loads platform font
   files by path with per-device pagination as a documented consequence.
   Content coordinates keep all stored positions valid in both modes.
2. **Opinionated typography.** Reader settings and theme own the look;
   publisher CSS is mined for semantics only (§6).
3. **Hard cut on `dev/core`.** Readium is removed on this branch now; `main`
   keeps shipping the frozen Readium beta (bugfix-only); merge happens at the
   parity gate. No dual path.
4. **Fixed-layout unsupported in v1** with clean detection and a localized
   "not yet supported" state.
5. **No backward compatibility.** Beta-stage app: destructive rebaselines
   allowed; existing reading positions convert at chapter-level accuracy, once.
6. **Selection is in the v1 gate** (strict parity with the current reader).

## 1. Workspace restructure

`core/crates/` grows from 2 to 5 crates. Only `inkuna-ffi` exports FFI symbols,
so the `--library` bindgen flow and both build scripts are unchanged; CI
hardcodes no crate names.

| Crate | Contents | Depends on |
|---|---|---|
| `inkuna-content` | EPUB container layer extracted from `formats/epub/`: bounded zip reads (`archive.rs`), `container.xml`, OPF/spine/manifest (media types now retained), nav/NCX TOC, href normalization, entity/XML helpers. The resource-bounds postmortem rules are enforced here, once, for import and engine alike. | — |
| `inkuna-format` | Import-side conversion moved wholesale: MOBI/AZW3/TXT→EPUB converters and the EPUB 3 writer (`write.rs`), plus their CSS scrubbers. | `inkuna-content` |
| `inkuna-engine` | The reader engine (§2): modules `session/`, `dom/`, `style/`, `shape/`, `layout/`, `paginate/`, `display/`. | `inkuna-content` |
| `inkuna-core` | What remains: `Library` facade, DB (pool, migrate, schema), import orchestration, features (search, progress, settings, stats). | `inkuna-content`, `inkuna-format`, `inkuna-engine` |
| `inkuna-ffi` | UniFFI surface, restructured into facades (§3). | `inkuna-core` |

Conventions carry over unchanged to all crates: thiserror-only errors (each
crate gets its own error enum, converted into `CoreError` at the `inkuna-core`
boundary), declaration-only `mod.rs`, ≤400-line target / 500 hard ceiling,
sibling `*_tests.rs` files, CJK fixtures mandatory, no binary fixtures in git
(fixture builders extend `test_support.rs` patterns; `inkuna-content` hosts the
shared EPUB builder so all crates can construct fixtures).

New dependencies (latest stable verified against live registries at
implementation time, per stack policy): `rustybuzz` (+ its `ttf-parser`),
`unicode-bidi`, `unicode-script`, `unicode-linebreak`, `hyphenation`,
`cssparser`, `icu_segmenter` (word boundaries for selection; sibling of the
`icu_*` crates already in-tree). Deliberately **not** used: `cosmic-text`
(no vertical-writing support — the gap this engine exists to fill) and
`html5ever` (see assumption A1).

## 2. Engine pipeline (`inkuna-engine`)

Per spine resource, a pure, deterministic pipeline with no I/O past the archive
read: **parse → style → shape → break → paginate**. Determinism is a hard
invariant: identical inputs (resource bytes, viewport, settings fingerprint,
font set, engine version) produce identical output on every platform — this is
what makes cross-platform pagination a test rather than a hope.

**Parse (`dom/`).** quick-xml streaming into a compact arena tree (node kinds:
element, text; attributes interned). Extends the proven `text.rs` machinery:
entity resolution, `SKIPPED` (head/script/style/template) and `BLOCK` element
sets, per-resource budgets. EPUB 3 mandates XHTML and all non-EPUB imports are
core-authored XHTML; the parser is leniently recovering for mild malformation
(unclosed inline tags, stray entities) and fails closed into a per-book
"unsupported content" error state — never a panic — for structurally broken
input.

**Style (`style/`).** Opinionated resolution. Element semantics drive
structure: h1–h6, p, em/i, strong/b, blockquote, pre/code, ol/ul/li, ruby/rt/rb,
img/image, table/tr/td (simple tables), br, hr, a. A `cssparser`-based pass
over publisher stylesheets and inline `style` attributes mines only the honored
properties: `writing-mode` (horizontal-tb / vertical-rl), `direction`,
`font-style`, `font-weight`, `text-align`, `ruby-position`, `display: none`.
Everything visual — faces, sizes, line spacing, letter/word spacing, margins,
colors — comes from reader settings and theme roles. Class selectors resolve
only as carriers of the honored properties; cascade order is spec-correct
within the mined subset. Simple tables lay out as equal-weight grids capped at
a complexity budget (≤ 8 columns, no spans); tables over budget degrade to
sequential blocks.

**Shape (`shape/`).** Runs are itemized by script (`unicode-script`) and bidi
level (`unicode-bidi`), then shaped with `rustybuzz` against the font registry
with an explicit fallback chain: reading face → CJK face → symbol face. Missing
glyphs fall through the chain; final fallback renders `.notdef` (never drops
text). Vertical mode shapes with `vert`/`vrt2` features, applies vertical
punctuation forms, and marks sideways-Latin runs with a rotation flag for the
display list. Ruby annotations shape as attached runs sized by the engine
(base-relative scale) and positioned per `ruby-position`.

**Break (`layout/`).** UAX #14 line breaking (`unicode-linebreak` — encodes
CJK kinsoku), plus `hyphenation` for Latin-script languages keyed off the
publication language. Justified by default: inter-word stretch for Latin,
inter-character for CJK; last lines ragged. Line height, paragraph spacing and
indents come from settings-derived metrics (matching the current reader's
default look, including the EPUB writer's `text-indent: 2em` convention).

**Paginate (`paginate/`).** Block layout into page frames = viewport minus
margins. Widow/orphan control (min 2 lines), block-level keep rules for
headings, image placement (scaled to fit content box, never upscaled beyond
intrinsic size), page breaks only at line boundaries. `vertical-rl` swaps the
pagination axis (columns of vertical lines flowing right-to-left; pages
progress right-to-left). RTL is a page-progression flag only — the pager's
"purely geometric" property is preserved: no progression-aware branches below
the "forward means what?" tap/key mapping.

**Display (`display/`).** Emits `PageDisplayList` (§3) plus per-page character
ranges, and maintains the bidirectional maps used by `locate`/`hit_test`/
`selection_rects`.

**Sessions & caching (`session/`).** A `ReaderSession` opens the archive via
`inkuna-content`, builds the spine model, and lays out chapters on demand:
current chapter synchronously on open (it is ~tens of ms; see the 43 ms
Readium-path open baseline), neighbors prefetched in the background, LRU cache
(default 5 chapters) keyed by
`(resource, viewport, settings fingerprint, font set, engine version)`.
`update_layout` (viewport/settings change) invalidates and relays the current
chapter first. Page numbers and overall progress do **not** require whole-book
layout: they stay on the synthetic-position model, now core-computed (§8).

## 3. FFI contract (`inkuna-ffi`)

**Facade restructure.** `Bookshelf` today accretes one `impl` block per
feature and would keep growing; it shrinks to a root object — constructor,
lifecycle, data-dir ownership — plus accessors returning cached feature
facades, each its own `uniffi::Object` in its own file wrapping the shared
`Arc<inkuna_core::Library>`:

```
Bookshelf.open(data_dir)         // unchanged constructor
bookshelf.library()  -> ShelfLibrary
bookshelf.importer() -> ShelfImport
bookshelf.search()   -> ShelfSearch
bookshelf.settings() -> ShelfSettings
bookshelf.progress() -> ShelfProgress
bookshelf.stats()    -> ShelfStats
bookshelf.open_reader(id, viewport, settings) -> ReaderSession   // async
```

Facades are constructed once and cached (cheap `Arc` clones). All methods stay
async via the shared `blocking()` / `spawn_blocking` helper. Names are checked
against generated-code collisions before landing (the `Library`/JNA and
`message`/`Throwable` precedents); the `Shelf*` prefix is the working choice.
Existing record/enum conversions move file-by-file with their facades — the
shells' call sites change mechanically (`shelf.search().inBook(...)`).

**`ReaderSession`** (new object, owns one open book):

- `chapter(spine_idx) -> ChapterGeometry` — page count, char range, writing
  mode, progression direction. Synchronous-cheap after layout; triggers layout
  if uncached.
- `page(spine_idx, page_idx) -> PageDisplayList`
- `update_layout(viewport, settings)` — invalidates caches, relays current.
- `locate(spine_idx, char_offset) -> PageLocation` and
  `hit_test(spine_idx, page_idx, x, y) -> HitResult` (char position; link href
  if inside a link region).
- `selection_rects(spine_idx, start, end) -> Vec<SelectionRect>` —
  writing-mode-aware rects (vertical selection is vertical natively).
- `word_at(spine_idx, char_offset) -> CharRange` — `icu_segmenter` word
  boundaries, for long-press selection seeding.
- `text_range(spine_idx, start, end) -> String` — selected text for
  copy/share; also serves accessibility page text via per-page char ranges.
- `match_rects(spine_idx, char_offset, len) -> Vec<SelectionRect>` — search
  highlight geometry (same machinery as selection).
- `resource(href) -> Vec<u8>` — image bytes on demand, budget-capped.
- `font_registry() -> Vec<FontEntry { id, file_path }>` — the exact files the
  core shaped with, for shell-side `CTFont`/`Typeface` construction.

**`PageDisplayList`** — one owned record per page (coarse boundary per
convention): glyph runs `{font_id, size, color_role, glyph_ids: Vec<u16>,
positions: Vec<f32> (x,y interleaved), orientation}`, image placements
`{href, rect}`, decorations (rules/underlines as rects; ruby geometry is baked
into glyph runs), link regions `{rect, target}`. **Color roles, not RGB** —
shells map roles (text, secondary, link) through their theme tokens, so
paper/calm/quiet/moon and night mode stay shell-owned and theme switches never
touch layout. Coordinates are layout points at 1×; shells apply screen scale.

## 4. Fonts

Font files move to repo-level `assets/fonts/` as the single source of truth,
copied into both app bundles at build time (XcodeGen file group; Gradle assets
sourceSet entry) — the same bytes reach core shaping and shell rasterization.
The set: today's Latin Noto Sans/Serif (+italics) plus **Noto Serif CJK and
Noto Sans CJK** (accepted app-size cost; exact packaging — variable vs.
per-weight, SC/TC/J/K coverage — resolved against current Noto releases at
implementation time and recorded in the commit). The core receives the font
directory path at `Bookshelf.open` and lazily loads faces; the registry maps
stable font IDs to files.

**System-font mode** (user-selectable): the core loads platform font files by
path (`/System/Library/Fonts` on iOS, `/system/fonts` on Android — both
readable from sandboxed apps). Pagination becomes per-device; the setting is
labeled accordingly. All stored positions are content coordinates, so nothing
breaks when switching modes. If a system font file cannot be parsed, the mode
falls back to bundled faces with a logged warning, never a broken reader.

## 5. Shell rendering

Each shell gains one new view class, **`PageView`**, rendering exactly one
`PageDisplayList` — a static drawing, no scrolling content, which is what keeps
the pager simple:

- **iOS**: `UIView.draw(_:)` with Core Text — `CTFontCreateWithGraphicsFont`
  per registry entry (fonts cached app-wide), `CTFontDrawGlyphs` per run,
  rotation transforms for sideways-Latin runs, `UIImage` draws for images.
- **Android**: custom `View.onDraw` with `Canvas.drawGlyphs` (API 31+, minSdk
  33), `Typeface.createFromFile` per registry entry, cached. Hosted in the
  pager exactly as WebViews are today; Compose integration stays `AndroidView`.

Images decode natively and asynchronously (`resource(href)` bytes → platform
decoder), placeholder rect first, invalidate on arrival. Color roles resolve
through the existing token layers (`ReadingTheme.swift` / `ReadingTheme.kt`).

**Deleted with the WebView path**: iOS `ReadiumPagerSurface.swift`,
`ReaderStyleSurface.swift`, `ReaderUserStyle.swift`,
`ReadingFontDeclarations.swift`, the Readium SPM dependency (3 products);
Android `ReaderNavigatorHost.kt`, `ReaderStyleInjector.kt`, `ReaderUserCss.kt`,
`ReaderWebViewTuner.kt`, `ReaderPageTurnListener.kt`, the three Readium Maven
artifacts, and the four Readium-forced constraints (androidx.webkit strict pin,
core-library desugaring, viewpager, fragment-ktx — each removed only after
verifying no other consumer).

## 6. Pager integration

- **iOS**: a new `EnginePagerSurface` implements the existing
  `ReaderPagerSurface` protocol. `innerMetrics`/`outerMetrics` become
  synchronous math (page count × page width, known before first frame);
  `neighborIsReady` reads the prefetch cache; `commitBoundaryCrossing` is a
  cache pointer swap that cannot fail; `verifyBoundaryCommit` becomes trivially
  true. The rescue-layer family — `seedInnerMax` reseeding, flick-rescue,
  commit verification retries — is deleted, not ported. `ReaderPager.swift`
  and `ReaderPagerPhysics.swift` carry over unchanged.
- **Android**: extract the same surface as a Kotlin interface from
  `ReaderPagerLayout.kt` (the seam iOS already has), then implement it
  engine-backed. ViewPager fake-drag glue and WebView `scrollTo` column
  driving go away; the pager translates real `PageView`s directly.
  `SettleSpring.kt` carries over unchanged. The extracted interface mirrors
  `ReaderPagerSurface` member-for-member (the mirroring convention applies:
  change one, change its sibling).

## 7. Selection (v1 gate)

Core owns geometry; shells own UI. Long-press → `word_at` seeds a word
selection; draggable handles (shell-drawn, following each platform's handle
idiom) call `hit_test` on drag and `selection_rects` to render highlight
overlays — writing-mode-aware, so vertical text selects vertically without
shell-side special cases. System menus via `UIEditMenuInteraction` (iOS) and
floating `ActionMode` (Android) offer Copy / Look Up / Share, fetching text
through `text_range`. Selection state lives where `hasActiveSelection` /
`SelectionModeTracker` sit today, so the pager's gesture arbitration is
unchanged. Cross-page selection is bounded to the visible page in v1 (drag past
the edge does not auto-turn; documented limitation, consistent with the
gesture-arbitration model).

## 8. Data rebaseline (schema V8)

One append-only migration (`V8_SQL` + `7 =>` arm per the migrate.rs
convention), destructive conversions allowed:

- `publications.locator` and `bookmarks.locator` (opaque Readium JSON TEXT) are
  replaced by structured content coordinates: `spine_idx INTEGER`,
  `char_offset INTEGER`. Existing rows convert best-effort from the JSON's
  `href`/`progression` at chapter-level accuracy (position within the chapter
  approximated by progression × chapter char length; a one-time cost the owner
  has accepted for beta users).
- `resources` gains `media_type TEXT` (parsed at import today, discarded);
  backfilled lazily by the first import-reconcile pass, since the engine reads
  media types from the manifest at open anyway.
- `resource_positions` semantics flip from shell-reported to core-computed at
  import time; existing books backfill via a one-shot reconcile on first
  library open (same pattern as the search-index reconcile).
  `report_position_ranges` and `InvalidPositionRanges` are deleted — the
  reading-order/spine mismatch class they existed for disappears when core owns
  both sides.
- The settings units reinterpretation (§9) is documented in the V8 comment
  block, following the V7 precedent.

Shell-side, `ChapterHref.swift` and its Kotlin twin (the core↔Readium href
impedance layer) are deleted.

## 9. Settings reinterpretation

The six typography settings keep their stored shapes and clamp ranges (no UI
work): `reading_font`, `reading_bold`, `text_size_step` 0–4, `line_spacing`
1.30–2.10 (line-height multiplier, unchanged meaning), `letter_spacing` /
`word_spacing` (em-relative, now applied by the shaper/justifier),
`reading_margins` 16–48 — reinterpreted from "CSS px inside the rendering web
view" to engine layout points (numerically identical at 1×, so users see no
jump). Settings flow into `open_reader` / `update_layout` as a record; the
settings fingerprint keys the layout cache. Font ids remain opaque strings
owned by the shells, resolved to registry entries at session open.

## 10. Search integration

`search_in_book` already returns `(spine_idx, char_offset)` — exactly the
content-coordinate model — so hits jump via `locate` and highlight via
`match_rects`. The shell-side progression→position conversions
(`ReaderViewController.position(of:)`, `ReaderViewModel.searchLocator`) move
into core as a position lookup on the synthetic-position table. Tantivy/jieba
library search is untouched. One correctness note carried into testing: search
folds text NFKC+casefold, so `match_rects` maps folded match offsets back
through the fold map to original char ranges (the fold module already tracks
offsets for hit excerpts).

## 11. Book-open flow (both shells, after the swap)

Shell → `Bookshelf.open_reader(id, viewport, settings)` → `ReaderSession`
(current chapter laid out synchronously, neighbors prefetching) → pager renders
`PageView`s from `page(...)` → restore position via `locate(stored coordinate)`.
No per-format branch: every reflowable is already an EPUB on disk; PDF/CBZ/CBR
remain detected-but-unimportable; fixed-layout EPUB detected at open (OPF
`rendition:layout`) → localized "not yet supported" state. The
`Opening`-state / `transitionSettled` mount-delay machinery on Android is
removed — there is no process spawn to hide; first page renders in the enter
transition's first frame.

## 12. Error handling & security bounds

The engine re-opens archives at read time, so the resource-bounds postmortem
applies with new pressure. Enforced as engine invariants with adversarial
fixture tests:

- Per-resource decompression budgets inherited from `inkuna-content` (single
  enforcement point).
- Hard caps per resource: parsed node count, text length, attribute sizes.
- Image caps: declared-dimension and byte-size limits before decode; decode
  happens shell-side but `resource()` enforces byte budgets.
- Layout guards: max lines per paragraph, max pages per chapter — a crafted
  book degrades to a truncated chapter with a visible notice, never OOM.
- Error taxonomy: each crate's thiserror enum converts to `CoreError` variants
  (`UnsupportedContent`, `LayoutBudgetExceeded`, …) mirrored as `InkunaError`
  with `detail` fields; shells present the existing error-state UI. Panics
  remain forbidden; every engine entry point returns `Result`.

## 13. Docs & rules sweep (same-commit discipline)

- `docs/dev/architecture.md`: ADR amendment — decision ("core owns layout:
  parse → style → shape → paginate → positioned glyph runs; shells own drawing
  and interaction"), rationale (Android has no vertical-CJK text stack;
  identical pagination; positions in core; WebView race/jank class), rewritten
  Format-strategy table ("Rendering path" column), rewritten roadmap, and the
  superseded "Readium renders / custom engine is long-term" passages replaced.
- Root `CLAUDE.md` (the literal "the Rust core never renders" sentence),
  `core/CLAUDE.md` §0 table, `apps/ios/CLAUDE.md`, `apps/android/CLAUDE.md`,
  `apps/ios/project.yml` comment, and module docs in
  `inkuna-core/src/lib.rs`, `formats/mod.rs`, `formats/epub/mod.rs` (the "never
  re-opens the book after import" sentence) — all updated; every `AGENTS.md`
  re-copied byte-identical in the same commit as its `CLAUDE.md`.

## 14. Testing strategy & parity gate

**Engine tests (core, the bulk):**

- Golden layout tests: fixture EPUBs (Latin, CJK horizontal, CJK vertical with
  ruby, RTL, mixed-script, image-heavy, simple-table) built by the shared
  fixture builder, laid out at fixed viewports/settings, asserting page counts
  and glyph positions against committed snapshots. These run on every platform
  CI already (`cargo test --workspace` on ubuntu gates both releases) — the
  cross-platform identity claim is enforced by determinism plus these
  snapshots.
- Property tests: every char lands on exactly one page;
  `locate(hit_test(x)) = x` round-trips; selection rects cover exactly the
  selected range; re-layout after `update_layout` preserves content
  coordinates.
- Adversarial fixtures per §12 (budget bombs, malformed XHTML, absurd images).
- Determinism test: layout twice, byte-compare display lists.

**Shell tests:** existing per-platform styles; pager unit tests keep passing
against the engine-backed surface (the surface contract is unchanged on iOS and
newly pinned by interface on Android).

**Parity gate (merge `dev/core` → `main`):**

1. Every book in a seeded test library opens and restores position after the
   V8 conversion.
2. Page/chapter navigation including fast-swipe chains across chapter
   boundaries — the bug class that motivated the swap — with no rescue layers.
3. A vertical-CJK book with ruby reads correctly end-to-end; RTL progression
   honored; tap/key direction mapping correct.
4. In-book and library search, with highlight rects on the page.
5. Bookmarks and progress survive; position numbers stable across settings
   changes.
6. All four reading themes + night mode; typography settings live-apply.
7. Selection with copy/look-up/share on both platforms, horizontal and
   vertical.
8. VoiceOver/TalkBack read the current page (via `text_range` over the page's
   char range).
9. Performance: book-open to first rendered page ≤ the frozen Readium build's
   43 ms baseline path (no WebView spawn to amortize); no jank regression on
   the "Keep Reading" flow (gfxinfo/Instruments comparison).
10. FFI bindings regenerated via both `scripts/build-core-*.sh`; both shells
    build clean with zero Readium references (`grep -ri readium` finds only
    docs/history).

## Open questions / assumptions

- **A1 — quick-xml over html5ever.** Assumes EPUB-mandated XHTML plus lenient
  recovery covers the real-world corpus; structurally broken books fail closed.
  If review or practice shows a meaningful tail of tag-soup EPUBs, swap the
  `dom/` parser for html5ever behind the same arena-tree interface.
- **A2 — composing rustybuzz + own line/vertical layout instead of
  cosmic-text** (which lacks vertical writing). The vertical-writing layout is
  the largest from-scratch surface in the project.
- **A3 — Noto CJK packaging** (variable vs. weights, OTC vs. per-region) and
  final app-size delta: resolved at implementation against current releases.
- **A4 — justified-with-hyphenation default** matches the current reader's
  perceived look closely enough that no new user setting is needed in v1.
- **A5 — chapter-level accuracy** for the one-time locator conversion is
  acceptable (owner-approved for beta).
- **A6 — visible-page-bounded selection** (no auto-page-turn while dragging)
  is acceptable for the v1 gate.
- **A7 — `Canvas.drawGlyphs` and `CTFontDrawGlyphs`** are sufficient for all
  run drawing including rotated sideways-Latin (transform per run). No known
  gaps at minSdk 33 / iOS 18, to be verified in the first spike.
- **A8 — synthetic positions remain the user-facing "position N of M"**
  currency, as today; page numbers within a chapter are layout-local. No
  whole-book pagination is ever required.
- **A9 —** the `Bookshelf::open` single-instance-per-data-dir constraint
  extends to sessions: `ReaderSession`s are children of the one `Bookshelf`
  and close with it; one live session per publication.

## Out of scope

- Highlights/annotations, TTS, dictionary integration — next features on the
  new position model, each its own spec.
- Scrolled reading mode — paginated-only, as today.
- Fixed-layout EPUB, CBZ/CBR, PDF — dedicated-navigator track.
- Publisher embedded fonts, MathML, SVG, media overlays — opinionated-engine
  non-goals; revisit only with evidence from real libraries.
- Search-index or tantivy changes — untouched subsystem.
- Sync/cross-device positions — content coordinates are designed to serve it
  later.
