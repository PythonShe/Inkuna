# Inkuna Core Rules

You are working inside `core/`, the Rust workspace that owns all non-UI logic.
Follow the rules below; they capture project-specific conventions and override
common defaults.

## 0. Tech Stack & Dev Commands

| Layer | Technology | Notes |
|------|------|------|
| Language | Rust, latest stable via rustup | never the Homebrew rustc (see root CLAUDE.md) |
| Storage | rusqlite (bundled SQLite, WAL) | one DB per install, owned by `Library`; append-only `user_version` migrations (refinery once it supports latest rusqlite) |
| Async | tokio, at the FFI layer only | core stays sync; `inkuna-ffi` wraps calls in `spawn_blocking` so shells get `await`/`suspend` |
| Time | chrono | unix-seconds `i64` in the DB |
| Archives | `zip` (EPUB/CBZ); `unrar` planned for CBR | format detection is magic-byte based (TXT is the documented extension-gated exception) |
| XML | quick-xml | streaming events, no DOM; entity unescape via `quick_xml::escape::unescape` |
| FFI | UniFFI proc-macros, latest | Swift + Kotlin bindings from one surface; async methods use `#[uniffi::export(async_runtime = "tokio")]` |

Formats: EPUB (full metadata), MOBI/AZW3 (hand-rolled clean-room Palm/KF8
readers in `formats/mobi` + `formats/azw3`; DRM-free only), TXT
(chardetng/encoding_rs charset detection, CJK chapter auto-detection) — all
reflowables normalize to EPUB at import through `formats/epub/write.rs`.
PDF and CBZ/CBR are detected but not yet importable; see root CLAUDE.md.

```bash
cargo test                      # full workspace tests — must pass before commit
../scripts/build-core-ios.sh    # rebuild iOS xcframework + Swift bindings
../scripts/build-core-android.sh# rebuild Android .so + Kotlin bindings
```

`Cargo.lock` is committed; never hand-edit it.

## 1. Crate Layering (hard boundary)

- `crates/inkuna-core` — pure Rust domain logic. **No UniFFI types, no FFI
  concerns.** Free to use borrows, rich enums, `&str` parameters.
- `crates/inkuna-ffi` — the only UniFFI surface. Mirrors core types as
  `uniffi::Record`/`Enum`/`Object` with `From` conversions; owned values only;
  keep the boundary coarse-grained (no chatty per-field calls).
- New capability = implement in `inkuna-core` first with tests, then expose a
  minimal wrapper in `inkuna-ffi`.
- After ANY `inkuna-ffi` change, regenerate bindings with both build scripts
  before touching the shells.

## 2. Module Layout (`inkuna-core`)

| Path | Holds |
|------|------|
| `src/lib.rs` | crate root: module declarations plus the public `pub use` re-exports — keep it thin |
| `src/core/` | infrastructure leaves: `error`, `db/` (connection setup, reader pool, migrations), `files`, `time`. No business logic |
| `src/formats/` | detection (`format.rs`) and one module per format (`epub/`, `mobi/`, `azw3/`, `txt/`; CBZ/CBR land beside them) |
| `src/features/` | vertical slices — `library/`, `import/`, `progress/`, `stats/`, `settings/` — each owning its types, reads, and writes |
| `src/test_support.rs` | shared `#[cfg(test)]` fixtures (the `write_epub` / `write_cbz` / `write_mobi` builders) |
| `src/mobi_test_support.rs` | shared `#[cfg(test)]` MOBI/KF8 fixtures (`MobiTestBuilder`, INDX/KF8 fixture builders, `palmdoc_compress`) |

- **`mod.rs` is declarations only**: module doc comments, `mod`/`pub mod`,
  `use`/`pub use`, and module-level constants. Never a `fn`, `struct`, `enum`,
  `impl`, or `trait`. Canonical order: `//!` doc → `mod` declarations
  (alphabetical) → `#[cfg(test)] mod tests;` → re-exports.
- Group by domain, not by technical layer; files that change together live
  together. A new capability starts in `features/` and only moves down into
  `core/` once a second feature needs it.
- Every module directory carries its own `mod.rs` — never the `foo.rs` +
  `foo/` sibling style. Domain folders are singular (`library`, `import`),
  role folders plural (`queries`, `writes`), with `model`/`store` singular.
- Target ≤400 lines per source file, 500 as the hard ceiling; `tests.rs` /
  `*_tests.rs` are exempt. Split along domain seams, not arbitrary ones.
- `Library` is the shared facade: each feature contributes its own
  `impl Library` block from its own module — inherent impls may live in any
  module of the crate.
- Everything public reaches consumers from the crate root: `inkuna-ffi`
  imports `inkuna_core::Foo`, never a module path. A new public type means a
  new `pub use` in `lib.rs`.

## 3. Code Quality & Robustness

- Every fallible function returns `Result<T, CoreError>`; `.unwrap()` /
  `.expect()` outside tests and lock poisoning is forbidden.
- New error cases get a `CoreError` variant and a mirrored `InkunaError`
  variant + `From` arm in `inkuna-ffi` — never stringly-typed errors.
- Prefer borrowing over moving; comment any required `clone()`.
- `Library` guards its connection with a `Mutex` because UniFFI objects must
  be `Send + Sync`; keep lock scopes minimal.

## 4. CJK Correctness (product-critical)

- All text handling is UTF-8-safe; never index strings by byte offset in user
  data. Tests must include CJK fixtures (see `features/library/tests.rs` as
  the pattern).
- Search is two engines over one corpus (`resource_text`), never SQLite
  FTS5: library-wide ranked search is tantivy + jieba (`features/search/`,
  index under `<data_dir>/index/`, reconciled against the DB on open) with
  a CJK-unigram field so single-char and substring CJK queries match;
  in-book search is an exact per-char NFKC+case-fold scan, because only a
  scan can return every occurrence with char offsets and partial-word
  Latin matches. The index is derived data — always rebuildable, never
  the truth.

## 5. Naming & FFI Conventions

- Rust standard casing; files snake_case.
- The FFI library object is `Bookshelf`, not `Library` — UniFFI's Kotlin
  output imports JNA's `com.sun.jna.Library` and the names collide. Check
  generated-code collisions before naming new exported types.
- Bindings config lives in `crates/inkuna-ffi/uniffi.toml` (Swift module
  `InkunaCore`, Kotlin package `app.inkuna.core`).

## 6. Testing

- Unit tests live beside the code but never inline: extract every
  `#[cfg(test)]` module into its own file. A leaf module `foo.rs` gets a
  sibling `foo_tests.rs`, wired as
  `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`; a folder module gets
  `tests.rs` inside it, declared from `mod.rs` as `#[cfg(test)] mod tests;`.
- Tests build their own fixtures in `tempfile` dirs; shared builders live in
  `src/test_support.rs` (the `write_epub` helper) and
  `src/mobi_test_support.rs` (MOBI/KF8) — no binary fixtures in git.
- Format detection, metadata parsing, and DB roundtrips must stay covered.
