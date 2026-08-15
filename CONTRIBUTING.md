# Contributing to Inkuna

Thanks for your interest. Bug reports, translations, and code are all welcome.

## Contributor License Agreement

Before a pull request can be merged, you need to sign the Inkuna CLA:

**[Inkuna Contributor License Agreement](https://gist.github.com/PythonShe/3c97ab17f679a42d675ffbebf62f42a2)**

You don't need to do anything in advance. Open your pull request and
[CLA Assistant](https://cla-assistant.io) will comment with a link to sign; the
`license/cla` check goes green once you have. Signing is a one-time step that
covers all of your past and future contributions.

Please read it rather than clicking through. In short:

- You keep the copyright in what you write.
- You grant a broad, permanent licence to use it, plus a patent licence for any
  of your patents your contribution needs.
- **That licence includes the right to release Inkuna under licences other than
  the AGPL-3.0, including proprietary or commercial terms.** Inkuna is AGPL-3.0
  today and there is no plan to change that, but the option is deliberately kept
  open, and signing means you agree to it in advance. Section 4 spells this out.
- Anything already released under the AGPL-3.0 stays available under the
  AGPL-3.0 to everyone who received it. That cannot be taken back.
- You are not paid for contributions.

If those terms don't work for you, that's a legitimate answer — please open an
issue instead, or fork under the AGPL-3.0.

## Before you start

For anything larger than a bug fix, open an issue first. It's a small project
with a specific idea of what it wants to be, and it's better to find out that a
feature is out of scope before you build it than after.

Two hard rules:

- **No DRM circumvention.** MOBI/AZW3 support covers DRM-free files only. Code
  that removes, bypasses, or works around DRM will not be accepted.
- **No dependency that pins another below its latest stable.** The project
  tracks latest stable across Rust, Swift, Kotlin, Gradle, AGP, JDK and SDKs.

## Building

See [Building](README.md#building) in the README for toolchain requirements and
per-platform commands. Run `cd core && cargo test` before opening a pull
request — the release workflows gate on it.

## Project layout and local rules

Each component carries its own `CLAUDE.md` with rules specific to it — read the
one for the directory you're touching, plus the root `CLAUDE.md` for
repo-wide constraints. `AGENTS.md` files are verbatim copies for other tooling;
if you edit a `CLAUDE.md`, copy it over its sibling `AGENTS.md` in the same
commit.

Changes to `core/crates/inkuna-ffi` require regenerating bindings with **both**
`scripts/build-core-ios.sh` and `scripts/build-core-android.sh` before the
shells will build.

Generated artifacts are never committed: `apps/ios/Generated/`,
`apps/ios/Frameworks/`, `apps/ios/Inkuna.xcodeproj/`,
`apps/android/app/src/generated/`, `apps/android/app/src/main/jniLibs/`,
`core/target/`.

## Commits

Format is `<type>(<scope>): <description>`.

Types: `feat`, `fix`, `refactor`, `docs`, `chore`, `test`, `perf`, `style`.

| scope | covers |
|-------|--------|
| `core` | `core/` |
| `ios` | `apps/ios/` |
| `android` | `apps/android/` |
| `website` | `website/` |
| `docs` | `docs/` |
| `workspace` | repo root (`CLAUDE.md`, `scripts/`, …) |

Combine scopes for cross-component changes (`core,ios`), or refine them
(`core/epub`, `ios/reader`). Keep commits in logical groups rather than one
large drop.

## A note on quality

Apple Books-level feel is the bar, and CJK support — vertical writing, CJK-aware
search — is a core product goal rather than an afterthought. Contributions that
touch typography or text layout should be checked against CJK content, not only
Latin.

## License

Inkuna is licensed under the [AGPL-3.0](LICENSE). Contributions are accepted
under that licence together with the CLA above.
