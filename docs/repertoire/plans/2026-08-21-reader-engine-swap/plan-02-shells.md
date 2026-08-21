# Reader Engine Swap — Plan 02: Shells — Implementation Plan

> **For the conductor:** this plan is structured into movements — task groups
> sized for one fresh implementer each, with clean seams between them. Execute
> with `/repertoire:maestro` (or any plan executor). Tasks use checkbox syntax
> for tracking.

**Spec:** `docs/repertoire/specs/2026-08-21-reader-engine-swap-spec.md`
**Overview:** `00-overview.md`
**Goal:** Both native shells read on the core engine — `PageView` drawing,
engine-backed pager surfaces, native selection, block-granular accessibility,
complete Readium removal — sealed by the parity-gate evidence.
**Architecture:** The core (plan 01, already landed) owns layout and emits
per-page glyph-run display lists behind a `ReaderSession` FFI whose
interaction path is synchronous and cache-only. Each shell gains one
`PageView` that rasterizes a `PageDisplayList` natively (Core Text /
`Canvas.drawGlyphs`), an engine-backed implementation of the existing
`ReaderPagerSurface` seam so the custom pager's physics carry over untouched,
and shell-idiomatic selection/accessibility over core-computed geometry.
Every Readium-era workaround dies with the WebView it compensated for.
**Build:** iOS `cd apps/ios && xcodegen generate && xcodebuild -project
Inkuna.xcodeproj -scheme Inkuna -destination 'generic/platform=iOS Simulator'
build` · Android `cd apps/android && ./gradlew assembleDebug` · bindings
(only if plan 01's FFI needs regenerating): `./scripts/build-core-ios.sh` and
`./scripts/build-core-android.sh` from repo root.
**Test:** Neither shell has a test target. Every task's Verify is its compile
command plus concrete simulator/emulator checks (`xcrun simctl` / `adb`);
Movement 6 is the full parity gate.

## Cross-plan contracts consumed (from `00-overview.md`, restated once here; each task restates what it uses)

- `Bookshelf.open(data_dir: String, font_dir: String)` — **breaking change**;
  facade accessors `library() / importer() / search() / settings() /
  progress() / stats()`; `open_reader(id, viewport: Viewport, settings:
  ReaderLayoutSettings, listener: LayoutListener) -> ReaderSession` (async,
  last-open-wins).
- Records: `Viewport { width: f64, height: f64 }` (content area excluding
  shell chrome insets, *including* reading margins);
  `ReaderLayoutSettings { reading_font: String, reading_bold: bool,
  text_size_step: u8, line_spacing: f64, letter_spacing: f64,
  word_spacing: f64, reading_margins: u32 }`;
  `Coordinate { spine_idx: u32, char_offset: u64 }`; `CharRange { start: u64,
  end: u64 }` (end exclusive); `Rect { x, y, width, height: f64 }`;
  `WritingMode { HorizontalTb, VerticalRl }`; `SelectionRect { rect,
  writing_mode }`; `ChapterGeometry { generation: u64, page_count: u32,
  char_range, writing_mode, rtl_progression: bool }`; `PageLocation {
  generation: u64, spine_idx: u32, page_idx: u32 }`; `HitResult { coordinate,
  link_target: Option<String> }`; `ColorRole { Text, Secondary, Link }`;
  `RunOrientation { Upright, SidewaysRotated }`; `GlyphRun { font_id: u32,
  size: f64, color_role, glyph_ids: Vec<u16>, positions: Vec<f32> (x,y
  interleaved), orientation }`; `ImagePlacement { href, rect }`;
  `Decoration { kind: Rule|Underline, rect, colorRole }`; `LinkRegion { rect, target }`;
  `A11yBlock { text, rect, lang: Option<String>, is_link: bool, role:
  Body|Heading|Link }`; `PageDisplayList { generation, glyph_runs, images,
  decorations, links, a11y }`; `FontEntry { id: u32, file_path: String,
  collection_index: u32, axes: Vec<FontAxis { tag: String, value: f64 }> }`.
- `ReaderSession` sync/cache-only (throws `InkunaError.NotReady`, never
  blocks; `chapter`/`page` on an un-laid-out chapter schedules layout):
  `chapter(spine_idx)`, `page(spine_idx, page_idx)`, `is_ready(spine_idx)`,
  `locate(coordinate) -> PageLocation`, `locate_href(href, fragment) ->
  Coordinate` (throws `AnchorNotFound`), `hit_test(spine_idx, page_idx, x, y)
  -> HitResult`, `selection_rects(spine_idx, range) -> [SelectionRect]`,
  `word_at(coordinate) -> CharRange`, `text_range(spine_idx, range) ->
  String`, `match_rects(spine_idx, char_offset, len) -> [SelectionRect]`,
  `accessibility_blocks(spine_idx, page_idx) -> [A11yBlock]`,
  `font_registry() -> [FontEntry]`, `page_digest(spine_idx, page_idx) ->
  String`. Async: `update_layout(viewport, settings)`, `resource(href) ->
  Vec<u8>`.
- `LayoutListener` (foreign-implemented):
  `on_first_page_ready(generation: u64, spine_idx: u32)`,
  `on_chapter_ready(generation: u64, spine_idx: u32, page_count: u32)` —
  may arrive on any thread; shells hop to their main thread.
- All geometry in **layout points at 1× scale** (iOS points; Android dp) —
  shells apply screen scale when drawing. All char offsets index the
  canonical text projection.
- Progress/bookmark FFI: same method names as today, `locator: String`
  parameters/fields are now `coordinate: Coordinate`
  (`updateProgress(id, coordinate, progression, position)`,
  `addBookmark(id, coordinate, progression)`, `Publication.coordinate:
  Coordinate?`, `Bookmark.coordinate: Coordinate`).
  `reportPositionCount`/`reportPositionRanges`/`InvalidPositionRanges` are
  gone; `chapterPositionRanges(id) -> [ChapterPositionRange { chapterIdx,
  startPosition, endPosition }]` survives, now core-computed from synthetic
  positions (**fixed 1024-character blocks per spine resource, minimum one**
  — spec §8), one row per spine resource in spine order.
- Fonts: repo-root `assets/fonts/` is the single source of truth (four Noto
  Latin TTFs, OFL.txt, plus the CJK faces plan 01 chose). Plan 02 copies the
  directory wholesale — never names individual CJK files.
- Fixed-layout EPUB: `open_reader` throws `InkunaError.UnsupportedContent`
  (detail names fixed-layout); shells show a localized "not yet supported"
  state.
- Parity numbers: tap-to-first-rendered-page ≤ 250 ms;
  session-open-to-`on_first_page_ready` ≤ 100 ms (cold open, reference
  devices, seeded benchmark library incl. one long-chapter book);
  `page_digest` equality across platforms over the corpus; zero `readium`
  references outside docs/git history (ignoring gitignored `apps/ios/build/`
  and `apps/ios/.derivedData/`).

## Shell-shared derivations (the mirroring convention applies: change one shell's copy, change its sibling in the same commit)

- **Synthetic position of a coordinate** (used for "p. N", progress writes,
  Tonight/Detail, search hit labels): **always a core lookup, never
  shell-side math** (overview: shells never mirror the 1024-char constant).
  In-reader (sync): `session.positionOf(coordinate) -> UInt32` and
  `session.positionCount() -> UInt32`. Outside a session (async):
  `bookshelf.progress().positionOf(id:coordinate:)` /
  `positionCount(id:)`. `progression(c) = positionOf(c) / positionCount`,
  clamped to `0…1`; 0 when `positionCount` is 0. The `ReaderPositions`
  helpers below are thin wrappers over exactly these calls.
- **Current-page anchor coordinate** (progress writes, re-anchoring across
  `update_layout`/rotation): `Coordinate(spineIdx: spine, charOffset:
  session.pageCharRange(spineIdx: spine, pageIdx: page).start)` — the first
  projection character the page covers, by contract.
- **Chapter href navigation:** split the href at the first `#` into
  `(resource, fragment)`, then `session.locate_href(resource, fragment)`;
  `AnchorNotFound` surfaces as the existing non-fatal "link not followed"
  toast. Never normalize further shell-side — percent-decoding and
  leading-slash handling are core-owned now.
- **Spine count:** `session.spineCount()` (in-reader; the parity runner's
  completion check too). `chapterPositionRanges(id)` remains only for
  chapter-row display (contents sheet, pages-left labels) — never as a
  spine-count proxy.
- **Highlight/selection palette** (identical constants both shells):
  selection and search-highlight fill `#B4863B` at 0.30 alpha on day themes
  (paper/calm), `#D9AE63` at 0.30 alpha on night themes (quiet/moon);
  selection handles the same hex at full alpha (these are the DesignSystem
  accent values).
- **Parity cases come from plan 01's export, never from shell constants**
  (overview contract): `cd core && cargo run -p inkuna-engine --example
  export-parity-fixtures --features test_support -- <dir>` produces the
  fixture EPUBs plus `manifest.json` — `[{ "file", "viewport": { "width",
  "height" }, "settings": { <ReaderLayoutSettings values> } }]`. Both
  harness hooks read `manifest.json` from the corpus directory and run
  exactly its cases; the compare script records the manifest's SHA-256.

## File structure

Repo root / scripts:
- Create `scripts/parity-compare.sh` — jq-based digest-JSON comparison for the parity gate.
- Create `docs/repertoire/plans/2026-08-21-reader-engine-swap/parity-evidence.md` — the recorded gate evidence (Movement 6 only).

iOS — create:
- `apps/ios/Inkuna/Reader/ReaderPagerSurface.swift` — the slimmed pager-surface protocol + `ReaderPagerStrip`, moved out of the dying Readium file.
- `apps/ios/Inkuna/Reader/Engine/ReaderFontStore.swift` — app-wide CTFont cache built from `FontEntry`.
- `apps/ios/Inkuna/Reader/Engine/PageView.swift` — draws one `PageDisplayList`; exposes a11y elements.
- `apps/ios/Inkuna/Reader/Engine/PageImageProvider.swift` — async image bytes → `UIImage`, memoized.
- `apps/ios/Inkuna/Reader/Engine/EnginePageCanvas.swift` — hosts/pools `PageView`s, positions the strips.
- `apps/ios/Inkuna/Reader/Engine/EnginePagerSurface.swift` — `ReaderPagerSurface` over `ReaderSession` + canvas.
- `apps/ios/Inkuna/Reader/Engine/ReaderLayoutRelay.swift` — `LayoutListener` → MainActor hop.
- `apps/ios/Inkuna/Reader/Engine/ReaderSelectionController.swift` — long-press/word/drag/edit-menu selection.
- `apps/ios/Inkuna/Reader/Engine/SelectionOverlayView.swift` — selection rect overlay + handle views.
- `apps/ios/Inkuna/Model/ReaderPositions.swift` — synthetic-position derivation helper (mirrored on Android).
- `apps/ios/Inkuna/Debug/ParityDigestRunner.swift` — DEBUG-only digest dump over a corpus directory.

iOS — modify: `apps/ios/project.yml` (fonts resource folder, UIAppFonts, stale comment; later: Readium package removal), `apps/ios/Inkuna/Library/LibraryStore.swift`, `apps/ios/Inkuna/Reader/ReaderViewController.swift` (open-path rewrite), `apps/ios/Inkuna/Reader/ReaderPager.swift` (rescue-layer deletion), `apps/ios/Inkuna/Reader/ReaderSearchPanel.swift`, `apps/ios/Inkuna/Reader/ReaderCustomizeViewController.swift`, `apps/ios/Inkuna/Reader/ContentsSheetViewController.swift`, `apps/ios/Inkuna/Home/TonightViewController.swift`, `apps/ios/Inkuna/Detail/BookDetailViewController.swift`, `apps/ios/Inkuna/SceneDelegate.swift` (parity debug route), `apps/ios/Inkuna/DesignSystem/ReadingFont.swift` (bundled-only pruning), `apps/ios/Inkuna/Localizable.xcstrings` (new keys).

iOS — delete: `apps/ios/Inkuna/Reader/ReadiumPagerSurface.swift`, `apps/ios/Inkuna/Reader/ReaderStyleSurface.swift`, `apps/ios/Inkuna/Reader/ReaderUserStyle.swift`, `apps/ios/Inkuna/Reader/ReadingFontDeclarations.swift`, `apps/ios/Inkuna/Model/ChapterHref.swift`, `apps/ios/Inkuna/Fonts/` (all four TTFs), the Readium SPM package entry in `project.yml`.

Android — create (package `app.inkuna.android` unless noted):
- `app/src/main/java/app/inkuna/android/ui/reader/ReaderPagerSurface.kt` — Kotlin mirror of the iOS protocol + `ReaderPagerStrip`.
- `app/src/main/java/app/inkuna/android/ui/reader/ReadiumPagerSurface.kt` — **temporary** (M4) Readium-backed implementation, deleted in M5.
- `app/src/main/java/app/inkuna/android/ui/reader/engine/ReaderFontStore.kt` — `android.graphics.fonts.Font` cache from `FontEntry`.
- `app/src/main/java/app/inkuna/android/ui/reader/engine/PageView.kt` — draws one `PageDisplayList` via `Canvas.drawGlyphs`.
- `app/src/main/java/app/inkuna/android/ui/reader/engine/PageAccessibilityHelper.kt` — `ExploreByTouchHelper` over `A11yBlock`s.
- `app/src/main/java/app/inkuna/android/ui/reader/engine/PageImageLoader.kt` — async image bytes → `Bitmap`, memoized.
- `app/src/main/java/app/inkuna/android/ui/reader/engine/EnginePageCanvas.kt` — hosts/pools `PageView`s, positions the strips.
- `app/src/main/java/app/inkuna/android/ui/reader/engine/EnginePagerSurface.kt` — engine-backed `ReaderPagerSurface`.
- `app/src/main/java/app/inkuna/android/ui/reader/engine/ReaderSelectionController.kt` — long-press/word/drag/ActionMode selection.
- `app/src/main/java/app/inkuna/android/ui/reader/ReaderPositions.kt` — mirror of `ReaderPositions.swift`.
- `app/src/main/java/app/inkuna/android/model/CoreFonts.kt` — APK-asset → filesDir font extraction.
- `app/src/main/java/app/inkuna/android/debug/ParityDigestRunner.kt` — DEBUG-only digest dump.

Android — modify: `app/build.gradle.kts` (font sync task; later dependency removals), `app/src/main/java/app/inkuna/android/model/LibraryStore.kt`, `ui/reader/ReaderViewModel.kt` (rewrite), `ui/reader/ReaderScreen.kt` (rewrite), `ui/reader/ReaderPagerLayout.kt` (surface refactor), `ui/reader/ReaderAppearance.kt`, `ui/reader/ReaderSearchPanel.kt`, `ui/reader/ReaderSheets.kt`, `MainActivity.kt`, `importing/ImportIntentActivity.kt` (comment only), `ui/tonight/TonightViewModel.kt`, `ui/detail/BookDetailViewModel.kt`, `ui/InkunaApp.kt` (perf timestamp), `ui/theme/ReadingFont.kt`, `res/values*/strings.xml` (new keys, all 14 locale dirs).

Android — delete: `ui/reader/ReaderNavigatorHost.kt`, `ui/reader/ReaderStyleInjector.kt`, `ui/reader/ReaderUserCss.kt`, `ui/reader/ReaderWebViewTuner.kt`, `ui/reader/ReaderPageTurnListener.kt`, `ui/reader/ReadiumPagerSurface.kt` (the M4 temporary), `app/src/main/assets/fonts/`, and from `build.gradle.kts`: the three `org.readium.kotlin-toolkit` artifacts, the `androidx.webkit` strict pin, `coreLibraryDesugaring` + `desugar_jdk_libs`, `androidx.viewpager`, `androidx.fragment:fragment-ktx` (each after verifying no other consumer).

## Movement 1: Font bundling, mechanical FFI migration, iOS PageView

Plan 01 landed a breaking FFI (facades, `Bookshelf.open(data_dir, font_dir)`,
`Coordinate`-based progress/bookmarks, deleted position reporting), so
neither shell compiles until this movement's mechanical migration is done —
it travels with the font-bundle wiring because `open` cannot be called
without a real `font_dir`. Then the first engine-rendering component lands:
the iOS `PageView`.
**Depends on:** plan 01 fully landed with bindings regenerated via both
`scripts/build-core-*.sh`.

- [ ] **Task 1.1 — iOS: bundle `assets/fonts/` + mechanical FFI migration (compile restorer)**
  - **Files:** Modify `apps/ios/project.yml`,
    `apps/ios/Inkuna/Library/LibraryStore.swift`,
    `apps/ios/Inkuna/Home/TonightViewController.swift`,
    `apps/ios/Inkuna/Detail/BookDetailViewController.swift`,
    `apps/ios/Inkuna/Reader/ReaderViewController.swift`,
    `apps/ios/Inkuna/Reader/ContentsSheetViewController.swift`,
    `apps/ios/Inkuna/Reader/ReaderSearchPanel.swift` (and any other call
    site the compiler flags) · Create
    `apps/ios/Inkuna/Model/ReaderPositions.swift` · Delete
    `apps/ios/Inkuna/Fonts/` (NotoSans.ttf, NotoSans-Italic.ttf,
    NotoSerif.ttf, NotoSerif-Italic.ttf).
  - **Behavior:**
    - `project.yml`: remove the four bare `UIAppFonts` filenames' source (the
      deleted `Inkuna/Fonts/` group needs no entry — it was globbed) and add
      the repo fonts directory as a **folder reference resource** on the
      `Inkuna` target: under `sources:`, `- path: ../../assets/fonts`,
      `type: folder`, `buildPhase: resources`. The bundle then contains a
      `fonts/` directory with every file in `assets/fonts/` (CJK faces and
      OFL.txt included, wholesale — never name individual CJK files).
      `UIAppFonts` becomes the four Latin entries with the folder prefix:
      `fonts/NotoSerif.ttf`, `fonts/NotoSerif-Italic.ttf`,
      `fonts/NotoSans.ttf`, `fonts/NotoSans-Italic.ttf` (registration still
      feeds the native font-picker previews). Rewrite the stale comment above
      `UIAppFonts` ("WKWebView never sees app-registered fonts…") to:
      registration serves UIKit font previews only; the reading surface draws
      glyph runs via Core Text from `FontEntry.file_path`, which needs no
      registration. Also rewrite the now-false Readium comment block at the
      top of `packages:` to say the package is being removed by this plan
      (the entry itself is deleted in Task 3.2, not here — Readium code still
      compiles until then).
    - `LibraryStore.swift`: `Bookshelf.open(dataDir:fontDir:)` — consume the
      overview contract `Bookshelf.open(data_dir: String, font_dir: String)`
      with `font_dir = Bundle.main.resourceURL!.appendingPathComponent("fonts").path`;
      throw a new `LibraryStoreError.missingFontDirectory` case if
      `FileManager.default.fileExists` is false for it (a broken bundle is a
      build error surfaced honestly, not a crash in core).
    - Facade migration, whole target: every `bookshelf.<method>(...)` becomes
      `bookshelf.<facade>().<method>(...)` per the overview mapping —
      `library()` (list/publication/remove/chapters/chapterPositionRanges/
      addBookmark/bookmarks/removeBookmark/setFinished/searchLibrary),
      `importer()` (import/importBatch/importFd/importBatchFds/
      optimizeCovers), `search()` (searchAllBooks/searchInBook),
      `settings()` (settings/setSettings), `progress()` (updateProgress/
      sessionStart/sessionEnd), `stats()` (statsOverview). Facade accessor
      results may be cached in a local, never stored past the `Bookshelf`.
    - Coordinate migration: `Publication.locator: String?` is now
      `Publication.coordinate: Coordinate?` (`Coordinate { spineIdx: UInt32,
      charOffset: UInt64 }`); `Bookmark.locator` → `Bookmark.coordinate`;
      `updateProgress(id:coordinate:progression:position:)`;
      `addBookmark(id:coordinate:progression:)`. Create
      `Model/ReaderPositions.swift`:
      `enum ReaderPositions { static func position(of coordinate: Coordinate,
      id: String, on shelf: Bookshelf) async throws -> UInt32;
      static func progression(of coordinate: Coordinate, id: String,
      on shelf: Bookshelf) async throws -> Double }` — thin wrappers over
      `shelf.progress().positionOf(id:coordinate:)` / `positionCount(id:)`
      (shared-derivations rule: no shell-side position math, no 1024
      constant anywhere in Swift; header comment names the Android sibling
      `ui/reader/ReaderPositions.kt`). Tonight/Detail replace their
      `Locator` JSON decoding with `publication.coordinate` +
      `ReaderPositions` (positions-left-in-chapter = `row.endPosition −
      position`, where `row` is the matching `ChapterPositionRange` used
      for display only); their `import ReadiumShared` lines drop out
      here if nothing else needs them (Task 2.4 sweeps whatever survives).
    - Interim Readium bridge (explicitly temporary, deleted by Task 2.2 —
      mark every site `// ENGINE-SWAP INTERIM: dies with the Readium open
      path (plan-02 Task 2.2)`): `ReaderViewController` still opens through
      Readium in this movement. Restore position by mapping
      `publication.coordinate` to a chapter-start `Locator` (reading-order
      link at index `spineIdx`, `progression: 0`); write progress with
      `Coordinate(spineIdx: currentResourceIndex, charOffset: 0)` and the
      navigator's `totalProgression`; bookmarks the same way. Delete
      `reportPositionCountIfNeeded()` and every
      `reportPositionCount`/`reportPositionRanges` call (methods no longer
      exist); `chapterPositionRanges` keeps feeding the contents sheet and
      search "p. N" labels unchanged.
  - **Error handling:** `Bookshelf.open` failure keeps the existing thrown
    path (retryable library error screen). `ReaderPositions.position`
    throws on a missing book — callers hide the position label, exactly as
    today's nil-position path does.
  - **Verify:** `cd apps/ios && xcodegen generate && xcodebuild -project
    Inkuna.xcodeproj -scheme Inkuna -destination 'generic/platform=iOS
    Simulator' build` → succeeds. Then build for a named simulator, `xcrun
    simctl install booted <.app path>` and launch: library lists books,
    Tonight and Detail show progress, a book opens via the interim Readium
    path at its chapter start. `unzip -l` (or `ls` on the built .app) shows
    `fonts/` containing every `assets/fonts/` file.

