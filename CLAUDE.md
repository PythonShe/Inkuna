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
| `website/` | Inkuna Website | Astro (static output) + pnpm | Marketing site for `inkuna.app`, built with Astro and deployed via Cloudflare Pages |

Rendering will live in the shells on Readium's native toolkits (Swift/Kotlin);
the Rust core never renders.

## Stack Policy

- **Latest everything**: newest stable Rust, Swift, Kotlin, Gradle, AGP, JDK,
  SDKs, and all dependencies. When bumping, query the registries
  (crates.io / Google Maven / Maven Central / services.gradle.org / npm) for
  actual latest stable — never guess from training data. A dependency that caps
  another below latest loses (e.g. refinery is deferred because it pins
  rusqlite below current).
- **pnpm, never npm/yarn**: `website/` is the only JavaScript component and it
  uses pnpm exclusively — `pnpm-lock.yaml` is the committed lockfile and the
  pnpm version is pinned by `packageManager` in `website/package.json`. Never
  run `npm`/`npx`/`yarn` in this repo (`pnpm dlx` replaces `npx`).
- **Mainstream crates over hand-rolling**: if a well-adopted crate exists for
  a need, use it. Designated choices for upcoming needs: `deadpool-sqlite`
  (concurrent DB access), `notify` (watch-folder import), `rayon` (parallel
  batch import), `argon2` (if auth lands), `lofty` (if audio lands).
- **Format strategy**: reflowable formats (EPUB, MOBI, AZW3, TXT) normalize
  to EPUB in the core at import; fixed-layout (PDF, CBZ/CBR) get dedicated
  navigators. MOBI/AZW3 support is DRM-free files only — never implement or
  integrate DRM circumvention.

## Scope Rules

- These root rules cover only monorepo-level constraints, not component
  implementation details.
- For a task within `core/`, `apps/ios/`, `apps/android/`, or `website/`, also
  follow that directory's `CLAUDE.md`.
- For tasks spanning components (e.g. an FFI change), follow this file first,
  then each affected component's rules. After any `core/crates/inkuna-ffi`
  change, regenerate bindings via BOTH `scripts/build-core-*.sh` before
  building the shells.
- Each `AGENTS.md` is a verbatim copy of its sibling `CLAUDE.md` (for Codex).
  After editing any `CLAUDE.md`, re-copy it over the sibling `AGENTS.md` in
  the same commit.

## Documentation Priority

- `docs/dev/architecture.md` — architecture decision record (why Rust core +
  native shells, version targets, roadmap). Read before structural changes.

## CI/CD (GitHub Actions)

- `deploy-website.yml` — builds `website/` with Astro (pnpm via
  `pnpm/action-setup`, `pnpm install --frozen-lockfile`) and deploys `dist/`
  to Cloudflare Pages (project `inkuna`, direct upload) on every `main` push
  touching `website/`.
- `release-ios.yml` / `release-android.yml` — tag-driven releases
  (`ios-vX.Y.Z+N` / `android-vX.Y.Z+N`): core tests gate the build; iOS
  archives with manual signing and uploads to TestFlight, Android builds a
  signed APK attached to a GitHub release.
- `release-notes.yml` — standalone/reusable AI release-notes generation
  (OpenAI chat completions; model/endpoint overridable via the `LLM_MODEL` /
  `LLM_BASE_URL` repo variables), called by both release workflows.
- Versions live in the component files (`core/Cargo.toml` workspace version,
  `apps/ios/project.yml`, `apps/android/app/build.gradle.kts`); bump them ONLY
  via `scripts/bump-version.sh <core|ios|android> <major|minor|patch|X.Y.Z>
  [--tag]` — the release workflows fail on tag/file version mismatch.
- Secrets and signing material never enter git history: Android signing comes
  from `ANDROID_*` secrets (keystore + credentials also live locally in
  `~/Documents/Dev/inkuna/`), iOS from `APPSTORE_*`/`IOS_*`/`APPLE_TEAM_ID` secrets
  (ExportOptions.plist is generated at CI time), website deploys from
  `CLOUDFLARE_*` secrets, notes from `OPENAI_API_KEY`.

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
| `website` | Changes within `website/` |
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
- `apps/android/gradle.properties` pins `org.gradle.java.home` to the latest
  Homebrew OpenJDK; if a Gradle release ever lags the JDK, fall back to
  Android Studio's JBR.
- `rustup run stable` does NOT shadow PATH lookups here: pin `RUSTC` (and
  `RUSTDOC` for doctests) to `rustup which --toolchain stable ...` or builds
  silently use the Homebrew rustc, which fails cross-compiles with E0463.
- A global cargo config redirects target-dir to `~/.sonelis/cargo-target`;
  the scripts override with `CARGO_TARGET_DIR=core/target`.
