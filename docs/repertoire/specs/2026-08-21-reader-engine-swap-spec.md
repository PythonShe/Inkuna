# Reader Engine Swap — Readium Removal and Core-Owned Layout

Status: revised after adversarial panel (3 lens reviewers + Codex, all findings
triaged) · Branch: `dev/core` · Date: 2026-08-21

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
design is carried** (owner directive): stored Readium locators are rebaselined,
the DB schema migrates destructively where needed, and the reader FFI surface is
redesigned clean.

## Terminology

- **Canonical text projection** — THE single text stream per spine resource
  that every character offset in the system indexes. Defined precisely in §2
  (Parse); produced by one shared function in `inkuna-engine`, used identically
  by layout, search corpus extraction, positions, locators, and migration.
  Offsets are **Unicode scalar value counts** (Rust `char` indices) into this
  projection, always in original (pre-NFKC-fold) space.
- **Display list** — the per-page render description the core emits: positioned
  glyph runs, image placements, decoration geometry, link regions,
  accessibility blocks.
- **Content coordinate** — `(spine_idx, char_offset)` into the canonical text
  projection: a position independent of layout, fonts, or settings. Replaces
  Readium locator JSON everywhere.
- **Synthetic positions** — fixed-size character blocks over the canonical
  projection (§8) used for "position N of M" and progress percentage;
  layout-independent by design.
- **Layout generation** — a monotonic counter bumped by every
  `update_layout`; stamped on all geometry so shells can discard stale results.
- **Parity gate** — the checklist that must pass before `dev/core` merges to
  `main` (§14).
- **Font registry** — the bundled font binaries the core shapes with and the
  shells rasterize with; core-owned stable font IDs.

## Goals / non-goals

**Goals**

- Remove Readium entirely from both shells; zero Readium dependencies remain.
- Rust core owns parse → style → shape → line-break → paginate; shells draw
  glyph runs and own interaction.
- Byte-identical pagination across platforms, enforced by deterministic
  fixed-point layout plus cross-device display-list digest checks (§14).
- First-class vertical CJK (`vertical-rl`, vertical punctuation forms, sideways
  Latin), ruby annotations, RTL page progression.
- Positions, hit-testing, selection geometry, link/anchor resolution, and
  search-highlight geometry computed in core; content coordinates as the single
  position model.
- Native text selection (select → copy/look-up/share) on both platforms —
  inside the v1 parity gate.
- The custom pager survives with its physics and feel intact, minus every
  WebView rescue layer; all interaction-path queries are synchronous.
- `core/` restructured into focused crates and the FFI into facade objects
  (both owner-directed), sequenced to land as early mechanical commits.
- `docs/dev/architecture.md` amended as an ADR-level change; every echo of the
  "core never renders" rule updated.

**Non-goals**

- Fixed-layout (pre-paginated) EPUB rendering — detected and surfaced as
  "not yet supported" in v1.
- High-fidelity publisher CSS. The engine is deliberately opinionated (§2).
- System-font reading mode — **cut from v1 by review** (it re-opened the
  pagination-identity guarantee and added a large font-parsing surface for
  nothing the swap needs). Bundled fonts only; revisit post-swap.
- Hyphenation — the current Readium path does not enable it, so it would be a
  new typographic feature, not parity. Post-v1.
- Per-element `writing-mode` mixing within one chapter — writing mode is
  resolved per spine resource (from its root/body styles); mixed-mode
  documents render in the resource's dominant mode.
- Table grid layout — all tables degrade to sequential blocks in v1.
- Scrolled (non-paginated) reading mode; the reader is paginated-only today and
  stays so.
- Highlights/annotations, TTS, dictionary popovers — designed-for (the position
  model serves them) but not built here.
- MathML, SVG rendering, audio/media overlays, publisher-embedded fonts.
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

Decisions locked during the design dialogue and review triage:

1. **Fonts: bundled only (v1).** Bundled faces guarantee identical pagination.
   The optional system-font mode from the draft was cut by review as
   beyond-parity scope.
2. **Opinionated typography.** Reader settings and theme own the look;
   publisher CSS is mined for semantics only (§2).
3. **Hard cut on `dev/core`.** Readium is removed on this branch now; `main`
   keeps shipping the frozen Readium beta (bugfix-only); merge happens at the
   parity gate. No dual path. The crate/FFI restructures land first as
   mechanical commits to keep the `main`-backport window cheap.
