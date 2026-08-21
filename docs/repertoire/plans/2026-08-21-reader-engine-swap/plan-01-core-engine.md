# Reader Engine Swap — Plan 01: Core Engine — Implementation Plan

> **For the conductor:** this plan is structured into movements — task groups
> sized for one fresh implementer each, with clean seams between them. Execute
> with `/repertoire:maestro` (or any plan executor). Tasks use checkbox syntax
> for tracking.

**Spec:** `docs/repertoire/specs/2026-08-21-reader-engine-swap-spec.md`
**Overview:** `00-overview.md`
**Goal:** Everything Rust for the reader engine swap: the 5-crate workspace
restructure, the `inkuna-engine` layout pipeline, the `ReaderSession` FFI with
facade split, DB migration V8 + the reconcile pass, and search offset
unification — engine complete and tested behind regenerated bindings, with
both shells still compiling after every movement.
**Architecture:** The core becomes a 5-crate workspace running a deterministic
fixed-point (i32, 1/64 layout point) pipeline — XHTML parse → opinionated
style → rustybuzz shaping with bundled fonts → UAX-14 breaking → progressive
pagination — behind a new `ReaderSession` FFI object whose interaction path is
synchronous and cache-only. All character offsets index the canonical text
projection; content coordinates `(spine_idx, char_offset)` replace Readium
locators everywhere, rebaselined by V8 + a per-book reconcile pass.
**Build:** `cd core && cargo build --workspace` ·
**Test:** `cd core && cargo test --workspace` (single test:
`cargo test -p <crate> <module::path>`)
**Bindings:** after ANY `inkuna-ffi` change run BOTH
`./scripts/build-core-ios.sh` and `./scripts/build-core-android.sh` from the
repo root before touching shell code. Never strip before bindgen; never
hand-edit generated files.
**Shell compile checks** (required at the end of M1 and M6):
`cd apps/ios && xcodegen generate && xcodebuild -project Inkuna.xcodeproj
-scheme Inkuna -destination 'generic/platform=iOS Simulator' build` and
`cd apps/android && ./gradlew assembleDebug`.

## File structure

Movement 1 — workspace & FFI restructure:

- Modify `core/Cargo.toml` — workspace members become the five crates.
- Create `core/crates/inkuna-content/Cargo.toml` + `src/lib.rs` — EPUB
  container layer crate (no FFI, no DB).
- Move into `core/crates/inkuna-content/src/`: `archive.rs`, `container.rs`,
  `opf.rs`, `toc.rs`, `href.rs`, `xml.rs`, `model.rs`, `package.rs`,
  `cover.rs` (+ their `*_tests.rs`) from
  `core/crates/inkuna-core/src/formats/epub/`.
- Create `core/crates/inkuna-content/src/error.rs` — `ContentError` thiserror
  enum.
- Create `core/crates/inkuna-content/src/test_support.rs` — feature-gated
  shared EPUB fixture builders (moved `write_epub_with` + friends).
- Create `core/crates/inkuna-format/Cargo.toml` + `src/lib.rs` — import-side
  conversion crate.
- Move into `core/crates/inkuna-format/src/`: `mobi/`, `azw3/`, `txt/` from
  `core/crates/inkuna-core/src/formats/`, plus `epub_write.rs` (was
  `formats/epub/write.rs`) and `src/test_support.rs` (was
  `src/mobi_test_support.rs`, feature-gated).
- Create `core/crates/inkuna-format/src/error.rs` — `FormatError` thiserror
  enum.
- Create `core/crates/inkuna-engine/Cargo.toml` + `src/lib.rs` +
  `src/error.rs` — empty-shell engine crate with `EngineError`.
- Modify `core/crates/inkuna-core/src/lib.rs`,
  `src/formats/mod.rs`, `src/formats/epub/mod.rs`,
  `src/features/import/pipeline.rs`, `src/core/error.rs`,
  `src/test_support.rs` — consume the new crates; corrected module docs.
- Modify `core/crates/inkuna-ffi/src/bookshelf.rs` — root object shrinks to
  constructor + facade accessors; `open` gains `font_dir`.
- Modify `core/crates/inkuna-ffi/src/{library,import,search,settings,progress,stats}.rs`
  — each becomes a `Shelf*` facade object.
- Modify `apps/ios/Inkuna/Library/LibraryStore.swift`,
  `apps/android/app/src/main/java/app/inkuna/android/model/LibraryStore.kt`
  and every shell call site of a relocated method — mechanical facade-accessor
  insertion.

Movement 2 — content, DOM, projection, style (all in
`core/crates/inkuna-engine/` unless noted):

- Create `src/dom/mod.rs`, `src/dom/arena.rs`, `src/dom/parse.rs`,
  `src/dom/tests.rs` — quick-xml streaming into a compact arena tree with
  budgets and lenient recovery.
- Create `src/style/mod.rs`, `src/style/model.rs`, `src/style/sheet.rs`,
  `src/style/cascade.rs`, `src/style/tests.rs` — cssparser-based opinionated
  resolution.
- Create `src/settings.rs` + `src/settings_tests.rs` — `LayoutSettings`,
  the transcribed §9 typography mapping table, settings fingerprint.
- Create `src/text/mod.rs`, `src/text/projection.rs`,
  `src/text/projection_tests.rs` — canonical text projection + `Coordinate`.
- Modify `core/crates/inkuna-content/src/test_support.rs` — add the general
  `EpubBuilder` fixture builder.
- Create `src/test_support.rs` (inkuna-engine) — engine test helpers.

Movement 3 — fonts & shaping:

- Create `assets/fonts/` (repo root) — bundled font binaries + `OFL.txt`.
- Create `src/fixed.rs` + `src/fixed_tests.rs` — 1/64-pt fixed-point type and
  arithmetic helpers.
- Create `src/fonts/mod.rs`, `src/fonts/registry.rs`,
  `src/fonts/registry_tests.rs` — font registry with core-owned ids.
- Create `src/shape/mod.rs`, `src/shape/itemize.rs`, `src/shape/shape.rs`,
  `src/shape/vertical.rs`, `src/shape/ruby.rs`, `src/shape/tests.rs` —
  script/bidi itemization, rustybuzz shaping, fallback chain, vertical mode,
  ruby runs.

Movement 4 — breaking & pagination:

- Create `src/layout/mod.rs`, `src/layout/lines.rs`, `src/layout/justify.rs`,
  `src/layout/tests.rs` — UAX-14 breaking and justification.
- Create `src/paginate/mod.rs`, `src/paginate/blocks.rs`,
  `src/paginate/pages.rs`, `src/paginate/images.rs`, `src/paginate/tests.rs`
  — progressive block pagination, widow/orphan, keeps, images, vertical-rl.

Movement 5 — sessions, display lists, digests:

- Create `src/display/mod.rs`, `src/display/list.rs`, `src/display/maps.rs`,
  `src/display/a11y.rs`, `src/display/digest.rs`, `src/display/tests.rs` —
  `PageDisplayList` emission, locate/hit-test maps, a11y blocks, canonical
  serialization + blake3 digest.
- Create `src/session/mod.rs`, `src/session/session.rs`,
  `src/session/worker.rs`, `src/session/cache.rs`, `src/session/tests.rs` —
  `EngineSession`, background layout, generations, readiness, LRU cache.
- Create `src/corpus.rs` + `src/corpus_tests.rs` — book-level corpus
  extraction via the canonical projection (import + reconcile entry point).
- Create `core/crates/inkuna-engine/golden/` — committed text snapshots
  (page counts, digests, page-0 dumps, projection texts).

Movement 6 — ReaderSession FFI, V8 + reconcile, search unification:

- Modify `core/crates/inkuna-core/src/core/error.rs` — new `CoreError`
  variants; delete `InvalidPositionRanges`.
- Modify `core/crates/inkuna-core/src/core/db/migrate.rs` — V8 SQL.
- Modify `core/crates/inkuna-core/src/features/import/pipeline.rs` — corpus
  via projection; import-time synthetic positions; delete
  `formats/epub/text.rs` + `text_tests.rs` (and `formats/epub/` if empty).
- Create `core/crates/inkuna-core/src/features/library/rebaseline.rs` +
  `rebaseline_tests.rs` — the V8 reconcile pass.
- Modify `core/crates/inkuna-core/src/features/library/{store,model,queries,bookmarks}.rs`,
  `src/features/progress/{writes,positions,model}.rs` — Coordinate columns,
  reconcile kick, deleted shell-reporting APIs.
- Modify `core/crates/inkuna-core/src/features/search/fold.rs` +
  `queries.rs` — original-space offset guarantee + tests.
- Modify `core/crates/inkuna-core/src/lib.rs` — re-export engine session API
  and `Coordinate`.
- Create `core/crates/inkuna-ffi/src/reader/mod.rs`, `records.rs`,
  `session.rs`, `listener.rs` — the full `ReaderSession` FFI surface.
- Modify `core/crates/inkuna-ffi/src/{error,library,progress,bookshelf,lib}.rs`
  — new error variants, Coordinate swap, `open_reader`.
- Modify shell call sites of changed progress/bookmark/search methods —
  compile-only mechanical fixes (plan 02 rewrites the reader).

## Movement 1: Workspace & FFI restructure

Mechanical, behavior-preserving refactors that land first so later `main`
bugfixes cherry-pick onto a stable layout (spec §1, assumption A9). After
every task `cd core && cargo test --workspace` is green; after task 1.6 both
shells build.
**Depends on:** nothing.

- [ ] **Task 1.1 — `inkuna-content` crate extraction**
  - **Files:** Create `core/crates/inkuna-content/{Cargo.toml, src/lib.rs,
    src/error.rs, src/test_support.rs}` · Move
    `core/crates/inkuna-core/src/formats/epub/{archive.rs, container.rs,
    opf.rs, opf_tests.rs, toc.rs, toc_tests.rs, href.rs, href_tests.rs,
    xml.rs, model.rs, package.rs, cover.rs, cover_tests.rs, archive_tests.rs}`
    into `core/crates/inkuna-content/src/` · Modify `core/Cargo.toml`
    (members += `crates/inkuna-content`),
    `core/crates/inkuna-core/{Cargo.toml, src/lib.rs,
    src/formats/epub/mod.rs, src/test_support.rs}`.
  - **Behavior:** New crate `inkuna-content` (version/edition/license
    `.workspace = true`; deps: `zip` (same features as core), `quick-xml`,
    `percent-encoding`, `thiserror`, `log`; dev-deps: `tempfile`; feature
    `test-support = []`). `src/lib.rs` is declarations + re-exports only:
    `pub use` for `read_package`, `EpubPackage`, `EpubMetadata`, `TocEntry`,
    `Cover`, `ContentError`, and the href API. The href functions become
    `pub` (they were `pub(super)`): `pub fn resolve_href(base_dir: &str,
    href: &str) -> String`, `pub fn resolve_relative(doc_path: &str,
    href: &str) -> String`, plus new `pub fn split_fragment(href: &str) ->
    (&str, Option<&str>)` (splits at the first `#`). The archive module
    exposes `pub fn read_resource(epub_path: &Path, href: &str) ->
    Result<Vec<u8>, ContentError>` — a budget-bounded read of one
    package-root-relative entry, reusing the existing per-entry decompression
    caps (single enforcement point for the resource-bounds postmortem rules;
    the engine and `cover.rs` both call it). `ContentError` (thiserror) has
    variants `Io(#[from] std::io::Error)`, `Archive(String)`,
    `InvalidPublication(String)`, `FileTooLarge(u64)` — mapped from the
    current `CoreError` usages inside the moved files.
    `core/crates/inkuna-core/src/core/error.rs` does NOT gain a
    transparent `Content` variant — instead add a manual
    `From<ContentError> for CoreError` mapping `Io→Io`, `Archive→Archive`,
    `InvalidPublication→InvalidPublication`, `FileTooLarge→FileTooLarge`,
    so the existing `CoreError` variants and the FFI mirror stay
    unchanged. `test_support.rs` holds the moved
    `write_epub_with` / `TocKind` / `CoverKind` builders behind
    `#[cfg(any(test, feature = "test-support"))]`, now `pub`;
    `inkuna-core`'s dev-dependencies gain `inkuna-content = { path =
    "../inkuna-content", features = ["test-support"] }` and its
    `src/test_support.rs` re-exports them so existing core tests keep
    compiling unchanged. Module docs corrected as files move: the epub
    `mod.rs` sentence "Rendering and pagination stay in the shells (Readium
    navigators); the core parses once at import and never re-opens the book
    afterwards" is replaced with "the container layer serves both import and
    the core-owned reader engine, which re-opens archives at read time".
    `inkuna-core/src/formats/epub/mod.rs` shrinks to declaring `text` (and
    re-exporting `extract_spine_text`, `MAX_TOTAL_TEXT_BYTES`) plus `pub use
    inkuna_content::...` shims so `crate::formats::epub::read_package` call
    sites in the import pipeline keep compiling.
  - **Error handling:** no new failure modes — errors are renamed, not
    reshaped; the `From<ContentError> for CoreError` arm is exhaustive (no
    catch-all), so a future `ContentError` variant is a compile error.
  - **Tests:** the moved `*_tests.rs` files pass unchanged inside
    `inkuna-content` (fixture builders now local). Add
    `read_resource_respects_entry_budget` (in `archive_tests.rs`): a crafted
    entry over the per-entry cap → `ContentError::Archive`;
    `split_fragment_variants`: `"a.xhtml#x"` → `("a.xhtml", Some("x"))`,
    `"a.xhtml"` → `("a.xhtml", None)`, `"#x"` → `("", Some("x"))`.
  - **Verify:** `cd core && cargo test --workspace` → green;
    `cargo test -p inkuna-content` → moved suites pass.

- [ ] **Task 1.2 — OPF spine/rendition metadata extension**
  - **Files:** Modify `core/crates/inkuna-content/src/{opf.rs, opf_tests.rs,
    model.rs, package.rs, lib.rs}` ·
    `core/crates/inkuna-core/src/features/import/pipeline.rs` (mechanical).
  - **Behavior:** `EpubPackage` gains the fields the engine needs at open:
    `spine` changes from `Vec<String>` to `Vec<SpineItem>` where
    `pub struct SpineItem { pub href: String, pub media_type:
    Option<String> }` (media type joined from the manifest item backing each
    `itemref`); new `pub manifest: Vec<ManifestItem>` with
    `pub struct ManifestItem { pub href: String, pub media_type:
    Option<String> }` (all manifest entries, hrefs normalized, capped by the
    existing `MAX_MANIFEST_ITEMS`); new `pub rendition_layout:
    RenditionLayout` with `pub enum RenditionLayout { Reflowable,
    PrePaginated }` read from the OPF `<meta property="rendition:layout">`
    (default `Reflowable`; any value other than `pre-paginated` is
    `Reflowable`); new `pub page_progression_rtl: bool` from the spine
    element's `page-progression-direction="rtl"` attribute (absent or any
    other value → `false`). All existing dedupe/href-cap behavior for spine
    entries is preserved. Import pipeline call sites switch to
    `item.href` mechanically; no behavior change there.
  - **Error handling:** malformed `rendition:layout`/`page-progression-
    direction` values are never errors — they take the defaults above.
  - **Tests:** `spine_items_carry_media_types`: fixture OPF with
    `application/xhtml+xml` itemrefs → media types present;
    `rendition_prepaginated_detected`: OPF with
    `<meta property="rendition:layout">pre-paginated</meta>` →
    `PrePaginated`; `page_progression_rtl_detected`: spine with
    `page-progression-direction="rtl"` → `true`, absent → `false`;
    `unknown_rendition_value_is_reflowable`.
  - **Verify:** `cd core && cargo test -p inkuna-content opf` → green;
    `cargo test --workspace` → green.

- [ ] **Task 1.3 — `inkuna-format` crate**
  - **Files:** Create `core/crates/inkuna-format/{Cargo.toml, src/lib.rs,
    src/error.rs, src/test_support.rs}` · Move
    `core/crates/inkuna-core/src/formats/{mobi/, azw3/, txt/}` to
    `core/crates/inkuna-format/src/{mobi/, azw3/, txt/}`, and
    `core/crates/inkuna-core/src/formats/epub/{write.rs, write_tests.rs}` to
    `core/crates/inkuna-format/src/{epub_write.rs, epub_write_tests.rs}`;
    move `core/crates/inkuna-core/src/mobi_test_support.rs` content into
    `core/crates/inkuna-format/src/test_support.rs` · Modify
    `core/Cargo.toml` (members += `crates/inkuna-format`),
    `core/crates/inkuna-core/{Cargo.toml, src/lib.rs, src/formats/mod.rs,
    src/test_support.rs}` and the import pipeline's converter imports.
  - **Behavior:** `inkuna-format` depends on `inkuna-content` (the EPUB
    writer and converters consume its href/xml helpers where they already
    did) plus exactly the deps its moved code requires — start from `zip`,
    `quick-xml`, `thiserror`, `chardetng`, `encoding_rs`, `log`, `regex`,
    `percent-encoding` and let `cargo check -p inkuna-format` settle the
    final list; every dep no longer required by `inkuna-core` is removed from
    its `Cargo.toml` (verify by `cargo check -p inkuna-core` after removal —
    a dep stays only if the build requires it). Public surface re-exported
    from `lib.rs`: the converter entry points exactly as the import pipeline
    calls them today (preserve names/signatures — read them from
    `features/import/pipeline.rs` before moving) plus `EpubWriter`.
    `FormatError` mirrors the `CoreError` variants the moved code constructs
    (`Io`, `InvalidPublication`, `UnsupportedFormat`, `FileTooLarge`) with a
    manual exhaustive `From<FormatError> for CoreError`. `test_support.rs`
    is the moved MOBI/KF8 fixture kit behind
    `#[cfg(any(test, feature = "test-support"))]`, `pub`; `inkuna-core`
    dev-deps gain `inkuna-format = { path = "../inkuna-format", features =
    ["test-support"] }` and `inkuna-core/src/test_support.rs` re-exports the
    builders so import-pipeline tests compile unchanged. Format *detection*
    (`formats/format.rs`) stays in `inkuna-core`.
  - **Error handling:** as 1.1 — exhaustive `From`, no reshaping.
  - **Tests:** moved converter/writer suites pass in `inkuna-format`;
    `inkuna-core` import-pipeline tests (MOBI/AZW3/TXT end-to-end) pass
    unchanged.
  - **Verify:** `cd core && cargo test --workspace` → green.

