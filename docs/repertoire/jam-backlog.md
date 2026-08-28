# Jam backlog

What is still worth doing, by lens. Scouts re-verify every entry against
current code before it may compete as a candidate again.

Updated: 2026-08-28 · c6bc448

## Features

- [P5] Show placed bookmarks and let readers jump back to them — bookmarks are write-only in both shells while the core API is complete · core/crates/inkuna-ffi/src/library.rs:279, apps/ios/Inkuna/Reader/ReaderViewController+Interactions.swift:162 (dead `jump(to bookmark:)`), ReaderSheets.kt:456 · rejected 2026-08-28 (M across both shells, too big for a fast session)
- [P4] Add "Find in book" to the text-selection menu — selecting a phrase and searching it means retyping today · ReaderSelectionController.swift:214-234, ReaderSelectionController.kt:180, panel preset at ReaderSearchPanel.swift:204 · rejected 2026-08-28 (docket full)
- [P4] Publish a changelog RSS/Atom feed on inkuna.app — no subscribe path for release-followers · website/src/lib/changelog.ts:1-60, astro.config.mjs:5 · rejected 2026-08-28 (docket full; hand-roll endpoint, per-locale, no new deps)
- [P3] "N pages left in this chapter" in the reader footer — Apple Books cue; computation exists for the home screen · ReaderViewController+Chrome.swift:9-23, ReaderChromeLayer.kt:137, TonightViewController.swift:163 · rejected 2026-08-28 (new plural strings ×16 locales ×2 shells)
- [P3] "Back to page N" return pill after TOC/search jumps — no way home after a jump · ReaderViewController+Interactions.swift:92-175, InkToastView.swift · rejected 2026-08-28 (anchor invalidation touches past-regression area 903a35d/1d74061)
- [P3] Library sort control (title/author) — both shells hardcode RecentlyOpened; core parks Title sort pending a shell affordance · inkuna-core/src/features/library/model.rs:79, LibraryViewController.swift:147, LibraryViewModel.kt:103 · rejected 2026-08-28 (needs Sort variant + full FFI bindings regen)

## Fixes

- [P3] Correct stale facts in docs/dev/architecture.md — says compile/target 35 (actual 37, build.gradle.kts:51,56) and routes CBZ "via Readium" though Readium is removed · architecture.md:81,:120 · rejected 2026-08-28 (contributor-facing, not user-facing)
- [P2] Historical GitHub release body still claims PDF import (baked into /en/changelog at build time) — fix by editing that Android release's body on GitHub, not in git · website/src/lib/changelog.ts fetch path · found by QC 2026-08-28 (out of repo's reach)

## Polish

- [P4] Open Graph preview card for inkuna.app — no og:image/twitter:card/canonical; shared links show text only · website/src/layouts/Base.astro:49-72 · rejected 2026-08-28 (wants a ~1200×630 raster via gen-icons.sh conventions)
- [P2] Delete dead PlaceholderLibrary calendar/facts fields — StatsViewController derives all of it from Calendar.current now · apps/ios/Inkuna/Model/PlaceholderLibrary.swift:34-53 · rejected 2026-08-28 (invisible to users; free cleanup when already in the file)
- [P2] Android finished-toggle lacks in-flight disable (iOS has it) — rapid double-tap idempotently re-writes · BookDetailScreen.kt:165 · noted by review 2026-08-28 (cosmetic asymmetry)