- [ ] **Task 1.2 — Android: bundle `assets/fonts/` + extraction + mechanical FFI migration (compile restorer)**
  - **Files:** Modify `apps/android/app/build.gradle.kts`,
    `app/src/main/java/app/inkuna/android/model/LibraryStore.kt`,
    `ui/tonight/TonightViewModel.kt`, `ui/detail/BookDetailViewModel.kt`,
    `ui/reader/ReaderViewModel.kt`, `ui/reader/ReaderScreen.kt`,
    `ui/reader/ReaderSheets.kt`, `ui/reader/ReaderSearchPanel.kt` (and any
    other call site the compiler flags) · Create
    `app/src/main/java/app/inkuna/android/model/CoreFonts.kt`,
    `app/src/main/java/app/inkuna/android/ui/reader/ReaderPositions.kt` ·
    Delete `apps/android/app/src/main/assets/fonts/` (the four TTFs).
  - **Behavior:**
    - `build.gradle.kts`: register
      `val syncCoreFonts by tasks.registering(Sync::class)` copying
      `rootProject.file("../assets/fonts")` into
      `layout.buildDirectory.dir("generated/inkunaFonts/fonts")`; add
      `android.sourceSets.getByName("main").assets.srcDir(layout.buildDirectory.dir("generated/inkunaFonts"))`;
      `tasks.named("preBuild") { dependsOn(syncCoreFonts) }`. (Do NOT point
      an assets srcDir at repo `assets/` directly — `assets/brand/` must not
      ship in the APK.) Delete `app/src/main/assets/fonts/`.
    - `CoreFonts.kt`: `object CoreFonts { fun ensureExtracted(context:
      Context): File }` — copies every APK asset under `fonts/` into
      `File(context.filesDir, "fonts")` (the Rust core needs real file
      paths; APK assets are not files). Idempotence: a sibling marker file
      `fonts/.version` holds `BuildConfig.VERSION_CODE` plus the sorted
      asset name list; matching marker → return immediately; mismatch →
      delete the directory and re-copy atomically (copy to `fonts.tmp`,
      rename). Runs on the caller's dispatcher (LibraryStore already opens
      on `Dispatchers.IO`).
    - `LibraryStore.kt`: `Bookshelf.open(dataDir.absolutePath,
      CoreFonts.ensureExtracted(context).absolutePath)` inside the existing
      IO block.
    - Facade + Coordinate migration: identical mapping to Task 1.1 (Kotlin:
      `bookshelf.library().publication(id)` etc.; `Publication.coordinate:
      Coordinate?`, `Bookmark.coordinate`,
      `updateProgress(id, coordinate, progression, position)`,
      `addBookmark(id, coordinate, progression)`).
      `ReaderPositions.kt`: `object ReaderPositions {
      suspend fun position(coordinate: Coordinate, id: String,
      shelf: Bookshelf): UInt;
      suspend fun progression(coordinate: Coordinate, id: String,
      shelf: Bookshelf): Double }` — thin wrappers over
      `shelf.progress().positionOf(id, coordinate)` / `positionCount(id)`
      (shared-derivations rule: no shell-side position math, no 1024
      constant anywhere in Kotlin; header comment names the iOS sibling).
      Tonight/Detail ViewModels drop their `Locator` JSON decoding
      for `publication.coordinate` + `ReaderPositions`.
    - Interim Readium bridge in `ReaderViewModel.kt` (marked
      `// ENGINE-SWAP INTERIM: dies with the Readium open path (plan-02
      Task 5.1)`): restore at chapter start from `coordinate.spineIdx`
      (reading-order locator, progression 0); progress/bookmark writes send
      `Coordinate(spineIdx, 0u)`; delete the `positionsByReadingOrder()`
      reporting block (`reportPositionCount`/`reportPositionRanges` no
      longer exist) — `chapterPositionRanges` now feeds "p. N" and
      pages-left directly.
  - **Error handling:** `CoreFonts.ensureExtracted` IO failure throws; the
    existing LibraryStore failure path (retryable) handles it. A partial
    copy never becomes current thanks to the tmp-dir rename.
  - **Verify:** `cd apps/android && ./gradlew assembleDebug` → succeeds;
    `./gradlew installDebug`, launch on the emulator (`adb shell am start -n
    app.inkuna.android/.MainActivity`): library lists, Tonight/Detail show
    progress, a book opens at chapter start. `adb shell run-as
    app.inkuna.android ls files/fonts` lists every `assets/fonts/` file plus
    `.version`. `unzip -l app/build/outputs/apk/debug/app-debug.apk | grep
    assets/` shows `assets/fonts/…` and nothing from `assets/brand/`.