- [ ] **Task 1.4 — `inkuna-engine` empty shell + core dependency edges**
  - **Files:** Create `core/crates/inkuna-engine/{Cargo.toml, src/lib.rs,
    src/error.rs}` · Modify `core/Cargo.toml` (members += —, final list:
    `crates/inkuna-content`, `crates/inkuna-format`, `crates/inkuna-engine`,
    `crates/inkuna-core`, `crates/inkuna-ffi`),
    `core/crates/inkuna-core/{Cargo.toml, src/lib.rs}`.
  - **Behavior:** `inkuna-engine` deps: `inkuna-content` (path), `thiserror`,
    `log`. (The heavy deps — `rustybuzz`, `unicode-bidi`, `unicode-script`,
    `unicode-linebreak`, `cssparser`, `icu_segmenter`, `imagesize`, `blake3`,
    `quick-xml` — are added by the movement that first uses each, always the
    latest stable queried from crates.io at that moment per stack policy.)
    `src/error.rs` defines the crate's thiserror enum, final shape now so
    later movements only add call sites:
    `pub enum EngineError { UnsupportedContent { detail: String },
    BudgetExceeded { detail: String }, NotReady,
    AnchorNotFound { detail: String }, Io(#[from] std::io::Error),
    Content(#[from] inkuna_content::ContentError) }`. `src/lib.rs` declares
    `mod error;` + `pub use error::EngineError;` and the module doc: "The
    reader layout engine: parse → style → shape → break → paginate →
    display lists. Deterministic fixed-point layout; no DB access; archive
    reads via inkuna-content only." `inkuna-core` adds `inkuna-engine` as a
    dependency (unused until M6 — that is fine, cargo does not warn) and its
    `lib.rs` module doc drops "(Readium navigators)": rendering is described
    as "layout in the core's engine, drawing in the shells".
  - **Error handling:** n/a (shell crate).
  - **Tests:** none yet (`cargo test -p inkuna-engine` compiles empty).
  - **Verify:** `cd core && cargo build --workspace` → all five crates
    build; `cargo test --workspace` → green.

- [ ] **Task 1.5 — FFI facade split + `font_dir`**
  - **Files:** Modify `core/crates/inkuna-ffi/src/{bookshelf.rs, library.rs,
    import.rs, search.rs, settings.rs, progress.rs, stats.rs, lib.rs}`.
  - **Behavior:** Each feature module's `impl Bookshelf` block becomes its
    own cached `uniffi::Object` wrapping the shared library — pattern,
    identical in all six files:
    `#[derive(uniffi::Object)] pub struct ShelfLibrary(pub(crate)
    Arc<inkuna_core::Library>);` with the module's methods moved verbatim
    into `#[uniffi::export(async_runtime = "tokio")] impl ShelfLibrary
    { ... }` (bodies unchanged — `self.0.clone()` still works). Mapping:
    `library.rs` → `ShelfLibrary` (`list`, `publication`, `remove`,
    `search_library`, `chapters`, `add_bookmark`, `bookmarks`,
    `remove_bookmark`); `import.rs` → `ShelfImport` (`import`,
    `import_batch`, `import_fd`, `import_batch_fds`, `optimize_covers`);
    `search.rs` → `ShelfSearch` (`search_in_book`, `search_all_books`);
    `settings.rs` → `ShelfSettings` (`settings`, `set_settings`);
    `progress.rs` → `ShelfProgress` (`update_progress`,
    `report_position_count`, `report_position_ranges`,
    `chapter_position_ranges`, `set_finished`); `stats.rs` → `ShelfStats`
    (`session_start`, `session_end`, `stats_overview`). `Bookshelf` becomes:
    ```rust
    #[derive(uniffi::Object)]
    pub struct Bookshelf {
        pub(crate) library: Arc<inkuna_core::Library>,
        pub(crate) font_dir: std::path::PathBuf,
        library_facade: Arc<ShelfLibrary>,
        importer: Arc<ShelfImport>,
        search: Arc<ShelfSearch>,
        settings: Arc<ShelfSettings>,
        progress: Arc<ShelfProgress>,
        stats: Arc<ShelfStats>,
    }
    ```
    Constructor `pub fn open(data_dir: String, font_dir: String) ->
    Result<Arc<Self>, InkunaError>` builds the `Library` as today, validates
    `font_dir` is an existing directory (see error handling), and constructs
    all six facades once. Accessors, exported sync (cheap `Arc` clones, no
    I/O): `pub fn library(&self) -> Arc<ShelfLibrary>`, `importer()`,
    `search()`, `settings()`, `progress()`, `stats()`. `blocking()` stays
    `pub(crate)` in `bookshelf.rs`; `core_version()` unchanged. `lib.rs`
    re-exports the six facade types. Doc comments on `Bookshelf` gain:
    "`font_dir` is the bundled fonts directory the reader engine shapes
    with; the shells pass their bundled copy of repo `assets/fonts/`."
    Before landing, check generated Swift/Kotlin for name collisions
    (the `Library`/JNA and `message`/`Throwable` precedents) — `Shelf*` is
    the approved prefix; if `ShelfImport.import` collides with the Kotlin
    keyword, UniFFI already escapes it today (method exists on `Bookshelf`)
    so no rename is expected.
  - **Error handling:** `font_dir` missing or not a directory →
    `InkunaError::Io { detail: "font_dir does not exist: <path>" }` at
    `open` — fail fast at startup rather than at first reader open.
  - **Tests:** none in `inkuna-ffi` (crate has no test suite today);
    correctness is `cargo build -p inkuna-ffi` + bindgen in 1.6.
  - **Verify:** `cd core && cargo test --workspace` → green.

- [ ] **Task 1.6 — bindings regeneration + mechanical shell call-site update**
  - **Files:** Run `./scripts/build-core-ios.sh` and
    `./scripts/build-core-android.sh` (repo root) · Modify
    `apps/ios/Inkuna/Library/LibraryStore.swift` (line ~49: `try
    Bookshelf.open(dataDir: directory.path, fontDir:
    Bundle.main.bundlePath)` — placeholder; plan 02 wires the real bundled
    fonts directory, comment it as such) ·
    `apps/android/app/src/main/java/app/inkuna/android/model/LibraryStore.kt`
    (line ~56: `Bookshelf.open(dataDir.absolutePath, File(dataDir,
    "fonts").apply { mkdirs() }.absolutePath)` — same placeholder comment) ·
    every Swift/Kotlin call site of a relocated method.
  - **Behavior:** Purely mechanical: each `bookshelf.foo(...)` becomes
    `bookshelf.<facade>().foo(...)` per the 1.5 mapping (Swift:
    `bookshelf.library().list(...)`; Kotlin the same). Find call sites by
    grepping each moved method name (camelCase in shells: `searchInBook`,
    `updateProgress`, `importBatchFds`, `sessionStart`, …) across
    `apps/ios/Inkuna/` and `apps/android/app/src/main/java/`; known consumer
    files: iOS `ReaderViewController.swift`, `LibraryStore.swift`,
    `ImportService.swift` (plus any Home/Detail/Settings callers the grep
    finds); Android `ReaderViewModel.kt`, `TonightViewModel.kt`,
    `ImportBooks.kt`, `LibraryStore.kt`, `ImportEngine.kt`. Where a shell
    store caches the `Bookshelf`, it may equally cache the facade — but do
    NOT restructure shell stores; insert the accessor call inline. No
    behavior change; both apps run exactly as before.
  - **Error handling:** n/a (compile-time exercise).
  - **Tests:** none (shells have no test targets — overview).
  - **Verify:** both build scripts succeed; `cd apps/ios && xcodegen
    generate && xcodebuild -project Inkuna.xcodeproj -scheme Inkuna
    -destination 'generic/platform=iOS Simulator' build` → succeeds;
    `cd apps/android && ./gradlew assembleDebug` → succeeds;
    `grep -rn "bookshelf\.\(list\|searchInBook\|updateProgress\)" apps/ |
    grep -v "()\."` finds no un-migrated direct calls.

## Movement 2: Content, DOM, projection, style

The front half of the engine pipeline: parse → style → canonical projection.
Everything here is pure (no I/O — callers hand in bytes), deterministic, and
budget-guarded. Interfaces defined here are consumed verbatim by M4/M5.
**Depends on:** Movement 1.

- [ ] **Task 2.1 — `EpubBuilder` general fixture builder**
  - **Files:** Modify `core/crates/inkuna-content/src/test_support.rs` ·
    Create `core/crates/inkuna-engine/src/test_support.rs` (declared from
    `lib.rs` as `#[cfg(test)] mod test_support;`) · Modify
    `core/crates/inkuna-engine/Cargo.toml` (dev-deps: `tempfile`,
    `inkuna-content` with `test-support`).
  - **Behavior:** `inkuna-content::test_support` gains a general builder the
    engine suites use for every fixture (existing `write_epub_with` stays
    for old tests):
    ```rust
    pub struct EpubBuilder { /* private */ }
    impl EpubBuilder {
        pub fn new() -> Self;
        pub fn language(self, tag: &str) -> Self;
        pub fn resource(self, href: &str, media_type: &str,
                        bytes: &[u8]) -> Self;      // manifest entry
        pub fn spine(self, hrefs: &[&str]) -> Self; // itemrefs, in order
        pub fn toc(self, entries: &[(&str, &str, u32)]) -> Self;
                                    // (title, href, depth), nav doc
        pub fn rtl_progression(self) -> Self;
        pub fn pre_paginated(self) -> Self;
        pub fn write(self, path: &Path);            // panics on I/O error
    }
    ```
    XHTML resources are passed as full documents by tests. Engine
    `test_support.rs` wraps it with helpers used across M2–M5:
    `fn build_epub(dir: &TempDir, b: EpubBuilder) -> PathBuf`, plus the
    canonical fixture documents as `const` XHTML strings: `LATIN_DOC`
    (headings, paragraphs, em/strong, a link with fragment target),
    `CJK_HORIZONTAL_DOC` (Chinese paragraphs — CJK fixtures are mandatory),
    `CJK_VERTICAL_RUBY_DOC` (`writing-mode: vertical-rl` on body via
    `<style>`, ruby with rt), `RTL_DOC` (Hebrew text, `dir="rtl"`),
    `MIXED_SCRIPT_DOC` (Latin+CJK+digits in one paragraph), `IMAGE_DOC`
    (`<img>` with width/height-bearing PNG bytes helper `fn tiny_png(w: u32,
    h: u32) -> Vec<u8>` emitting a minimal valid PNG header), `TABLE_DOC`
    (2×2 table), `MALFORMED_DOC` (unclosed `<em>`, stray `&`).
  - **Error handling:** builders panic on I/O errors (test-only code —
    allowed by convention).
  - **Tests:** `epub_builder_roundtrip` (in content's test_support-covered
    suite, `package_tests.rs` or new sibling): built EPUB parses via
    `read_package` with the declared spine order, TOC, rtl flag, and
    rendition layout.
  - **Verify:** `cd core && cargo test -p inkuna-content` → green.

