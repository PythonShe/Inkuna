# Inkuna Core

Rust workspace that owns all non-UI logic. Section numbers are referenced
from code comments and `.coderabbit.yaml`; keep them stable.

## 0. Stack and commands

| Layer | Choice | Notes |
|---|---|---|
| Language | Rust 2024, channel `stable` via `rust-toolchain.toml` | `rust-version` is a minimum, not a pin; CI's `rustup update stable` tracks latest |
| Storage | rusqlite (bundled, WAL) | one DB per install owned by `Library`; append-only `user_version` migrations |
| Async | tokio in `inkuna-ffi` only | core stays sync; FFI wraps calls in `spawn_blocking` and exports with `#[uniffi::export(async_runtime = "tokio")]`; the reader interaction path is synchronous and cache-only |
| Archives / XML | `zip`; quick-xml | format detection is magic-byte based (TXT is the extension-gated exception); `unrar` planned for CBR |
| Layout engine | `inkuna-engine`: harfrust + read-fonts, icu_segmenter, unicode-bidi/script, cssparser | fixed-point (1/64 pt) math; per-page glyph-run display lists; the canonical text projection defines all char offsets |
| FFI | UniFFI proc-macros | one surface for Swift and Kotlin |

Formats: EPUB (`inkuna-content`); MOBI/AZW3 (clean-room readers in
`inkuna-format/src/mobi`, `src/azw3`; DRM-free only); TXT (chardetng
charset detection, CJK chapter detection; `inkuna-format/src/txt`). All
normalize to EPUB at import via `inkuna-format/src/epub_write.rs`. PDF and
CBZ/CBR are detected but not yet importable.

```bash
cargo test                        # full workspace; must pass before commit
../scripts/build-core-ios.sh      # xcframework + Swift bindings
../scripts/build-core-android.sh  # .so + Kotlin bindings
```

## 1. Crate layering (hard boundary)

- `inkuna-content`: EPUB container layer (bounded archive reads, OPF, spine,
  TOC, href normalization). No FFI, no DB.
- `inkuna-format`: import-side conversion (MOBI/AZW3/TXT to EPUB, EPUB 3
  writer).
- `inkuna-engine`: reader layout engine. Deterministic, no DB access.
- `inkuna-core`: domain services (library, DB, import, features). No UniFFI
  types or FFI concerns.
- `inkuna-ffi`: the only UniFFI surface (bindgen runs in `--library` mode, so
  all exports stay in this crate). Root `Bookshelf` object plus per-feature
  facades; mirrors core types as `uniffi::Record`/`Enum`/`Object` with `From`
  conversions; owned values only; coarse-grained calls.
- New capability: implement and test in `inkuna-core`, then add a minimal
  wrapper in `inkuna-ffi`.

## 2. Module layout (`inkuna-core`)

| Path | Holds |
|---|---|
| `src/lib.rs` | module declarations and public `pub use` re-exports; keep thin |
| `src/core/` | infrastructure leaves: `error`, `db/` (connection, reader pool, migrations), `files`, `time`; no business logic |
| `src/formats/` | detection (`format.rs`) plus thin re-exports keeping the `epub::` / `mobi::` / `txt::` call shape |
| `src/features/` | vertical slices: `library`, `import`, `progress`, `search`, `settings`, `stats` |
| `src/test_support.rs` | shared `#[cfg(test)]` builders, re-exporting those of `inkuna-content` and `inkuna-format` |

- `mod.rs` is declarations only: `//!` doc, `mod` lines (alphabetical),
  `#[cfg(test)] mod tests;`, then `use`/`pub use` and module constants. Never
  a `fn`, `struct`, `enum`, `impl`, or `trait`.
- Every module directory has its own `mod.rs`; never `foo.rs` beside `foo/`.
  Domain folders singular (`library`), role folders plural (`queries`,
  `writes`), `model`/`store` singular.
- Group by domain. A capability starts in `features/` and moves into `core/`
  only when a second feature needs it.
- Source files target 400 lines, hard ceiling 500; test files are exempt.
  Split along domain seams.
- Each feature adds its own `impl Library` block in its own module.
- Consumers import from the crate root (`inkuna_core::Foo`); a new public
  type needs a `pub use` in `lib.rs`.

## 3. Errors and robustness

- Fallible functions return `Result<T, CoreError>`. No `.unwrap()` /
  `.expect()` outside tests and lock poisoning.
- A new error case gets a `CoreError` variant plus a mirrored `InkunaError`
  variant and `From` arm in `inkuna-ffi`; never stringly-typed errors.
- Prefer borrowing; comment any required `clone()`.
- `Library` holds its writer connection in a `Mutex` (UniFFI objects must be
  `Send + Sync`); keep lock scopes minimal.

## 4. CJK correctness (product-critical)

- Never index user text by byte offset. Tests include CJK fixtures.
- Search is two engines over one corpus (`resource_text`), never SQLite FTS5:
  - library-wide ranked search: tantivy + jieba (`features/search/`, index
    under `<data_dir>/index/`, reconciled against the DB on open) with a CJK
    unigram field so single-char and substring queries match. The index is
    derived data, always rebuildable.
  - in-book search: exact per-char NFKC + case-fold scan, the only way to
    return every occurrence with char offsets and partial-word Latin matches.

## 5. Naming and FFI

- The FFI library object is `Bookshelf`, not `Library`: UniFFI's Kotlin
  output imports JNA's `com.sun.jna.Library`. Check generated-code name
  collisions before naming new exported types.
- Bindings config: `crates/inkuna-ffi/uniffi.toml` (Swift module
  `InkunaCore`, Kotlin package `app.inkuna.core`).

## 6. Testing

- Never inline test modules. Leaf `foo.rs` gets `foo_tests.rs`, wired as
  `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`; a folder module gets
  `tests.rs` declared from `mod.rs`.
- Tests build fixtures in `tempfile` dirs from the `test_support.rs`
  builders (`write_epub` in `inkuna-content`, `MobiTestBuilder` and KF8
  fixtures in `inkuna-format`, `write_cbz` / `write_mobi` in `inkuna-core`).
  No binary fixtures in git.
- Format detection, metadata parsing, and DB roundtrips stay covered.