- [ ] **Task 1.3 — iOS `ReaderFontStore`**
  - **Files:** Create `apps/ios/Inkuna/Reader/Engine/ReaderFontStore.swift`.
  - **Behavior:** App-wide cache mapping the core font registry to CTFonts.
    `@MainActor final class ReaderFontStore { static let shared:
    ReaderFontStore; func prime(_ registry: [FontEntry]);
    func font(id: UInt32, size: CGFloat) -> CTFont? }`. Consumes `FontEntry
    { id: UInt32, filePath: String, collectionIndex: UInt32, axes:
    [FontAxis { tag: String, value: Double }] }` (plain TTFs have
    `collectionIndex 0`, empty `axes`). Face construction:
    `CTFontManagerCreateFontDescriptorsFromURL(fileURL)` → descriptor at
    `collectionIndex` → apply non-empty `axes` via a
    `kCTFontVariationAttribute` dictionary (four-char tag string →
    FourCharCode UInt32 key, Double value) with
    `CTFontDescriptorCreateCopyWithAttributes` →
    `CTFontCreateWithFontDescriptor(desc, size, nil)`. Two caches:
    descriptors keyed by `id` (built once at `prime`), fonts keyed by
    `(id, size)` (sizes come from `GlyphRun.size` — a handful of distinct
    values per settings state; unbounded dictionary is fine, cleared on
    re-`prime`).
  - **Error handling:** A registry entry whose file is missing or yields no
    descriptor logs once (`Logger(subsystem: "app.inkuna.ios", category:
    "reader")`) and `font(id:size:)` returns nil — `PageView` skips runs
    with a nil font (never crashes; visible loss is the honest failure for
    a broken bundle).
  - **Verify:** iOS build command → succeeds (consumed for real in 1.4).

