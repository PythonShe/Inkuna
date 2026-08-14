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

Formats: EPUB (full metadata), MOBI/AZW3 (PalmDB header distinguishes by KF8
version; DRM-free only), TXT (charset-aware CJK handling planned), PDF,
CBZ/CBR. Reflowables normalize to EPUB at import; see root CLAUDE.md.

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

## 2. Code Quality & Robustness

- Every fallible function returns `Result<T, CoreError>`; `.unwrap()` /
  `.expect()` outside tests and lock poisoning is forbidden.
- New error cases get a `CoreError` variant and a mirrored `InkunaError`
  variant + `From` arm in `inkuna-ffi` — never stringly-typed errors.
- Prefer borrowing over moving; comment any required `clone()`.
- `Library` guards its connection with a `Mutex` because UniFFI objects must
  be `Send + Sync`; keep lock scopes minimal.

## 3. CJK Correctness (product-critical)

- All text handling is UTF-8-safe; never index strings by byte offset in user
  data. Tests must include CJK fixtures (see `library.rs` tests as the
  pattern).
- Future search work uses tantivy + jieba-style tokenization, not SQLite FTS5.

## 4. Naming & FFI Conventions

- Rust standard casing; files snake_case.
- The FFI library object is `Bookshelf`, not `Library` — UniFFI's Kotlin
  output imports JNA's `com.sun.jna.Library` and the names collide. Check
  generated-code collisions before naming new exported types.
- Bindings config lives in `crates/inkuna-ffi/uniffi.toml` (Swift module
  `InkunaCore`, Kotlin package `app.inkuna.core`).

## 5. Testing

- Unit tests live beside the code (`#[cfg(test)]`), build their own fixtures
  in `tempfile` dirs (see `write_epub` helper) — no binary fixtures in git.
- Format detection, metadata parsing, and DB roundtrips must stay covered.