- [ ] **Task 2.2 — arena DOM parser (`dom/`)**
  - **Files:** Create `core/crates/inkuna-engine/src/dom/{mod.rs, arena.rs,
    parse.rs, tests.rs}` · Modify `core/crates/inkuna-engine/Cargo.toml`
    (add `quick-xml`, same version/features as inkuna-content) and
    `src/lib.rs` (declare `pub mod dom;` etc. — every module task hereafter
    also adds its declaration + `pub use`; not restated again).
  - **Behavior:** `arena.rs` defines the compact tree:
    ```rust
    pub struct NodeId(pub u32);
    pub enum NodeKind { Element(ElementData), Text(String) }
    pub struct ElementData {
        pub name: ElementName,          // interned, see below
        pub class: Option<Box<str>>,    // `class` attr, verbatim
        pub id: Option<Box<str>>,       // `id` attr
        pub lang: Option<Box<str>>,     // `lang` or `xml:lang`
        pub style_attr: Option<Box<str>>, // inline `style`
        pub href: Option<Box<str>>,     // `a href` / `img src`, verbatim
        pub dir_rtl: Option<bool>,      // `dir` attr: rtl/ltr
    }
    pub struct Node { pub kind: NodeKind, pub parent: Option<NodeId>,
                      pub children: Vec<NodeId> }
    pub struct Document {
        pub nodes: Vec<Node>, pub root: NodeId,
        pub anchors: Vec<(String, NodeId)>,   // every `id` attr, doc order
        pub stylesheets: Vec<StylesheetSource>,
        pub truncated: bool,
    }
    pub enum StylesheetSource { Linked(String /* href, verbatim */),
                                Inline(String) }
    ```
    `ElementName` is an enum of the semantically honored elements — `Html,
    Body, H1..H6, P, Div, Span, Em, Strong, Blockquote, Pre, Code, Ol, Ul,
    Li, Ruby, Rt, Rb, Img, Image, Table, Tr, Td, Th, Caption, Br, Hr, A,
    Section, Article, Figure, Figcaption, Dt, Dd, Other(Box<str>)` — with
    `i` mapping to `Em` and `b` to `Strong` at parse time. `parse.rs`
    exposes `pub fn parse(xhtml: &[u8]) -> Result<Document, EngineError>`:
    quick-xml streaming (entity resolution via
    `quick_xml::escape::unescape`, matching content's `xml.rs` machinery),
    lenient recovery — an unclosed inline element is closed at its parent's
    end; a stray `&` or unknown entity becomes literal text; mismatched end
    tags pop to the nearest matching open element, or are dropped if none
    matches. `script` and `template` subtrees are skipped entirely; `head`
    is scanned, not skipped: `<link rel="stylesheet" href>` (any case,
    `type` ignored) appends `Linked`, `<style>` text appends `Inline`; head
    produces no arena nodes. Budgets (module consts in `parse.rs`):
    `MAX_DOM_NODES: usize = 262_144`, `MAX_TEXT_BYTES: usize = 4_194_304`
    (sum of text-node bytes), `MAX_ATTR_BYTES: usize = 4_096` (a longer
    attribute value is truncated on a char boundary),
    `MAX_STYLESHEET_BYTES: usize = 1_048_576` (sum of inline styles here;
    linked sheets are budgeted at load in 2.4), `MAX_DEPTH: usize = 256`
    (deeper elements are flattened into their ancestor at MAX_DEPTH).
    Hitting node/text budgets stops parsing, keeps the tree built so far,
    and sets `Document::truncated = true` — never an error (§12: truncated
    resources render their prefix).
  - **Error handling:** structurally unusable input — no root element
    decodable, or quick-xml fails before any element — returns
    `EngineError::UnsupportedContent { detail }` (fails closed per-resource,
    never a panic). Everything milder recovers as above.
  - **Tests:** `parses_well_formed_cjk_doc`: `CJK_HORIZONTAL_DOC` → tree
    with expected paragraph count and text; `recovers_unclosed_inline`:
    `MALFORMED_DOC` parses, `<em>` text present;
    `skips_script_scans_head`: doc with `<script>`, `<style>`, `<link
    rel="stylesheet">` → no script text in tree, both stylesheet sources
    captured in order; `anchor_map_collects_ids`: nested elements with `id`
    → `anchors` in document order; `node_budget_truncates_not_errors`:
    generated doc with 300k elements → `Ok`, `truncated == true`, nodes ≤
    `MAX_DOM_NODES`; `garbage_input_fails_closed`: random bytes →
    `UnsupportedContent`; `attr_budget_truncates_on_char_boundary`: 8 KiB
    CJK `class` value → retained prefix is valid UTF-8 ≤ 4096 bytes.
  - **Verify:** `cd core && cargo test -p inkuna-engine dom` → green.

- [ ] **Task 2.3 — settings module + §9 transcription**
  - **Files:** Create `core/crates/inkuna-engine/src/{settings.rs,
    settings_tests.rs}` · Modify `core/crates/inkuna-engine/Cargo.toml`
    (add `blake3`).
  - **Behavior:**
    ```rust
    pub struct LayoutSettings {
        pub reading_font: String, pub reading_bold: bool,
        pub text_size_step: u8,      // 0..=4, clamped
        pub line_spacing: f64,       // 1.30..=2.10, clamped
        pub letter_spacing: f64,     // em, 0.0..=0.06, clamped
        pub word_spacing: f64,       // em, 0.0..=0.30, clamped
        pub reading_margins: u32,    // layout points, 16..=48, clamped
    }
    impl LayoutSettings {
        pub fn clamped(self) -> Self;
        pub fn fingerprint(&self) -> u64;  // first 8 bytes (LE) of blake3
            // over a canonical encoding: each field in declaration order,
            // strings as len(u64 LE)+bytes, f64 as to_bits() LE
        pub fn typography(&self) -> Typography; // resolved numbers below
    }
    pub struct Typography {
        pub font_size: f64,                // body size in layout points
        pub line_height: f64,              // points (size × line_spacing)
        pub paragraph_spacing: f64, pub paragraph_indent: f64, // points
        pub heading_scale: [f64; 6],       // h1..h6 multiplier over body
        pub ruby_scale: f64,               // ruby size / base size
        pub bold_base: bool,
    }
    ```
    (Until 3.1 lands `Fx`, `Typography` carries `f64` point values; M4
    converts to fixed-point at consumption via `Fx::from_pt`.) **The
    concrete numbers are transcribed, not invented**: read
    `apps/ios/Inkuna/Reader/ReaderUserStyle.swift` and
    `apps/android/app/src/main/java/app/inkuna/android/ui/reader/ReaderUserCss.kt`
    and transcribe the step→font-size table (5 steps), paragraph
    spacing/indent, heading scales, and any ruby sizing into module consts
    with a doc comment citing both source files ("transcribed 2026-08-21
    from ReaderUserStyle.swift / ReaderUserCss.kt before their plan-02
    deletion — the deleted files must never be the only record of the
    current look", spec §9). Where the two shells disagree on a value, take
    the iOS value and note the Android one in the comment; where a value
    does not exist in either (ruby scale, if absent), use 0.5 and mark it
    `// engine-chosen: no Readium precedent`. `reading_margins` is
    reinterpreted as layout points (numerically identical at 1×). Font-id
    mapping also transcribed: the shells' opaque `reading_font` roster ids
    (read them from the same files / `ReadingFont.kt`) map to registry
    faces — serif-family ids → NotoSerif, sans ids → NotoSans; `publisher`
    and any unknown id → NotoSerif (publisher-embedded fonts are a
    non-goal).
  - **Error handling:** none — out-of-range values clamp, unknown fonts
    default; `clamped()` is applied at every engine entry (session open,
    update_layout, corpus never needs it).
  - **Tests:** `fingerprint_is_stable_and_sensitive`: equal settings → equal
    fingerprint; changing any single field changes it;
    `clamps_out_of_range`: step 9 → 4, line_spacing 5.0 → 2.10;
    `unknown_font_maps_to_serif`; `typography_matches_transcription`: step 2
    → the transcribed default body size (assert the actual transcribed
    number, written into the test at transcription time).
  - **Verify:** `cd core && cargo test -p inkuna-engine settings` → green.

- [ ] **Task 2.4 — opinionated style resolution (`style/`)**
  - **Files:** Create `core/crates/inkuna-engine/src/style/{mod.rs, model.rs,
    sheet.rs, cascade.rs, tests.rs}` · Modify Cargo.toml (add `cssparser`,
    latest stable from crates.io).
  - **Behavior:** `model.rs`:
    ```rust
    pub enum WritingMode { HorizontalTb, VerticalRl }
    pub enum Direction { Ltr, Rtl }
    pub enum FontStyle { Normal, Italic }
    pub enum FontWeight { Normal, Bold }   // numeric ≥ 600 → Bold
    pub enum TextAlign { Start, Center, End, Justify }
    pub enum RubyPosition { Over, Under }
    pub struct ComputedStyle {
        pub display_none: bool, pub direction: Direction,
        pub font_style: FontStyle, pub font_weight: FontWeight,
        pub text_align: TextAlign, pub ruby_position: RubyPosition,
    }
    pub struct StyledDocument<'d> {
        pub doc: &'d Document,
        pub styles: Vec<ComputedStyle>,     // parallel to doc.nodes
        pub writing_mode: WritingMode,      // per-resource, from root/body
    }
    ```
    `sheet.rs`: `pub fn parse_sheet(css: &str) -> Stylesheet` — cssparser
    tokenization; retained rules are only `(selector, honored-declarations)`
    pairs where the selector is element, `.class`, `#id`, or a
    descendant-combinator chain of those, and the declarations are only:
    `writing-mode` (values `vertical-rl` honored; anything else →
    horizontal), `direction`, `font-style`, `font-weight`, `text-align`,
    `ruby-position`, `display: none` (other `display` values ignored). Any
    other selector, property, at-rule, or parse error is silently skipped —
    never an error (spec §2). Total CSS input is capped by the caller at
    `MAX_STYLESHEET_BYTES` (2.2's const, now covering the linked+inline sum
    per resource; when the sum exceeds the cap, whole sheets are dropped
    from the END of the cascade list, keeping earlier sheets intact). `cascade.rs`: `pub fn resolve(doc: &Document,
    sheets: &[Stylesheet]) -> StyledDocument` — UA defaults (em/i→italic,
    strong/b→bold, h1–h6→bold, `rt` display treated specially in
    projection, `dir` attribute → direction) < publisher sheets in given
    order < inline `style` attr; within publisher rules, CSS specificity
    (id 100 / class 10 / type 1) then source order. Inheritance:
    `direction`, `font-style`, `font-weight`, `text-align`,
    `ruby-position` inherit; `display_none` propagates to the whole
    subtree. `writing_mode` is read ONLY from `html`/`body` rules (or their
    inline styles) — per-resource, never per-element. Reader settings are
    NOT an input here: settings win visual properties later (M4 uses
    `Typography` for sizes/spacing; `TextAlign::Justify` is the default UA
    value for body text — publisher `text-align` on a block is honored).
    Tables: no style work — the table elements simply remain block-level
    (2.5/M4 lay `tr`/`caption` rows out as sequential blocks).
  - **Error handling:** none surfaced — the whole pass is
    ignore-what-you-don't-know by design; `resolve` is infallible.
  - **Tests:** `class_id_descendant_selectors_apply`: `.note strong` chain
    matches nested node; `specificity_and_order`: `#x` beats `.x` beats
    `p`; later rule wins ties; `inline_style_beats_publisher`;
    `display_none_hides_subtree`: styles of all descendants have
    `display_none`; `writing_mode_from_body_only`: `vertical-rl` on a `div`
    does NOT set resource mode, on `body` does;
    `dir_attr_sets_direction`: `RTL_DOC` → `Rtl` on the marked subtree;
    `unsupported_css_ignored`: `@media`, `p::first-line`, `float: left` →
    resolve succeeds, no effect; `cjk_doc_defaults`: `CJK_HORIZONTAL_DOC`
    → horizontal, justify default.
  - **Verify:** `cd core && cargo test -p inkuna-engine style` → green.

- [ ] **Task 2.5 — canonical text projection (`text/`)**
  - **Files:** Create `core/crates/inkuna-engine/src/text/{mod.rs,
    projection.rs, projection_tests.rs}`.
  - **Behavior:** Defines the coordinate type the whole system shares:
    `pub struct Coordinate { pub spine_idx: u32, pub char_offset: u64 }`
    (in `text/mod.rs`, re-exported at crate root — this is the type
    `inkuna-core` re-exports in M6 and the FFI mirrors). `projection.rs`:
    ```rust
    pub struct Projection {
        pub text: String,                 // THE canonical stream
        pub char_len: u64,                // text.chars().count()
        pub spans: Vec<TextSpan>,         // every contributing text node
        pub anchors: Vec<(String, u64)>,  // id → char offset (doc order)
        pub truncated: bool,
    }
    pub struct TextSpan { pub node: NodeId,
        pub char_range: std::ops::Range<u64>,  // into Projection::text
        pub node_char_start: u64 }             // offset inside the node's
                                               // own collapsed text
    pub fn project(styled: &StyledDocument) -> Projection;
    ```
    Rules (spec §2, exact): text nodes of rendered elements in document
    order; `display:none` subtrees excluded; `rt` subtree text excluded
    (ruby annotation is display-only); ruby base (`rb` or bare ruby text)
    included; whitespace collapsed per the existing
    `formats/epub/text.rs` rules — runs of Unicode whitespace collapse to
    one ASCII space, and a block-element boundary emits exactly one `\n`
    (block set = the `BLOCK` const list in `text.rs`: p, div, h1–h6, li,
    blockquote, section, article, tr, caption, figcaption, dt, dd, pre —
    read the list from the file and keep it identical), `br` emits `\n`,
    leading/trailing whitespace of each block trimmed, no double `\n`.
    No generated content, no soft hyphens (U+00AD is dropped), no list
    markers. Offsets are Unicode scalar counts. Anchors: each `id` maps to
    the offset of the first projected char at-or-after the element's start
    (elements projecting no text anchor at the next projected char, or
    `char_len` at end of document). Projection depends only on publisher
    CSS + UA defaults — never on reader settings — and this invariant is
    stated in the module doc: search corpus, positions, locators, layout,
    and migration all index this exact stream.
  - **Error handling:** infallible given a `StyledDocument`; truncation
    propagates from the DOM flag.
  - **Tests:** `excludes_rt_includes_base`: `CJK_VERTICAL_RUBY_DOC` →
    projection contains base kanji, not the rt readings, offsets
    contiguous; `display_none_excluded`; `whitespace_collapse_matches_rules`:
    doc with tabs/newlines/nbsp runs → single spaces, block boundaries one
    `\n`; `soft_hyphen_dropped`; `offsets_are_scalar_counts`: text with
    astral chars (𠀋) → `char_len` counts scalars, spans consistent;
    `anchor_offsets`: ids on empty and text-bearing elements → documented
    offsets; `br_emits_newline`; `projection_snapshot_cjk`: committed
    expected string for `CJK_HORIZONTAL_DOC` asserted verbatim.
  - **Verify:** `cd core && cargo test -p inkuna-engine text` → green.

## Movement 3: Fonts & shaping

The bundled font set becomes the repo-level source of truth, the engine gets
its registry and fixed-point arithmetic, and text turns into positioned glyph
runs via rustybuzz.
**Depends on:** Movement 2 (uses `StyledDocument`, `Projection`,
`Typography`).

- [ ] **Task 3.1 — fixed-point module (`fixed.rs`)**
  - **Files:** Create `core/crates/inkuna-engine/src/{fixed.rs,
    fixed_tests.rs}`.
  - **Behavior:** All layout arithmetic runs in i32 units of 1/64 layout
    point (spec §2 determinism invariant; M4 states it as a review rule for
    every consumer):
    ```rust
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug)]
    pub struct Fx(pub i32);                 // 1/64 layout point
    impl Fx {
        pub const ZERO: Fx;
        pub fn from_pt(pt: f64) -> Fx;      // round half away from zero
        pub fn to_f32(self) -> f32;         // self.0 as f32 / 64.0 —
                                            // the ONLY exit to float
        pub fn saturating_add(self, o: Fx) -> Fx;  // + Add/Sub/Neg impls,
                                            // all saturating, never panic
        pub fn mul_ratio(self, num: i32, den: i32) -> Fx;
                                            // (i64 mul, round half up)
        pub fn scale_font_units(units: i32, size: Fx, upem: u16) -> Fx;
            // (units as i64 * size.0 as i64 + upem/2) / upem, i64 mul —
            // the one formula every font-unit→layout conversion uses
    }
    ```
    `f64`/`f32` appear only at `from_pt` (viewport/settings entry) and
    `to_f32` (display-list emission). Accumulation order is the caller's
    responsibility; this module guarantees each operation is exact integer
    math.
  - **Error handling:** overflow saturates (a budget-capped layout cannot
    legitimately reach ±2^31/64 points ≈ ±33M pt; saturation is the
    never-panic backstop).
  - **Tests:** `from_pt_rounds_half_away`: 0.5/64 pt boundary cases;
    `scale_font_units_matches_reference`: known upem 1000, size 16 pt →
    exact expected i32 for several unit values, including negatives;
    `saturating_never_wraps`: i32::MAX add → clamps.
  - **Verify:** `cd core && cargo test -p inkuna-engine fixed` → green.

- [ ] **Task 3.2 — repo-level `assets/fonts/`**
  - **Files:** Create `assets/fonts/` at the repo root: copy the five files
    from `apps/ios/Inkuna/Fonts/` (`NotoSans.ttf`, `NotoSans-Italic.ttf`,
    `NotoSerif.ttf`, `NotoSerif-Italic.ttf`, `OFL.txt`) — the shell copies
    in `apps/ios/Inkuna/Fonts/` and
    `apps/android/app/src/main/assets/fonts/` stay untouched until plan 02
    rewires bundling — then ADD the new faces below.
  - **Behavior:** New faces, all from official Noto releases
    (fonts.google.com / github.com/notofonts, latest release at
    implementation time — never a random mirror):
    1. **Latin bold statics** from the same Noto Sans/Serif family releases
       as the existing four: `NotoSans-Bold.ttf`, `NotoSans-BoldItalic.ttf`,
       `NotoSerif-Bold.ttf`, `NotoSerif-BoldItalic.ttf` — required because
       the engine honors `font-weight`/`reading_bold` and cannot synthesize
       bold.
    2. **Noto CJK Serif + Sans** (spec §4, assumption A3 — packaging decided
       here, at implementation, against current github.com/notofonts/noto-cjk
       releases, and recorded): decision procedure, in order of preference —
       (a) the language-specific **OTC** per family covering SC/TC/JP/KR
       in one file (e.g. `NotoSerifCJK-Regular.ttc` + bold, and the Sans
       equivalents) IF `ttf-parser`/`rustybuzz` opens each collection face
       by index (prove with a loader smoke test before committing); else
       (b) per-region static OTFs for SC/TC/JP/KR, Regular + Bold weights.
       Variable-weight files are used only if option (a)/(b) files are
       variable by default in the current release AND `rustybuzz` shaping
       with an explicit `wght` coordinate works in the smoke test —
       otherwise prefer statics (fewer moving parts; the registry's `axes`
       field carries whichever is chosen losslessly).
    3. **`NotoSansSymbols2-Regular.ttf`** — the spec's fallback chain ends
       at a symbol face before `.notdef`.
    Record in the commit message: exact files, release tags, byte sizes,
    and the (a)/(b) choice with the smoke-test evidence. Update `OFL.txt`
    only if the new downloads ship a differing license text (they are all
    OFL 1.1; keep one copy). Total size lands in the commit message too —
    the app-size cost is owner-accepted (spec §4).
  - **Error handling:** n/a (assets).
  - **Tests:** none here; 3.3's registry tests load every file and are the
    gate.
  - **Verify:** `ls assets/fonts` shows the four originals + OFL.txt + the
    additions; `git lfs` is NOT used (plain files, matching the existing
    in-repo TTFs).

- [ ] **Task 3.3 — font registry (`fonts/`)**
  - **Files:** Create `core/crates/inkuna-engine/src/fonts/{mod.rs,
    registry.rs, registry_tests.rs}` · Modify Cargo.toml (add `rustybuzz`,
    latest stable; it brings `ttf-parser`).
  - **Behavior:**
    ```rust
    pub struct FontAxis { pub tag: String, pub value: f64 }
    pub struct FontEntry { pub id: u32, pub file_path: String, // absolute
        pub collection_index: u32, pub axes: Vec<FontAxis> }
    pub enum FaceRole { Reading, Cjk, Symbols }
    pub struct FontRegistry { /* private: entries + loaded faces */ }
    impl FontRegistry {
        pub fn load(font_dir: &Path) -> Result<Arc<FontRegistry>, EngineError>;
        pub fn entries(&self) -> Vec<FontEntry>;   // for the FFI
        pub fn face(&self, id: u32) -> &LoadedFace; // panic-free: ids are
                                                    // registry-issued
        pub fn select(&self, family: FontFamily, style: FontStyle,
                      weight: FontWeight) -> u32;   // reading-face id
        pub fn cjk(&self, lang: Option<&str>, style_serif: bool,
                   weight: FontWeight) -> u32;
        pub fn symbols(&self) -> u32;
    }
    pub enum FontFamily { Serif, Sans }
    pub struct LoadedFace { pub data: Arc<Vec<u8>>, pub upem: u16,
        /* + cached rustybuzz::Face construction per call site — rustybuzz
           Face borrows; store data and rebuild Face cheaply, or use
           self_cell if profiling demands; start with rebuild-per-shape */ }
    ```
    Ids are assigned in a fixed, documented order at load (stable across
    platforms because the file set is fixed): 0 NotoSerif, 1
    NotoSerif-Italic, 2 NotoSerif-Bold, 3 NotoSerif-BoldItalic, 4 NotoSans,
    5 NotoSans-Italic, 6 NotoSans-Bold, 7 NotoSans-BoldItalic, then the CJK
    faces in the order (Serif before Sans; SC, TC, JP, KR; Regular before
    Bold) as collection entries or files per the 3.2 packaging, then
    Symbols last. `file_path` in entries is the absolute path under the
    registry's `font_dir` (shells rebuild platform fonts from it —
    overview `FontEntry` contract). CJK region selection: `cjk(lang, …)`
    maps BCP-47 primary/script — `ja*`→JP, `ko*`→KR,
    `zh-Hant`/`zh-TW`/`zh-HK`→TC, all else (incl. `zh`, `zh-Hans`, None)
    →SC. Font bytes are read fully into `Arc<Vec<u8>>` (no mmap), cached
    per file; `entries()` never forces byte loads.
  - **Error handling:** a missing/unparsable REQUIRED file at `load` →
    `EngineError::UnsupportedContent { detail: "font missing: <name>" }`
    listing the file — sessions cannot open without the full set (fail
    fast; the shells bundle the directory wholesale). `load` reads and
    parses every file eagerly (ttf-parser `Face::parse` per collection
    index), so per-face failure after a successful `load` is impossible
    by construction; registry load happens once per process, off the UI
    thread (6.6 caches it on `Bookshelf`).
  - **Tests:** `loads_repo_font_set`: `FontRegistry::load` against
    `concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts")` (the
    real shipped bytes — assets are product files, not fixtures, so this
    honors the no-binary-fixtures rule) → every entry parses, upem > 0;
    `id_order_is_stable`: entry ids match the documented order;
    `cjk_region_mapping`: ja/ko/zh-Hant/zh/None → expected face ids;
    `select_bold_italic`: (Serif, Italic, Bold) → id 3;
    `missing_font_dir_fails`: empty tempdir → `UnsupportedContent` naming
    the first missing file.
  - **Verify:** `cd core && cargo test -p inkuna-engine fonts` → green.

- [ ] **Task 3.4 — itemization + shaping (`shape/`)**
  - **Files:** Create `core/crates/inkuna-engine/src/shape/{mod.rs,
    itemize.rs, shape.rs, tests.rs}` · Modify Cargo.toml (add
    `unicode-script`, `unicode-bidi`, latest stable).
  - **Behavior:** `itemize.rs`: `pub fn itemize(text: &str, base_rtl: bool)
    -> Vec<Item>` where `pub struct Item { pub range: Range<usize> /* byte
    range into the input */, pub script: unicode_script::Script,
    pub bidi_level: u8 }` — UBA levels via `unicode-bidi` (paragraph base
    level from `Direction`), then script itemization with common/inherited
    characters merged into the adjacent real script (preceding run wins;
    leading commons join the following run). `shape.rs`:
    ```rust
    pub struct Glyph { pub glyph_id: u16, pub cluster: u32, // char offset
                                     // into the shaped slice's chars
        pub advance: Fx, pub offset_x: Fx, pub offset_y: Fx }
    pub struct ShapedRun { pub font_id: u32, pub size: Fx,
        pub glyphs: Vec<Glyph>, pub bidi_level: u8,
        pub orientation: RunOrientation, // Upright | SidewaysRotated
        pub style: RunStyle }            // FontStyle+FontWeight+is_ruby
    pub enum RunOrientation { Upright, SidewaysRotated }
    pub fn shape_text(text: &str, ctx: &ShapeContext) -> Vec<ShapedRun>;
    pub struct ShapeContext<'a> { pub fonts: &'a FontRegistry,
        pub family: FontFamily, pub font_style: FontStyle,
        pub font_weight: FontWeight, pub size: Fx,
        pub letter_spacing: Fx, pub word_spacing: Fx,
        pub lang: Option<&'a str>, pub vertical: bool,
        pub base_rtl: bool }
    ```
    Font selection per item, the explicit fallback chain (spec §2): the
    reading face (`select(family, style, weight)`); any cluster whose
    characters have no glyph there re-shapes against the CJK face
    (`cjk(lang, family==Serif, weight)`); still missing → symbols face;
    still missing → the reading face's `.notdef` (glyph 0) — text is never
    dropped. Missing-glyph detection: shape, split the run at clusters
    rustybuzz maps to glyph 0, re-shape those subranges down the chain
    (cluster-granular, so ligatures never straddle fonts). Advances scale
    via `Fx::scale_font_units`; `letter_spacing` adds to every inter-cluster
    advance, `word_spacing` to space glyphs (em values resolved against
    `size` at call time by the caller into `Fx`). Determinism: rustybuzz is
    pure Rust over fixed bytes — same output everywhere; no HashMap
    iteration in any path that orders output.
  - **Error handling:** shaping is infallible past registry load; an empty
    input yields an empty vec.
  - **Tests:** `latin_shapes_with_reading_face`: "Hello" → one run, font 0
    (Serif Regular), advances > 0; `cjk_falls_back_to_cjk_face`: "汉字"
    with Serif family → run with a CJK face id, no glyph 0;
    `mixed_script_splits_runs`: "abc汉def" → ≥3 runs, cluster offsets
    partition the chars; `bidi_levels_split`: Hebrew+Latin → distinct
    levels, RTL run level odd; `unknown_char_reaches_notdef`: U+10FFFD →
    glyph 0 retained, advance ≥ 0; `letter_spacing_adds_per_cluster`:
    advance delta equals the spacing; `bold_selects_bold_face`: weight
    Bold → font id 2 (Serif Bold).
  - **Verify:** `cd core && cargo test -p inkuna-engine shape` → green.

- [ ] **Task 3.5 — vertical mode + ruby runs**
  - **Files:** Create `core/crates/inkuna-engine/src/shape/{vertical.rs,
    ruby.rs}` · Modify `src/shape/{mod.rs, shape.rs, tests.rs}`.
  - **Behavior:** `vertical.rs`: when `ShapeContext.vertical` is true,
    shape with rustybuzz features `vert` + `vrt2` and direction
    top-to-bottom; advances are vertical advances (still `Fx` along the
    line axis — M4's line layout is axis-agnostic: "inline advance" +
    "block extent"). Character classes that stay upright: Han, Hiragana,
    Katakana, Hangul, CJK symbols/punctuation (vertical presentation forms
    applied by the font via `vert`), full-width forms. Latin/digit/other
    horizontal-script runs get `orientation = SidewaysRotated`: shaped
    HORIZONTALLY with the same face, advance consumed along the vertical
    line axis, and the rotation flag carried through the display list for
    the shells' per-run transform (spec A6). `ruby.rs`: `pub struct
    RubyRun { pub base: Vec<ShapedRun>, pub annotation: Vec<ShapedRun>,
    pub position: RubyPosition }`; `pub fn shape_ruby(base_text: &str,
    rt_text: &str, ctx: &ShapeContext, ruby_scale: f64) -> RubyRun` —
    annotation shaped at `size.mul_ratio(scale_num, scale_den)` where the
    scale is `Typography::ruby_scale` expressed as an exact ratio (store
    ruby_scale as a rational const, e.g. 1/2, in 2.3's transcription —
    adjust 2.3 if it landed as f64: `Typography.ruby_scale` becomes
    `(u32, u32)`), marked `is_ruby` in `RunStyle`. Annotation text is
    display-only — its glyphs carry `cluster` values pointing at the BASE
    run's char offsets (first base char), so selection/hit-testing land on
    the base (spec §2: "selectable as its base").
  - **Error handling:** empty rt → `RubyRun` with empty annotation (renders
    as plain base).
  - **Tests:** `vertical_cjk_uses_vert_feature`: vertical "「漢」" → upright
    orientation, vertical punctuation glyph differs from horizontal shape
    of the same char (assert glyph ids differ between vertical and
    horizontal shaping of 「); `latin_in_vertical_is_sideways`: vertical
    "abc漢" → Latin run `SidewaysRotated`, CJK run `Upright`;
    `ruby_annotation_scaled_and_mapped`: base 漢字 rt かんじ → annotation
    size == base size × ruby scale, all annotation clusters == base start
    offset.
  - **Verify:** `cd core && cargo test -p inkuna-engine shape` → green.

## Movement 4: Breaking & pagination

Lines and pages, all in fixed-point, progressively emitted. **Review rule
for this movement:** every extent/position/advance is `Fx`; an `f32`/`f64`
in `layout/` or `paginate/` outside `Fx::from_pt`/`to_f32` call sites is a
defect.
**Depends on:** Movement 3.

- [ ] **Task 4.1 — line breaking + justification (`layout/`)**
  - **Files:** Create `core/crates/inkuna-engine/src/layout/{mod.rs,
    lines.rs, justify.rs, tests.rs}` · Modify Cargo.toml (add
    `unicode-linebreak`, latest stable).
  - **Behavior:** `lines.rs`:
    ```rust
    pub struct Line { pub runs: Vec<PositionedRun>,
        pub char_range: Range<u64>,   // canonical projection offsets
        pub inline_extent: Fx,        // used width (height in vertical)
        pub ascent: Fx, pub descent: Fx,
        pub ruby_extent: Fx }         // extra block extent for ruby, 0 if none
    pub struct PositionedRun { pub run: ShapedRun,
        pub inline_offset: Fx,        // from line start, visual order
        pub char_range: Range<u64> }
    pub fn break_paragraph(p: &ShapedParagraph, width: Fx,
                           opts: &LineOptions) -> Vec<Line>;
    pub struct LineOptions { pub justify: TextAlign, pub last_line_ragged:
        bool /* always true */, pub max_lines: u32 }
    ```
    (`ShapedParagraph` is this movement's glue type — the paragraph's
    projection char range, its shaped/ruby runs in logical order, and its
    resolved style — assembled by 4.2's block walk and defined in
    `lines.rs`, since definitions never live in `mod.rs`.) Break opportunities from `unicode-linebreak` over the
    paragraph's projection text (UAX-14 encodes CJK kinsoku); no
    hyphenation. A single unbreakable segment wider than `width` overflows
    (clipped by the page box) rather than force-breaking — cap via
    `MAX_LINES_PER_PARAGRAPH: u32 = 8_192`: excess lines are dropped and
    the paragraph marked truncated (budget guard, spec §12). Bidi
    reordering: runs within a line are reordered to visual order by bidi
    level (UBA L2) and `inline_offset` assigned in strictly ascending
    accumulation order — one pass, left to right (or top-to-bottom
    vertical). `justify.rs`: for `TextAlign::Justify`, distribute the
    deficit `width - natural_extent`: inter-word (space-glyph) stretch for
    lines containing spaces; lines with no spaces but ≥2 CJK glyphs
    stretch inter-character gaps between CJK clusters; mixed lines: spaces
    first, then CJK gaps if spaces alone would each grow beyond 0.5 em —
    remainder distributed one 1/64 unit at a time to the first N gaps
    (deterministic — never divided as float). Last line of a paragraph and
    lines ending in a forced break stay ragged (aligned per Start/Center/
    End). `Center`/`End` set `inline_offset`s accordingly.
  - **Error handling:** infallible; truncation flags propagate.
  - **Tests:** `kinsoku_no_leading_close_bracket`: vertical-agnostic CJK
    text where 」 would start a line naturally → UAX-14 keeps it with the
    previous char; `justify_latin_stretches_spaces`: forced two-line
    paragraph → line 0 `inline_extent == width` exactly, space advances
    grew, last line ragged; `justify_cjk_inter_character`: pure-CJK
    paragraph → line 0 exactly `width`, gap delta uniform ±1/64;
    `remainder_distribution_deterministic`: same input twice → identical
    `inline_offset` vectors; `bidi_visual_reorder`: RTL-embedded Latin →
    visual order differs from logical, offsets ascending;
    `char_ranges_partition_paragraph`: concatenated line char_ranges ==
    paragraph range, no gaps/overlaps (CJK fixture).
  - **Verify:** `cd core && cargo test -p inkuna-engine layout` → green.

- [ ] **Task 4.2 — block flow + progressive pagination (`paginate/`)**
  - **Files:** Create `core/crates/inkuna-engine/src/paginate/{mod.rs,
    blocks.rs, pages.rs, tests.rs}`.
  - **Behavior:** `blocks.rs` walks the `StyledDocument` body in document
    order producing a block sequence: paragraphs (p/div/li/dt/dd with
    text), headings (h1–h6, keep-rules), blockquote (extra inline inset:
    1.5 em both sides), pre/code (no justification, `Start` align;
    interior whitespace renders collapsed in v1 because the projection
    already collapsed it — document this in a code comment), list items
    (no markers,
    per projection), hr (a `Rule` decoration block), images, table
    elements degraded sequentially (each tr/caption a paragraph block).
    Each block assembles its `ShapedParagraph` (4.1) by slicing the
    projection spans under its subtree and shaping per 3.4/3.5 with the
    resolved style + `Typography` sizes (heading scale, bold). `pages.rs`:
    ```rust
    pub struct LaidPage { pub index: u32,
        pub char_range: Range<u64>,
        pub lines: Vec<PlacedLine>,     // line + block position
        pub images: Vec<PlacedImage>,
        pub decorations: Vec<PlacedDecoration> }
    pub struct ChapterLayoutResult { pub page_count: u32,
        pub char_len: u64, pub truncated: bool }
    pub fn paginate(input: &ChapterInput,
                    emit: &mut dyn FnMut(LaidPage))
        -> Result<ChapterLayoutResult, EngineError>;
    pub struct ChapterInput<'a> { pub styled: &'a StyledDocument<'a>,
        pub projection: &'a Projection, pub fonts: &'a FontRegistry,
        pub typography: &'a Typography, pub settings: &'a LayoutSettings,
        pub viewport: FxSize, pub lang: Option<&'a str> }
    pub struct FxSize { pub width: Fx, pub height: Fx }
    ```
    Page frame = viewport minus `reading_margins` on the inline sides and
    a transcribed vertical inset (take the top/bottom padding from the 2.3
    transcription; if the shells had none beyond margins, use
    `reading_margins` on all four sides). Pages are emitted via `emit` as
    their lines complete — progressive (spec §2): the callback fires the
    moment a page is full, long before the chapter finishes. Page breaks
    only at line boundaries. Widow/orphan min-2 — the rule: a paragraph
    split must leave ≥2 lines on BOTH sides of the break; push the break
    earlier when it would not (consequence: 2- and 3-line paragraphs never
    split). Heading keep: a heading
    block plus the first 2 lines of the following block must fit, else the
    heading moves to the next page. Line stacking: baseline advances by
    `line_height` (from `Typography`, as `Fx::from_pt` once per layout) +
    `ruby_extent`; paragraph spacing/indent per `Typography`.
    `MAX_PAGES_PER_CHAPTER: u32 = 16_384` — exceeding it stops layout,
    marks the result truncated (§12 prefix semantics; `char_range`s cover
    exactly the emitted prefix). Vertical-rl: the axis swap — lines are
    columns, inline axis is vertical (height-bounded), block axis advances
    right-to-left; `LaidPage` geometry is emitted in normal page
    coordinates (x from the right edge). RTL progression does NOT touch
    geometry — it is a flag surfaced in M5's chapter geometry only (the
    pager stays purely geometric, spec §2).
  - **Error handling:** an `UnsupportedContent` from parse never reaches
    here (session catches it); `paginate` itself errors only on internal
    budget trips that cannot degrade — none currently; truncation is the
    degradation path.
  - **Tests:** `progressive_emission_order`: multi-page CJK chapter →
    `emit` called page_count times with ascending contiguous
    `char_range`s partitioning `0..char_len`;
    `every_char_on_exactly_one_page` (the §14 property, horizontal +
    vertical fixtures); `widow_orphan_min_two`: crafted paragraph
    straddling a page → both sides ≥2 lines, and a 3-line paragraph never
    splits; `heading_keeps_two_lines`: heading at page bottom → moved;
    `vertical_rl_axis_swap`: `CJK_VERTICAL_RUBY_DOC` at a wide-short
    viewport → columns right-to-left (first line x > last line x), page
    count > 1; `page_cap_truncates`: absurd generated chapter → truncated
    result, `char_len` == retained prefix length;
    `blockquote_inset_narrower_lines`.
  - **Verify:** `cd core && cargo test -p inkuna-engine paginate` → green.

- [ ] **Task 4.3 — image placement (`paginate/images.rs`)**
  - **Files:** Create `core/crates/inkuna-engine/src/paginate/images.rs` ·
    Modify `src/paginate/{mod.rs, blocks.rs, pages.rs, tests.rs}` ·
    Modify Cargo.toml (add `imagesize`, latest stable).
  - **Behavior:** `pub struct PlacedImage { pub href: String, // normalized
    package-root-relative, resolved via inkuna-content against the
    resource's own path
    pub frame: FxRect }` with `pub struct FxRect { pub x: Fx, pub y: Fx,
    pub w: Fx, pub h: Fx }` (defined in `pages.rs`, used crate-wide).
    Intrinsic dimensions are header-sniffed in core via `imagesize` over
    bytes from `inkuna_content::read_resource` — never decoded. Scaling:
    fit within the content box preserving aspect ratio, never upscaled
    beyond intrinsic size at 1× (intrinsic px treated as layout points).
    Dimension caps (spec §12): sniffed width or height > 16_384 px, or
    unsniffable bytes/missing resource → the fixed placeholder box, 160 ×
    160 pt (centered on the inline axis), still emitted with the href so
    the shell can show its broken-image treatment. An image taller than
    the remaining page space moves to the next page; taller than a whole
    page → scaled to page height. Images occupy the block flow between
    lines; an image contributes no chars (its position in `char_range` is
    the boundary offset between surrounding text). Vertical-rl: the
    content box axes swap with the flow (frame still emitted in page
    coordinates).
  - **Error handling:** every image failure (missing entry, over budget,
    unsniffable) degrades to the placeholder box — never an error, never a
    dropped block; log at `debug`.
  - **Tests:** `image_scaled_to_fit_never_upscaled`: 100×50 intrinsic in a
    400 pt box → 100×50; 1000×500 → 400×200 (with margins accounted);
    `unsniffable_gets_placeholder`: garbage bytes → 160×160;
    `oversize_dimensions_get_placeholder`: crafted PNG header claiming
    60_000 px → placeholder; `image_breaks_to_next_page`: image after
    near-full page → lands on next page's top; `image_href_normalized`:
    `<img src="../images/a%20b.png">` from `OEBPS/text/ch1.xhtml` →
    `OEBPS/images/a b.png`.
  - **Verify:** `cd core && cargo test -p inkuna-engine paginate` → green.

## Movement 5: Sessions, display lists, digests

The engine's public face: display-list emission with all query maps, the
canonical serialization + blake3 digest that proves cross-platform identity,
the background-threaded `EngineSession`, corpus extraction, and the golden /
property / adversarial test corpus.
**Depends on:** Movement 4.

- [ ] **Task 5.1 — display lists, query maps, a11y (`display/`)**
  - **Files:** Create `core/crates/inkuna-engine/src/display/{mod.rs,
    list.rs, maps.rs, a11y.rs, tests.rs}`.
  - **Behavior:** `list.rs` — the engine-native records the FFI mirrors 1:1
    in M6 (shapes are the overview's, restated):
    ```rust
    pub enum ColorRole { Text, Secondary, Link }
    pub struct GlyphRun { pub font_id: u32, pub size: f64,
        pub color_role: ColorRole, pub glyph_ids: Vec<u16>,
        pub positions: Vec<f32>,   // x,y interleaved, len = 2 × glyphs,
                                   // page coordinates, layout points at 1×
        pub orientation: RunOrientation }
    pub enum DecorationKind { Rule, Underline }
    pub struct Decoration { pub kind: DecorationKind, pub rect: Rect,
        pub color_role: ColorRole }
    pub struct Rect { pub x: f64, pub y: f64, pub width: f64,
        pub height: f64 }
    pub struct ImagePlacement { pub href: String, pub rect: Rect }
    pub struct LinkRegion { pub rect: Rect, pub target: String }
    pub enum A11yRole { Body, Heading, Link }
    pub struct A11yBlock { pub text: String, pub rect: Rect,
        pub lang: Option<String>, pub is_link: bool, pub role: A11yRole }
    pub struct PageDisplayList { pub generation: u64,
        pub glyph_runs: Vec<GlyphRun>, pub images: Vec<ImagePlacement>,
        pub decorations: Vec<Decoration>, pub links: Vec<LinkRegion>,
        pub a11y: Vec<A11yBlock> }
    pub fn build_page(page: &LaidPage, ctx: &DisplayContext)
        -> (PageDisplayList, PageMaps);
    ```
    Conversion `Fx → f32/f64` happens here and only here (`Fx::to_f32`).
    Color roles: `Link` for glyphs inside `a`-with-href subtrees,
    `Secondary` for ruby annotation runs and the truncation rule, `Text`
    otherwise. Underline decorations under link runs; `hr` → `Rule`.
    Decoration roles are core-assigned (overview contract): `Rule` →
    `Secondary`; `Underline` → `Link` when inside a link region, else
    `Text` — shells never infer decoration colors. Link
    targets: internal hrefs normalized package-root-relative (fragment
    kept); external `http(s):`/`mailto:` kept verbatim. `maps.rs` —
    `pub struct PageMaps { pub char_range: Range<u64>, pub lines:
    Vec<LineGeom> }` with `pub struct LineGeom { pub char_range:
    Range<u64>, pub band: FxRect, // full-bleed hit band across the frame
    pub cells: Vec<(u64 /*char*/, Fx /*inline start*/, Fx /*inline
    extent*/)> }` — the bidirectional locate/hit-test data: char → rect
    (for `selection_rects`) and point → char (for `hit_test`, nearest-cell
    within nearest-band, so any point on a page resolves; an empty page
    resolves to `char_range.start`). Writing-mode awareness lives in
    `LineGeom` band orientation. `a11y.rs`: one `A11yBlock` per block-level
    element with text on the page (a block split across pages contributes
    its on-page lines' text), text taken from the canonical projection
    slice, ruby readings appended parenthetically per block ("漢字（かんじ）"
    — build from `RubyRun` annotations in order), `lang` = nearest ancestor
    `lang`/`xml:lang` else `None`, role Heading for h1–h6, Link + `is_link`
    for standalone link blocks, else Body; ordered in logical reading
    order.
  - **Error handling:** infallible transforms of already-validated layout.
  - **Tests:** `positions_interleaved_len`: every run `positions.len() ==
    2 * glyph_ids.len()`; `link_regions_and_color`: `LATIN_DOC` link →
    `LinkRegion` with normalized target + `Link` color + underline;
    `ruby_is_secondary_and_parenthetical`: vertical ruby fixture → ruby
    runs `Secondary`, a11y text carries （かんじ）; `a11y_blocks_ordered_with_lang`:
    blocks in reading order, `lang` inherited; `maps_cover_page_chars`:
    union of `LineGeom.char_range` == page `char_range`.
  - **Verify:** `cd core && cargo test -p inkuna-engine display` → green.

- [ ] **Task 5.2 — canonical serialization + digest (`display/digest.rs`)**
  - **Files:** Create `core/crates/inkuna-engine/src/display/digest.rs` ·
    Modify `src/display/{mod.rs, tests.rs}`.
  - **Behavior:** `pub fn page_digest(list: &PageDisplayList) -> String` —
    lowercase blake3 hex (64 chars) of a canonical byte serialization,
    specified exactly (this is the cross-platform parity currency, spec
    §14): all integers little-endian fixed width; `u64 generation` first;
    then each vector as `u32 len` followed by elements in order; strings
    as `u64 len + UTF-8 bytes`; `f32`/`f64` as IEEE-754 `to_bits()` LE
    (values originate in `Fx::to_f32`, so bit patterns are identical
    cross-platform by construction); enums as `u8` discriminants in
    declaration order; fields within each record in declaration order.
    Vectors are already deterministically ordered by construction (no
    hash-ordered collections anywhere in `display/` — assert in review).
    Also `pub fn digest_hex(bytes: &[u8]) -> String` helper. The FFI's
    `page_digest` (M6) and the golden tests (5.5) both call this.
  - **Error handling:** infallible.
  - **Tests:** `digest_stable_across_calls`: same list twice → same hex;
    `digest_sensitive_to_any_field`: mutate one glyph position bit / one
    color role → different hex; `digest_reference_vector`: a tiny
    hand-built list → the exact hex committed in the test (guards the
    serialization spec itself against refactors).
  - **Verify:** `cd core && cargo test -p inkuna-engine display` → green.

- [ ] **Task 5.3 — `EngineSession` (`session/`)**
  - **Files:** Create `core/crates/inkuna-engine/src/session/{mod.rs,
    session.rs, worker.rs, cache.rs, tests.rs}` · Modify Cargo.toml (add
    `icu_segmenter`, latest stable) and `src/lib.rs` re-exports.
  - **Behavior:** The engine-native session `inkuna-ffi` wraps in M6.
    ```rust
    pub struct Viewport { pub width: f64, pub height: f64 }
    pub struct ChapterGeometry { pub generation: u64, pub page_count: u32,
        pub char_range: CharRange, pub writing_mode: WritingMode,
        pub rtl_progression: bool, pub truncated: bool }
    // truncated == the resource hit a parse/layout budget and rendered
    // only its laid-out prefix (spec §12; shells show a notice, plan 02)
    pub struct CharRange { pub start: u64, pub end: u64 } // end exclusive
    pub struct PageLocation { pub generation: u64, pub spine_idx: u32,
        pub page_idx: u32 }
    pub struct HitResult { pub coordinate: Coordinate,
        pub link_target: Option<String> }
    pub struct SelectionRect { pub rect: Rect,
        pub writing_mode: WritingMode }
    pub trait LayoutEvents: Send + Sync + 'static {
        fn first_page_ready(&self, generation: u64, spine_idx: u32);
        fn chapter_ready(&self, generation: u64, spine_idx: u32,
                         page_count: u32);
    }
    pub struct EngineSession { /* private */ }
    impl EngineSession {
        pub fn open(epub_path: &Path, fonts: Arc<FontRegistry>,
            viewport: Viewport, settings: LayoutSettings,
            lang: Option<String>, opening_chapter: u32,
            events: Arc<dyn LayoutEvents>)
            -> Result<Arc<EngineSession>, EngineError>;
        pub fn close(&self);
        // sync, cache-only, non-blocking (spec §3):
        pub fn chapter(&self, spine_idx: u32)
            -> Result<ChapterGeometry, EngineError>;      // NotReady
        pub fn page(&self, spine_idx: u32, page_idx: u32)
            -> Result<PageDisplayList, EngineError>;
        pub fn page_digest(&self, spine_idx: u32, page_idx: u32)
            -> Result<String, EngineError>;
        pub fn is_ready(&self, spine_idx: u32) -> bool;
        pub fn locate(&self, c: Coordinate)
            -> Result<PageLocation, EngineError>;
        pub fn locate_href(&self, href: &str, fragment: Option<&str>)
            -> Result<Coordinate, EngineError>;           // AnchorNotFound
        pub fn hit_test(&self, spine_idx: u32, page_idx: u32,
            x: f64, y: f64) -> Result<HitResult, EngineError>;
        pub fn selection_rects(&self, spine_idx: u32, range: CharRange)
            -> Result<Vec<SelectionRect>, EngineError>;
        pub fn word_at(&self, c: Coordinate)
            -> Result<CharRange, EngineError>;
        pub fn text_range(&self, spine_idx: u32, range: CharRange)
            -> Result<String, EngineError>;
        pub fn match_rects(&self, spine_idx: u32, char_offset: u64,
            len: u64) -> Result<Vec<SelectionRect>, EngineError>;
        pub fn accessibility_blocks(&self, spine_idx: u32, page_idx: u32)
            -> Result<Vec<A11yBlock>, EngineError>;
        pub fn page_char_range(&self, spine_idx: u32, page_idx: u32)
            -> Result<CharRange, EngineError>; // from PageMaps.char_range
        pub fn spine_len(&self) -> u32;
        // async-shaped (heavy; called from blocking pool in FFI):
        pub fn update_layout(&self, viewport: Viewport,
            settings: LayoutSettings);
        pub fn resource(&self, href: &str)
            -> Result<Vec<u8>, EngineError>;
    }
    ```
    `open` reads the package via `inkuna_content::read_package`; a
    `RenditionLayout::PrePaginated` package → `EngineError::
    UnsupportedContent { detail: "fixed-layout" }` (spec §11). It builds
    the spine model, then spawns ONE worker thread (`worker.rs`) owning a
    priority work queue: opening chapter first, then spine neighbors
    (±1, ±2 …) of the most recently queried chapter, then explicit
    schedules from cache misses. Per chapter the worker: reads bytes
    (`read_resource`), parses (2.2), loads + budgets linked stylesheets
    (each via `read_resource`, resolved with `resolve_relative` against
    the chapter's path; unreadable sheet → skipped, logged), resolves
    style (2.4), projects (2.5), paginates (4.2) — publishing each
    `LaidPage`'s `(PageDisplayList, PageMaps)` into the chapter's cache
    slot AS EMITTED, firing `first_page_ready` on page 0 and
    `chapter_ready` on completion. A chapter whose parse fails closed is
    cached as `Failed(UnsupportedContent)` — `chapter()`/`page()` on it
    return that error (the shell renders its localized placeholder page;
    plan 02). Cache (`cache.rs`): LRU, capacity 5 complete chapters
    (spec §2), keyed `(spine_idx, generation)`; the current chapter is
    never evicted. Generations: `update_layout` bumps a monotonic
    `AtomicU64`, records the new `(viewport, settings)`, clears the
    queue, invalidates the cache, and schedules the current chapter first
    (spec §3); the worker checks the generation before publishing and
    abandons stale results. Contract detail (documented on the methods):
    `page`/`page_digest`/`accessibility_blocks`/`page_char_range` succeed
    as soon as THAT page is published (progressive — page 0 available
    before the chapter completes); `chapter`, `locate`, `hit_test`, `selection_rects`,
    `match_rects`, `text_range`, `word_at` require the COMPLETE chapter;
    any cache miss returns `NotReady` after scheduling the chapter at
    queue front — never blocks. `locate` clamps `char_offset ≥ char_len`
    to the last page; `locate_href` resolves via `inkuna-content`
    normalization + the per-resource anchor map. Fragment-free lookups
    resolve from the spine model alone (offset 0 of the resolved spine
    index) and never wait; a fragment lookup needs that chapter's anchor
    map, which is computed at layout — an un-laid chapter schedules and
    returns `NotReady`. `word_at`:
    `icu_segmenter::WordSegmenter::new_auto()` over the chapter's
    projection (segmenter built once per session, reused). `text_range`
    slices the projection by chars. `match_rects(s, off, len)` ==
    `selection_rects(s, CharRange{off, off+len})`. `resource` returns
    budget-capped bytes (content's per-entry cap). `close()` sets a
    closed flag, drains the queue, joins the worker; all sync methods on
    a closed session return `NotReady`; `Drop` calls `close`.
    Determinism note in module doc: layout inputs are `(resource bytes,
    viewport, settings fingerprint, font set, engine version)` — nothing
    thread-timing-dependent ever reaches page content.
  - **Error handling:** as specified per method above: `NotReady`
    (cache-miss/incomplete/closed), `AnchorNotFound { detail: href }`
    (unresolvable href or unknown fragment id, after the chapter is
    laid out), `UnsupportedContent` (fixed-layout at open; failed-parse
    chapters), `Io`/`Content` from archive reads at `open`/`resource`.
    Worker panics are converted: the worker runs each chapter under
    `std::panic::catch_unwind` and caches `Failed` — a panic in layout
    never kills the session (panics remain forbidden, this is the
    backstop; log at `error`).
  - **Tests:** (using a `TestEvents` recorder implementing `LayoutEvents`
    with channels) `first_page_before_chapter_ready`: multi-page chapter
    → `first_page_ready` observed, `page(s,0)` succeeds while
    `is_ready(s)` may still be false, then `chapter_ready` with final
    count; `not_ready_then_ready`: `chapter(2)` before layout →
    `NotReady`, after `chapter_ready(2)` → geometry;
    `update_layout_bumps_generation_and_invalidates`: change viewport →
    new geometry has new generation, old generation never re-observed in
    events after the bump; `locate_hit_test_round_trip`: for a grid of
    page points, `locate(hit_test(p).coordinate).page_idx == page_idx`
    (the §14 property, horizontal + vertical fixtures);
    `selection_rects_cover_range`: CJK range → rects nonempty, union
    spans the range's cells, `writing_mode` correct on the vertical
    fixture; `locate_href_fragment`: TOC href with `#anchor` → coordinate
    == anchor offset; unknown fragment → `AnchorNotFound`;
    `word_at_cjk`: offset inside 漢字 word → range covering the
    segmenter's word; `fixed_layout_rejected_at_open`: `pre_paginated`
    fixture → `UnsupportedContent`; `failed_chapter_scoped`: spine of
    [good, garbage, good] → chapters 0/2 lay out, chapter 1 returns
    `UnsupportedContent`, book usable (§12 smallest-unit degradation);
    `close_makes_everything_not_ready`;
    `coordinates_survive_update_layout`: locate the same coordinate
    before/after a viewport change → both succeed, page text at the
    located pages contains the coordinate's char.
  - **Verify:** `cd core && cargo test -p inkuna-engine session` → green.