- [ ] **Task 1.4 — iOS `PageView`: glyph runs, decorations, images**
  - **Files:** Create `apps/ios/Inkuna/Reader/Engine/PageView.swift`,
    `apps/ios/Inkuna/Reader/Engine/PageImageProvider.swift`.
  - **Behavior:** `final class PageView: UIView` — renders exactly one page,
    static drawing, no scrolling. API:
    `func present(_ list: PageDisplayList?, spineIdx: UInt32, pageIdx:
    UInt32, session: ReaderSession)`, `var theme: ReadingTheme` (didSet →
    `setNeedsDisplay`), `var imageProvider: PageImageProvider?`. Consumes
    `PageDisplayList { generation, glyphRuns, images, decorations, links,
    a11y }` and `GlyphRun { fontId, size, colorRole, glyphIds: [UInt16],
    positions: [Float] (x,y interleaved, len = 2×glyphs), orientation }`.
    `isOpaque = true`, `backgroundColor = theme.background`,
    `contentMode = .redraw`.
    - `draw(_:)`: fill background; flip the context once
      (`translateBy(0, bounds.height)`, `scaleBy(1, −1)`,
      `textMatrix = .identity`). Positions are layout points, top-left
      origin, y = baseline; upright glyphs draw at `(x, H − y)` via
      `CTFontDrawGlyphs(ReaderFontStore.shared.font(id:size:), glyphs,
      points, count, ctx)` — one call per run, fill color from
      `color(for: run.colorRole)`. `SidewaysRotated` runs: save state,
      translate to the run's first pen position, rotate 90° so the run
      reads top-to-bottom on screen (sideways Latin inside `vertical-rl`;
      verify direction visually against the vertical-CJK fixture), draw
      with positions rebased to the run origin, restore.
    - Color roles → `ReadingTheme` (the existing token type: `paper`,
      `calm`, `quiet`, `moon`): `Text` → `theme.foreground`, `Secondary` →
      `theme.dimmedForeground`, `Link` → the accent hex for the theme's
      day/night side (`#B4863B` day, `#D9AE63` night — shared palette
      above). Theme switches only repaint; they never touch layout.
    - Decorations `{ kind: Rule|Underline, rect, colorRole }`: fill `rect`
      with the theme color for `colorRole` — core assigns the role
      (overview contract); the shell never infers decoration colors.
    - Images: for each `ImagePlacement { href, rect }` draw the decoded
      image aspect-fit inside `rect` if the provider has it; otherwise draw
      the placeholder (secondary color at 8% alpha fill, 1-pt hairline
      border at 20% alpha) and ask the provider to load; on arrival the
      provider calls back and the view `setNeedsDisplay()`s.
    - `PageImageProvider`: `@MainActor final class PageImageProvider {
    init(session: ReaderSession); func image(for href: String, onReady:
    @escaping () -> Void) -> UIImage? }` — synchronous cache hit or nil +
    kick an async `session.resource(href)` (the overview's async byte
    fetch, budget-capped) → `UIImage(data:)` decoded off-main via
    `UIImage.prepareForDisplay` → NSCache (`totalCostLimit` 32 MB, cost =
    byte count) → `onReady` on main. One in-flight task per href.
  - **Error handling:** nil font from the store → skip the run.
    `resource(href)` throwing (budget cap, missing resource) → cache a
    permanent-miss marker so the placeholder stays and the fetch is never
    retried in a loop. A `present` with a display list whose `generation`
    differs from the canvas's current generation is discarded by the caller
    (canvas, Task 2.2) — `PageView` itself just draws what it's given.
  - **Verify:** iOS build command → succeeds. (First on-screen use is
    Task 2.3's open-path rewrite; no standalone harness — the compile plus
    Movement 2's visual checks cover it.)

- [ ] **Task 1.5 — iOS `PageView` accessibility elements**
  - **Files:** Modify `apps/ios/Inkuna/Reader/Engine/PageView.swift`.
  - **Behavior:** On `present(...)`, fetch
    `session.accessibilityBlocks(spineIdx:pageIdx:)` (sync, cache-only;
    on `InkunaError.NotReady` leave elements empty — the caller re-presents
    when ready) and build `accessibilityElements`: one
    `UIAccessibilityElement(accessibilityContainer: self)` per `A11yBlock {
    text, rect, lang, isLink, role }`, in array order (logical reading
    order). Per element: `accessibilityFrameInContainerSpace = rect` (layout
    points are UIKit points — no scaling); `accessibilityAttributedLabel` =
    the block text with `.accessibilitySpeechLanguage` set to `lang` when
    present; traits: `.staticText` for `Body`, `.header` for `Heading`,
    `.link` when `isLink` or role `Link`. The view itself
    `isAccessibilityElement = false`. Ruby is already appended
    parenthetically inside `text` by the core — no shell handling.
  - **Error handling:** covered above (`NotReady` → empty, re-built on the
    next `present`).
  - **Verify:** iOS build command → succeeds. Functional check lands with
    Movement 2 (VoiceOver step in Task 2.3's verify).

## Movement 2: iOS engine surface + open path

The iOS reader leaves Readium: the pager seam slims to its
engine-shaped contract, an engine-backed surface implements it, and
`ReaderViewController` opens books through `Bookshelf.open_reader`. At the
end of this movement the iOS reader reads real books on the core engine
(selection arrives in Movement 3; Readium code still compiles but is dead).
**Depends on:** Movement 1.

- [ ] **Task 2.1 — Slim `ReaderPagerSurface`, delete the pager's rescue layers**
  - **Files:** Create `apps/ios/Inkuna/Reader/ReaderPagerSurface.swift` ·
    Modify `apps/ios/Inkuna/Reader/ReaderPager.swift`,
    `apps/ios/Inkuna/Reader/ReadiumPagerSurface.swift` (temporarily, to keep
    compiling until Task 3.2 deletes it).
  - **Behavior:** Move the protocol out of the dying file and slim it to the
    engine-shaped contract (this exact shape is mirrored member-for-member
    by Android's Task 4.1 — the mirroring convention applies):
    ```swift
    @MainActor
    protocol ReaderPagerSurface: AnyObject {
        var isEngageable: Bool { get }
        var isBusy: Bool { get }
        var hasActiveSelection: Bool { get }
        var isRightToLeft: Bool { get }
        func innerMetrics() -> ReaderPagerStrip?
        func setInnerOffset(_ x: CGFloat)
        func outerMetrics() -> ReaderPagerStrip?
        func setOuterOffset(_ x: CGFloat)
        func neighborIsReady(toRight: Bool) -> Bool
        func commitBoundaryCrossing(toRight: Bool) -> Bool
    }
    struct ReaderPagerStrip {
        var offset: CGFloat
        var range: ClosedRange<CGFloat>
        var pageWidth: CGFloat
    }
    ```
    Dropped from the old protocol, per the workaround inventory:
    `suppressNativeGestures`/`restoreNativeGestures` (no native renderer
    gestures exist to silence), async `commitBoundaryCrossing` (now a
    synchronous cache-pointer swap that reports only whether the neighbor's
    geometry was present) and `verifyBoundaryCommit` (nothing asynchronous
    remains to verify). `ReaderPager.swift` edits: remove the
    suppress/restore call sites; make the boundary commit synchronous at its
    one call site (the `outerSpringSettled` path around line 707–767) and
    **delete the verification task, its retry/rescue reaction, and any
    widened arrival-commit thresholds** — a `false` commit (neighbor
    geometry evicted mid-flight, effectively impossible) is handled by
    recapturing baselines on the next gesture, not by rescue logic. Physics,
    springs, chained-turn velocity, hold-loop, rubber-banding stay
    byte-identical — rescue/verify/latch/timing-compensation is workaround;
    gesture physics and animation curves are feel. Patch the old
    `ReadiumPagerSurface` class minimally to conform (wrap its async commit
    in a `false`-returning sync stub is NOT acceptable — instead keep it
    compiling by making its commit synchronous-best-effort and marking the
    file `// dies in Task 3.2`; it is never executed after Task 2.3).
    Delete the now-orphaned `ReaderAccessibilityScrolling` protocol and
    `ReadiumNavigatorShim` only in Task 3.2 (they live in the dying file);
    `ReaderViewController.accessibilityScroll` (its own override, line
    ~859) already routes to the pager and survives unchanged.
  - **Error handling:** none new — the protocol slims, behavior contracts
    move to the engine surface.
  - **Verify:** iOS build command → succeeds (the Readium surface still
    conforms; the reader still opens via the interim path).

- [ ] **Task 2.2 — `EnginePageCanvas` + `EnginePagerSurface`**
  - **Files:** Create `apps/ios/Inkuna/Reader/Engine/EnginePageCanvas.swift`,
    `apps/ios/Inkuna/Reader/Engine/EnginePagerSurface.swift`,
    `apps/ios/Inkuna/Reader/Engine/ReaderLayoutRelay.swift`.
  - **Behavior:**
    - `ReaderLayoutRelay`: the foreign `LayoutListener` implementation —
      `final class ReaderLayoutRelay: LayoutListener` holding two
      `@Sendable` closures set at init:
      `onFirstPageReady: @Sendable (UInt64, UInt32) -> Void`,
      `onChapterReady: @Sendable (UInt64, UInt32, UInt32) -> Void`.
      Callbacks may arrive on any thread (overview contract); each method
      body is `Task { @MainActor in … }` hopping before touching UI state.
    - `EnginePageCanvas: UIView` — owns a pool of `PageView`s (max 6, keyed
      `(spineIdx, pageIdx)`, LRU-recycled) and positions them; one shared
      `PageImageProvider`. API:
      `init(session: ReaderSession, theme: ReadingTheme)`,
      `func setScene(_ scene: PageScene)`, `var theme` (didSet → repaint all
      mounted views), `func invalidate(generation: UInt64)` (drops every
      mounted view whose list generation is stale, re-fetches).
      `struct PageScene { var spineIdx: UInt32; var pageCount: UInt32;
      var rtl: Bool; var innerOffset: CGFloat; var outerDisplacement:
      CGFloat; var neighborEdge: (spineIdx: UInt32, pageIdx: UInt32,
      toRight: Bool)? }`.
      Geometry (the strip model the pager already speaks):
      `pageWidth = bounds.width`; slot of a page =
      `rtl ? (pageCount − 1 − pageIdx) : pageIdx`; page at slot `s` sits at
      `x = s·w − innerOffset + outerDisplacement`. The canvas mounts the
      pages whose frames intersect the bounds (at rest: one; mid-drag: two)
      plus, when `neighborEdge` is set, that page offset one full width to
      the indicated side. Display lists come from
      `session.page(spineIdx:pageIdx:)` (sync, cache-only) at mount time.
    - `EnginePagerSurface` — implements the Task 2.1 protocol over
      `(session, canvas)` and owns the reader's position state:
      ```swift
      @MainActor final class EnginePagerSurface: ReaderPagerSurface {
          init(session: ReaderSession, canvas: EnginePageCanvas)
          private(set) var spineIdx: UInt32
          private(set) var pageIdx: UInt32
          var spineCount: UInt32          // set once by the reader (session.spineCount())
          var onPageSettled: ((UInt32, UInt32) -> Void)?  // spine, page — after any turn/jump lands
          func display(spineIdx: UInt32, pageIdx: UInt32) // programmatic jump: reset strips, setScene, fire onPageSettled
          func chapterBecameReady(generation: UInt64, spineIdx: UInt32)    // from the relay
          func layoutInvalidated(generation: UInt64)      // update_layout: mark busy, canvas.invalidate
          var selectionActive: Bool                        // set by the selection controller
      }
      ```
      Protocol mapping: `isEngageable` — session open ∧ current
      `ChapterGeometry` cached (`session.chapter(spineIdx)` succeeded) ∧
      canvas laid out; `isBusy` — true from `layoutInvalidated` until
      `chapterBecameReady` arrives for the current chapter at the new
      generation (the only busy window left; there is no renderer to wait
      on); `hasActiveSelection` — `selectionActive`; `isRightToLeft` —
      current geometry's `rtlProgression`. `innerMetrics()` —
      `ReaderPagerStrip(offset: innerOffset, range: 0…(pageCount−1)·w,
      pageWidth: w)` from the cached `ChapterGeometry { generation,
      pageCount, charRange, writingMode, rtlProgression }`; nil while not
      ready. `setInnerOffset` — clamp, store, `canvas.setScene`.
      `outerMetrics()` — the synthetic 3-slot chapter strip: `pageWidth =
      w`, `offset = w`, `range = (leftNeighborExists ? 0 : w) …
      (rightNeighborExists ? 2w : w)` where the geometric neighbor spine is
      `rtl ? spineIdx∓1 : spineIdx±1` and existence means `0 ≤ n <
      spineCount`. `setOuterOffset(x)` — `outerDisplacement = w − x` pushed
      into the scene along with `neighborEdge` (the geometric neighbor's
      entry page: its slot-0 page when revealed from the right, its last
      slot when revealed from the left — with the slot formula this lands
      on "next chapter first page / previous chapter last page" in both
      progressions automatically). `neighborIsReady(toRight:)` — neighbor
      exists ∧ `session.isReady(neighborSpine)`; calling
      `session.chapter(neighborSpine)` first schedules its layout (the
      overview's contract) so readiness converges while the user reads.
      `commitBoundaryCrossing(toRight:)` — read the neighbor's
      `ChapterGeometry` (must be cached — the pager only commits to ready
      neighbors); set `spineIdx = neighbor`, `pageIdx` = entry page
      (first page when entering forward in reading order, last page when
      entering backward), `innerOffset` = that page's rest offset, reset
      `outerDisplacement` to 0, `setScene`, fire `onPageSettled`; returns
      false only if the geometry read throws `NotReady` (then nothing
      changes — the pager recaptures baselines on the next gesture).
      `onPageSettled` also fires when a drag/spring rests on a new
      `pageIdx` (detected in `setInnerOffset` when offset lands on a page
      boundary and no gesture is active — the pager calls the existing
      settle path, which the reader observes for progress/chrome).
  - **Error handling:** every sync session call catches
    `InkunaError.NotReady` and degrades (nil metrics, blank page slot —
    the pager already treats nil metrics/unready neighbors as "no
    neighbor": rubber band, honest snap). Stale generations: any
    `PageDisplayList`/`ChapterGeometry`/`PageLocation` whose `generation`
    differs from the latest `layoutInvalidated`-known generation is
    discarded, per the overview's generation rule.
  - **Verify:** iOS build command → succeeds (goes live in 2.3).

- [ ] **Task 2.3 — `ReaderViewController` open-path rewrite**
  - **Files:** Modify `apps/ios/Inkuna/Reader/ReaderViewController.swift`,
    `apps/ios/Inkuna/Reader/ReaderCustomizeViewController.swift`,
    `apps/ios/Inkuna/DesignSystem/ReadingFont.swift`,
    `apps/ios/Inkuna/Reader/ContentsSheetViewController.swift`.
  - **Behavior:** Replace the Readium open path (`openPublication`,
    `openBook`, `injectingInkunaStyles`, `indexResources`, `applyUserStyle`,
    `jumpLocator`, the navigator delegate conformances and the
    `EPUBNavigatorViewController` child) with the engine flow. The interim
    bridges from Task 1.1 die here.
    - **Open:** in `viewDidLoad`/`openPublication` compute the viewport —
      `Viewport(width: view.bounds.width, height: view.bounds.height −
      insets.top − insets.bottom)` with `insets =
      ReaderMetrics.contentInsets(safeArea:isPad:)` (the overview: viewport
      excludes shell chrome insets, includes reading margins — margins are
      settings, applied inside core) — and the settings record
      `ReaderLayoutSettings(readingFont:readingBold:textSizeStep:
      lineSpacing:letterSpacing:wordSpacing:readingMargins:)` mapped 1:1
      from the core settings facade's stored values, with `readingFont`
      normalized through `ReadingFont` (below). Then `session = try await
      bookshelf.openReader(id:viewport:settings:listener:)` with a
      `ReaderLayoutRelay` whose closures route to the surface
      (`chapterBecameReady`) and to first-render bookkeeping. Build
      `EnginePageCanvas` (pinned inside the reading band: top/bottom =
      `ReaderMetrics` insets) + `EnginePagerSurface`; `surface.spineCount =
      session.spineCount()` (the shared-derivations rule). `ReaderPager`
      attaches to the new surface unchanged.
    - **Restore & progressive first page:** target coordinate = the
      `initialChapter`'s href through the shared href rule (split at `#`,
      `session.locateHref(resource:fragment:)`) when launched from a
      contents row, else `publication.coordinate ?? Coordinate(spineIdx: 0,
      charOffset: 0)`. On `on_first_page_ready` for the target's chapter (or
      immediately if `session.locate(coordinate)` already succeeds):
      `surface.display(spineIdx:pageIdx:)` from the `PageLocation {
      generation, spineIdx, pageIdx }` and hide the loading state — the
      first page shows before neighbors exist. `NotReady` from `locate`
      before that callback → keep the loading state; the callback always
      follows (or the open throws).
    - **Progress & sessions:** on every `onPageSettled` debounce-write
      (the existing `enqueueCoreWrite` machinery): anchor coordinate = the
      shared probe (`hit_test` at the reading-start corner), `position =
      ReaderPositions.position(of:in:)`, `progression =
      ReaderPositions.progression(...)` →
      `progress().updateProgress(id:coordinate:progression:position:)`.
      Session start/end, backgrounding, `endSession` flow unchanged.
    - **Taps & links:** replace `didTapAt` with the canvas's tap
      recognizer: `session.hitTest(spineIdx:pageIdx:x:y:) -> HitResult {
      coordinate, linkTarget }` (point in page coordinates). `linkTarget`
      internal → shared href rule → `locate` → `surface.display` (footnote/
      internal links); `http(s)` scheme → the existing external-URL
      presentation; no link → the existing edge-tap zones / chrome toggle
      (`edgeTapZone(for:)` and `setChrome` untouched). `AnchorNotFound` →
      the existing `showLinkNotFollowed()` toast.
    - **TOC & bookmarks:** `jump(to chapter:)` = shared href rule →
      `locate` → `display`. `placeBookmark()` uses the anchor coordinate;
      bookmark rows jump via `locate(bookmark.coordinate)`; the contents
      sheet's "p. N" labels switch to `ReaderPositions` (chapter rows:
      `startPosition` of the row).
    - **Theme & typography:** theme changes (`presentThemeSheet` /
      customize panel) repaint only: `canvas.theme = newTheme` — never a
      relayout. Typography changes (font/bold/size/spacing/margins from
      `ReaderCustomizeViewController`) persist via the settings facade,
      then: capture anchor coordinate → `try await
      session.updateLayout(viewport:settings:)` (async; bumps generation,
      relays current chapter first, listener re-fires) →
      `surface.layoutInvalidated(generation:)` → on the current chapter's
      `chapterBecameReady`, `locate(anchor)` → `display` (content
      coordinates survive `update_layout` — overview §property). Rotation
      (`viewWillTransition`) is the same flow with the new viewport.
      `ReadingFont` prunes to the two bundled faces: cases `notoSerif
      ("noto-serif")`, `notoSans ("noto-sans")`; a static
      `normalize(_ stored: String) -> ReadingFont` maps legacy values
      (`system-sans` → `.notoSans`; `publisher`/`system-serif`/unknown →
      `.notoSerif`) — the customize panel now offers exactly two options
      (bundled fonts only in v1; Android Task 5.2 mirrors). Delete the CSS
      `fontFamilyStack` property.
    - **Page info & a11y announcements:** `pageInfoText` = "position N of
      M" from the anchor coordinate via `ReaderPositions` (unchanged
      user-facing currency, spec A7); `announcePageAfterTurn` posts the same
      phrase. `ReaderViewController.accessibilityScroll` keeps routing
      three-finger swipes to `pager.turnForward()`/`turnBackward()` —
      `UIAccessibility.post(notification: .pageScrolled, …)` after a turn.
    - **In-book search (minimal here):** keep the panel working by mapping
      a hit (`BookSearchHit` with `spineIdx`, `charOffset`) to
      `locate(Coordinate(spineIdx:charOffset:))` → `display`; highlight
      rects land in Task 6.1. `position(of:)` switches to
      `ReaderPositions`.
    - **Instrumentation (consumed by Task 6.4):** `Logger(subsystem:
      "app.inkuna.ios", category: "perf")`: log
      `open_to_first_page_ready_ms` (from just before `openReader` to the
      first `on_first_page_ready`) and `tap_to_first_page_ms` (from a
      static timestamp stamped in `ReaderLauncher.push` to the first
      `draw(_:)` completion of a presented current page).
    - **Failure states:** `open_reader` throwing anything except
      `UnsupportedContent` → the existing failure/retry state.
      `UnsupportedContent` → same screen but the Task 6.2 fixed-layout
      string once it exists; until then the generic failure text.
  - **Error handling:** every sync geometry call wraps `NotReady` (loading
    state or silent skip as specified per call above); a mid-read chapter
    that stops being cached (LRU) re-schedules via `chapter()` and shows
    the loading treatment only if the *current* page cannot render — never
    an assert.
  - **Verify:** iOS build command → succeeds. On a simulator (install +
    `xcrun simctl launch booted app.inkuna.ios -inkuna.debugScreen reader`):
    a book opens on the engine (no WebView in the hierarchy), restores its
    position, swipes pages with the same physics, crosses chapter
    boundaries with fast-swipe chains (open a multi-chapter book, flick
    rapidly across a boundary ≥5×: every landing shows a rendered page —
    the e002a8f/0c899c1 behavior, now structurally guaranteed), theme
    switch repaints without a relayout flash, text-size change relayouts
    and re-anchors to the same text, rotation preserves position, TOC and
    bookmark jumps land, tap on a footnote link follows it, page-info
    footer shows "N of M". With VoiceOver (simulator Accessibility
    Inspector): blocks read in order with correct frames; links expose the
    link trait. `xcrun simctl io booted screenshot` before/after a theme
    switch for the record.

- [ ] **Task 2.4 — Non-reader Readium sweep (iOS)**
  - **Files:** Modify `apps/ios/Inkuna/Home/TonightViewController.swift`,
    `apps/ios/Inkuna/Detail/BookDetailViewController.swift`.
  - **Behavior:** Remove the `import ReadiumShared` lines and any remaining
    `Locator`/`AnyURL` plumbing left after Task 1.1 (both files' progress
    display already runs on `publication.coordinate` + `ReaderPositions`).
    Update the header comments that explain the import ("the Readium import
    is for Locator…") to describe the coordinate model. After this task the
    only files matching `grep -rl Readium apps/ios/Inkuna` are the four
    death-row files Task 3.2 deletes.
  - **Verify:** iOS build command → succeeds; `grep -rln readium
    apps/ios/Inkuna --include='*.swift' -i` lists exactly
    `Reader/ReadiumPagerSurface.swift`, `Reader/ReaderStyleSurface.swift`,
    `Reader/ReaderUserStyle.swift`, `Reader/ReadingFontDeclarations.swift`
    (+ `Model/ChapterHref.swift` if its comment survives). Tonight and
    Detail still show correct progress on the simulator.

## Movement 3: iOS selection + Readium removal

Native selection over core geometry completes the iOS v1 gate surface; then
Readium leaves the project file entirely.
**Depends on:** Movement 2.

- [ ] **Task 3.1 — iOS selection: long-press, handles, edit menu**
  - **Files:** Create
    `apps/ios/Inkuna/Reader/Engine/ReaderSelectionController.swift`,
    `apps/ios/Inkuna/Reader/Engine/SelectionOverlayView.swift` · Modify
    `apps/ios/Inkuna/Reader/ReaderViewController.swift` (wire-up),
    `apps/ios/Inkuna/Reader/Engine/EnginePagerSurface.swift`
    (`selectionActive` already exists — wire only),
    `apps/ios/Inkuna/Localizable.xcstrings` (keys below).
  - **Behavior:** Core owns geometry; the shell owns UI (spec §7).
    `@MainActor final class ReaderSelectionController: NSObject,
    UIEditMenuInteractionDelegate { init(session: ReaderSession, canvas:
    EnginePageCanvas, surface: EnginePagerSurface, presenter:
    UIViewController); func clear(); var isActive: Bool }`.
    - **Seed:** `UILongPressGestureRecognizer` (0.35 s) on the canvas →
      `hitTest(spine, page, x, y).coordinate` →
      `session.wordAt(coordinate) -> CharRange { start, end }` (end
      exclusive) → active selection `{ spineIdx, range }` + light haptic.
    - **Overlay:** `SelectionOverlayView` (a passthrough view above the
      current `PageView`) fills
      `session.selectionRects(spineIdx:range:) -> [SelectionRect { rect,
      writingMode }]` with the shared highlight palette (accent hex per
      theme side, 0.30 alpha) and draws two handles: 12-pt knob + 2-pt
      stem in the accent color, oriented by `writingMode` —
      `HorizontalTb`: stems vertical, start-handle knob above the first
      rect's leading-top, end-handle knob below the last rect's
      trailing-bottom (the iOS idiom); `VerticalRl`: stems horizontal,
      handles at the first rect's top-right and last rect's bottom-left.
      Vertical text therefore selects vertically with no shell-side
      special cases beyond handle orientation.
    - **Drag:** pan on a handle → `hitTest` at the touch point → new
      boundary coordinate; the opposite end stays anchored; swap ends when
      the drag crosses the anchor; re-query `selectionRects` per move
      (sync + cache-only — cheap on the UI thread by contract). Selection
      is bounded to the visible page in v1: clamp to the page's rects;
      dragging past the edge does not auto-turn (spec A4, documented).
    - **Menu:** `UIEditMenuInteraction` on the canvas, presented at the
      selection's bounding rect on seed and on drag end. Actions: system
      Copy via the responder chain (`copy(_:)` on the canvas sets
      `UIPasteboard.general.string = session.textRange(spineIdx:range:)` —
      system-localized title for free), plus custom actions
      `reader_look_up` → `UIReferenceLibraryViewController(term:)`
      presented by the reader, and `reader_share_selection` →
      `UIActivityViewController` with the text.
    - **Lifecycle:** `isActive` mirrors into `surface.selectionActive`
      (the pager then leaves horizontal drags to the handles — its
      existing `hasActiveSelection` arbitration, unchanged). Clear on:
      tap outside the selection, page turn commit, jump, `update_layout`,
      chapter change, reader dismiss.
    - **Strings:** add to `Localizable.xcstrings`: `reader_look_up`
      ("Look Up"), `reader_share_selection` ("Share") with translations
      for all 14 languages (English placeholder acceptable; note it in the
      commit message).
  - **Error handling:** `wordAt` on whitespace returning an empty range →
    no selection, no menu. `NotReady` from any geometry call mid-drag →
    keep the last rects (never flicker); `textRange` failure → menu action
    no-ops with a warning log.
  - **Verify:** iOS build command → succeeds. Simulator: long-press a word
    → word highlights with handles + menu; drag handles across lines →
    highlight follows exactly; Copy puts the text on the pasteboard
    (paste into the search field to prove); Look Up presents the
    dictionary; Share presents the sheet; horizontal drag on a handle does
    NOT turn the page; tap elsewhere clears; open a vertical-CJK book
    (fixture from the parity corpus) → selection rects are vertical and
    handles sit horizontal-stem.

- [ ] **Task 3.2 — Delete Readium from iOS**
  - **Files:** Delete `apps/ios/Inkuna/Reader/ReadiumPagerSurface.swift`,
    `apps/ios/Inkuna/Reader/ReaderStyleSurface.swift`,
    `apps/ios/Inkuna/Reader/ReaderUserStyle.swift`,
    `apps/ios/Inkuna/Reader/ReadingFontDeclarations.swift`,
    `apps/ios/Inkuna/Model/ChapterHref.swift` · Modify
    `apps/ios/project.yml` (remove the `packages:` block's `Readium` entry
    and the target's `- package: Readium` dependency with its three
    products ReadiumShared/ReadiumStreamer/ReadiumNavigator), plus any
    straggler references the build then flags.
  - **Behavior:** The §9 typography constants in `ReaderUserStyle.swift`
    were transcribed into the core by plan 01 — deletion is safe by
    contract; do not re-transcribe. `ChapterHref` callers were rewired to
    `locate_href` in Movement 2 (the split-at-`#` rule is the only
    shell-side remnant and lives at the call sites). With the shim gone,
    confirm `ReaderViewController` no longer references
    `ReaderAccessibilityScrolling`/`ReadiumNavigatorShim`.
  - **Verify:** `cd apps/ios && xcodegen generate && xcodebuild …` →
    succeeds with zero SPM checkout of Readium (build log has no
    `readium` lines). `grep -rin readium apps/ios --include='*.swift'
    --include='*.yml'` → no hits (gitignored `apps/ios/build/` and
    `apps/ios/.derivedData/` excluded per the overview). Full simulator
    smoke: open, read, cross chapters, select, search-jump, TOC jump.

## Movement 4: Android surface extraction + PageView

Android gains the seam iOS already has: `ReaderPagerLayout` learns to speak
`ReaderPagerSurface` (with a temporary Readium-backed implementation so the
app keeps reading), and the engine rendering stack lands (`PageView`, fonts,
a11y, canvas, engine surface) ready for Movement 5's switchover.
**Depends on:** Movement 1 (Movements 2–3 are iOS-only; 4 can run in
parallel with them if the conductor chooses, but after 2.1 so the protocol
shape being mirrored is final).

- [ ] **Task 4.1 — Extract `ReaderPagerSurface` (Kotlin) + refactor `ReaderPagerLayout`**
  - **Files:** Create
    `app/src/main/java/app/inkuna/android/ui/reader/ReaderPagerSurface.kt`,
    `app/src/main/java/app/inkuna/android/ui/reader/ReadiumPagerSurface.kt`
    (temporary — deleted in Task 5.4) · Modify
    `app/src/main/java/app/inkuna/android/ui/reader/ReaderPagerLayout.kt`,
    `app/src/main/java/app/inkuna/android/ui/reader/ReaderScreen.kt`
    (construction site only).
  - **Behavior:** Mirror of iOS Task 2.1, member-for-member (the mirroring
    convention: change one, change its sibling):
    ```kotlin
    interface ReaderPagerSurface {
        val isEngageable: Boolean
        val isBusy: Boolean
        val hasActiveSelection: Boolean
        val isRightToLeft: Boolean
        fun innerMetrics(): ReaderPagerStrip?
        fun setInnerOffset(x: Float)
        fun outerMetrics(): ReaderPagerStrip?
        fun setOuterOffset(x: Float)
        fun neighborIsReady(toRight: Boolean): Boolean
        fun commitBoundaryCrossing(toRight: Boolean): Boolean
    }
    data class ReaderPagerStrip(
        val offset: Float,
        val range: ClosedFloatingPointRange<Float>,
        val pageWidth: Float,
    )
    ```
    `ReaderPagerLayout` keeps every gesture/physics/settle member
    (`onInterceptTouchEvent`, `onTouchEvent`, `considerClaim`,
    `freezeSettle`/`claimFrozen`/`resumeFrozen`, `rubberBand`,
    `settlePager`/`settleInner`/`settleRubber`, `SettleSpring` use,
    `turnLogical`/`turnGeometric`, velocity feeding) and gains
    `var surface: ReaderPagerSurface?` + `fun bind(surface:
    ReaderPagerSurface)`. Everything Readium-shaped moves into
    `ReadiumPagerSurface(navigator: EpubNavigatorFragment, hostView: View)`
    behind the interface: WebView walking (`webViewsIn`/`visibleWebView`/
    `neighbourWebView`), `seedInnerMax`, `prepareNeighbour`/
    `prePositionNeighbour` (folded into its `neighborIsReady`, which
    prepares then reports), ViewPager discovery + fake-drag
    (`pagerIn`/`rearmFakeDrag`/`closeFakeDrag` — folded into its
    `commitBoundaryCrossing`, best-effort synchronous), `drivePager`'s
    WebView `scrollTo` (its `setInnerOffset`), `childTranslationX` driving
    (its `setOuterOffset`, via the host view). The strip mapping follows
    the iOS surface semantics: inner strip = pages inside the current
    resource; outer strip = the resource strip. `ReaderScreen` constructs
    `ReadiumPagerSurface` at the existing navigator-mount site and
    `bind`s it. Rescue behavior embedded in the moved code survives inside
    the temporary class only (it dies whole in Task 5.4) — do not port any
    of it into `ReaderPagerLayout` itself; the layout's own
    rescue/verification branches around boundary commits are deleted now,
    matching iOS Task 2.1 (synchronous commit, no verify pass).
  - **Error handling:** the temporary surface returns nil metrics whenever
    its WebView walk fails, which the pager already treats as
    disengagement — behavior identical to today's internal nil paths.
  - **Verify:** `./gradlew assembleDebug` → succeeds; emulator: the
    Readium-era reader still opens (interim path), swipes pages, crosses
    chapter boundaries with fast-swipe chains, rubber-bands at book ends —
    feel unchanged.

- [ ] **Task 4.2 — Android `ReaderFontStore` + `PageView` drawing**
  - **Files:** Create
    `app/src/main/java/app/inkuna/android/ui/reader/engine/ReaderFontStore.kt`,
    `app/src/main/java/app/inkuna/android/ui/reader/engine/PageView.kt`,
    `app/src/main/java/app/inkuna/android/ui/reader/engine/PageImageLoader.kt`.
  - **Behavior:**
    - `ReaderFontStore`: `object ReaderFontStore { fun prime(registry:
      List<FontEntry>); fun font(id: UInt): android.graphics.fonts.Font? }`
      — `Canvas.drawGlyphs` takes `android.graphics.fonts.Font`, not
      `Typeface`. Build per `FontEntry { id, filePath, collectionIndex,
      axes }`: `Font.Builder(File(filePath))
      .setTtcIndex(collectionIndex.toInt())` + non-empty axes via
      `setFontVariationSettings(axes.joinToString { "'${it.tag}' ${it.value}" })`
      → `build()`. Cached by `id` (Font carries no size; size lives on the
      Paint). Thread-safe via an immutable map swapped at `prime`.
    - `PageView(context) : View` — renders one page. API: `fun present(
      list: PageDisplayList?, spineIdx: UInt, pageIdx: UInt, session:
      ReaderSession)`, `var palette: PagePalette` (data class `PagePalette(
      val background: Int, val text: Int, val secondary: Int, val link:
      Int)` built from `ReadingTheme` + the shared accent hex — same role
      mapping as iOS Task 1.4), `var imageLoader: PageImageLoader?`.
      `onDraw`: `canvas.scale(density, density)` once (geometry arrives in
      layout points = dp at 1×; density from `resources.displayMetrics`),
      fill background; per `GlyphRun { fontId, size, colorRole, glyphIds,
      positions, orientation }`: reuse one `Paint` (`isAntiAlias`,
      `textSize = size.toFloat()`, color by role),
      `canvas.drawGlyphs(glyphIds.toIntArray(), 0, positionsFloatArray, 0,
      glyphCount, font, paint)` — positions are x,y interleaved pen
      positions with y = baseline, top-left origin: exactly `drawGlyphs`'
      contract, no flip needed on Android. `SidewaysRotated`: save,
      translate to the run's first pen position, rotate 90° so the run
      reads top-to-bottom on screen, draw rebased, restore. Convert
      `glyphIds`/`positions` to primitive arrays once at `present`, not
      per frame. Decorations and image placeholders exactly as iOS
      Task 1.4 (theme color from each decoration's core-assigned
      `colorRole`; the shell never infers decoration colors).
    - `PageImageLoader(session, scope: CoroutineScope)`: `fun image(href:
      String, onReady: () -> Unit): Bitmap?` — LruCache (32 MB, byte-count
      sized), one in-flight coroutine per href calling the async
      `session.resource(href)` then `BitmapFactory.decodeByteArray` on
      `Dispatchers.Default`, `onReady` on Main; permanent-miss marker on
      failure (mirrors `PageImageProvider`).
  - **Error handling:** nil font → skip run; decode/resource failure →
    placeholder persists, no retry loop; a `present` for a stale
    generation is discarded by the canvas (Task 4.4).
  - **Verify:** `./gradlew assembleDebug` → succeeds (first on-screen use
    in Movement 5; compile + Movement 5 visual checks cover it).

- [ ] **Task 4.3 — Android `PageView` accessibility**
  - **Files:** Create
    `app/src/main/java/app/inkuna/android/ui/reader/engine/PageAccessibilityHelper.kt`
    · Modify `engine/PageView.kt`,
    `apps/android/app/build.gradle.kts` (add
    `implementation("androidx.customview:customview:<latest stable —
    query Google Maven at implementation time, ≥1.2.0>")`).
  - **Behavior:** `PageAccessibilityHelper(view: PageView) :
    ExploreByTouchHelper(view)` exposing
    `session.accessibilityBlocks(spineIdx, pageIdx)` as virtual views —
    one per `A11yBlock { text, rect, lang, isLink, role }`, ids = array
    index (logical reading order → TalkBack order).
    `onPopulateNodeForVirtualView`: text = `SpannableString(text)` wrapped
    in `LocaleSpan(Locale.forLanguageTag(lang))` when `lang` present;
    `setBoundsInParent` = rect scaled by density; `isHeading = role ==
    Heading`; links: `isClickable = true` + `ACTION_CLICK` routed to the
    reader's link handler (the canvas's tap path with the block's rect
    center). `getVirtualViewAt(x, y)` maps by rect hit. PageView installs
    the helper via `ViewCompat.setAccessibilityDelegate` and forwards
    `dispatchHoverEvent`; blocks refresh on every `present`
    (`invalidateRoot()`); `NotReady` → zero virtual views until the next
    `present`.
  - **Error handling:** malformed `lang` tags fall back to no LocaleSpan.
  - **Verify:** `./gradlew assembleDebug` → succeeds; functional TalkBack
    check in Task 5.2's verify.

- [ ] **Task 4.4 — `EnginePageCanvas` + `EnginePagerSurface` (Android)**
  - **Files:** Create
    `app/src/main/java/app/inkuna/android/ui/reader/engine/EnginePageCanvas.kt`,
    `app/src/main/java/app/inkuna/android/ui/reader/engine/EnginePagerSurface.kt`.
  - **Behavior:** Member-for-member mirror of iOS Task 2.2 (same scene
    model, same slot formula `slot = if (rtl) pageCount−1−pageIdx else
    pageIdx`, same 3-slot synthetic outer strip, same entry-page rule on
    commit, same generation-discard rule, same pool of 6 recycled
    `PageView`s — positioned via `translationX`, sized to the canvas).
    `EnginePageCanvas(context) : FrameLayout` with `fun setScene(scene:
    PageScene)`, `var palette: PagePalette`, `fun invalidate(generation:
    ULong)`; `EnginePagerSurface(session: ReaderSession, canvas:
    EnginePageCanvas) : ReaderPagerSurface` with the engine-side members
    `spineIdx`/`pageIdx`/`spineCount`, `onPageSettled: ((UInt, UInt) ->
    Unit)?`, `display(spineIdx, pageIdx)`, `chapterBecameReady(generation,
    spineIdx)`, `layoutInvalidated(generation)`, `var selectionActive:
    Boolean`. All calls happen on the main thread (the ViewModel hops
    listener callbacks before touching the surface). Sync session calls
    catch `NotReadyException` and degrade to nil metrics / blank slot,
    exactly as iOS.
  - **Error handling:** as iOS Task 2.2.
  - **Verify:** `./gradlew assembleDebug` → succeeds (goes live in 5.2).

## Movement 5: Android open path + selection + Readium removal

The Android reader switches onto the engine, gains native selection, and
Readium leaves the build entirely — dependencies, forced constraints, and
every workaround.
**Depends on:** Movement 4.

- [ ] **Task 5.1 — `ReaderViewModel` rewrite**
  - **Files:** Modify
    `app/src/main/java/app/inkuna/android/ui/reader/ReaderViewModel.kt`.
  - **Behavior:** Delete the Readium open (`AssetRetriever`,
    `PublicationOpener`, `fixFragmentation`, `ByteArray.lastIndexOf`,
    `TransformingContainer`, locator machinery) and the Task 1.2 interim
    bridge. New shape:
    - `sealed interface UiState { Opening; Failed;
      FixedLayoutUnsupported; Ready(book: ReaderBook) }` where
      `class ReaderBook(val session: ReaderSession, val publication:
      Publication, val chapters: List<Chapter>, val positionRanges:
      List<ChapterPositionRange>, val spineCount: Int, val
      initialLocation: PageLocation?)`.
    - `doOpen()`: `LibraryStore.bookshelf(app)` → `publication` +
      `chapters` + `chapterPositionRanges` (spineCount = ranges.size) →
      `bookshelf.openReader(publicationId, viewport, settings, listener)`
      (suspend). Viewport = the window's current bounds in dp minus
      `ReaderMetrics` top/bottom insets (computed from
      `WindowManager.currentWindowMetrics` + `WindowInsets` before
      composition; `ReaderLayoutSettings` mapped 1:1 from AppSettings with
      `ReadingFont.normalize` — Task 5.2). Listener = an object whose
      `onFirstPageReady(generation, spineIdx)` / `onChapterReady(generation,
      spineIdx, pageCount)` post into `viewModelScope.launch(Dispatchers.
      Main.immediate)` and feed a `MutableSharedFlow<LayoutEvent>` the
      screen collects (`sealed interface LayoutEvent { data class
      FirstPage(generation, spineIdx); data class Chapter(generation,
      spineIdx, pageCount) }`) — callbacks may arrive on any thread
      (overview contract). Initial location: `initialChapterHref` (nav
      arg, unchanged plumbing) through the shared href rule
      (split-at-`#` → `session.locateHref(resource, fragment)`), else
      `publication.coordinate ?? Coordinate(0u, 0uL)`; resolved to
      `PageLocation` lazily by the screen once the chapter is ready.
    - Progress: `fun onPageSettled(spineIdx: UInt, pageIdx: UInt)` —
      anchor coordinate via the shared probe (`hitTest` at the
      reading-start corner), `ReaderPositions.position/progression`,
      debounced `progress().updateProgress(id, coordinate, progression,
      position)` through the existing `enqueueCoreWrite`; sitting
      start/end (`onReaderVisible`/`onReaderHidden`/`endSession`)
      unchanged; `onCleared` closes nothing core-side (sessions close
      with last-open-wins / bookshelf drop — overview contract) but
      cancels in-flight shell work.
    - Search: `search(query)` keeps `search().searchInBook`; a hit maps to
      `Coordinate(hit.spineIdx, hit.charOffset)`; `searchLocator` and
      `positionOf` die — "p. N" via `ReaderPositions`. Match length for
      highlights = `hit.snippetMatch` character count (Task 6.1 consumes).
    - Bookmarks: `addBookmark(coordinate, progression)` from the current
      anchor; jump via `locate(bookmark.coordinate)`.
    - `updateAppearance(settings)`: capture anchor → suspend
      `session.updateLayout(viewport, settings)` → emit
      `LayoutEvent`-driven re-anchor (screen calls `locate(anchor)` →
      `display`) — mirrors iOS Task 2.3's flow.
    - Instrumentation: `Log.i("InkunaPerf", "open_to_first_page_ready_ms=…")`
      and `tap_to_first_page_ms` (tap timestamp stamped in
      `InkunaApp.openReader` via a shared `object ReaderPerf { var
      tapUptimeMs: Long }`, logged at the first canvas draw of a current
      page).
    - Failure: `UnsupportedContent` → `UiState.FixedLayoutUnsupported`;
      everything else → `UiState.Failed` (existing retry UI).
  - **Error handling:** as listed; `locateHref` `AnchorNotFound` on the nav
    arg falls back to the stored coordinate (never a failed open).
  - **Verify:** `./gradlew assembleDebug` → succeeds (screen still on the
    old path until 5.2 — this task may land together with 5.2 in one
    commit if the interim doesn't compile standalone; the implementer may
    combine their commits but keeps both task scopes).

- [ ] **Task 5.2 — `ReaderScreen` rewrite + appearance rewire**
  - **Files:** Modify
    `app/src/main/java/app/inkuna/android/ui/reader/ReaderScreen.kt`,
    `ui/reader/ReaderAppearance.kt`, `ui/reader/ReaderSheets.kt`,
    `ui/theme/ReadingFont.kt`.
  - **Behavior:**
    - **Delete the `transitionSettled` latch** (commit `0c899c1`'s
      Chromium-spawn mount delay — there is no process spawn to hide;
      overview workaround inventory): `UiState.Ready` content mounts
      immediately; the enter slide runs over the already-rendering first
      page (progressive readiness makes this the fast path).
    - `ReaderContent` hosts via `AndroidView`: one `ReaderPagerLayout`
      whose single child is the `EnginePageCanvas`; construct
      `EnginePagerSurface(session, canvas)` and `bind` it;
      `surface.spineCount = book.spineCount`. Collect the ViewModel's
      `LayoutEvent` flow → `surface.chapterBecameReady` / initial
      `display` at the resolved `PageLocation` (first frame shows as soon
      as `FirstPage` for the target chapter arrives). `onPageSettled` →
      `viewModel.onPageSettled` + chrome auto-hide (existing behavior).
      Tap handling moves off Readium's `onTap`: the canvas exposes a tap
      listener delivering page-space points → `hitTest` → link follow
      (shared href rule / external `Intent.ACTION_VIEW`) or edge-tap
      zones / chrome toggle — the existing zone math survives. Theme
      changes set `canvas.palette` only (repaint, no relayout);
      typography changes call `viewModel.updateAppearance` (re-anchor
      flow). `readerPercent`/`readerPageInfo` switch to
      `ReaderPositions` ("position N of M", spec A7). Predictive back and
      chrome/sheet composables unchanged.
    - `ReaderAppearance.kt`: delete the JS re-anchor block
      (`readium.findFirstVisibleLocator` / `scrollToLocator`) — the
      re-anchor is now the coordinate capture + `locate` flow in the
      ViewModel; the file keeps only the settings→`ReaderLayoutSettings`
      mapping and the appearance-state plumbing.
    - `ReadingFont.kt`: prune to `NOTO_SERIF("noto-serif")` /
      `NOTO_SANS("noto-sans")` + `fun normalize(stored: String):
      ReadingFont` (`system-sans` → NOTO_SANS; else NOTO_SERIF), mirroring
      iOS Task 2.3; the appearance sheet offers exactly two faces.
    - `ReaderSheets.kt` (contents/bookmarks): chapter rows jump via the
      shared href rule → `locate` → `surface.display`; bookmark rows via
      `locate(bookmark.coordinate)`; "p. N" labels via `ReaderPositions`.
  - **Error handling:** `AnchorNotFound` → the existing
    `ReaderToast.LinkNotFollowed`; `NotReady` on a jump target → schedule
    via `chapter(spineIdx)` and complete on that chapter's `LayoutEvent`
    (single pending jump, newest wins).
  - **Verify:** `./gradlew installDebug`; emulator: book opens on the
    engine (no WebView — `adb shell dumpsys activity top | grep -c
    WebView` → 0), restores position, pager feel unchanged, fast-swipe
    chapter chains land rendered pages every time (≥5 rapid flicks across
    a boundary), theme repaint without relayout, text-size re-anchor,
    TOC/bookmark jumps, link taps, page info "N of M", rotation
    preserves position. TalkBack on: blocks navigate in order with
    correct bounds; a link block activates. `adb exec-out screencap -p >
    /tmp/reader.png` before/after theme switch for the record.

- [ ] **Task 5.3 — Android selection (ActionMode)**
  - **Files:** Create
    `app/src/main/java/app/inkuna/android/ui/reader/engine/ReaderSelectionController.kt`
    · Modify `engine/EnginePageCanvas.kt` (overlay hosting + long-press),
    `ui/reader/SelectionModeTracker.kt` (wire, not rewrite),
    `res/values/strings.xml` + all 13 locale siblings (keys below).
  - **Behavior:** Mirror of iOS Task 3.1 over the same core geometry.
    Long-press (GestureDetector on the canvas) → `hitTest` → `wordAt` →
    `CharRange` + haptic. Overlay: a child view of the canvas drawing
    `selectionRects(spineIdx, range)` in the shared highlight palette
    (accent hex per theme side @ 0.30 alpha) + two handle drawables
    (Android teardrop idiom, accent color), orientation by
    `SelectionRect.writingMode` as on iOS. Handle drag → `hitTest` →
    boundary update, anchor-swap, visible-page-bounded (spec A4).
    Floating `ActionMode` (`startActionMode(callback,
    ActionMode.TYPE_FLOATING)` on the canvas; `onGetContentRect` = the
    selection's bounding rect): menu items — Copy (`android.R.string.copy`,
    `ClipboardManager` with `session.textRange(spineIdx, range)`), Share
    (`reader_share_selection`, `Intent.ACTION_SEND` chooser), Web Search
    (`reader_web_search`, `Intent.ACTION_WEB_SEARCH` with the text —
    Android's look-up idiom). Selection state sets
    `surface.selectionActive` AND `SelectionModeTracker` (the pager's
    existing arbitration input — survives, rewired from Readium's
    selection callbacks to this controller). Clear on tap-outside, page
    turn, jump, `update_layout`, back.
    Strings: `reader_share_selection` ("Share"),
    `reader_web_search` ("Search the web") in `values/strings.xml` and
    every `values-<locale>/strings.xml` (de, es, fr, id, it, ja, ko, pt,
    ru, th, vi, zh-rCN, zh-rTW — English placeholder acceptable, note in
    commit).
  - **Error handling:** as iOS Task 3.1 (empty word range → no-op; stale
    geometry mid-drag → keep last rects).
  - **Verify:** `./gradlew installDebug`; emulator: long-press selects a
    word with handles + floating toolbar; drag extends; Copy lands on the
    clipboard (`adb shell dumpsys clipboard | grep -A2 text` or paste into
    search); Share opens the chooser; handle drags never turn pages;
    vertical-CJK book selects vertically.

- [ ] **Task 5.4 — Delete Readium from Android (code + build constraints)**
  - **Files:** Delete `ui/reader/ReaderNavigatorHost.kt`,
    `ui/reader/ReaderStyleInjector.kt`, `ui/reader/ReaderUserCss.kt`,
    `ui/reader/ReaderWebViewTuner.kt`, `ui/reader/ReaderPageTurnListener.kt`,
    `ui/reader/ReadiumPagerSurface.kt` (the Task 4.1 temporary) · Modify
    `apps/android/app/build.gradle.kts`,
    `app/src/main/java/app/inkuna/android/MainActivity.kt`,
    `app/src/main/java/app/inkuna/android/importing/ImportIntentActivity.kt`
    (comment), `ui/tonight/TonightViewModel.kt`,
    `ui/detail/BookDetailViewModel.kt`, `ui/reader/SettleSpring.kt` +
    `ui/reader/SelectionModeTracker.kt` (comments only).
  - **Behavior:** `ReaderUserCss.kt`'s §9 constants are already in the core
    (plan 01) — delete without re-transcribing. `MainActivity` drops
    `FragmentActivity` for `androidx.activity.ComponentActivity` (the
    fragment host existed only for Readium's navigator fragment); remove
    the `EpubNavigatorFragment` factory registration in `onCreate`.
    `ImportIntentActivity`'s comment explaining the FragmentActivity split
    is rewritten (the split itself stays — it exists for import isolation
    too). Remove stray Readium imports/comments from Tonight/Detail
    ViewModels and the physics files. `build.gradle.kts`, each removal
    preceded by its no-other-consumer check (run the grep, record it in
    the commit message):
    - the three `org.readium.kotlin-toolkit:*:3.3.0` artifacts
      (`grep -rn "readium" app/src/main/java` → no hits first);
    - `androidx.webkit` strict pin (`grep -rn "androidx.webkit\|WebViewAssetLoader\|WebSettingsCompat" app/src/main/java` → no hits — the CORS-bug pin comment dies with it);
    - `coreLibraryDesugaring` flag + `desugar_jdk_libs` (`grep -rn "java.time\|desugar" app/src/main/java` — the app targets minSdk 33; the flag existed for Readium AARs only);
    - `androidx.viewpager:viewpager` (`grep -rn "ViewPager\|viewpager" app/src/main/java` → no hits — fake-drag glue died in this movement);
    - `androidx.fragment:fragment-ktx` (`grep -rn "Fragment\|fragment" app/src/main/java` → no non-comment hits after the MainActivity change; transitive fragment via activity/navigation stays — only the direct dependency goes).
    Also prune `proguard-rules.pro` of Readium-specific keep rules if any
    (`grep -in readium app/proguard-rules.pro`), keeping JNA/UniFFI rules
    intact.
  - **Verify:** `./gradlew assembleDebug` and `./gradlew assembleRelease`
    → both succeed (release proves R8 still happy). `grep -rin readium
    apps/android --include='*.kt' --include='*.kts' --include='*.pro'
    --include='*.xml'` → no hits. Emulator full smoke: open, read, cross
    chapters, select, TOC jump, settings sheet, Tonight "Keep Reading".
    APK sanity: `unzip -l app-debug.apk | grep -ci readium` → 0.

## Movement 6: Cross-shell wiring + parity gate

Search-highlight geometry, the fixed-layout state, the digest harness, the
performance numbers, and the full spec-§14 checklist — the evidence that
lets `dev/core` merge.
**Depends on:** Movements 3 and 5.

- [ ] **Task 6.1 — Search jump + highlight rects (both shells)**
  - **Files:** Modify `apps/ios/Inkuna/Reader/ReaderViewController.swift`,
    `apps/ios/Inkuna/Reader/ReaderSearchPanel.swift`,
    `apps/ios/Inkuna/Reader/Engine/EnginePageCanvas.swift` (highlight
    overlay hook) · `app/src/main/java/app/inkuna/android/ui/reader/ReaderSearchPanel.kt`,
    `ui/reader/ReaderScreen.kt`, `engine/EnginePageCanvas.kt`.
  - **Behavior:** A chosen hit (`BookSearchHit { spineIdx, charOffset,
    snippetMatch, … }` — offsets are content coordinates with no
    conversion step, spec §10) jumps via
    `locate(Coordinate(spineIdx, charOffset))` → `surface.display`, then
    paints `matchRects(spineIdx, charOffset, len)` (len =
    `snippetMatch` Unicode scalar count) as an overlay on the landed page:
    shared highlight palette @ 0.35 alpha, 3-pt corner radius, held 1.5 s
    then faded over 600 ms (both shells, same numbers). If the hit's
    chapter is `NotReady`, `chapter(spineIdx)` schedules it and the jump
    completes on that chapter's ready event (single pending jump). "p. N"
    labels in both panels already run on `ReaderPositions` (Movements
    2/5); confirm and keep. Panel UI, debounce, and keyboard behavior
    untouched.
  - **Error handling:** `matchRects` returning empty (match fell across a
    truncated resource boundary) → jump lands without a highlight, no
    error surfaced.
  - **Verify:** both build commands → succeed. Both platforms: search a
    word with multiple hits including a CJK query in a CJK book; tapping a
    hit lands on the right page with the match visibly highlighted, fading
    out; "p. N" matches the landed page's position label.

- [ ] **Task 6.2 — Degradation states: fixed-layout, unreadable chapter,
  truncated chapter (both shells, 14 locales)**
  - **Files:** Modify `apps/ios/Inkuna/Localizable.xcstrings`,
    `apps/ios/Inkuna/Reader/ReaderViewController.swift`,
    `apps/ios/Inkuna/Reader/Engine/EnginePageCanvas.swift` ·
    `apps/android/app/src/main/res/values/strings.xml` + the 13
    `values-<locale>/strings.xml` siblings,
    `ui/reader/ReaderScreen.kt`.
  - **Behavior:** Three keys, per the overview's degradation contract:
    `reader_fixed_layout_unsupported` — "This book uses a fixed layout,
    which Inkuna can't display yet." (iOS: the `UnsupportedContent`
    branch of the open-failure state, existing back affordance, no retry
    button — retry cannot succeed; Android:
    `UiState.FixedLayoutUnsupported`, centered text + the ever-present
    back button, no retry). `reader_chapter_unreadable` — "This chapter
    can't be displayed." (shown as a centered placeholder page when
    `chapter()`/`page()` throws `UnsupportedContent` for one spine index;
    the rest of the book stays navigable — the canvas renders the
    placeholder in that chapter's slot, page count 1).
    `reader_chapter_truncated` — "This chapter was too large to display
    completely." (a dismissible one-line notice shown once per chapter
    when `chapter(spine).truncated` is true; the truncated prefix still
    renders normally). All 14 languages in both catalogs (English
    placeholder in non-English entries is acceptable — say so in the
    commit message).
  - **Error handling:** n/a — this IS the error surface.
  - **Verify:** both builds succeed. With a fixed-layout EPUB (take one
    from the parity corpus's archetypes or any `rendition:layout
    pre-paginated` sample; the owner has test files): opening shows the
    localized state and back works, both platforms. Switch device
    language to ja → the key resolves (placeholder English acceptable but
    the lookup must not crash or show the raw key).

- [ ] **Task 6.3 — Parity digest harness (debug hooks + compare script)**
  - **Files:** Create `apps/ios/Inkuna/Debug/ParityDigestRunner.swift`,
    `app/src/main/java/app/inkuna/android/debug/ParityDigestRunner.kt`,
    `scripts/parity-compare.sh` · Modify
    `apps/ios/Inkuna/SceneDelegate.swift` (route),
    `apps/android/app/src/main/java/app/inkuna/android/MainActivity.kt`
    (route).
  - **Behavior:** Debug-only, both hooks byte-identical in behavior. Each
    case (file + viewport + settings) comes from the corpus directory's
    `manifest.json`, produced by plan 01's `export-parity-fixtures`
    example (shell-shared rule above) — the hooks contain no layout
    constants of their own.
    - iOS: `-inkuna.debugScreen parityDigest` (the existing
      `debugRoute()` switch in `SceneDelegate`) runs `ParityDigestRunner`
      (`#if DEBUG`): for each case in
      `Documents/ParityCorpus/manifest.json` (in manifest order) —
      `importer().import(path:)`, `openReader` with the case's
      viewport/settings and a relay that counts `on_chapter_ready` until
      it equals `session.spineCount()`; then for every `spineIdx` and every
      `pageIdx < chapter(spineIdx).pageCount`:
      `session.pageDigest(spineIdx:pageIdx:)` (blake3 hex of the
      canonical display-list serialization — the overview's parity
      method). Output `Documents/parity-ios.json`:
      `{ "<filename>": { "<spineIdx>": ["<digest>", …] } }` (page order),
      then `print("PARITY DONE <n books>")`. Per-book timeout 120 s →
      record `"TIMEOUT"` in place of that book's object and continue.
    - Android: launch extra `--ez inkuna.parityDigest true` in
      `MainActivity.onCreate` (guarded `BuildConfig.DEBUG`) runs the
      Kotlin runner over `getExternalFilesDir(null)/ParityCorpus/`,
      writing `parity-android.json` beside it; identical JSON shape,
      identical semantics, `Log.i("InkunaParity", "PARITY DONE <n>")`.
    - `scripts/parity-compare.sh a.json b.json`: `jq -S .` both, `diff`;
      exit 0 + "PARITY OK (<n> books)" on identical, else print the diff
      and exit 1. Header comment documents the full run recipe:
      iOS — build Debug for simulator, `xcrun simctl install booted …`,
      `CONTAINER=$(xcrun simctl get_app_container booted app.inkuna.ios
      data)`, `mkdir -p "$CONTAINER/Documents/ParityCorpus" && cp
      corpus/*.epub` there, `xcrun simctl launch booted app.inkuna.ios
      -inkuna.debugScreen parityDigest`, wait for PARITY DONE in
      `xcrun simctl spawn booted log stream`, copy
      `parity-ios.json` out of the container;
      Android — `./gradlew installDebug`, `adb push corpus/.
      /sdcard/Android/data/app.inkuna.android/files/ParityCorpus/`,
      `adb shell am start -n app.inkuna.android/.MainActivity --ez
      inkuna.parityDigest true`, wait via `adb logcat -s InkunaParity`,
      `adb pull …/parity-android.json`.
    - **Corpus:** exactly the output of plan 01's `export-parity-fixtures`
      example (fixture EPUBs + `manifest.json`), copied verbatim to both
      devices — it covers the spec-§14 archetypes by construction. The
      gate records the manifest SHA-256 and each file's SHA-256 in the
      evidence file. (The seeded benchmark library's long-chapter book is
      the perf corpus, Task 6.4 — not part of the digest corpus.)
  - **Error handling:** import failure or `open_reader` failure for a
    corpus file records `"ERROR: <message>"` for that book and continues
    — the compare then fails loudly on the string mismatch if only one
    platform errored.
  - **Verify:** both builds succeed. Dry-run each hook with a 2-book mini
    corpus and diff — identical digests expected (this is plan 01's
    fixed-point determinism made visible; a mismatch here is a plan-01
    bug and blocks the gate, not something to patch shell-side).

- [ ] **Task 6.4 — Performance measurement**
  - **Files:** none new (instrumentation landed in Tasks 2.3 and 5.1);
    measurements recorded in the Task 6.5 evidence file.
  - **Behavior / procedure:** Reference devices per the overview: the
    owner's iOS dev device and a Pixel-class emulator (the owner runs
    on-device steps interactively; the implementer prepares builds and
    exact commands). Cold open = app freshly launched, target book not
    yet opened this run, seeded benchmark library imported (must include
    the long-chapter book).
    - **Numbers:** `tap_to_first_page_ms ≤ 250` and
      `open_to_first_page_ready_ms ≤ 100` from the perf log lines, 5
      runs each on: the long-chapter book and a normal book, both
      platforms. iOS: Release configuration build; read the lines via
      Console.app (device) / `xcrun simctl spawn booted log show --last
      5m --predicate 'subsystem == "app.inkuna.ios" AND category ==
      "perf"'` (simulator rehearsal). Android: `assembleRelease`
      (debug-signed locally is fine) — but the perf log lives in release
      too (plain `Log.i`, two lines per open, deliberately kept);
      `adb logcat -d -s InkunaPerf`.
    - **Jank comparison ("Keep Reading" open + first five page turns):**
      Android — `adb shell dumpsys gfxinfo app.inkuna.android reset`,
      perform the flow, `adb shell dumpsys gfxinfo app.inkuna.android`
      → record jank % and 95th percentile frame time; run the same on
      the frozen Readium build (current `main`, same device/emulator) and
      require no-worse. iOS — Instruments "Animation Hitches" over the
      same flow on device, hitch duration vs. the `main` build,
      no-worse required.
    - Median of the 5 runs is the recorded number (cold-open variance is
      real; the gate metric is the median, worst run also recorded).
  - **Error handling:** a miss on any number is a gate failure → file the
    finding against plan 01's engine (layout speed) or this plan's shell
    path (draw/first-present) with the split visible in the two metrics —
    `open_to_first_page_ready` isolates the core, the difference to
    `tap_to_first_page` isolates the shell.
  - **Verify:** the recorded table in the evidence file shows every cell
    within gate numbers; both raw log captures attached (pasted) under it.

- [ ] **Task 6.5 — Parity checklist run, zero-Readium sweep, evidence**
  - **Files:** Create
    `docs/repertoire/plans/2026-08-21-reader-engine-swap/parity-evidence.md`.
  - **Behavior:** Execute spec §14's merge checklist end-to-end and record
    each item with its evidence (command output, screenshot filename, or
    "owner-verified on device <date>"):
    1. Every book in the seeded library opens and restores position
       (post-V8 + reconcile — plan 01's migration ran on this library).
    2. Page/chapter navigation incl. fast-swipe boundary chains, both
       platforms, no rescue layers (≥5 rapid flicks across ≥3 different
       chapter boundaries; every landing rendered).
    3. Vertical-CJK book with ruby end-to-end; RTL progression honored;
       tap/key direction mapping correct (iOS arrow keys + edge taps;
       Android edge taps).
    4. TOC + internal-link/footnote taps via `locate_href`, fragment
       targets included.
    5. In-book and library search with on-page highlight rects (Task 6.1).
    6. Bookmarks and progress survive; position numbers stable across
       settings changes (change text size twice; "position N of M"
       unchanged — synthetic positions are layout-independent).
    7. All four themes + night mode; typography settings live-apply.
    8. Selection with copy/look-up(share/web-search) both platforms,
       horizontal and vertical.
    9. VoiceOver/TalkBack block-granular navigation with correct bounds,
       language switching (CJK book), link traits — the documented §7
       scope.
    10. Performance gate (Task 6.4 table).
    11. Cross-device digest check (Task 6.3): `scripts/parity-compare.sh
        parity-ios.json parity-android.json` → PARITY OK; corpus
        manifest (names + SHA-256) recorded.
    12. Bindings + zero-Readium: run `./scripts/build-core-ios.sh` and
        `./scripts/build-core-android.sh`, then both shell builds clean;
        `git status --short` shows no unexpected generated-file drift;
        repo-wide sweep `grep -rin readium --include='*' apps core
        scripts assets website .github 2>/dev/null | grep -v
        -e '^apps/ios/build/' -e '^apps/ios/.derivedData/'` → zero hits
        (docs/ and git history are the only permitted resting places, per
        the overview).
    The evidence file lists each item, its evidence, the corpus manifest,
    the perf table, and the two digest JSON file hashes. It is the
    artifact the owner reads before merging `dev/core` → `main` (the
    merge itself is an owner action — out of scope).
  - **Error handling:** any failed item blocks the gate; the evidence file
    records the failure and the movement/task it points back to rather
    than being massaged.
  - **Verify:** the evidence file exists, every checklist row has
    evidence, and rows 10–12's commands are reproducible as written.

## Notes for the conductor

- **Mid-plan degraded states are deliberate:** after Movement 1 both shells
  read via the interim Readium bridge at chapter-start accuracy (positions
  written as `charOffset 0`); precision returns in Movements 2 (iOS) and 5
  (Android). `dev/core` only — nothing ships between movements.
- **Task 5.1 and 5.2 may need to land as one commit** (the ViewModel
  rewrite and the screen that hosts it are hard to compile apart); both
  task write-ups stay authoritative for scope.
- **Movement 4 may run in parallel with Movements 2–3** but only after
  Task 2.1, whose slimmed protocol is the shape Android mirrors.
- **Assumptions carried from the overview/spec:** `chapterPositionRanges`
  returns one row per spine resource in spine order (spineCount source);
  `hit_test` at the reading-start corner returns the first character shown
  on a page (backed by the spec's `locate(hit_test(x)) = x` round-trip
  property); the 1024-char synthetic block size (spec §8) is stable enough
  to mirror in the two `ReaderPositions` helpers — if plan 01 shipped a
  core-side position lookup instead, prefer it and delete the helpers.
- **`Decoration` carries no color role** (overview shape `{ kind, rect }`);
  the link-region intersection rule in Tasks 1.4/4.2 is this plan's local
  decision — revisit only if plan 01 extended the record.
- **Parity corpus sourcing** (Task 6.3): prefer plan-01 fixture exports if
  its test tooling exposes them; otherwise the owner's benchmark library.
  Either way the evidence file pins names + hashes so the run is
  reproducible.
- **Owner-interactive steps:** on-device iOS performance numbers,
  Instruments hitch comparison, and the on-device checklist rows — prepare
  builds and exact commands; the owner executes and reports, per this
  machine's workflow.