4. **Fixed-layout unsupported in v1** with clean detection and a localized
   "not yet supported" state.
5. **No backward compatibility.** Beta-stage app: destructive rebaselines
   allowed; existing reading positions convert at chapter-level accuracy, once.
6. **Selection is in the v1 gate** (strict parity with the current reader).
7. **Interaction-path FFI is synchronous** (§3) — the async-at-interaction-time
   failure mode is the thing being killed; layout progress flows through an
   explicit readiness/callback model instead.

## 1. Workspace restructure

`core/crates/` grows from 2 to 5 crates (owner-directed, alongside the FFI
facade split). Only `inkuna-ffi` exports FFI symbols, so the `--library`
bindgen flow and both build scripts are unchanged; CI hardcodes no crate names.

| Crate | Contents | Depends on |
|---|---|---|
| `inkuna-content` | EPUB container layer extracted from `formats/epub/`: bounded zip reads (`archive.rs`), `container.xml`, OPF/spine/manifest (media types, `rendition:layout`, `page-progression-direction`), nav/NCX TOC, **core-owned href normalization** (percent-decoding, fragment split, leading-slash — replacing the shells' `ChapterHref` glue), entity/XML helpers. The resource-bounds postmortem rules are enforced here, once, for import and engine alike. | — |
| `inkuna-format` | Import-side conversion moved wholesale: MOBI/AZW3/TXT→EPUB converters and the EPUB 3 writer (`write.rs`), plus their CSS scrubbers. | `inkuna-content` |
| `inkuna-engine` | The reader engine (§2): modules `session/`, `dom/`, `style/`, `shape/`, `layout/`, `paginate/`, `display/`, `text/` (canonical projection). | `inkuna-content` |
| `inkuna-core` | What remains: `Library` facade, DB (pool, migrate, schema), import orchestration, features (search, progress, settings, stats). | `inkuna-content`, `inkuna-format`, `inkuna-engine` |
| `inkuna-ffi` | UniFFI surface, restructured into facades (§3). | `inkuna-core` |

Sequencing note (review finding): the crate moves and the FFI facade split are
mechanical refactors with no behavior change; they land as the branch's first
commit train, so later `main` bugfixes cherry-pick onto a stable layout.

Conventions carry over unchanged to all crates: thiserror-only errors (each
crate gets its own error enum, converted into `CoreError` at the `inkuna-core`
boundary), declaration-only `mod.rs`, ≤400-line target / 500 hard ceiling,
sibling `*_tests.rs` files, CJK fixtures mandatory, no binary fixtures in git
(fixture builders extend `test_support.rs` patterns; `inkuna-content` hosts the
shared EPUB builder so all crates can construct fixtures).

New dependencies (latest stable verified against live registries at
implementation time, per stack policy): `rustybuzz` (+ its `ttf-parser`),
`unicode-bidi`, `unicode-script`, `unicode-linebreak`, `cssparser`,
`icu_segmenter`, `imagesize` (header-sniffed intrinsic dimensions, §2
Paginate). Deliberately **not** used: `cosmic-text` (no vertical-writing
support), `html5ever` (assumption A1), `hyphenation` (cut with the feature).

## 2. Engine pipeline (`inkuna-engine`)

Per spine resource, a pure, deterministic pipeline with no I/O past the archive
read: **parse → style → shape → break → paginate**. Two hard invariants:

- **Determinism:** identical inputs (resource bytes, viewport, settings
  fingerprint, font set, engine version) produce identical display lists on
  every platform. All layout arithmetic — advances, line positions, page
  extents — runs in **fixed-point subpixel units (i32, 1/64 layout point)**
  with strictly ordered accumulation; `f32` appears only in the emitted display
  list. This removes FMA/rounding divergence across compilers and targets and
  makes cross-platform identity a property of integer math, not luck.
- **Canonical text projection:** one function (`text/projection.rs`) maps a
  parsed DOM to the text stream all offsets index: text nodes of rendered
  elements in document order; `display:none` subtrees and `rt` (ruby
  annotation) text **excluded**; ruby base text included; whitespace collapsed
  per the existing `text.rs` rules (runs of whitespace → one space, block
  boundaries → one `\n`); no generated content, no soft hyphens, no list
  markers. Import-time search-corpus extraction (`resource_text`) is
  **re-implemented as a call to this same function** so search offsets and
  layout offsets index the same stream by construction (§8 reconciles existing
  rows). Layout may insert visual-only artifacts (line breaks, justification
  spaces); these never alter offsets.

**Parse (`dom/`).** quick-xml streaming into a compact arena tree (node kinds:
element, text; attributes interned; **`id` attributes retained into a
per-resource anchor map** `id → char_offset` for §3 `locate_href`). Extends the
proven `text.rs` machinery: entity resolution, budgets. `script`/`template`
are skipped; `head` is scanned (not skipped) to collect `<link
rel="stylesheet">` hrefs and inline `<style>` text for the style pass. The
parser is leniently recovering for mild malformation (unclosed inline tags,
stray entities) and fails closed per-resource — never a panic (failure
contract in §12).

**Style (`style/`).** Opinionated resolution. Element semantics drive
structure: h1–h6, p, em/i, strong/b, blockquote, pre/code, ol/ul/li, ruby/rt/rb,
img/image, table (degraded to sequential blocks), br, hr, a. Stylesheet
handling, pinned by review: linked stylesheets resolve via `inkuna-content`
href normalization and load under the same per-resource budgets; sources
cascade in spec order (UA defaults < linked/embedded publisher CSS < inline
`style`), and reader settings always win on visual properties. The
`cssparser`-based pass mines **only** the honored properties: `writing-mode`
(resolved on the resource's root/html/body — per-resource, not per-element),
`direction` (per-element, feeds bidi), `font-style`, `font-weight`,
`text-align`, `ruby-position`, `display: none`. Supported selectors: element,
`.class`, `#id`, and descendant combinations thereof; anything else is ignored
(never an error). Publication-level `page-progression-direction` (OPF spine)
sets RTL paging; a resource-level `vertical-rl` sets vertical layout — both
surface in `ChapterGeometry`.

**Shape (`shape/`).** Runs are itemized by script (`unicode-script`) and bidi
level (`unicode-bidi`), then shaped with `rustybuzz` against the font registry
with an explicit fallback chain: reading face → CJK face → symbol face. Missing
glyphs fall through the chain; final fallback renders `.notdef` (never drops
text). Vertical mode shapes with `vert`/`vrt2` features, applies vertical
punctuation forms, and marks sideways-Latin runs with a rotation flag. Ruby
annotations shape as attached runs sized by the engine (base-relative scale)
and positioned per `ruby-position`; ruby text is display-only (excluded from
the canonical projection, selectable as its base).

**Break (`layout/`).** UAX #14 line breaking (`unicode-linebreak` — encodes
CJK kinsoku). No hyphenation in v1 (parity: the Readium path never enabled
it). Justified by default: inter-word stretch for Latin, inter-character for
CJK; last lines ragged. Line height, paragraph spacing and indents come from
the settings mapping (§9).

**Paginate (`paginate/`).** Block layout into page frames = viewport minus
margins, **progressively**: pages are emitted as their lines complete, so the
first page of a chapter is available long before the chapter finishes
(readiness model in §3). Widow/orphan control (min 2 lines), keep rules for
headings, image placement (scaled to fit the content box, never upscaled
beyond intrinsic size — intrinsic dimensions header-sniffed in core via
`imagesize` without decoding; unsniffable images get a fixed placeholder box),
page breaks only at line boundaries. `vertical-rl` swaps the pagination axis;
RTL is a page-progression flag only — the pager's "purely geometric" property
is preserved.

**Display (`display/`).** Emits `PageDisplayList` (§3), per-page character
ranges, accessibility blocks, and the bidirectional maps behind
`locate`/`hit_test`/`selection_rects`.

**Sessions (`session/`).** A `ReaderSession` opens the archive via
`inkuna-content`, builds the spine model and anchor maps, and lays chapters out
on core-owned background threads: the opening chapter first (progressively —
first page fast), then neighbors, LRU cache (default 5 chapters) keyed by
`(resource, viewport, settings fingerprint, font set, engine version)` and
stamped with the layout generation. `update_layout` bumps the generation,
invalidates, and relays the current chapter first; in-flight work for stale
generations is abandoned.

## 3. FFI contract (`inkuna-ffi`)

**Facade restructure** (owner-directed). `Bookshelf` shrinks to a root object —
constructor, lifecycle, data-dir + font-dir ownership — plus accessors
returning cached feature facades, each its own `uniffi::Object` in its own file
wrapping the shared `Arc<inkuna_core::Library>`:

```
Bookshelf.open(data_dir, font_dir)   // signature CHANGES (breaking, allowed):
                                     // font_dir is the bundled assets/fonts path
bookshelf.library()  -> ShelfLibrary
bookshelf.importer() -> ShelfImport
bookshelf.search()   -> ShelfSearch
bookshelf.settings() -> ShelfSettings
bookshelf.progress() -> ShelfProgress
bookshelf.stats()    -> ShelfStats
bookshelf.open_reader(id, viewport, settings, listener) -> ReaderSession  // async
```

Facades are constructed once and cached (cheap `Arc` clones); their methods
keep the existing async/`spawn_blocking` convention. Names are checked against
generated-code collisions before landing (the `Library`/JNA and
`message`/`Throwable` precedents); `Shelf*` is the working prefix. Existing
record/enum conversions move file-by-file; shell call sites change
mechanically.

**Sync/async split** (review-critical). The draft's "all methods async" would
rebuild the async-at-interaction-time problem the swap exists to kill. The
contract is now explicit:

- **Async (may do I/O or heavy work):** `Bookshelf.open`, `open_reader`,
  `update_layout`, `resource`.
- **Synchronous, non-blocking, cache-only** (safe from the UI thread; return
  `NotReady` errors rather than ever blocking): everything on the interaction
  path — `chapter`, `page`, `is_ready`, `locate`, `locate_href`, `hit_test`,
  `selection_rects`, `word_at`, `text_range`, `match_rects`,
  `accessibility_blocks`, `font_registry`.

**Readiness model.** Layout runs on core-owned threads. `open_reader` takes a
`LayoutListener` callback interface (`#[uniffi::export(with_foreign)]`,
following the `ImportProgressListener` precedent):
`on_chapter_ready(generation, spine_idx, page_count)` and
`on_first_page_ready(generation, spine_idx)` (progressive pagination, §2).
`is_ready(spine_idx) -> bool` backs the pager's `neighborIsReady`; every
geometry record carries its `generation`, and shells drop results whose
generation is stale. This replaces polling and makes "blank neighbor on fast
boundary swipe" structurally impossible: the pager only commits to ready
chapters, exactly as it does today, but readiness is now truthful and local.

**`ReaderSession`** (new object, owns one open book; opening an id that already
has a live session closes and replaces it — last-open-wins; sessions close on
drop and with their `Bookshelf`):

- `chapter(spine_idx) -> ChapterGeometry { generation, page_count, char_range,
  writing_mode, rtl_progression }` — cache-only; schedules layout and returns
  `NotReady` if absent.
- `page(spine_idx, page_idx) -> PageDisplayList`
- `is_ready(spine_idx) -> bool`
- `update_layout(viewport, settings)` — async; bumps generation, relays
  current chapter first, listener re-fires.
- `locate(coordinate) -> PageLocation { generation, spine_idx, page_idx }` and
  `locate_href(href, fragment: Option<String>) -> Coordinate` — **the anchor
  path** (review-critical): resolves via `inkuna-content` href normalization +
  the per-resource anchor map; TOC entries, internal-link taps, and footnote
  jumps all land through it. Unresolvable targets return a typed error the
  shells surface non-fatally.
- `hit_test(spine_idx, page_idx, x, y) -> HitResult { coordinate,
  link_target: Option<String> }`
- `selection_rects(spine_idx, start, end) -> Vec<SelectionRect>` —
  writing-mode-aware.
- `word_at(coordinate) -> CharRange` — `icu_segmenter` word boundaries.
- `text_range(spine_idx, start, end) -> String` — text in canonical
  projection space.
- `match_rects(spine_idx, char_offset, len) -> Vec<SelectionRect>` — search
  highlight geometry.
- `accessibility_blocks(spine_idx, page_idx) -> Vec<A11yBlock { text, rect,
  lang, is_link, role }>` — per-page blocks in logical reading order (§7).
- `resource(href) -> Vec<u8>` — image bytes on demand, budget-capped (async).
- `font_registry() -> Vec<FontEntry { id, file_path, collection_index,
  axes: Vec<(tag, value)> }>` — sufficient for exact face reconstruction on
  both platforms (plain TTFs have `collection_index 0`, empty `axes`).

**Shared record shapes** (pinned by review; all coordinates in layout points at
1×, shells apply screen scale): `Viewport { width, height }` — the page
content area's available box *excluding* shell chrome insets but *including*
reading margins (margins are settings, applied inside core);
`Coordinate { spine_idx, char_offset }`; `CharRange { start, end }` (end
exclusive); `SelectionRect { rect, writing_mode }`; `Rect { x, y, width,
height }`.

**`PageDisplayList`** — one owned record per page (coarse boundary per
convention): glyph runs `{font_id, size, color_role, glyph_ids: Vec<u16>,
positions: Vec<f32> (x,y interleaved), orientation}`, image placements
`{href, rect}`, decorations (rules/underlines as rects; ruby geometry baked
into glyph runs), link regions `{rect, target}`, accessibility block indices.
**Color roles, not RGB** — shells map roles (text, secondary, link) through
their theme tokens, so paper/calm/quiet/moon and night mode stay shell-owned
and theme switches never touch layout.

## 4. Fonts

Font files move to repo-level `assets/fonts/` as the single source of truth,
copied into both app bundles at build time (XcodeGen file group; Gradle assets
sourceSet entry) — the same bytes reach core shaping and shell rasterization.
The set: today's Latin Noto Sans/Serif (+italics) plus **Noto Serif CJK and
Noto Sans CJK** (accepted app-size cost; exact packaging — variable vs.
per-weight, OTC vs. per-region — resolved against current Noto releases at
implementation time and recorded in the commit; whatever is chosen, the
registry's `collection_index`/`axes` fields carry it losslessly to the shells).
`Bookshelf.open` receives the font directory (§3); the core lazily loads faces
and **owns font IDs**; shells are pure consumers of the registry. Bundled
fonts only in v1 — the system-font mode was cut by review (see Non-goals).

## 5. Shell rendering

Each shell gains one new view class, **`PageView`**, rendering exactly one
`PageDisplayList` — a static drawing, no scrolling content, which is what keeps
the pager simple:

- **iOS**: `UIView.draw(_:)` with Core Text — `CTFontCreateWithGraphicsFont`
  per registry entry (honoring `collection_index`/`axes`; fonts cached
  app-wide), `CTFontDrawGlyphs` per run, rotation transforms for
  sideways-Latin runs, `UIImage` draws for images.
- **Android**: custom `View.onDraw` with `Canvas.drawGlyphs` (API 31+, minSdk
  33), `Typeface` built per registry entry (`Typeface.Builder` supports ttc
  index and axes), cached. Hosted in the pager exactly as WebViews are today;
  Compose integration stays `AndroidView`.

Images decode natively and asynchronously (`resource(href)` bytes → platform
decoder), placeholder rect first, invalidate on arrival. Color roles resolve
through the existing token layers (`ReadingTheme.swift` / `ReadingTheme.kt`).
Accessibility: each `PageView` exposes its `accessibility_blocks` as ordered
accessibility elements with per-block frames, language, and link traits (§7).

**Deleted with the WebView path**: iOS `ReadiumPagerSurface.swift`,
`ReaderStyleSurface.swift`, `ReaderUserStyle.swift`,
`ReadingFontDeclarations.swift`, the Readium SPM dependency (3 products);
Android `ReaderNavigatorHost.kt`, `ReaderStyleInjector.kt`, `ReaderUserCss.kt`,
`ReaderWebViewTuner.kt`, `ReaderPageTurnListener.kt`, the three Readium Maven
artifacts, and the four Readium-forced constraints (androidx.webkit strict pin,
core-library desugaring, viewpager, fragment-ktx — each removed only after
verifying no other consumer). **Before deletion**, the concrete typography
constants in `ReaderUserStyle.swift`/`ReaderUserCss.kt` are transcribed into
the §9 mapping table — the deleted files must never be the only record of the
current look.

## 6. Pager integration

- **iOS**: a new `EnginePagerSurface` implements the existing
  `ReaderPagerSurface` protocol. `innerMetrics`/`outerMetrics` become
  synchronous math over `chapter()` geometry (known before first frame for the
  current chapter); `neighborIsReady` delegates to `is_ready`;
  `commitBoundaryCrossing` is a cache pointer swap that cannot fail;
  `verifyBoundaryCommit` becomes trivially true. The rescue-layer family —
  `seedInnerMax` reseeding, flick-rescue, commit verification retries — is
  deleted, not ported. `ReaderPager.swift` and `ReaderPagerPhysics.swift`
  carry over unchanged.
- **Android**: extract the same surface as a Kotlin interface from
  `ReaderPagerLayout.kt` (the seam iOS already has), then implement it
  engine-backed. ViewPager fake-drag glue and WebView `scrollTo` column
  driving go away; the pager translates real `PageView`s directly.
  `SettleSpring.kt` carries over unchanged. The extracted interface mirrors
  `ReaderPagerSurface` member-for-member (the mirroring convention applies:
  change one, change its sibling).

## 7. Selection & accessibility (v1 gate)

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

**Accessibility (scope made explicit by review):** v1 exposes per-page
`A11yBlock`s — logical reading order, per-block text (canonical projection,
ruby annotations appended parenthetically per block), frame, language tag, and
link/heading roles — as native accessibility elements. This gives
VoiceOver/TalkBack block-granular navigation with correct bounds and language
switching. It is deliberately less than WebView DOM semantics (no
character-level rotor over styled content); that delta is a documented v1
limitation, not silent regression, and the gate tests what is specified here.

## 8. Data rebaseline (schema V8 + reconcile)

Review split this into two mechanisms with explicit failure semantics:

**V8 (pure SQL, append-only per the migrate.rs convention):**

- `publications`: add `position_spine_idx INTEGER`, `position_char_offset
  INTEGER` (nullable); rename nothing; the old `locator` TEXT column is
  retained as-is until reconcile consumes it.
- `bookmarks`: same pair of nullable columns beside the retained `locator`.
- No `media_type` column (draft idea cut by review — the engine reads the
  manifest at open; no other consumer exists).
- The §9 settings-units reinterpretation is documented in the V8 comment
  block, following the V7 precedent.

**Reconcile pass (idempotent, per-book, on first library open after V8 —
same pattern as the search-index reconcile):**

- Re-extract `resource_text` via the canonical projection (§2) so search
  corpus offsets and engine offsets agree by construction; re-index search.
- Compute synthetic positions over the canonical projection: **fixed
  1024-character blocks per resource, minimum one** (replacing shell-reported
  counts; `report_position_ranges` and `InvalidPositionRanges` are deleted —
  the reading-order mismatch class they existed for disappears).
- Convert each retained Readium `locator` best-effort: normalize its `href`
  to a `spine_idx` (via `inkuna-content`), map `progression × chapter char
  length` to a `char_offset`. Success writes the new columns and clears
  `locator`; failure (unparseable JSON, unresolvable href) defaults to the
  chapter start — or `(0, 0)` if even the href is gone — and clears `locator`.
  Chapter-level accuracy is the owner-accepted one-time cost; nothing is
  dropped without a written default.
- Each book reconciles in its own write transaction; a crash mid-pass resumes
  where it left off (per-book idempotency), and a book that fails reconcile
  still opens — position falls back to the default above at read time.

Shell-side, `ChapterHref.swift` and its Kotlin twin are deleted; their job
(href impedance) moves into `inkuna-content` normalization behind
`locate_href`.

## 9. Settings mapping

The six typography settings keep their stored shapes and clamp ranges (no UI
work): `reading_font`, `reading_bold`, `text_size_step` 0–4, `line_spacing`
1.30–2.10 (line-height multiplier, unchanged meaning), `letter_spacing` /
`word_spacing` (em-relative, applied by the shaper/justifier),
`reading_margins` 16–48 — reinterpreted from "CSS px inside the rendering web
view" to engine layout points (numerically identical at 1×, so users see no
jump). **The concrete mapping table — step → point size, base line-height,
paragraph spacing/indent, heading scale, ruby scale — is transcribed from
`ReaderUserStyle.swift`/`ReaderUserCss.kt` into `inkuna-engine`'s settings
module before those files are deleted** (review finding: the deleted files
must not be the only record of the current look). Settings flow into
`open_reader` / `update_layout` as a record; the settings fingerprint keys the
layout cache. Font ids remain opaque strings owned by the shells, resolved to
registry entries at session open.

## 10. Search integration

With the canonical projection shared by construction (§2, §8), `search_in_book`
hits `(spine_idx, char_offset)` are content coordinates with **no conversion
step**: jumps go through `locate`, highlights through `match_rects`. Offsets
returned by search are always in original (pre-fold) space — the fold module
already maintains the fold↔original offset map for excerpts, and the invariant
is now stated: **no folded offset ever crosses the FFI**. The shell-side
progression→position conversions (`ReaderViewController.position(of:)`,
`ReaderViewModel.searchLocator`) move into core as a synthetic-position
lookup. Tantivy/jieba library search is untouched.

## 11. Book-open flow (both shells, after the swap)

Shell → `Bookshelf.open_reader(id, viewport, settings, listener)` →
`ReaderSession` (first page of the opening chapter available via
`on_first_page_ready`, typically well before the enter transition ends;
remaining pages and neighbors follow) → pager renders `PageView`s from
`page(...)` → restore position via `locate(stored coordinate)`. No per-format
branch: every reflowable is already an EPUB on disk; PDF/CBZ/CBR remain
detected-but-unimportable; fixed-layout EPUB detected at open (OPF
`rendition:layout`) → localized "not yet supported" state. The
`Opening`-state / `transitionSettled` mount-delay machinery on Android is
removed — there is no process spawn to hide.

## 12. Error handling & security bounds

The engine re-opens archives at read time, so the resource-bounds postmortem
applies with new pressure. Enforced as engine invariants with adversarial
fixture tests:

- Per-resource decompression budgets inherited from `inkuna-content` (single
  enforcement point).
- Hard caps per resource: parsed node count, text length, attribute sizes,
  stylesheet bytes.
- Image caps: header-sniffed dimension limits and byte-size limits; decode
  happens shell-side but `resource()` enforces byte budgets.
- Layout guards: max lines per paragraph, max pages per chapter.

**Unified degradation contract** (review finding — one rule, not two): failure
is always scoped to the smallest unit. A resource that fails structural
parsing renders as a single placeholder page ("unsupported content", localized);
a resource that exceeds a budget renders its laid-out prefix plus a truncation
notice line; the rest of the book is unaffected in both cases. Only an
unreadable container/spine fails the book (existing library error state).
Canonical-projection offsets for a truncated resource cover exactly the
retained prefix, so positions never point past what exists.

Error taxonomy: each crate's thiserror enum converts to `CoreError` variants
(`UnsupportedContent`, `LayoutBudgetExceeded`, `NotReady`, `AnchorNotFound`,
…) mirrored as `InkunaError` with `detail` fields; shells present the existing
error-state UI. Panics remain forbidden; every engine entry point returns
`Result`.

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
  ruby, RTL, mixed-script, image-heavy, table-degradation) built by the shared
  fixture builder, laid out at fixed viewports/settings, asserting page counts
  and glyph positions against committed snapshots.