- [ ] **Task 5.4 — corpus extraction (`corpus.rs`)**
  - **Files:** Create `core/crates/inkuna-engine/src/{corpus.rs,
    corpus_tests.rs}`.
  - **Behavior:** The single function import (M6.3) and reconcile (M6.4)
    call so search offsets and layout offsets index the same stream by
    construction (spec §2):
    ```rust
    pub fn extract_corpus(epub_path: &Path, spine: &[String],
        max_total_bytes: usize) -> Vec<Option<String>>
    ```
    Per spine href, sequentially: read → parse → load linked/inline CSS
    (same loading rules as 5.3's worker) → resolve → project → the
    projection's `text`. `None` for resources that fail closed
    (malformed, missing, unreadable). The aggregate budget mirrors the
    old `extract_spine_text` contract: retained text bytes are charged in
    spine order against `max_total_bytes`; a resource that would exceed
    it yields `None` for it and every later resource (deterministic — a
    function of the publication alone, no scheduling dependence; the old
    parallel implementation's budget subtleties die with it). Duplicate
    spine hrefs re-use the first extraction's result (clone). Module doc:
    "THE canonical projection at rest — `resource_text` rows are exactly
    this output; any change here is a corpus-versioning event."
  - **Error handling:** never errors — per-resource `None` degradation
    only, matching the import pipeline's existing expectations.
  - **Tests:** `corpus_equals_session_projection`: for the CJK fixture
    book, `extract_corpus` text == the projection the session lays out
    (compare via `text_range` over the full chapter) — the by-construction
    identity, asserted; `budget_stops_in_spine_order`: tiny budget → early
    resources Some, later None; `malformed_resource_is_none_others_fine`;
    `duplicate_spine_entries_alias`.
  - **Verify:** `cd core && cargo test -p inkuna-engine corpus` → green.

- [ ] **Task 5.5 — golden corpus, determinism digests, adversarial suite,
  parity fixture export**
  - **Files:** Create `core/crates/inkuna-engine/golden/` (committed text
    files: `<fixture>.pages.txt` — per-chapter page counts + per-page
    digests; `<fixture>.page0.txt` — the canonical-serialization dump of
    chapter 0 page 0 rendered as text lines; `<fixture>.projection.txt`)
    · Create `core/crates/inkuna-engine/src/golden_tests.rs` (declared
    `#[cfg(test)] #[path = "golden_tests.rs"] mod golden_tests;` from
    `lib.rs`) · Create
    `core/crates/inkuna-engine/examples/export-parity-fixtures.rs` ·
    Extend `src/test_support.rs` with the fixture roster +
    `INKUNA_UPDATE_GOLDEN` regeneration (gate the roster behind the same
    `test_support` feature so the example can build it).
  - **Behavior:** Fixture roster (spec §14), each built by `EpubBuilder`
    at fixed viewport 390×664 pt, default settings, deterministic
    content: `latin`, `cjk_horizontal`, `cjk_vertical_ruby`, `rtl`
    (RTL progression + Hebrew), `mixed_script`, `image_heavy`,
    `table_degradation`. Test flow per fixture: open an `EngineSession`,
    wait for all chapters (a test-only helper `wait_all(&session,
    timeout)` polling `is_ready` on a channel of events), then assert
    against the committed golden files. Running with env
    `INKUNA_UPDATE_GOLDEN=1` rewrites the golden files instead of
    asserting (documented in the file header comment). These same digests
    are what the shells' debug parity harness compares on devices in plan
    02 — the golden files are the contract artifact. The example
    (overview contract): `cd core && cargo run -p inkuna-engine --example
    export-parity-fixtures --features test_support -- <dir>` writes every
    roster fixture EPUB deterministically into `<dir>` plus
    `manifest.json` — an array of `{ "file": "<name>.epub", "viewport":
    { "width": 390.0, "height": 664.0 }, "settings": { <the exact
    ReaderLayoutSettings field values used by the golden tests> } }` —
    plan 02's device harness imports exactly these files and runs exactly
    these cases; it never invents its own corpus. Adversarial suite
    (spec §12/§14), same file: `budget_bomb_nodes` (300k elements),
    `budget_bomb_attr`, `stylesheet_bomb` (2 MB CSS → dropped-from-end,
    layout succeeds), `deep_nesting` (1000 deep), `absurd_image_header`
    (claims 60k×60k), `malformed_xhtml_recovers`, `zip_entry_bomb`
    (over-cap entry → chapter fails closed, book survives) — each
    asserting the specified degradation (truncated prefix or
    `UnsupportedContent` scoped to the resource) and that the process
    neither panics nor allocates unboundedly (budgets hold: page counts
    and char lengths under the caps).
  - **Error handling:** n/a (tests).
  - **Tests:** named above, plus `digests_match_golden` per fixture — the
    determinism gate `cargo test` runs on every platform it executes on
    (ubuntu CI, this mac): committed digest == computed digest.
  - **Verify:** `cd core && cargo test -p inkuna-engine golden` → green;
    run twice → identical results (determinism smoke).

## Movement 6: ReaderSession FFI, V8 + reconcile, search unification

The engine meets the database and the boundary: content coordinates replace
locators end to end, the corpus is rebaselined onto the canonical projection,
and the full `ReaderSession` surface ships behind regenerated bindings. Both
shells compile at the end (6.8).
**Depends on:** Movement 5.

- [ ] **Task 6.1 — error taxonomy + deleted shell-reporting APIs**
  - **Files:** Modify `core/crates/inkuna-core/src/core/error.rs`,
    `src/lib.rs`, `src/features/progress/{positions.rs, positions_tests.rs,
    writes.rs, mod.rs}`, `src/features/progress/tests.rs` ·
    `core/crates/inkuna-ffi/src/{error.rs, progress.rs, lib.rs}`.
  - **Behavior:** `CoreError` gains variants `NotReady`,
    `UnsupportedContent(String)`, `LayoutBudgetExceeded(String)`,
    `AnchorNotFound(String)` and an exhaustive
    `From<inkuna_engine::EngineError> for CoreError`: `NotReady→NotReady`,
    `UnsupportedContent{detail}→UnsupportedContent`,
    `BudgetExceeded{detail}→LayoutBudgetExceeded`,
    `AnchorNotFound{detail}→AnchorNotFound`, `Io→Io`,
    `Content→` (via the existing ContentError arm). Delete
    `CoreError::InvalidPositionRanges` and everything that constructs it:
    `Library::report_position_ranges` (all of `positions.rs`'s write half
    + its tests), `Library::report_position_count` (in `writes.rs`) —
    synthetic positions are core-computed from 6.3/6.4 on; the
    reading-order-mismatch class those APIs guarded dies with
    shell-reported counts (spec §8). `chapter_position_ranges` STAYS
    (read-only; now fed by core-computed `resource_positions`).
    `ChapterPositionRange` type stays. FFI mirror: `InkunaError` gains
    `NotReady { detail: String }`, `UnsupportedContent { detail: String }`,
    `LayoutBudgetExceeded { detail: String }`, `AnchorNotFound { detail:
    String }` with `From` arms; `InvalidPositionRanges` deleted;
    `ShelfProgress::report_position_ranges` and `report_position_count`
    deleted.
  - **Error handling:** the `From` arms are exhaustive matches — new
    variants on either side become compile errors.
  - **Tests:** `engine_error_maps_to_core` (in a small
    `core/error_tests.rs` sibling if none exists — check; else extend the
    existing db `tests.rs`): each `EngineError` variant → expected
    `CoreError` variant; existing progress tests updated: reporting tests
    deleted with the API, `chapter_position_ranges` tests re-seeded by
    inserting `resource_positions` rows directly (the queries are
    unchanged).
  - **Verify:** `cd core && cargo test --workspace` → green;
    `grep -rn "InvalidPositionRanges\|report_position_ranges\|report_position_count"
    core/crates` → no hits outside migrate.rs comments/git history.

- [ ] **Task 6.2 — migration V8**
  - **Files:** Modify `core/crates/inkuna-core/src/core/db/migrate.rs`,
    `src/core/db/tests.rs`.
  - **Behavior:** `SCHEMA_VERSION` → 8; append (never edit shipped
    entries):
    ```sql
    -- 0008: content coordinates (engine swap). position_spine_idx /
    -- position_char_offset are the canonical-projection coordinate
    -- replacing Readium locator JSON; `locator` columns are retained
    -- as-is until the per-book reconcile pass consumes them (publications
    -- NULLed / bookmarks set to '' after conversion). reconciled_at
    -- stamps a book whose corpus, synthetic positions, and locators have
    -- been rebaselined. Settings-units note (V7 precedent):
    -- reading_margins is reinterpreted from CSS px inside the rendering
    -- web view to engine layout points — numerically identical at 1x,
    -- so stored values carry over unchanged.
    ALTER TABLE publications ADD COLUMN position_spine_idx   INTEGER;
    ALTER TABLE publications ADD COLUMN position_char_offset INTEGER;
    ALTER TABLE publications ADD COLUMN reconciled_at        INTEGER;
    ALTER TABLE bookmarks    ADD COLUMN position_spine_idx   INTEGER;
    ALTER TABLE bookmarks    ADD COLUMN position_char_offset INTEGER;
    ```
    `V8_SQL` const + `7 => tx.execute_batch(V8_SQL)?` arm. Pure SQL,
    append-only; NO `media_type` column (cut by review, spec §8).
  - **Error handling:** the migrate loop's existing transaction semantics.
  - **Tests:** `v8_migrates_from_v7`: open a DB at version 7 with seeded
    publication+bookmark rows → columns exist, NULL, old data intact,
    `user_version == 8`; `v8_fresh_install`: new DB reaches 8 in one run.
  - **Verify:** `cd core && cargo test -p inkuna-core db` → green.

- [ ] **Task 6.3 — import pipeline on the canonical projection**
  - **Files:** Modify
    `core/crates/inkuna-core/src/features/import/pipeline.rs`, its
    `tests.rs` · Delete
    `core/crates/inkuna-core/src/formats/epub/{text.rs, text_tests.rs}`
    and, if nothing else remains, collapse `formats/epub/mod.rs` to the
    `inkuna_content` re-export shims (keep `formats/mod.rs` +
    `format.rs`) · Modify `src/lib.rs` re-exports.
  - **Behavior:** The import pipeline's text extraction switches from
    `extract_spine_text` to
    `inkuna_engine::extract_corpus(path, &spine_hrefs,
    MAX_TOTAL_TEXT_BYTES)` — the const moves to the pipeline (same value,
    32 MiB, same doc comment). In the same import transaction the
    pipeline now also computes and inserts synthetic positions — the core
    invents them now; the "the core never invents page numbers" claim
    lives only in migrate.rs's shipped V3 comment, which is never edited,
    so the new code simply carries its own doc comment stating the new
    contract: per spine resource
    `count = ceil(char_len / 1024).max(1)` where `char_len` is the
    resource's projected char count (0 for `None` corpus entries → count
    1), cumulative 1-based `start_position` rows into
    `resource_positions` (exactly the shape `positions.rs` wrote), and
    `publications.position_count = total`. New imports also set
    `reconciled_at = unix_now()` and coordinate columns NULL (a fresh
    book has no position) — so the 6.4 pass skips them by construction.
    Extract the position-computation into
    `pub(crate) fn synthetic_positions(char_counts: &[u64]) ->
    Vec<(u32 /*spine_idx*/, u32 /*start*/, u32 /*count*/)>` in
    `features/progress/positions.rs` (reusing the file the write API
    vacated) so 6.4 shares it.
  - **Error handling:** unchanged pipeline degradation — a `None` corpus
    entry skips its `resource_text` row (logged) but still gets a
    position row (count 1) so position math never has spine holes.
  - **Tests:** `import_positions_computed`: import the CJK fixture EPUB →
    `resource_positions` rows with expected counts
    (`ceil(chars/1024).max(1)`), `position_count` == sum,
    `reconciled_at` set; `import_corpus_is_projection`: imported
    `resource_text.body` == `extract_corpus` output (and, transitively
    via 5.4's test, == the session projection);
    `textless_resource_still_positioned`: fixture with one garbage spine
    entry → no text row, position count 1 for it.
  - **Verify:** `cd core && cargo test -p inkuna-core import` → green;
    `grep -rn "extract_spine_text" core/` → no hits.

- [ ] **Task 6.4 — the V8 reconcile pass**
  - **Files:** Create
    `core/crates/inkuna-core/src/features/library/rebaseline.rs` +
    `rebaseline_tests.rs` · Modify `src/features/library/{mod.rs,
    store.rs}`, `src/features/search/index.rs` ·
    `core/crates/inkuna-core/Cargo.toml` (add `serde_json`, latest
    stable — Readium locator JSON parsing; mainstream crate per policy).
  - **Behavior:** Follows the search-reconcile prior art
    (`SearchIndex::spawn_reconcile`, kicked from `store.rs:47`) but
    chained so the index never indexes a stale corpus: one background
    thread that (1) runs the V8 rebaseline below, then (2) runs the
    existing search reconcile body on the same thread. Concretely, change
    `SearchIndex::spawn_reconcile(&self, db_path: PathBuf)` to
    `spawn_reconcile(&self, db_path: PathBuf, pre: impl FnOnce() + Send +
    'static)` — the closure runs first on the spawned thread and opens
    its own resources; the existing JoinHandle/Drop plumbing is
    unchanged, and test call sites pass `|| {}`. `Library::open`
    (store.rs:47) passes a closure capturing `data_dir` + `db_path` that
    runs `rebaseline::run(&data_dir, &db_path)`.
    The rebaseline body: open its OWN writer connection via
    `open_connection(&db_path)` (WAL + busy_timeout make a second writer
    safe; per-book transactions are short). Select
    `id, file_path, language FROM publications WHERE reconciled_at IS
    NULL ORDER BY last_opened_at DESC NULLS LAST` (most recently read
    books first — the user's current book converges fastest). Per book,
    all inside ONE `IMMEDIATE` transaction (crash → the whole book
    retries next open; idempotent because `reconciled_at` gates):
    1. `extract_corpus(data_dir/file_path, spine_hrefs_from_resources_rows,
       MAX_TOTAL_TEXT_BYTES)` → replace each `resource_text.body`
       (`INSERT OR REPLACE` keyed by resource id; `None` → `DELETE` the
       text row).
    2. Recompute `resource_positions` + `publications.position_count` via
       `synthetic_positions` (6.3).
    3. Convert `publications.locator` (if non-NULL): `serde_json` parse →
       `href` field (top-level) and `locations.progression` (f64,
       default 0.0). Normalize href minus fragment via
       `inkuna_content::resolve_href("", …)` and match against the
       book's `resources.href` rows → `spine_idx`; `char_offset =
       (progression.clamp(0,1) × resource_char_len) as u64`, clamped to
       `len.saturating_sub(1).max(0)`. Write the coordinate columns,
       set `locator = NULL`. Failure defaults, each written (nothing
       dropped without a default, spec §8): parseable href but bad/absent
       progression → `(spine_idx, 0)` (chapter start); unparseable JSON
       or unresolvable href → `(0, 0)`.
    4. Same conversion per bookmark row (`bookmarks.locator` → coordinate
       columns; after conversion set `locator = ''` — column is NOT
       NULL). Failure defaults identical.
    5. `reconciled_at = unix_now()`.
    A book whose FILE is missing/unreadable still reconciles steps 3–5
    with defaults — char lengths are unknowable then, so the coordinate
    is the chapter start `(spine_idx, 0)` when the href resolves via the
    `resources` table, else `(0, 0)` — keeps whatever `resource_text` it
    had, and is stamped: it "still
    opens; position falls back at read time" (spec §8). After each
    book's commit, re-index it in tantivy (`delete_term` + re-add from
    the new rows, the `index.rs` add path) — search stays consistent
    book-by-book while the pass runs.
  - **Error handling:** per-book: any error rolls back that book's
    transaction, logs at `warn`, and CONTINUES to the next book (one
    corrupt book never blocks the library); the book retries on next
    open. Thread-level: a failure opening the connection logs and
    returns (same as search reconcile today). `Library` reads meanwhile:
    coordinate columns NULL → read-time default `Coordinate { spine_idx:
    0, char_offset: 0 }` (6.5), so nothing waits on the pass.
  - **Tests:** (fixture DB seeded at V8 with real Readium locator JSON
    shapes) `valid_locator_converts`:
    `{"href":"OEBPS/ch02.xhtml","locations":{"progression":0.5}}` on a
    2-chapter imported fixture → `position_spine_idx == 1`,
    `char_offset == len/2 ± 1`, locator NULL, `reconciled_at` set;
    `corrupt_json_defaults_zero`: `"not json"` → `(0,0)`, stamped;
    `unresolvable_href_defaults_zero`; `href_without_progression_gets_
    chapter_start`; `bookmarks_converted_with_defaults`;
    `idempotent_second_run_noop`: run twice → identical rows, second run
    touches nothing (assert via `reconciled_at` unchanged);
    `crash_resume`: simulate by running the pass with a hook that errors
    after book 1 → book 1 stamped, book 2 not; re-run completes book 2;
    `corpus_reindexed_for_search`: post-reconcile `search_in_book` for a
    CJK term finds it at the projection offset; `missing_file_still_
    stamps_with_defaults`.
  - **Verify:** `cd core && cargo test -p inkuna-core rebaseline` →
    green; `cargo test --workspace` → green.

- [ ] **Task 6.5 — Coordinate through progress, bookmarks, publications**
  - **Files:** Modify `core/crates/inkuna-core/src/features/library/
    {model.rs, queries.rs, bookmarks.rs, tests.rs}`,
    `src/features/progress/{writes.rs, tests.rs}`, `src/lib.rs` ·
    `core/crates/inkuna-ffi/src/{library.rs, progress.rs, lib.rs}` ·
    Create `core/crates/inkuna-ffi/src/reader/{mod.rs, records.rs}` —
    this task creates the `reader` module with only the `Coordinate`
    record; 6.6 fills in the rest.
  - **Behavior:** Core: `inkuna-core/src/lib.rs` re-exports
    `inkuna_engine::Coordinate` (plus, for 6.6, `EngineSession`,
    `LayoutEvents`, `Viewport`, `LayoutSettings`, `FontRegistry`, and the
    display/session record types — add them all now). Core `Publication`
    swaps `locator: Option<String>` → `coordinate: Option<Coordinate>`
    (read: both columns non-NULL → Some, else None — pre-reconcile rows
    surface `None`, and `position_count` keeps meaning); core `Bookmark`
    swaps `locator: String` → `coordinate: Coordinate` (read: NULL
    columns → `Coordinate { spine_idx: 0, char_offset: 0 }` — the spec's
    read-time default). Writes: `Library::update_progress(&self, id:
    &str, coordinate: Coordinate, progression: f64, position:
    Option<u32>)` — stores the coordinate columns; when `position` is
    `None`, derives it from the coordinate against `resource_positions`
    (`start_position + char_offset / 1024`, clamped into the resource's
    range) so session stats keep flowing without a shell-side position
    model (doc comment: shells may pass `None`). All other semantics
    (clamping, finish threshold, session heartbeat) unchanged.
    `Library::add_bookmark(&self, id: &str, coordinate: Coordinate,
    progression: f64) -> Result<Bookmark, CoreError>` writes coordinate
    columns + `locator = ''`. FFI: new
    `#[derive(uniffi::Record)] pub struct Coordinate { pub spine_idx:
    u32, pub char_offset: u64 }` (in `reader/records.rs`, module
    declared from `lib.rs`) with `From` conversions both ways;
    `Publication` record: `locator` field deleted, `coordinate:
    Option<Coordinate>` added (`position_count` stays); `Bookmark`
    record: `locator` → `coordinate: Coordinate`;
    `ShelfProgress::update_progress(id: String, coordinate: Coordinate,
    progression: f64, position: Option<u32>)`;
    `ShelfLibrary::add_bookmark(id: String, coordinate: Coordinate,
    progression: f64)`. Method names all unchanged (overview: "same
    method names, `locator: String` parameters/fields become
    `coordinate: Coordinate`"). Session-free position lookups (overview
    contract — Home/Detail screens have no `ReaderSession`): core
    `Library::position_of(&self, id: &str, coordinate: Coordinate) ->
    Result<u32, CoreError>` and `Library::position_count(&self, id: &str)
    -> Result<u32, CoreError>` over `resource_positions` (1-based; clamp
    past-end coordinates to the resource's last position; no rows → 1 /
    1), mirrored async on `ShelfProgress::position_of(id: String,
    coordinate: Coordinate) -> u32` and
    `ShelfProgress::position_count(id: String) -> u32` — the single
    implementation `update_progress`'s internal derivation also calls, so
    the `/1024` math lives in exactly one place.
  - **Error handling:** unchanged (`NotFound` paths as today); coordinate
    derivation of `position` is best-effort — no `resource_positions`
    rows → `position` stays `None`.
  - **Tests:** `progress_roundtrip_coordinate`: update then read →
    coordinate survives; `position_derived_from_coordinate`: book with
    known position rows, update with `position: None`, coordinate mid
    chapter 2 → session `end_position` == expected;
    `bookmark_defaults_before_reconcile`: bookmark row with NULL
    coordinate columns → reads as `(0,0)`;
    `publication_coordinate_none_until_written`.
  - **Verify:** `cd core && cargo test -p inkuna-core` → green (ffi
    compiles: `cargo build -p inkuna-ffi`).

- [ ] **Task 6.6 — `ReaderSession` FFI surface**
  - **Files:** Create `core/crates/inkuna-ffi/src/reader/{mod.rs,
    records.rs, session.rs, listener.rs}` · Modify
    `core/crates/inkuna-ffi/src/{bookshelf.rs, lib.rs}`.
  - **Behavior:** `records.rs` — every overview record/enum as
    `uniffi::Record`/`uniffi::Enum`, exactly these shapes (overview is
    law; `From` impls from the engine types beside each):
    `Viewport { width: f64, height: f64 }` ·
    `ReaderLayoutSettings { reading_font: String, reading_bold: bool,
    text_size_step: u8, line_spacing: f64, letter_spacing: f64,
    word_spacing: f64, reading_margins: u32 }` (→
    `inkuna_engine::LayoutSettings`) · `CharRange { start: u64, end:
    u64 }` · `Rect { x: f64, y: f64, width: f64, height: f64 }` ·
    `WritingMode { HorizontalTb, VerticalRl }` ·
    `SelectionRect { rect: Rect, writing_mode: WritingMode }` ·
    `ChapterGeometry { generation: u64, page_count: u32, char_range:
    CharRange, writing_mode: WritingMode, rtl_progression: bool,
    truncated: bool }` ·
    `PageLocation { generation: u64, spine_idx: u32, page_idx: u32 }` ·
    `HitResult { coordinate: Coordinate, link_target: Option<String> }` ·
    `ColorRole { Text, Secondary, Link }` · `RunOrientation { Upright,
    SidewaysRotated }` · `GlyphRun { font_id: u32, size: f64,
    color_role: ColorRole, glyph_ids: Vec<u16>, positions: Vec<f32>,
    orientation: RunOrientation }` · `ImagePlacement { href: String,
    rect: Rect }` · `DecorationKind { Rule, Underline }` ·
    `Decoration { kind: DecorationKind, rect: Rect,
    color_role: ColorRole }` ·
    `LinkRegion { rect: Rect, target: String }` · `A11yRole { Body,
    Heading, Link }` · `A11yBlock { text: String, rect: Rect, lang:
    Option<String>, is_link: bool, role: A11yRole }` ·
    `PageDisplayList { generation: u64, glyph_runs: Vec<GlyphRun>,
    images: Vec<ImagePlacement>, decorations: Vec<Decoration>, links:
    Vec<LinkRegion>, a11y: Vec<A11yBlock> }` · `FontAxis { tag: String,
    value: f64 }` · `FontEntry { id: u32, file_path: String,
    collection_index: u32, axes: Vec<FontAxis> }`. `listener.rs`:
    ```rust
    #[uniffi::export(with_foreign)]
    pub trait LayoutListener: Send + Sync {
        fn on_first_page_ready(&self, generation: u64, spine_idx: u32);
        fn on_chapter_ready(&self, generation: u64, spine_idx: u32,
                            page_count: u32);
    }
    ```
    (the `ImportProgressListener` precedent; doc: callbacks arrive on
    engine threads — shells hop to main) plus a private adapter
    implementing `inkuna_core::LayoutEvents` over `Arc<dyn
    LayoutListener>`. `session.rs`:
    `#[derive(uniffi::Object)] pub struct ReaderSession(Arc<
    inkuna_core::EngineSession>)` — sync exports (`#[uniffi::export]`,
    NOT async: safe on the UI thread, cache-only, throw instead of
    blocking — spec §3): `chapter(spine_idx: u32) -> Result<
    ChapterGeometry, InkunaError>`, `page(spine_idx: u32, page_idx: u32)
    -> Result<PageDisplayList, InkunaError>`, `is_ready(spine_idx: u32)
    -> bool`, `locate(coordinate: Coordinate) -> Result<PageLocation,
    InkunaError>`, `locate_href(href: String, fragment: Option<String>)
    -> Result<Coordinate, InkunaError>`, `hit_test(spine_idx: u32,
    page_idx: u32, x: f64, y: f64) -> Result<HitResult, InkunaError>`,
    `selection_rects(spine_idx: u32, range: CharRange) ->
    Result<Vec<SelectionRect>, InkunaError>`, `word_at(coordinate:
    Coordinate) -> Result<CharRange, InkunaError>`,
    `text_range(spine_idx: u32, range: CharRange) -> Result<String,
    InkunaError>`, `match_rects(spine_idx: u32, char_offset: u64,
    len: u64) -> Result<Vec<SelectionRect>, InkunaError>`,
    `accessibility_blocks(spine_idx: u32, page_idx: u32) ->
    Result<Vec<A11yBlock>, InkunaError>`, `font_registry() ->
    Vec<FontEntry>`, `spine_count() -> u32` (wraps `spine_len`),
    `page_char_range(spine_idx: u32, page_idx: u32) -> Result<CharRange,
    InkunaError>`, `position_of(coordinate: Coordinate) -> u32` and
    `position_count() -> u32` (1-based synthetic position lookup over a
    snapshot of the publication's `resource_positions` ranges loaded once
    in `open_reader` — sync-safe, no DB access after open; a coordinate
    past the snapshot clamps to the last position; shells never mirror
    the 1024-char constant), `page_digest(spine_idx: u32, page_idx: u32)
    -> Result<String, InkunaError>`; async exports (`async_runtime =
    "tokio"`, via `blocking()`): `update_layout(viewport: Viewport,
    settings: ReaderLayoutSettings) -> Result<(), InkunaError>`,
    `resource(href: String) -> Result<Vec<u8>, InkunaError>`.
    `bookshelf.rs` gains the async constructor method:
    `pub async fn open_reader(&self, id: String, viewport: Viewport,
    settings: ReaderLayoutSettings, listener: Arc<dyn LayoutListener>)
    -> Result<Arc<ReaderSession>, InkunaError>` — resolves the
    publication (absolute path via `data_dir`, language, and the stored
    coordinate's `spine_idx` as `opening_chapter`, 0 when `None`), loads
    the `FontRegistry` from `font_dir` (cached in a `OnceLock` on
    `Bookshelf` — loaded once per process), and enforces
    **last-open-wins**: `Bookshelf` holds `active_session:
    Mutex<Weak<inkuna_core::EngineSession>>`; a live previous session is
    `close()`d before the new `EngineSession::open` (any id — one live
    reader per `Bookshelf`; sessions also close on drop, satisfying
    "close with their Bookshelf"). `lib.rs` re-exports the reader
    module's public types.
  - **Error handling:** every engine error crosses as its 6.1
    `InkunaError` mirror; `open_reader` on a fixed-layout book →
    `UnsupportedContent` (shell shows "not yet supported", plan 02);
    unknown id → `NotFound`; font-dir problems surface here as
    `UnsupportedContent` (registry load) — with the M1 open-time
    directory validation, the only new cause is missing files.
  - **Tests:** none in `inkuna-ffi` (no suite; the engine methods are
    covered in 5.3, conversions are `From` impls checked by the
    compiler); the real gate is 6.8's bindgen + shell builds.
  - **Verify:** `cd core && cargo build -p inkuna-ffi` → green;
    `cargo test --workspace` → green.

- [ ] **Task 6.7 — search offset unification**
  - **Files:** Modify `core/crates/inkuna-core/src/features/search/
    {fold.rs, queries.rs, tests.rs}`.
  - **Behavior:** State and enforce the invariant (spec §10): **no folded
    offset ever crosses the FFI** — `BookSearchHit.char_offset` (and the
    `progression` denominator) index the ORIGINAL `resource_text` body,
    i.e. the canonical projection, in Unicode scalars. Audit
    `FoldedText::occurrences` + `snippet`: if `occurrences` yields
    folded-space offsets, map each hit's start through the existing
    fold↔original offset map before it becomes `char_offset` (the map
    already exists for excerpts — expose
    `FoldedText::to_original(folded_char: u32) -> u32` if not already
    public within the module). If the audit shows offsets are already
    original-space, the change is the added tests + a module-doc
    invariant statement — either way the tests below must pass. With the
    corpus rebaselined (6.3/6.4), hits are content coordinates with no
    conversion step: `Coordinate { spine_idx: hit.spine_idx,
    char_offset: hit.char_offset as u64 }` feeds `locate` /
    `match_rects` directly (documented on `BookSearchHit`).
  - **Error handling:** unchanged.
  - **Tests:** `offsets_original_space_under_fold_expansion`: body
    containing `ﬁ` (U+FB01, folds to "fi") before a CJK match → the
    hit's `char_offset` indexes the original body (assert
    `body.chars().nth(char_offset)` starts the match);
    `offsets_original_space_nfkc_contraction`: body with `㍿`
    (folds to 株式会社) → same assertion for a later hit;
    `search_offset_equals_projection_offset`: import the CJK fixture,
    search a unique term → `text_range(spine_idx, CharRange{off,
    off+len})` via an `EngineSession` on the same file returns the term
    (end-to-end coordinate identity — the §10 no-conversion guarantee).
  - **Verify:** `cd core && cargo test -p inkuna-core search` → green.

