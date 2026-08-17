# Contributing to Inkuna

Thanks for your interest. Bug reports, translations, and code are all welcome.

## Contributor License Agreement

Before a pull request can be merged, you need to sign the Inkuna CLA:

**[Inkuna Contributor License Agreement](https://gist.github.com/PythonShe/3c97ab17f679a42d675ffbebf62f42a2)**

Open your pull request and [CLA Assistant](https://cla-assistant.io) will comment
with a link to sign; the `license/cla` check goes green once you have. One
signature covers all of your contributions, past and future (Section 11) — but if
the agreement is ever revised, CLA Assistant will ask everyone to sign the new
version (Section 15).

The signing form asks for your name, your e-mail address and country of
residence, and whether you are signing personally or binding an employer.
Section 16 covers how that record is stored and for how long.

**Your username is fine.** For a bug fix, a translation, a documentation change
or an ordinary feature — which is very nearly everything — you do not need to
give a legal name, and a pseudonymous signature is a perfectly normal thing to
have on file. The form asks whether the name you gave is your legal one; answer
it honestly and you are done. Saying "no" is not held against you.

There are two narrow exceptions. If a contribution is large enough to stand on
its own — a whole subsystem, a rewrite — I may ask for a legal name before it is
merged; because that kind of work starts with an issue anyway (see below), you
would hear it there rather than after writing the code. And if you are signing on
behalf of a company, that is different by construction: the entity has to be
identified by name (Section 10), and so should whoever is binding it.

Please read it rather than clicking through. In short:

- You keep the copyright in what you write.
- You grant a permanent, irrevocable, sublicensable licence to use it, plus a
  patent licence for any of your patents your contribution needs — that patent
  licence ends for anyone who sues over it.
- **That licence includes the right to release Inkuna under licences other than
  the AGPL-3.0, including proprietary or commercial terms.** Inkuna is AGPL-3.0
  today and there is no plan to change that, but the option is deliberately kept
  open, and signing means you agree to it in advance. Section 4 spells this out,
  and Section 13 lets those rights pass to a successor entity without asking you
  again.
- Anything already released under the AGPL-3.0 stays available under the
  AGPL-3.0 to everyone who received it. That cannot be taken back.
- You are not paid for contributions, except where mandatory law says otherwise.
- Credit is the version-control history, and you agree not to use moral rights to
  block the licences above. Where Inkuna ships without that history, you are
  named in an acknowledgements list unless you ask not to be — Section 9.
- You confirm the contribution is genuinely yours to give. If your employer,
  client or university holds rights in what you write, you need their permission
  first — Section 6.
- The agreement is governed by German law (Section 14).

If those terms don't work for you, that's a legitimate answer — fork under the
AGPL-3.0, or open an issue. Note that the agreement's definition of "Submitted"
reaches issues and discussions as well as pull requests; mark anything you don't
want covered as "Not a Contribution" and it falls outside it (Section 1).

## Before you start

For anything larger than a bug fix, open an issue first. It's a small project
with a specific idea of what it wants to be, and it's better to find out that a
feature is out of scope before you build it than after.

One hard rule, and one strong preference:

- **No DRM circumvention.** MOBI/AZW3 support covers DRM-free files only. Code
  that removes, bypasses, or works around DRM will not be accepted.
- **Latest stable wins.** The project tracks latest stable across Rust, Swift,
  Kotlin, Gradle, AGP, JDK and SDKs, and a dependency that caps another below its
  latest stable normally loses. Raise it in the issue rather than the pull
  request — there are exceptions, but they're decided deliberately.

## Branching

**Do not branch from or target `main`.** `main` is release-only — tags are cut
from it and the release workflows build from it. Pull requests opened against
it will be asked to retarget.

Work goes to one of two integration branches, chosen by what the change
touches:

| your change touches | base branch |
|---------------------|-------------|
| anything under `core/` | `dev/core` |
| `apps/ios/` or `apps/android/` | `dev/shells` |

A change that spans both — an FFI signature plus the shell code that calls it —
counts as touching `core/`, so it goes to `dev/core` as a single pull request.

**`website/`, `docs/`, `scripts/` and the repository root are maintained
directly and are not open to pull requests.** That includes the `CLAUDE.md` /
`AGENTS.md` files, the build scripts, and the site. If you spot something wrong
in any of them, open an issue rather than a patch.

The flow:

```sh
# 1. Fork on GitHub, then clone your fork
git clone https://github.com/<you>/Inkuna.git
cd Inkuna

# 2. Track the upstream repository
git remote add upstream https://github.com/PythonShe/Inkuna.git
git fetch upstream

# 3. Branch from the integration branch you are targeting, not from main
git switch -c fix/epub-spine-order upstream/dev/core

# 4. Work, commit, push to your fork
git push -u origin fix/epub-spine-order
```

Then open the pull request **against `dev/core`** (GitHub defaults the base to
`main`, so change it in the dropdown). Name your branch for the work —
`fix/…`, `feat/…`, `docs/…` — and keep one branch per pull request.

If the base branch moves while you work, rebase rather than merge, so the
history stays linear:

```sh
git fetch upstream
git rebase upstream/dev/core
```

The maintainer merges the integration branches into `main` at release time; you
never need to touch `main` yourself.

## Building

See [Building](README.md#building) in the README for toolchain requirements and
per-platform commands. Run `cd core && cargo test` before opening a pull
request: nothing runs it on the pull request itself, and the release workflows
will fail on a regression that reaches them.

The website (`website/`) is maintainer-only, but it builds locally if you want
to run it: **pnpm**, never npm or yarn — `corepack enable pnpm` once, then
`pnpm install` and `pnpm dev` / `pnpm build` inside `website/`.

## Project layout and local rules

Each component carries its own `CLAUDE.md` with rules specific to it — read the
one for the directory you're touching, plus the root `CLAUDE.md` for repo-wide
constraints. They are reference, not something to patch: like the rest of the
repository root they are maintained directly, and `AGENTS.md` files are just
verbatim copies of them for other tooling.

Changes to `core/crates/inkuna-ffi` require regenerating bindings with **both**
`scripts/build-core-ios.sh` and `scripts/build-core-android.sh` before the
shells will build.

Generated artifacts are never committed: `apps/ios/Generated/`,
`apps/ios/Frameworks/`, `apps/ios/Inkuna.xcodeproj/`,
`apps/android/app/src/generated/`, `apps/android/app/src/main/jniLibs/`,
`core/target/`.

## Translations

Both apps are fully localized and ship 14 languages: English, German, Spanish,
French, Indonesian, Italian, Japanese, Korean, Portuguese, Russian, Thai,
Vietnamese, and Simplified and Traditional Chinese. English is the source
language everywhere.

- **iOS** — one string catalog, `apps/ios/Inkuna/Localizable.xcstrings`. Open it
  in Xcode rather than editing the JSON by hand; it lists every key with its
  translation state per language.
- **Android** — `apps/android/app/src/main/res/values/strings.xml` holds the
  English source, and each translation is a sibling
  `values-<locale>/strings.xml` (`values-de`, `values-zh-rCN`, …).
- **Website** — maintainer-only, and only `en`, `ja` and `zh` so far. It uses
  Astro's built-in i18n routing: `website/src/i18n/locales.ts` declares the
  locales, dictionary modules beside it (`home.ts`, `privacy.ts`, `support.ts`,
  `footer.ts`) hold the strings, and `website/src/pages/<lang>/` holds the
  routes. Raise website translations in an issue rather than a pull request.

Improving an existing translation is a welcome pull request and doesn't need an
issue first. Adding a fourteenth-plus language does — the two apps are expected
to stay in step, so a new locale means both catalogs in the same change.

Two conventions to match: translations are formal where the language marks it
(German *Sie*, Indonesian *Anda*), and Portuguese is a single Brazilian-flavored
locale using *você*.

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

## Pull requests

Small and single-purpose gets reviewed faster than large and mixed. This is a
one-maintainer project, so expect review to take a few days rather than hours.

Check the base branch before you submit — see [Branching](#branching). It is
`dev/core` or `dev/shells`, never `main`.

## Reporting a security issue

Don't open a public issue for a vulnerability. Use GitHub's
[private vulnerability reporting](https://github.com/PythonShe/Inkuna/security/advisories/new)
instead. Format parsers are the interesting attack surface here — the app reads
untrusted EPUB, MOBI, AZW3 and PDF files.

## A note on quality

Apple Books-level feel is the bar, and CJK support — vertical writing, CJK-aware
search — is a core product goal rather than an afterthought. Contributions that
touch typography or text layout should be checked against CJK content, not only
Latin.

## License

Inkuna is distributed under the [AGPL-3.0](LICENSE). You license your
contributions to the maintainer under the CLA above, which is broader than the
AGPL-3.0 — see Section 4.