- **Cross-platform identity is tested, not assumed** (review finding): display
  lists get a canonical deterministic serialization plus a blake3 digest.
  `cargo test` asserts digests on ubuntu; a debug-only hook in each shell
  computes the same digests for the fixture corpus **on an iOS device or
  simulator and an Android emulator**, compared as an explicit parity-gate
  step. Fixed-point layout math (§2) is what makes this expected to pass;
  the digest check is what proves it.
- Property tests: every char lands on exactly one page;
  `locate(hit_test(x)) = x` round-trips; selection rects cover exactly the
  selected range; content coordinates survive `update_layout`;
  projection(golden fixtures) matches committed text snapshots.
- Adversarial fixtures per §12 (budget bombs, malformed XHTML, absurd images).
- Reconcile tests: V8 + reconcile over a fixture DB seeded with real Readium
  locator JSON (valid, corrupt, unresolvable-href), asserting the §8 defaults;
  idempotency under interruption.

**Shell tests:** existing per-platform styles; pager unit tests keep passing
against the engine-backed surface (contract unchanged on iOS, newly pinned by
interface on Android).

**Performance gate definition** (review finding — pinned, reproducible):
measured on the reference pair (the iOS dev device / Pixel-class emulator used
for all profiling), cold open of the seeded benchmark library (which includes
one long-chapter book): tap-to-first-rendered-page ≤ 250 ms and
session-open-to-`on_first_page_ready` ≤ 100 ms; "Keep Reading"
jank profile (gfxinfo / Instruments) no worse than the frozen Readium build's.
The old 43 ms figure was the Readium `doOpen()` alone (excluding WebView spawn
and render); it is context, not the gate.

