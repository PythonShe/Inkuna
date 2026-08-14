# Inkuna Monorepo Rules

Inkuna is a minimalist book reader (EPUB now; CBZ/CBR comics planned) for iOS
and Android. Apple Books-level feel is the quality bar. CJK support — vertical
writing, CJK-aware search — is a core product goal, not an afterthought.
License: AGPL-3.0. Website: `inkuna.app`.

| Directory | Component | Tech Stack | Description |
|------|------|--------|------|
| `core/` | Inkuna Core | Rust workspace + UniFFI | All non-UI logic: library DB, import, formats, metadata, progress; later search/annotations/sync |
| `apps/ios/` | Inkuna iOS | UIKit + XcodeGen | Native iOS shell (`app.inkuna.ios`), min iOS 18 |
| `apps/android/` | Inkuna Android | Kotlin + Jetpack Compose | Native Android shell (`app.inkuna.android`), minSdk 33 |
| `scripts/` | Build scripts | bash | Core cross-builds + UniFFI bindings generation |

Rendering will live in the shells on Readium's native toolkits (Swift/Kotlin);
the Rust core never renders.

## Scope Rules

- These root rules cover only monorepo-level constraints, not component
  implementation details.
- For a task within `core/`, `apps/ios/`, or `apps/android/`, also follow that
  directory's `CLAUDE.md`.
- For tasks spanning components (e.g. an FFI change), follow this file first,
  then each affected component's rules. After any `core/crates/inkuna-ffi`
  change, regenerate bindings via BOTH `scripts/build-core-*.sh` before
  building the shells.

## Documentation Priority

- `docs/dev/architecture.md` — architecture decision record (why Rust core +
  native shells, version targets, roadmap). Read before structural changes.

## Git Conventions

- Commit and push in logical groups as work completes; the owner has granted
  this standing authorization.
- Never add "Generated with Claude Code" / `Co-Authored-By: Claude` footers.
- Commit message format: `<type>(<scope>): <description>` with type
  `feat/fix/refactor/docs/chore/test/perf/style` and scope:

| scope | Applicable scenario |
|-------|---------|
| `core` | Changes within `core/` |
| `ios` | Changes within `apps/ios/` |
| `android` | Changes within `apps/android/` |
| `docs` | Changes under `docs/` |
| `workspace` | Repo-root-level changes (`CLAUDE.md`, `scripts/`, etc.) |

Cross-component changes may combine scopes (`core,ios`); sub-dimensions may
refine them (`core/epub`, `ios/reader`).

## Directory Boundaries

- The repository root is not the root of any single component; do not place
  component-specific source or config at the root.
- Generated artifacts are never committed: `apps/ios/Generated/`,
  `apps/ios/Frameworks/`, `apps/ios/Inkuna.xcodeproj/`,
  `apps/android/app/src/generated/`, `apps/android/app/src/main/jniLibs/`,
  `core/target/`.
- No personal information (names, emails) in README or other public-facing
  docs.

## Machine/Toolchain Gotchas (this dev machine)

- Two Rust installs exist: Homebrew (`/opt/homebrew/bin`, no cross targets)
  shadows rustup. The scripts pin `RUSTC` + `rustup run stable`; do the same
  for any new cargo invocation that cross-compiles.
- System Java is too new for AGP; `apps/android/gradle.properties` pins
  `org.gradle.java.home` to Homebrew `openjdk@21`.
- A global cargo config redirects target-dir to `~/.sonelis/cargo-target`;
  the scripts override with `CARGO_TARGET_DIR=core/target`.