- [ ] **Task 6.8 — bindings regeneration + shell compile fixes**
  - **Files:** Run `./scripts/build-core-ios.sh` +
    `./scripts/build-core-android.sh` · Modify the shell call sites the
    6.1/6.5/6.6 surface changes break — expected set (confirm by
    compiling): iOS `ReaderViewController.swift` (updateProgress,
    addBookmark, bookmark restore, `publication.locator` reads,
    reportPositionCount/Ranges calls), `LibraryStore.swift`; Android
    `ReaderViewModel.kt`, `TonightViewModel.kt`, `LibraryStore.kt`.
  - **Behavior:** Compile-only mechanical fixes — the Readium reader
    still runs on `dev/core` until plan 02 replaces it, and interim
    reader UX degradation is accepted (spec: hard cut; `main` keeps the
    frozen beta): progress/bookmark writes pass `Coordinate(spineIdx: 0,
    charOffset: 0)` with a `// plan-02: real coordinates from the
    engine` comment (keep passing the real `progression`, so Keep
    Reading and shelves stay correct); `position` param: pass nil/null;
    reads of the deleted `locator` fields switch to the `coordinate`
    field or are stubbed to book-start restore; calls to the deleted
    `reportPositionCount`/`reportPositionRanges` are deleted along with
    the code that computed their inputs ONLY where that code is
    call-site-local (do not refactor reader internals — plan 02 deletes
    them wholesale). No new UI, no new strings, no behavior polish:
    smallest diff that compiles.
  - **Error handling:** n/a.
  - **Tests:** none (shells have no test targets).
  - **Verify:** both build scripts succeed; iOS `xcodegen generate &&
    xcodebuild … build` succeeds; Android `./gradlew assembleDebug`
    succeeds; `cd core && cargo test --workspace` green — the plan's
    exit state: engine proven end-to-end, bindings regenerated clean,
    shells compiling.