**Parity gate (merge `dev/core` → `main`):**

1. Every book in the seeded test library opens and restores position after
   V8 + reconcile.
2. Page/chapter navigation including fast-swipe chains across chapter
   boundaries — the bug class that motivated the swap — with no rescue layers.
3. A vertical-CJK book with ruby reads correctly end-to-end; RTL progression
   honored; tap/key direction mapping correct.
4. TOC navigation and internal-link/footnote taps land correctly via
   `locate_href` (including fragment targets).
5. In-book and library search, with highlight rects on the page.
6. Bookmarks and progress survive; position numbers stable across settings
   changes.
7. All four reading themes + night mode; typography settings live-apply and
   match the transcribed §9 mapping.
8. Selection with copy/look-up/share on both platforms, horizontal and
   vertical.
9. VoiceOver/TalkBack navigate the page at block granularity with correct
   bounds, language, and link traits (§7 scope — the documented v1 contract).
10. Performance gate above.
11. Cross-device display-list digest check over the fixture corpus.
12. FFI bindings regenerated via both `scripts/build-core-*.sh`; both shells
    build clean with zero Readium references (`grep -ri readium` finds only
    docs/history).

## Open questions / assumptions

- **A1 — quick-xml over html5ever.** Assumes EPUB-mandated XHTML plus lenient
  recovery covers the real-world corpus; structurally broken books fail closed
  per-resource (§12). If practice shows a meaningful tail of tag-soup EPUBs,
  swap the `dom/` parser for html5ever behind the same arena-tree interface.
- **A2 — composing rustybuzz + own line/vertical layout instead of
  cosmic-text** (which lacks vertical writing). The vertical-writing layout is
  the largest from-scratch surface in the project.
- **A3 — Noto CJK packaging** (variable vs. weights, OTC vs. per-region) and
  final app-size delta: resolved at implementation against current releases;
  the registry schema (§3) is packaging-agnostic.
- **A4 — visible-page-bounded selection** (no auto-page-turn while dragging)
  is acceptable for the v1 gate.
- **A5 — chapter-level accuracy** for the one-time locator conversion is
  acceptable (owner-approved for beta).
- **A6 — `Canvas.drawGlyphs` and `CTFontDrawGlyphs`** are sufficient for all
  run drawing including rotated sideways-Latin (transform per run). No known
  gaps at minSdk 33 / iOS 18; verified in the first spike.
- **A7 — synthetic positions remain the user-facing "position N of M"**
  currency; page numbers within a chapter are layout-local. No whole-book
  pagination is ever required.
- **A8 — fixed-point (1/64) layout precision** is sufficient for visually
  clean justification and ruby alignment at all supported text sizes; if a
  precision artifact surfaces, the unit can shrink (1/256) without contract
  changes.
- **A9 — the owner-directed crate/FFI restructures ride this branch** despite
  the backport-friction cost, mitigated by landing them first (§1). This was
  an explicit owner decision, re-confirmed over a reviewer's decomposition
  objection.

## Out of scope

- System-font reading mode — cut by review; revisit post-swap as its own
  small spec (per-device pagination consequences included).
- Hyphenation — not parity (Readium path never enabled it); post-v1
  typography work.
- Per-element writing-mode mixing; table grid layout — degradation paths
  specified instead.
- Highlights/annotations, TTS, dictionary integration — next features on the
  new position model, each its own spec.
- Scrolled reading mode — paginated-only, as today.
- Fixed-layout EPUB, CBZ/CBR, PDF — dedicated-navigator track.
- Publisher embedded fonts, MathML, SVG, media overlays — opinionated-engine
  non-goals; revisit only with evidence from real libraries.
- Cross-page drag selection — v1 limitation (A4).
- Sync/cross-device positions — content coordinates are designed to serve it
  later.