## Notes for the conductor

- **Movement seams are hard:** M1 must be fully green (including both
  shell builds) before anything engine-shaped starts — it is the
  cheap-backport window the spec's sequencing note exists for (A9).
  Commit M1 as its own `refactor(core)` / `refactor(core,ios,android)`
  train.
- **Network access is needed twice:** 3.2 downloads Noto CJK + bold +
  symbols faces from official releases, and every new-dependency task
  must query crates.io for the actual latest stable (stack policy —
  never trust training data for versions).
- **Golden regeneration:** `INKUNA_UPDATE_GOLDEN=1 cargo test -p
  inkuna-engine golden` rewrites snapshots; a reviewer should treat any
  golden diff after M5 lands as a layout-behavior change needing
  justification.
- **Between M6 and plan 02** the `dev/core` reader writes stub `(0,0)`
  coordinates; reading-position fidelity on the branch is intentionally
  degraded until plan 02's engine reader lands. Do not "fix" this by
  porting Readium locator logic forward.
- **Deviations decided by this plan** (flag to the owner if they look
  wrong): `report_position_count` is deleted along with
  `report_position_ranges` (spec names only the latter, but core-computed
  positions make both obsolete); V8 adds a `reconciled_at` column as the
  crash-resume marker (append-only, within §8's mechanism); `assets/fonts/`
  gains Latin Bold/BoldItalic statics and Noto Sans Symbols 2 beyond the
  overview's listed set (bold rendering and the spec's symbol-fallback
  chain require them; plan 02 copies the directory wholesale and reads
  faces from the registry, so it is unaffected).
- **Spec A2 risk watch:** vertical-writing layout (3.5/4.2) is the
  largest from-scratch surface; if `vert`/`vrt2` shaping via rustybuzz
  shows gaps on the fixture corpus, stop and surface it rather than
  hand-rolling glyph substitution.
